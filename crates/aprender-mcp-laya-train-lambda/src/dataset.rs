//! [`S3DatasetSource`]: the Laya training server's `DatasetSource` over S3.
//!
//! The request function reads a dataset only from its own bucket's `datasets/`
//! prefix. The bucket is fixed by the deployment's environment and the prefix has
//! already passed `aprender_mcp_laya_train::parse_dataset_uri`, so a caller names
//! neither a foreign bucket nor a key outside `datasets/`.
//!
//! The S3 calls sit behind the [`ObjectReader`] seam so every rule here (a missing
//! file, an absent shift probe) is proven offline against a fake.

use aprender_mcp_laya_train::validate::{DatasetFiles, Refusal};
use aprender_mcp_laya_train::DatasetSource;
use aws_sdk_s3::error::DisplayErrorContext;

/// Why an object read failed. Text never includes object content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The caller's role may not read the object (S3 answered 403 / AccessDenied).
    Denied(String),
    /// Anything else (a connection failure, a 5xx, a broken body stream).
    Other(String),
}

/// The two S3 calls the dataset source needs.
#[pmcp::async_trait]
pub trait ObjectReader: Send + Sync {
    /// The object's size in bytes, or `None` when it does not exist.
    ///
    /// # Errors
    ///
    /// [`ReadError::Denied`] on a 403, [`ReadError::Other`] on any other failure.
    async fn head(&self, bucket: &str, key: &str) -> Result<Option<u64>, ReadError>;

    /// Read the object, stopping after `max_bytes + 1` bytes so the caller can
    /// tell a body longer than `max_bytes` from one that fits.
    ///
    /// # Errors
    ///
    /// As [`ObjectReader::head`].
    async fn get(&self, bucket: &str, key: &str, max_bytes: u64) -> Result<Vec<u8>, ReadError>;
}

/// The aws-sdk-s3 [`ObjectReader`].
#[derive(Debug, Clone)]
pub struct S3ObjectReader {
    client: aws_sdk_s3::Client,
}

impl S3ObjectReader {
    /// Wrap a client.
    #[must_use]
    pub fn new(client: aws_sdk_s3::Client) -> Self {
        Self { client }
    }
}

/// 403 / AccessDenied, from the error code or the raw status. A HEAD response has
/// no body, so the code alone is not enough.
fn is_denied<E>(
    err: &aws_sdk_s3::error::SdkError<E, aws_sdk_s3::config::http::HttpResponse>,
) -> bool
where
    E: aws_sdk_s3::error::ProvideErrorMetadata,
{
    use aws_sdk_s3::error::ProvideErrorMetadata;
    let by_code = matches!(
        err.code(),
        Some("AccessDenied" | "Forbidden" | "403" | "AllAccessDisabled")
    );
    let by_status = err
        .raw_response()
        .is_some_and(|raw| raw.status().as_u16() == 403);
    by_code || by_status
}

#[pmcp::async_trait]
impl ObjectReader for S3ObjectReader {
    async fn head(&self, bucket: &str, key: &str) -> Result<Option<u64>, ReadError> {
        match self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(out) => Ok(Some(
                u64::try_from(out.content_length().unwrap_or(0)).unwrap_or(0),
            )),
            // Not-found is trusted as "absent" only because the grant carries a
            // prefix-scoped s3:ListBucket: without it S3 answers 403 for a missing key.
            Err(e) if e.as_service_error().is_some_and(|se| se.is_not_found()) => Ok(None),
            Err(e) if is_denied(&e) => Err(ReadError::Denied(DisplayErrorContext(&e).to_string())),
            Err(e) => Err(ReadError::Other(DisplayErrorContext(&e).to_string())),
        }
    }

    async fn get(&self, bucket: &str, key: &str, max_bytes: u64) -> Result<Vec<u8>, ReadError> {
        let out = match self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(out) => out,
            Err(e) if is_denied(&e) => {
                return Err(ReadError::Denied(DisplayErrorContext(&e).to_string()))
            }
            Err(e) => return Err(ReadError::Other(DisplayErrorContext(&e).to_string())),
        };
        let mut body = out.body;
        let mut buf: Vec<u8> = Vec::new();
        let stop_at = max_bytes.saturating_add(1);
        while let Some(chunk) = body
            .try_next()
            .await
            .map_err(|e| ReadError::Other(format!("reading {key}: {e}")))?
        {
            buf.extend_from_slice(&chunk);
            if buf.len() as u64 >= stop_at {
                buf.truncate(usize::try_from(stop_at).unwrap_or(usize::MAX));
                break;
            }
        }
        Ok(buf)
    }
}

