//! [`ObjectStore`]: the five S3 operations the finalizer performs, behind one seam.
//!
//! The finalizer reads what the jobs wrote (size-capped), writes `verdict.json`
//! and `manifest.json` with a retention tag, re-tags objects, reads the tags back
//! and counts what a run prefix holds. Nothing here deletes an object: expiry is
//! the lifecycle rule's job (D-19), and the trait has no delete to misuse.
//!
//! The in-memory implementation can be told to fail the n-th call of an
//! operation, so every publication step's crash-resume is provable offline.

use aprender_mcp_laya_train::layout::RETENTION_TAG_KEY;
use aws_sdk_s3::error::DisplayErrorContext;

/// Why a read did not return bytes. Text never includes object content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectError {
    /// The object is longer than the cap the caller named (the read stopped early).
    Oversize,
    /// Anything else (a connection failure, a 5xx, a denied call, a broken body).
    Failed(String),
}

impl std::fmt::Display for ObjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Oversize => f.write_str("object exceeds its read cap"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

/// The finalizer's view of one bucket. Keys are full S3 keys (`runs/...`).
#[pmcp::async_trait]
pub trait ObjectStore: Send + Sync {
    /// Read an object, or `None` when it does not exist.
    ///
    /// # Errors
    ///
    /// [`ObjectError::Oversize`] when the body is longer than `max_bytes`;
    /// [`ObjectError::Failed`] for any other failure.
    async fn get(&self, key: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, ObjectError>;

    /// Write an object carrying the single tag `laya-retention=<retention>`.
    ///
    /// # Errors
    ///
    /// The SDK's message.
    async fn put(&self, key: &str, bytes: Vec<u8>, retention: &str) -> Result<(), String>;

    /// Replace the object's tag set with `laya-retention=<value>`.
    ///
    /// # Errors
    ///
    /// The SDK's message.
    async fn put_tag(&self, key: &str, value: &str) -> Result<(), String>;

    /// The object's `laya-retention` value, `None` when it has no such tag.
    ///
    /// # Errors
    ///
    /// The SDK's message (including a missing object).
    async fn get_tag(&self, key: &str) -> Result<Option<String>, String>;

    /// Every object under `prefix` with its size, across all result pages.
    ///
    /// # Errors
    ///
    /// The SDK's message.
    async fn list(&self, prefix: &str) -> Result<Vec<(String, u64)>, String>;
}

/// The aws-sdk-s3 [`ObjectStore`], bound to one bucket.
#[derive(Debug, Clone)]
pub struct S3ObjectStore {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3ObjectStore {
    /// Bind a client to a bucket.
    #[must_use]
    pub fn new(client: aws_sdk_s3::Client, bucket: impl Into<String>) -> Self {
        Self {
            client,
            bucket: bucket.into(),
        }
    }

    fn tagging(value: &str) -> Result<aws_sdk_s3::types::Tagging, String> {
        let tag = aws_sdk_s3::types::Tag::builder()
            .key(RETENTION_TAG_KEY)
            .value(value)
            .build()
            .map_err(|e| e.to_string())?;
        aws_sdk_s3::types::Tagging::builder()
            .tag_set(tag)
            .build()
            .map_err(|e| e.to_string())
    }
}

#[pmcp::async_trait]
impl ObjectStore for S3ObjectStore {
    async fn get(&self, key: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, ObjectError> {
        let out = match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(out) => out,
            Err(e) if e.as_service_error().is_some_and(|se| se.is_no_such_key()) => {
                return Ok(None)
            }
            Err(e) => return Err(ObjectError::Failed(DisplayErrorContext(&e).to_string())),
        };
        if out
            .content_length()
            .is_some_and(|len| usize::try_from(len).map_or(true, |len| len > max_bytes))
        {
            return Err(ObjectError::Oversize);
        }
        let mut body = out.body;
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = body
            .try_next()
            .await
            .map_err(|e| ObjectError::Failed(format!("reading {key}: {e}")))?
        {
            buf.extend_from_slice(&chunk);
            if buf.len() > max_bytes {
                return Err(ObjectError::Oversize);
            }
        }
        Ok(Some(buf))
    }

    async fn put(&self, key: &str, bytes: Vec<u8>, retention: &str) -> Result<(), String> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type("application/json")
            .tagging(format!("{RETENTION_TAG_KEY}={retention}"))
            .body(aws_sdk_s3::primitives::ByteStream::from(bytes))
            .send()
            .await
            .map(|_| ())
            .map_err(|e| DisplayErrorContext(&e).to_string())
    }

    async fn put_tag(&self, key: &str, value: &str) -> Result<(), String> {
        self.client
            .put_object_tagging()
            .bucket(&self.bucket)
            .key(key)
            .tagging(Self::tagging(value)?)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| DisplayErrorContext(&e).to_string())
    }

    async fn get_tag(&self, key: &str) -> Result<Option<String>, String> {
        let out = self
            .client
            .get_object_tagging()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| DisplayErrorContext(&e).to_string())?;
        Ok(out
            .tag_set()
            .iter()
            .find(|tag| tag.key() == RETENTION_TAG_KEY)
            .map(|tag| tag.value().to_string()))
    }

    async fn list(&self, prefix: &str) -> Result<Vec<(String, u64)>, String> {
        let mut found = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);
            if let Some(t) = token.take() {
                request = request.continuation_token(t);
            }
            let page = request
                .send()
                .await
                .map_err(|e| DisplayErrorContext(&e).to_string())?;
            for object in page.contents() {
                if let Some(key) = object.key() {
                    let size = u64::try_from(object.size().unwrap_or(0)).unwrap_or(0);
                    found.push((key.to_string(), size));
                }
            }
            match page.next_continuation_token() {
                Some(next) if page.is_truncated().unwrap_or(false) => {
                    token = Some(next.to_string());
                }
                _ => break,
            }
        }
        Ok(found)
    }
}

