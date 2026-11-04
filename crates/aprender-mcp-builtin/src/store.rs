//! The object store the precheck and `verify` read through (design §5.4): `HeadObject`, a
//! bounded `GetObject` that never reads past its cap, and a create-only `PutObject` for the
//! verification record. An in-memory stand-in for tests and the S3 implementor.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, PoisonError};

use aws_sdk_s3::error::{DisplayErrorContext, ProvideErrorMetadata, SdkError};
use aws_sdk_s3::primitives::ByteStream;

/// A store failure: `<op> <uri>: <cause>`, the precheck's `s3-unreachable` detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError(pub String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

/// The outcome of a create-only put.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutOutcome {
    Written,
    AlreadyExists,
}

pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StoreError>> + Send + 'a>>;

/// HEAD, bounded GET and create-only PUT over `s3://bucket/key` URIs.
pub trait ObjectStore: Send + Sync {
    /// `Some(content_length)` when the object exists; `None` when it does not.
    fn head<'a>(&'a self, uri: &'a str) -> StoreFuture<'a, Option<u64>>;
    /// The bytes, or `None` when absent OR larger than `cap` (never read past the cap).
    fn get<'a>(&'a self, uri: &'a str, cap: u64) -> StoreFuture<'a, Option<Vec<u8>>>;
    /// Write `bytes` only if nothing is at `uri`.
    fn put_if_absent<'a>(&'a self, uri: &'a str, bytes: Vec<u8>) -> StoreFuture<'a, PutOutcome>;
}

/// Split `s3://bucket/key`.
///
/// # Errors
///
/// A [`StoreError`] naming the malformed URI.
pub fn parse_s3_uri(uri: &str) -> Result<(String, String), StoreError> {
    aprender_mcp_decide_lambda::parse_s3_uri(uri).map_err(|e| StoreError(format!("{uri}: {e}")))
}

// ---------------------------------------------------------------------------
// In-memory stand-in
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct MemInner {
    objects: HashMap<String, Vec<u8>>,
    declared_size: HashMap<String, u64>,
    unreachable: HashSet<String>,
}

/// Real bytes, optional declared sizes (to simulate a huge object without allocating it),
/// and a set of URIs that fail like a network error. Interior mutability so a shared
/// `Arc<MemStore>` can be edited by a test while a server holds it.
#[derive(Debug, Default)]
pub struct MemStore {
    inner: Mutex<MemInner>,
}

impl MemStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, MemInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn put(&self, uri: &str, bytes: impl Into<Vec<u8>>) {
        self.lock().objects.insert(uri.to_string(), bytes.into());
    }

    pub fn remove(&self, uri: &str) {
        let mut g = self.lock();
        g.objects.remove(uri);
        g.declared_size.remove(uri);
    }

    pub fn declare_size(&self, uri: &str, size: u64) {
        self.lock().declared_size.insert(uri.to_string(), size);
    }

    pub fn fail(&self, uri: &str) {
        self.lock().unreachable.insert(uri.to_string());
    }

    /// The stored bytes, synchronously (for assertions).
    #[must_use]
    pub fn get_sync(&self, uri: &str) -> Option<Vec<u8>> {
        self.lock().objects.get(uri).cloned()
    }

    /// Every stored URI under `prefix`, sorted.
    #[must_use]
    pub fn keys_under(&self, prefix: &str) -> Vec<String> {
        let mut keys: Vec<String> = self
            .lock()
            .objects
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    fn size_of(g: &MemInner, uri: &str) -> Option<u64> {
        g.declared_size
            .get(uri)
            .copied()
            .or_else(|| g.objects.get(uri).map(|b| b.len() as u64))
    }
}

impl ObjectStore for MemStore {
    fn head<'a>(&'a self, uri: &'a str) -> StoreFuture<'a, Option<u64>> {
        Box::pin(async move {
            let g = self.lock();
            if g.unreachable.contains(uri) {
                return Err(StoreError(format!("HeadObject {uri}: connection reset")));
            }
            Ok(Self::size_of(&g, uri))
        })
    }

    fn get<'a>(&'a self, uri: &'a str, cap: u64) -> StoreFuture<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            let g = self.lock();
            if g.unreachable.contains(uri) {
                return Err(StoreError(format!("GetObject {uri}: connection reset")));
            }
            match Self::size_of(&g, uri) {
                None => Ok(None),
                Some(n) if n > cap => Ok(None),
                Some(_) => Ok(g.objects.get(uri).cloned()),
            }
        })
    }

    fn put_if_absent<'a>(&'a self, uri: &'a str, bytes: Vec<u8>) -> StoreFuture<'a, PutOutcome> {
        Box::pin(async move {
            let mut g = self.lock();
            if g.unreachable.contains(uri) {
                return Err(StoreError(format!("PutObject {uri}: connection reset")));
            }
            if g.objects.contains_key(uri) {
                return Ok(PutOutcome::AlreadyExists);
            }
            g.objects.insert(uri.to_string(), bytes);
            Ok(PutOutcome::Written)
        })
    }
}