/// The dataset files a data dir may hold, in the order they are read.
const FILES: [&str; 4] = ["task.json", "train.jsonl", "eval.jsonl", "shift.jsonl"];

/// Reads `task.json`, `train.jsonl`, `eval.jsonl` and `shift.jsonl` under a
/// `datasets/<segs>/` prefix of one bucket.
#[derive(Debug, Clone)]
pub struct S3DatasetSource<R: ObjectReader> {
    reader: R,
    bucket: String,
}

impl<R: ObjectReader> S3DatasetSource<R> {
    /// A source over `bucket`.
    #[must_use]
    pub fn new(reader: R, bucket: impl Into<String>) -> Self {
        Self {
            reader,
            bucket: bucket.into(),
        }
    }
}

#[pmcp::async_trait]
impl<R: ObjectReader> DatasetSource for S3DatasetSource<R> {
    async fn fetch(&self, dataset_prefix: &str) -> Result<DatasetFiles, Refusal> {
        let mut out: [Option<Vec<u8>>; 4] = [None, None, None, None];
        for (slot, file) in out.iter_mut().zip(FILES) {
            let key = format!("{dataset_prefix}{file}");
            let size = self
                .reader
                .head(&self.bucket, &key)
                .await
                .map_err(|e| read_refusal(&e, file))?;
            if let Some(size) = size {
                *slot = Some(
                    self.reader
                        .get(&self.bucket, &key, size)
                        .await
                        .map_err(|e| read_refusal(&e, file))?,
                );
            }
        }
        let [task_json, train, eval, shift] = out;
        Ok(DatasetFiles {
            task_json,
            train,
            eval,
            shift,
        })
    }
}

fn read_refusal(err: &ReadError, file: &str) -> Refusal {
    match err {
        ReadError::Denied(_) | ReadError::Other(_) => {
            Refusal::new("dataset-read", format!("cannot read {file}"))
        }
    }
}

#[cfg(test)]
pub(crate) mod fakes {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::{ObjectReader, ReadError};

    /// An in-memory bucket: `(bucket, key)` to bytes, plus a log of calls.
    #[derive(Default)]
    pub struct FakeReader {
        pub objects: BTreeMap<(String, String), Vec<u8>>,
        pub calls: Mutex<Vec<String>>,
    }

    impl FakeReader {
        pub fn put(&mut self, bucket: &str, key: &str, bytes: Vec<u8>) {
            self.objects
                .insert((bucket.to_string(), key.to_string()), bytes);
        }
    }

    #[pmcp::async_trait]
    impl ObjectReader for FakeReader {
        async fn head(&self, bucket: &str, key: &str) -> Result<Option<u64>, ReadError> {
            self.calls.lock().expect("lock").push(format!("head {key}"));
            Ok(self
                .objects
                .get(&(bucket.to_string(), key.to_string()))
                .map(|b| b.len() as u64))
        }

        async fn get(&self, bucket: &str, key: &str, max_bytes: u64) -> Result<Vec<u8>, ReadError> {
            self.calls.lock().expect("lock").push(format!("get {key}"));
            let bytes = self
                .objects
                .get(&(bucket.to_string(), key.to_string()))
                .ok_or_else(|| ReadError::Other("no such key".to_string()))?;
            let cut = usize::try_from(max_bytes.saturating_add(1)).unwrap_or(usize::MAX);
            Ok(bytes.iter().copied().take(cut).collect())
        }
    }
}
