//! Shared pieces of the decide Lambda transport (Phase 8 D-15, D-18): the stateless
//! streamable-HTTP config, the loopback server, where the model comes from, the lazy
//! once-per-container load, and the cold-start evidence every request carries.
//!
//! A lib so the `bootstrap` binary and the loopback tests exercise the SAME
//! configuration — no drift between what ships and what is tested (the
//! `aprender-mcp-chronos-lambda` shape). The tool surface, the bounds and the load
//! ladder belong to `aprender-mcp-decide`; nothing here re-implements them.
//!
//! # Where the weights come from (D-18)
//!
//! Never baked into the package: the served model is the one named by
//! `APRENDER_DECIDE_S3_URI` and pinned by `APRENDER_DECIDE_SHA256`, fetched at cold
//! start straight into memory. Lambda's scratch disk is 512 MB and cargo-pmcp cannot
//! raise it (RESEARCH Pitfall 3), so the ~0.85 GB artifact never touches a path. For
//! local runs `APRENDER_DECIDE_MODEL` names a file (with an optional pin).
//!
//! The pin is checked on the bytes BEFORE the decide-apr-v1 ladder parses anything: it
//! is both the tamper guard (T-08-07-01) and the identity the probe verifies (D-11).

// serde_json::json! expands to .unwrap() internally (same precedent as the server crate).
#![allow(clippy::disallowed_methods)]

use std::fmt;
use std::future::Future;
use std::io::Read as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use aprender_decide::artifact::{read_decide_apr_bytes_bounded, ArtifactLimits};
use aprender_decide::ArtifactError;
pub use aprender_mcp_decide::{build_server as build_decide_server, Model, ModelLoadError};
use pmcp::server::streamable_http_server::{StreamableHttpServer, StreamableHttpServerConfig};
use sha2::{Digest, Sha256};

pub mod probe;

/// S3 object URI of the served artifact (`s3://bucket/key`).
pub const ENV_S3_URI: &str = "APRENDER_DECIDE_S3_URI";
/// The artifact's pinned sha256: 64 lowercase hex characters.
pub const ENV_SHA256: &str = "APRENDER_DECIDE_SHA256";
/// A local artifact path (the non-Lambda source).
pub const ENV_MODEL: &str = "APRENDER_DECIDE_MODEL";

/// Request header a probe sets so the server's load log line can be matched to it.
pub const PROBE_ID_HEADER: &str = "x-decide-probe-id";
/// Response header saying whether THIS request performed the model load.
pub const LOAD_HEADER: &str = "x-decide-load";
/// A probe id is at most this many `[A-Za-z0-9-]` characters; anything else is ignored.
pub const PROBE_ID_MAX_LEN: usize = 64;

/// The streamable-HTTP config for the deployed Lambda: **`stateless()`**.
///
/// Stateless/JSON is the only mode that survives serverless: sessions and SSE do not
/// carry across independent Lambda containers, and a cold container behind the gateway
/// can receive a `tools/call` whose `initialize` landed on another one.
#[must_use]
pub fn server_config() -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::stateless()
}

/// Build the MCP server over a loaded model (delegates to `aprender-mcp-decide`).
///
/// # Errors
///
/// `pmcp::Error` if the server builder refuses the configuration.
pub fn build_server(model: Arc<Model>, name: &str, version: &str) -> pmcp::Result<pmcp::Server> {
    build_decide_server(model, name, version)
}

/// Start `server` as a stateless streamable-HTTP server on `addr` (the loopback the
/// bootstrap proxies to). Returns the bound address and the serving task.
///
/// # Errors
///
/// `pmcp::Error` if the listener cannot bind.
pub async fn start_loopback(
    server: pmcp::Server,
    addr: SocketAddr,
) -> pmcp::Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    let server = Arc::new(tokio::sync::Mutex::new(server));
    StreamableHttpServer::with_config(addr, server, server_config())
        .start()
        .await
}

// ===========================================================================
// Where the model comes from
// ===========================================================================

/// A validated sha256 pin: exactly 64 lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sha256Pin(String);

