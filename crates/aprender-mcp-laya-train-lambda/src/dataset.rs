//! [`S3DatasetSource`]: the Laya training server's `DatasetSource` over S3.
//!
//! The request function reads a dataset only from its own bucket's `datasets/`
//! prefix. The bucket is fixed by the deployment's environment and the prefix has
//! already passed `aprender_mcp_laya_train::parse_dataset_uri`, so a caller names
//! neither a foreign bucket nor a key outside `datasets/`.
//!
//! The S3 calls sit behind the [`ObjectReader`] seam so every rule here (a missing
//! file, an absent shift probe) is proven offline against a fake.

use aprender_mcp_laya_train::layout::{MAX_FILE_BYTES, MAX_TOTAL_BYTES};
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
///
/// The transport bounds are enforced in two places on purpose: on the HeadObject
/// sizes BEFORE any GetObject (so an oversized dataset costs no transfer), and again
/// while the body streams (so an object that grew between the two calls, or lied
/// about its length, cannot make the function buffer more than the size it was
/// admitted at). The library's own `bound_check` stays the rule of record; these
/// are the same rule names and limits applied earlier.
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

    /// The refusal for a 403: names the bucket and prefix, never the SDK text.
    fn denied(&self, dataset_prefix: &str) -> Refusal {
        Refusal::new(
            "dataset-access",
            format!(
                "the server role has no read grant on s3://{}/{dataset_prefix}",
                self.bucket
            ),
        )
    }

    fn map_error(&self, err: &ReadError, dataset_prefix: &str, file: &str) -> Refusal {
        match err {
            ReadError::Denied(detail) => {
                tracing::warn!("laya-train dataset read denied for {file}: {detail}");
                self.denied(dataset_prefix)
            }
            ReadError::Other(detail) => {
                tracing::warn!("laya-train dataset read failed for {file}: {detail}");
                Refusal::new("dataset-read", format!("cannot read {file}"))
            }
        }
    }
}