#[cfg(test)]
pub(crate) mod fakes {
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Mutex;

    use super::{ObjectError, ObjectStore};

    /// The operations a fault can be aimed at.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Op {
        Get,
        Put,
        PutTag,
        GetTag,
        List,
    }

    #[derive(Default)]
    struct Inner {
        /// key -> (bytes, retention tag).
        objects: BTreeMap<String, (Vec<u8>, Option<String>)>,
        /// Calls seen so far, per operation.
        calls: HashMap<Op, usize>,
        /// `(op, n)`: fail the n-th call (1-based) of `op` exactly once.
        faults: Vec<(Op, usize)>,
    }

    /// An in-memory bucket that can fail the n-th call of an operation once.
    #[derive(Default)]
    pub struct FakeObjects {
        inner: Mutex<Inner>,
        /// Every mutating call, in order: `put:<key>`, `tag:<key>=<value>`. Shared
        /// so a test can interleave it with the backend's calls.
        log: std::sync::Arc<Mutex<Vec<String>>>,
    }

    impl FakeObjects {
        /// A bucket that records its mutating calls into `log`.
        pub fn with_log(log: std::sync::Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                inner: Mutex::default(),
                log,
            }
        }

        fn log_push(&self, line: String) {
            self.log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(line);
        }

        fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }

        /// Seed an object as a job would have written it (tag `expire`).
        pub fn seed(&self, key: &str, bytes: &[u8]) {
            self.inner().objects.insert(
                key.to_string(),
                (bytes.to_vec(), Some("expire".to_string())),
            );
        }

        /// Fail the `n`-th (1-based, counted from now) call of `op`, once.
        pub fn fail_nth(&self, op: Op, n: usize) {
            let mut inner = self.inner();
            let base = inner.calls.get(&op).copied().unwrap_or(0);
            inner.faults.push((op, base + n));
        }

        pub fn bytes(&self, key: &str) -> Option<Vec<u8>> {
            self.inner().objects.get(key).map(|(b, _)| b.clone())
        }

        pub fn tag(&self, key: &str) -> Option<String> {
            self.inner().objects.get(key).and_then(|(_, t)| t.clone())
        }

        /// Every key with its bytes and tag: the end state, for equality checks.
        pub fn snapshot(&self) -> BTreeMap<String, (Vec<u8>, Option<String>)> {
            self.inner().objects.clone()
        }

        /// The mutating calls so far, in order.
        pub fn log(&self) -> Vec<String> {
            self.log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }

        fn enter(&self, op: Op) -> Result<(), String> {
            let mut inner = self.inner();
            let n = inner.calls.entry(op).or_insert(0);
            *n += 1;
            let n = *n;
            if let Some(at) = inner.faults.iter().position(|f| *f == (op, n)) {
                inner.faults.remove(at);
                return Err(format!("injected {op:?} failure #{n}"));
            }
            Ok(())
        }
    }

    #[pmcp::async_trait]
    impl ObjectStore for FakeObjects {
        async fn get(&self, key: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, ObjectError> {
            self.enter(Op::Get).map_err(ObjectError::Failed)?;
            match self.inner().objects.get(key) {
                None => Ok(None),
                Some((bytes, _)) if bytes.len() > max_bytes => Err(ObjectError::Oversize),
                Some((bytes, _)) => Ok(Some(bytes.clone())),
            }
        }

        async fn put(&self, key: &str, bytes: Vec<u8>, retention: &str) -> Result<(), String> {
            self.enter(Op::Put)?;
            self.inner()
                .objects
                .insert(key.to_string(), (bytes, Some(retention.to_string())));
            self.log_push(format!("put:{key}"));
            Ok(())
        }

        async fn put_tag(&self, key: &str, value: &str) -> Result<(), String> {
            self.enter(Op::PutTag)?;
            {
                let mut inner = self.inner();
                let Some(entry) = inner.objects.get_mut(key) else {
                    return Err(format!("NoSuchKey {key}"));
                };
                entry.1 = Some(value.to_string());
            }
            self.log_push(format!("tag:{key}={value}"));
            Ok(())
        }

        async fn get_tag(&self, key: &str) -> Result<Option<String>, String> {
            self.enter(Op::GetTag)?;
            match self.inner().objects.get(key) {
                None => Err(format!("NoSuchKey {key}")),
                Some((_, tag)) => Ok(tag.clone()),
            }
        }

        async fn list(&self, prefix: &str) -> Result<Vec<(String, u64)>, String> {
            self.enter(Op::List)?;
            Ok(self
                .inner()
                .objects
                .iter()
                .filter(|(key, _)| key.starts_with(prefix))
                .map(|(key, (bytes, _))| (key.clone(), bytes.len() as u64))
                .collect())
        }
    }
}