impl Sha256Pin {
    /// Parse a pin.
    ///
    /// # Errors
    ///
    /// [`SourceError::BadSha256`] unless the value is 64 lowercase hex characters.
    pub fn parse(value: &str) -> Result<Self, SourceError> {
        let ok = value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if ok {
            Ok(Self(value.to_string()))
        } else {
            Err(SourceError::BadSha256 { len: value.len() })
        }
    }

    /// The pin as hex.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where the served model comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSource {
    /// An S3 object, fetched into memory at cold start. The pin is mandatory.
    S3 {
        /// Bucket name.
        bucket: String,
        /// Object key.
        key: String,
        /// The artifact's pinned sha256.
        sha256: Sha256Pin,
    },
    /// A local file (development and local serving), optionally pinned.
    Local {
        /// Path to the `.apr`.
        path: PathBuf,
        /// Optional pin, checked before the ladder when present.
        sha256: Option<Sha256Pin>,
    },
}

/// Why the model source could not be determined from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// Neither `APRENDER_DECIDE_S3_URI` nor `APRENDER_DECIDE_MODEL` is set.
    Missing,
    /// Both sources are set; which one is served would be a guess.
    Ambiguous,
    /// `APRENDER_DECIDE_S3_URI` is not `s3://bucket/key`.
    BadS3Uri {
        /// What is wrong with it.
        reason: &'static str,
    },
    /// An S3 source needs `APRENDER_DECIDE_SHA256`.
    MissingSha256,
    /// `APRENDER_DECIDE_SHA256` is not 64 lowercase hex characters.
    BadSha256 {
        /// The length seen (the value itself is not echoed).
        len: usize,
    },
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => write!(f, "set {ENV_S3_URI} (+ {ENV_SHA256}) or {ENV_MODEL}"),
            Self::Ambiguous => write!(f, "both {ENV_S3_URI} and {ENV_MODEL} are set; set one"),
            Self::BadS3Uri { reason } => write!(f, "{ENV_S3_URI} is not s3://bucket/key: {reason}"),
            Self::MissingSha256 => write!(f, "{ENV_S3_URI} requires {ENV_SHA256} (the pin)"),
            Self::BadSha256 { len } => write!(
                f,
                "{ENV_SHA256} must be 64 lowercase hex characters (got {len} characters)"
            ),
        }
    }
}

impl std::error::Error for SourceError {}

/// Split `s3://bucket/key` into `(bucket, key)`.
///
/// # Errors
///
/// [`SourceError::BadS3Uri`] for another scheme, an empty bucket or a missing key.
pub fn parse_s3_uri(uri: &str) -> Result<(String, String), SourceError> {
    let rest = uri.strip_prefix("s3://").ok_or(SourceError::BadS3Uri {
        reason: "scheme must be s3://",
    })?;
    let (bucket, key) = rest.split_once('/').ok_or(SourceError::BadS3Uri {
        reason: "missing object key",
    })?;
    if bucket.is_empty() {
        return Err(SourceError::BadS3Uri {
            reason: "empty bucket",
        });
    }
    if key.is_empty() {
        return Err(SourceError::BadS3Uri {
            reason: "missing object key",
        });
    }
    Ok((bucket.to_string(), key.to_string()))
}

impl ModelSource {
    /// Read the source from the process environment.
    ///
    /// # Errors
    ///
    /// A [`SourceError`] naming what is missing or malformed.
    pub fn from_env() -> Result<Self, SourceError> {
        Self::from_lookup(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
    }

    /// Read the source through `lookup` (the testable form of [`Self::from_env`]).
    ///
    /// # Errors
    ///
    /// A [`SourceError`] naming what is missing or malformed.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, SourceError> {
        let pin = lookup(ENV_SHA256)
            .map(|v| Sha256Pin::parse(&v))
            .transpose()?;
        match (lookup(ENV_S3_URI), lookup(ENV_MODEL)) {
            (Some(_), Some(_)) => Err(SourceError::Ambiguous),
            (None, None) => Err(SourceError::Missing),
            (Some(uri), None) => {
                let (bucket, key) = parse_s3_uri(&uri)?;
                let sha256 = pin.ok_or(SourceError::MissingSha256)?;
                Ok(Self::S3 {
                    bucket,
                    key,
                    sha256,
                })
            }
            (None, Some(path)) => Ok(Self::Local {
                path: PathBuf::from(path),
                sha256: pin,
            }),
        }
    }