#[pmcp::async_trait]
impl<R: ObjectReader> DatasetSource for S3DatasetSource<R> {
    async fn fetch(&self, dataset_prefix: &str) -> Result<DatasetFiles, Refusal> {
        // 1. HeadObject for every file: absence, denial and size, before any transfer.
        let mut sizes: [Option<u64>; 4] = [None; 4];
        for (slot, file) in sizes.iter_mut().zip(FILES) {
            let key = format!("{dataset_prefix}{file}");
            *slot = self
                .reader
                .head(&self.bucket, &key)
                .await
                .map_err(|e| self.map_error(&e, dataset_prefix, file))?;
        }

        // 2. The transport bounds, on the declared sizes.
        let mut total: u64 = 0;
        for (size, file) in sizes.iter().zip(FILES) {
            let size = size.unwrap_or(0);
            if size > MAX_FILE_BYTES as u64 {
                return Err(Refusal::new(
                    "bound-file",
                    format!("{file} is {size} bytes; at most {MAX_FILE_BYTES} bytes per file"),
                ));
            }
            total += size;
        }
        if total > MAX_TOTAL_BYTES as u64 {
            return Err(Refusal::new(
                "bound-total",
                format!("the dataset is {total} bytes in total; at most {MAX_TOTAL_BYTES} bytes"),
            ));
        }

        // 3. GetObject for the files that exist, capped at the admitted size while reading.
        let mut out: [Option<Vec<u8>>; 4] = [None, None, None, None];
        for ((slot, size), file) in out.iter_mut().zip(sizes).zip(FILES) {
            let Some(size) = size else { continue };
            let key = format!("{dataset_prefix}{file}");
            let bytes = self
                .reader
                .get(&self.bucket, &key, size)
                .await
                .map_err(|e| self.map_error(&e, dataset_prefix, file))?;
            if bytes.len() as u64 > size {
                return Err(Refusal::new(
                    "bound-file",
                    format!("{file} is longer than the {size} bytes it was admitted at"),
                ));
            }
            *slot = Some(bytes);
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

#[cfg(test)]
pub(crate) mod fakes {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::{ObjectReader, ReadError};

    /// An in-memory bucket: `(bucket, key)` to bytes, plus a log of calls.
    #[derive(Default)]
    pub struct FakeReader {
        pub objects: BTreeMap<(String, String), Vec<u8>>,
        /// Sizes HeadObject reports instead of the real length (key to size).
        pub head_sizes: BTreeMap<String, u64>,
        /// When set, every HeadObject fails with this.
        pub head_error: Option<ReadError>,
        pub calls: Mutex<Vec<String>>,
    }

    impl FakeReader {
        pub fn put(&mut self, bucket: &str, key: &str, bytes: Vec<u8>) {
            self.objects
                .insert((bucket.to_string(), key.to_string()), bytes);
        }

        pub fn called(&self) -> Vec<String> {
            self.calls.lock().expect("lock").clone()
        }
    }

    #[pmcp::async_trait]
    impl ObjectReader for FakeReader {
        async fn head(&self, bucket: &str, key: &str) -> Result<Option<u64>, ReadError> {
            self.calls.lock().expect("lock").push(format!("head {key}"));
            if let Some(e) = &self.head_error {
                return Err(e.clone());
            }
            if let Some(size) = self.head_sizes.get(key) {
                return Ok(Some(*size));
            }
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

#[cfg(test)]
mod tests {
    use aprender_mcp_laya_train::validate::{validate_files, ValidationPolicy};

    use super::fakes::FakeReader;
    use super::*;
    use crate::tests::dataset;

    const BUCKET: &str = "fixture-bucket";
    const PREFIX: &str = "datasets/d/";

    fn full_reader() -> FakeReader {
        let (task, train, eval) = dataset(20);
        let mut r = FakeReader::default();
        r.put(BUCKET, "datasets/d/task.json", task);
        r.put(BUCKET, "datasets/d/train.jsonl", train);
        r.put(BUCKET, "datasets/d/eval.jsonl", eval);
        r
    }

    #[tokio::test]
    async fn dataset_caps_file_refused_before_any_get() {
        let mut r = full_reader();
        r.head_sizes.insert(
            "datasets/d/train.jsonl".to_string(),
            MAX_FILE_BYTES as u64 + 1,
        );
        let src = S3DatasetSource::new(r, BUCKET);
        let err = src.fetch(PREFIX).await.expect_err("over the file cap");
        assert_eq!(err.rule, "bound-file", "{err}");
        assert!(err.detail.contains("train.jsonl"), "{err}");
        assert!(
            !src.reader.called().iter().any(|c| c.starts_with("get ")),
            "no GetObject may precede the size check: {:?}",
            src.reader.called()
        );
    }

    #[tokio::test]
    async fn dataset_caps_exactly_at_the_file_cap_is_admitted() {
        let mut r = full_reader();
        r.head_sizes
            .insert("datasets/d/train.jsonl".to_string(), MAX_FILE_BYTES as u64);
        // The declared size is the admission ticket; the body is shorter, which is fine.
        let src = S3DatasetSource::new(r, BUCKET);
        assert!(src.fetch(PREFIX).await.is_ok());
    }

    #[tokio::test]
    async fn dataset_caps_total_refused_before_any_get() {
        let mut r = full_reader();
        let each = (MAX_TOTAL_BYTES as u64) / 3 + 1;
        for f in ["task.json", "train.jsonl", "eval.jsonl"] {
            r.head_sizes.insert(format!("datasets/d/{f}"), each);
        }
        assert!(
            each <= MAX_FILE_BYTES as u64,
            "each file is under its own cap"
        );
        let src = S3DatasetSource::new(r, BUCKET);
        let err = src.fetch(PREFIX).await.expect_err("over the total cap");
        assert_eq!(err.rule, "bound-total", "{err}");
        assert!(!src.reader.called().iter().any(|c| c.starts_with("get ")));
    }

    #[tokio::test]
    async fn dataset_caps_body_longer_than_its_head_is_cut_and_refused() {
        let mut r = full_reader();
        // HeadObject says 10 bytes; the body is the real, longer train.jsonl.
        r.head_sizes
            .insert("datasets/d/train.jsonl".to_string(), 10);
        let src = S3DatasetSource::new(r, BUCKET);
        let err = src
            .fetch(PREFIX)
            .await
            .expect_err("a body longer than its head");
        assert_eq!(err.rule, "bound-file", "{err}");
        assert!(err.detail.contains("train.jsonl"), "{err}");
    }

    #[tokio::test]
    async fn dataset_missing_task_json_is_none_and_validate_names_it() {
        let mut r = full_reader();
        r.objects
            .remove(&(BUCKET.to_string(), "datasets/d/task.json".to_string()));
        let files = S3DatasetSource::new(r, BUCKET)
            .fetch(PREFIX)
            .await
            .expect("an absent file is not a fetch error");
        assert!(files.task_json.is_none());
        assert!(files.train.is_some() && files.eval.is_some());
        let policy = ValidationPolicy::shared().expect("policy");
        assert_eq!(
            validate_files(&files, policy)
                .expect_err("task is required")
                .rule,
            "task-missing"
        );
    }

    #[tokio::test]
    async fn dataset_missing_train_and_eval_map_to_their_rules() {
        for (gone, rule) in [
            ("train.jsonl", "train-missing"),
            ("eval.jsonl", "eval-missing"),
        ] {
            let mut r = full_reader();
            r.objects
                .remove(&(BUCKET.to_string(), format!("datasets/d/{gone}")));
            let files = S3DatasetSource::new(r, BUCKET)
                .fetch(PREFIX)
                .await
                .expect("fetch");
            let policy = ValidationPolicy::shared().expect("policy");
            assert_eq!(
                validate_files(&files, policy).expect_err("required").rule,
                rule
            );
        }
    }

    #[tokio::test]
    async fn dataset_missing_shift_is_simply_absent() {
        let files = S3DatasetSource::new(full_reader(), BUCKET)
            .fetch(PREFIX)
            .await
            .expect("fetch");
        assert!(files.shift.is_none());
        let policy = ValidationPolicy::shared().expect("policy");
        assert!(
            validate_files(&files, policy).is_ok(),
            "no shift is no refusal"
        );
    }

    #[tokio::test]
    async fn dataset_denied_is_a_typed_refusal_naming_bucket_and_prefix() {
        let mut r = FakeReader::default();
        r.head_error = Some(ReadError::Denied("AccessDenied: secret-detail".to_string()));
        let err = S3DatasetSource::new(r, BUCKET)
            .fetch(PREFIX)
            .await
            .expect_err("denied");
        assert_eq!(err.rule, "dataset-access");
        assert!(
            err.detail.contains(BUCKET) && err.detail.contains(PREFIX),
            "{err}"
        );
        assert!(err.detail.contains("no read grant"), "{err}");
        assert!(
            !err.detail.contains("secret-detail"),
            "SDK text stays in the log: {err}"
        );
    }

    #[tokio::test]
    async fn dataset_other_failure_is_dataset_read_not_a_crash() {
        let mut r = FakeReader::default();
        r.head_error = Some(ReadError::Other("connection reset".to_string()));
        let err = S3DatasetSource::new(r, BUCKET)
            .fetch(PREFIX)
            .await
            .expect_err("failed");
        assert_eq!(err.rule, "dataset-read");
    }
}