// ---------------------------------------------------------------------------
// S3
// ---------------------------------------------------------------------------

/// How an SDK error reads to the rules: a clean miss, or a failure with its cause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdkOutcome {
    Absent,
    Failure(String),
}

/// Classify an S3 error by its HTTP status and error code. A 404 (`NotFound`, `NoSuchKey`)
/// is a miss. A 403 names both causes, because without `s3:ListBucket` S3 answers a HEAD on
/// a missing key with 403 (design §5.2, §5.4). Everything else carries the SDK's display.
#[must_use]
pub fn classify_s3_error(
    op: &str,
    uri: &str,
    status: Option<u16>,
    code: Option<&str>,
    display: &str,
) -> SdkOutcome {
    if status == Some(404) || matches!(code, Some("NotFound" | "NoSuchKey")) {
        return SdkOutcome::Absent;
    }
    if status == Some(403) {
        return SdkOutcome::Failure(format!(
            "{op} {uri}: 403 Forbidden (the object is missing, or the builder role cannot read it)"
        ));
    }
    SdkOutcome::Failure(format!("{op} {uri}: {display}"))
}

fn sdk_outcome<E>(
    op: &str,
    uri: &str,
    err: &SdkError<E, aws_sdk_s3::config::http::HttpResponse>,
) -> SdkOutcome
where
    E: ProvideErrorMetadata + std::error::Error + 'static,
{
    let (status, code) = match err {
        SdkError::ServiceError(e) => (Some(e.raw().status().as_u16()), e.err().code()),
        SdkError::ResponseError(e) => (Some(e.raw().status().as_u16()), None),
        _ => (None, None),
    };
    classify_s3_error(op, uri, status, code, &DisplayErrorContext(err).to_string())
}

/// The S3 implementor over the default credential and region chain.
#[derive(Debug, Clone)]
pub struct S3Store {
    client: aws_sdk_s3::Client,
}

impl S3Store {
    #[must_use]
    pub fn new(client: aws_sdk_s3::Client) -> Self {
        Self { client }
    }

    /// A store using the default AWS credential and region chain (the Lambda or builder role).
    pub async fn from_default_config() -> Self {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        Self::new(aws_sdk_s3::Client::new(&config))
    }

    async fn head_len(&self, uri: &str) -> Result<Option<u64>, StoreError> {
        let (bucket, key) = parse_s3_uri(uri)?;
        match self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(head) => Ok(head.content_length().and_then(|n| u64::try_from(n).ok())),
            Err(e) => match sdk_outcome("HeadObject", uri, &e) {
                SdkOutcome::Absent => Ok(None),
                SdkOutcome::Failure(d) => Err(StoreError(d)),
            },
        }
    }
}

impl ObjectStore for S3Store {
    fn head<'a>(&'a self, uri: &'a str) -> StoreFuture<'a, Option<u64>> {
        Box::pin(self.head_len(uri))
    }

    fn get<'a>(&'a self, uri: &'a str, cap: u64) -> StoreFuture<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            // HEAD first: an absent or over-cap object is never fetched (never read past the cap).
            match self.head_len(uri).await? {
                None => return Ok(None),
                Some(n) if n > cap => return Ok(None),
                Some(_) => {}
            }
            let (bucket, key) = parse_s3_uri(uri)?;
            let object = match self
                .client
                .get_object()
                .bucket(bucket)
                .key(key)
                .send()
                .await
            {
                Ok(o) => o,
                Err(e) => {
                    return match sdk_outcome("GetObject", uri, &e) {
                        SdkOutcome::Absent => Ok(None),
                        SdkOutcome::Failure(d) => Err(StoreError(d)),
                    }
                }
            };
            let bytes = object
                .body
                .collect()
                .await
                .map_err(|e| StoreError(format!("GetObject {uri}: body: {e}")))?;
            Ok(Some(bytes.into_bytes().to_vec()))
        })
    }

    fn put_if_absent<'a>(&'a self, uri: &'a str, bytes: Vec<u8>) -> StoreFuture<'a, PutOutcome> {
        Box::pin(async move {
            let (bucket, key) = parse_s3_uri(uri)?;
            let sent = self
                .client
                .put_object()
                .bucket(bucket)
                .key(key)
                .if_none_match("*")
                .content_type("application/json")
                .body(ByteStream::from(bytes))
                .send()
                .await;
            match sent {
                Ok(_) => Ok(PutOutcome::Written),
                Err(e) => {
                    let precondition = match &e {
                        SdkError::ServiceError(s) => {
                            s.raw().status().as_u16() == 412
                                || s.err().code() == Some("PreconditionFailed")
                        }
                        _ => false,
                    };
                    if precondition {
                        return Ok(PutOutcome::AlreadyExists);
                    }
                    match sdk_outcome("PutObject", uri, &e) {
                        SdkOutcome::Absent => Err(StoreError(format!(
                            "PutObject {uri}: {}",
                            DisplayErrorContext(&e)
                        ))),
                        SdkOutcome::Failure(d) => Err(StoreError(d)),
                    }
                }
            }
        })
    }
}