    /// `"s3"` or `"local"` — for logs.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::S3 { .. } => "s3",
            Self::Local { .. } => "local",
        }
    }
}

// ===========================================================================
// Resolution: bytes -> pin -> ladder
// ===========================================================================

/// Why the model could not be resolved. Display never includes model bytes or text.
#[derive(Debug)]
pub enum ResolveError {
    /// The local file could not be opened or statted.
    Io {
        /// The path.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The artifact's declared length is over the decide-apr-v1 cap — refused before
    /// any byte was read or any buffer allocated.
    TooLarge {
        /// Which length (`declared_length`).
        what: &'static str,
        /// The observed length.
        observed: u64,
        /// The cap.
        cap: u64,
    },
    /// The bounded read refused (over cap on the stream, or an I/O error).
    Read(ArtifactError),
    /// This source kind is not available in this build.
    Unsupported(&'static str),
    /// sha256 of the bytes is not the pin — refused before the ladder parsed anything.
    HashMismatch {
        /// The pinned value.
        expected: String,
        /// sha256 of the bytes that arrived.
        actual: String,
    },
    /// The decide-apr-v1 load ladder refused the bytes.
    Load(ModelLoadError),
    /// The loaded model reports a different identity than the bytes hashed to.
    IdentityMismatch {
        /// sha256 of the bytes.
        hashed: String,
        /// `model.identity().artifact_sha256`.
        served: String,
    },
    /// A blocking section panicked or was cancelled.
    Join(String),
}

impl ResolveError {
    /// A short stable name for the failure class, safe to return to a caller.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Io { .. } => "io",
            Self::TooLarge { .. } => "too_large",
            Self::Read(_) => "read",
            Self::Unsupported(_) => "unsupported",
            Self::HashMismatch { .. } => "hash_mismatch",
            Self::Load(_) => "load",
            Self::IdentityMismatch { .. } => "identity_mismatch",
            Self::Join(_) => "join",
        }
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "cannot open {}: {source}", path.display()),
            Self::TooLarge {
                what,
                observed,
                cap,
            } => write!(
                f,
                "artifact {what} {observed} bytes is over the decide-apr-v1 cap {cap}"
            ),
            Self::Read(e) => write!(f, "bounded read refused: {e}"),
            Self::Unsupported(what) => write!(f, "model source {what} is not available"),
            Self::HashMismatch { expected, actual } => write!(
                f,
                "artifact sha256 {actual} is not the pinned {expected}; refused before parsing"
            ),
            Self::Load(e) => write!(f, "{e}"),
            Self::IdentityMismatch { hashed, served } => write!(
                f,
                "loaded model reports identity {served}, but the bytes hashed to {hashed}"
            ),
            Self::Join(e) => write!(f, "blocking section failed: {e}"),
        }
    }
}

impl std::error::Error for ResolveError {}

/// The contracted artifact cap (decide-apr-v1 `constants.max_artifact_bytes`).
#[must_use]
pub fn contracted_cap() -> u64 {
    ArtifactLimits::CONTRACTED.max_artifact_bytes()
}

/// sha256 of `bytes` as lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn ms_since(t: Instant) -> u128 {
    t.elapsed().as_millis()
}

/// What one model load cost, and on what host — counts and timings only, never text.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadTimeline {
    /// `"s3"` or `"local"`.
    pub source: &'static str,
    /// Artifact length in bytes.
    pub bytes: u64,
    /// Time to get the bytes into memory (the S3 download or the local read).
    pub fetch_ms: u128,
    /// Time to sha256 the buffer.
    pub sha_ms: u128,
    /// Time for the decide-apr-v1 ladder and model build.
    pub build_ms: u128,
    /// sha256 of the artifact (the served identity).
    pub artifact_sha256: String,
    /// Resident set after the load, from `/proc/self/status` `VmRSS` (Linux only).
    pub rss_mb: Option<f64>,
    /// Peak resident set, `VmHWM` (Linux only).
    pub peak_rss_mb: Option<f64>,
    /// `CPU part` from `/proc/cpuinfo` (Linux/aarch64 only).
    pub cpu_part: Option<String>,
    /// The Graviton generation that CPU part names.
    pub graviton: &'static str,
}

fn proc_status_mb(field: &str) -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with(field))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

fn cpu_part() -> Option<String> {
    let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let line = info.lines().find(|l| l.starts_with("CPU part"))?;
    Some(line.split(':').nth(1)?.trim().to_string())
}

/// Map an ARM `CPU part` to the Graviton generation (spike 026's forensics).
#[must_use]
pub fn graviton_generation(cpu_part: Option<&str>) -> &'static str {
    match cpu_part {
        Some("0xd0c") => "graviton2",
        Some("0xd40") => "graviton3",
        Some("0xd4f") => "graviton4",
        _ => "unknown",
    }
}

fn host_timeline(
    source: &'static str,
    bytes: u64,
    fetch_ms: u128,
    sha_ms: u128,
    build_ms: u128,
    artifact_sha256: String,
) -> LoadTimeline {
    let part = cpu_part();
    LoadTimeline {
        source,
        bytes,
        fetch_ms,
        sha_ms,
        build_ms,
        artifact_sha256,
        rss_mb: proc_status_mb("VmRSS"),
        peak_rss_mb: proc_status_mb("VmHWM"),
        graviton: graviton_generation(part.as_deref()),
        cpu_part: part,
    }
}

/// Hash `bytes`, compare with `pin` (when present) BEFORE any parse, run the ladder,
/// then drop the buffer. The blocking work runs on the blocking pool.
async fn verify_and_build(
    source: &'static str,
    bytes: Vec<u8>,
    pin: Option<Sha256Pin>,
    fetch_ms: u128,
) -> Result<(Model, LoadTimeline), ResolveError> {
    tokio::task::spawn_blocking(move || {
        let t = Instant::now();
        let actual = sha256_hex(&bytes);
        let sha_ms = ms_since(t);
        if let Some(pin) = pin {
            if pin.as_str() != actual {
                return Err(ResolveError::HashMismatch {
                    expected: pin.0,
                    actual,
                });
            }
        }
        let t = Instant::now();
        let model =
            aprender_mcp_decide::load_model_from_bytes(&bytes).map_err(ResolveError::Load)?;
        let len = bytes.len() as u64;
        // Peak memory is the bytes plus the widened model; the bytes go now.
        drop(bytes);
        let build_ms = ms_since(t);
        let served = model.identity().artifact_sha256.clone();
        if served != actual {
            return Err(ResolveError::IdentityMismatch {
                hashed: actual,
                served,
            });
        }
        let timeline = host_timeline(source, len, fetch_ms, sha_ms, build_ms, actual);
        Ok((model, timeline))
    })
    .await
    .map_err(|e| ResolveError::Join(e.to_string()))?
}

/// Resolve a LOCAL artifact: the metadata length is checked against `cap` BEFORE any
/// read, the read itself goes through decide-apr-v1's bounded reader (capped at
/// `cap + 1`, because metadata can lie), then the pin (if any), then the ladder.
///
/// # Errors
///
/// [`ResolveError::Io`], [`ResolveError::TooLarge`], [`ResolveError::Read`],
/// [`ResolveError::HashMismatch`] or [`ResolveError::Load`].
pub async fn resolve_local(
    path: &Path,
    pin: Option<&Sha256Pin>,
    cap: u64,
) -> Result<(Model, LoadTimeline), ResolveError> {
    let t = Instant::now();
    let owned = path.to_path_buf();
    let bytes = tokio::task::spawn_blocking(move || read_local_bounded(&owned, cap))
        .await
        .map_err(|e| ResolveError::Join(e.to_string()))??;
    let fetch_ms = ms_since(t);
    verify_and_build("local", bytes, pin.cloned(), fetch_ms).await
}

fn read_local_bounded(path: &Path, cap: u64) -> Result<Vec<u8>, ResolveError> {
    let io = |source| ResolveError::Io {
        path: path.to_path_buf(),
        source,
    };
    let file = std::fs::File::open(path).map_err(io)?;
    let declared = file.metadata().map_err(io)?.len();
    if declared > cap {
        return Err(ResolveError::TooLarge {
            what: "declared_length",
            observed: declared,
            cap,
        });
    }
    let bytes = read_decide_apr_bytes_bounded(file.take(cap.saturating_add(1)), Some(declared))
        .map_err(ResolveError::Read)?;
    if bytes.len() as u64 > cap {
        return Err(ResolveError::TooLarge {
            what: "stream",
            observed: bytes.len() as u64,
            cap,
        });
    }
    Ok(bytes)
}

/// Resolve the served model from `source` under the contracted cap.
///
/// # Errors
///
/// A [`ResolveError`] naming the step that refused.
pub async fn resolve_model(source: &ModelSource) -> Result<(Model, LoadTimeline), ResolveError> {
    match source {
        ModelSource::Local { path, sha256 } => {
            resolve_local(path, sha256.as_ref(), contracted_cap()).await
        }
        ModelSource::S3 { .. } => Err(ResolveError::Unsupported("s3")),
    }
}

// ===========================================================================
// The lazy, once-per-container load
// ===========================================================================

/// A value loaded at most once per process, behind a lock so concurrent first callers
/// load it once. A FAILED load leaves it empty and the lock released, so the next
/// caller retries — never a poisoned once-cell.
#[derive(Debug)]
pub struct LoadOnce<T> {
    value: OnceLock<Arc<T>>,
    lock: tokio::sync::Mutex<()>,
}

impl<T> Default for LoadOnce<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> LoadOnce<T> {
    /// An empty slot.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            value: OnceLock::new(),
            lock: tokio::sync::Mutex::const_new(()),
        }
    }

    /// The loaded value, if any.
    #[must_use]
    pub fn get(&self) -> Option<Arc<T>> {
        self.value.get().cloned()
    }

    /// Return the value, loading it with `load` if this is the first successful call.
    /// The flag is `true` only for the call that performed the load.
    ///
    /// # Errors
    ///
    /// Whatever `load` returned; the slot stays empty for the next caller.
    pub async fn get_or_try_load<E, F, Fut>(&self, load: F) -> Result<(Arc<T>, bool), E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        if let Some(v) = self.value.get() {
            return Ok((Arc::clone(v), false));
        }
        let _guard = self.lock.lock().await;
        if let Some(v) = self.value.get() {
            return Ok((Arc::clone(v), false));
        }
        let loaded = Arc::new(load().await?);
        let _ = self.value.set(Arc::clone(&loaded));
        Ok((loaded, true))
    }
}

// ===========================================================================
// Cold-start evidence
// ===========================================================================

/// Accept a probe id only as 1..=64 `[A-Za-z0-9-]` characters; anything else is
/// ignored, so a forged header can never inject into a log line (T-08-07-09).
#[must_use]
pub fn parse_probe_id(raw: Option<&str>) -> Option<&str> {
    let id = raw?;
    let ok = !id.is_empty()
        && id.len() <= PROBE_ID_MAX_LEN
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    ok.then_some(id)
}

/// The `x-decide-load` value: `cold;load_ms=<n>` for the request that loaded, else `warm`.
#[must_use]
pub fn load_header_value(performed_load: bool, load_ms: u128) -> String {
    if performed_load {
        format!("cold;load_ms={load_ms}")
    } else {
        "warm".to_string()
    }
}

/// The one log line per request that proves which request loaded the model.
#[must_use]
pub fn load_log_line(
    performed_load: bool,
    probe_id: Option<&str>,
    load_ms: u128,
    timeline: Option<&LoadTimeline>,
) -> String {
    let id = probe_id.unwrap_or("none");
    match (performed_load, timeline) {
        (true, Some(t)) => format!(
            "decide.load performed_load=true probe_id={id} load_ms={load_ms} \
             download_ms={} sha_ms={} build_ms={} bytes={} rss_mb={} peak_rss_mb={} \
             graviton={} source={}",
            t.fetch_ms,
            t.sha_ms,
            t.build_ms,
            t.bytes,
            t.rss_mb
                .map_or_else(|| "na".to_string(), |v| format!("{v:.0}")),
            t.peak_rss_mb
                .map_or_else(|| "na".to_string(), |v| format!("{v:.0}")),
            t.graviton,
            t.source,
        ),
        (true, None) => {
            format!("decide.load performed_load=true probe_id={id} load_ms={load_ms}")
        }
        (false, _) => format!("decide.load performed_load=false probe_id={id}"),
    }
}

#[cfg(test)]
mod tests;
