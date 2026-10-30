//! The vocabulary every plan of Phase 14 shares: S3 keys, tags, schema ids,
//! bounds, the shapes of the files the jobs write, and the one status payload.
//!
//! Fixing these in one file, with golden fixtures under `fixtures/`, is what lets
//! the request function (14-04), the finalizer (14-05), both containers (14-06,
//! 14-07) and the fetch recipe (14-11) read and write the same things without
//! drifting. Nothing here computes anything: it names, bounds and (de)serializes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- identity

/// The server identity every transport wrapper reports.
pub const SERVER_NAME: &str = "aprender-mcp-laya-train";
/// The inspection tool. Reader: `build_server`, the Lambda wrapper's tool listing.
pub const TOOL_VALIDATE: &str = "validate_dataset";
/// The submission tool (an MCP Task).
pub const TOOL_TRAIN: &str = "train";
/// The polling companion for clients without MCP Tasks support.
pub const TOOL_STATUS: &str = "train_status";

// ------------------------------------------------------------------ bounds

/// TTL requested for a minted task: 72 h = a 6 h job, pack and verify, and a
/// retrieval window (Pitfall 14: an expired task discards a finished verdict).
pub const TASK_TTL_MS: u64 = 259_200_000;
/// Single-flight lease: the 10 h state-machine timeout plus margin. Read by
/// `lock::acquire`; the finalizer's reconcile sweep (14-05) frees an expired one.
pub const LOCK_LEASE_SECS: u64 = 39_600;
/// Hard runtime cap of one training job (D-12). Read by the state machine (14-08).
pub const MAX_RUNTIME_SECS: u64 = 21_600;
/// The SageMaker instance type (D-13). Read by the state machine (14-08).
pub const INSTANCE_TYPE: &str = "ml.g5.xlarge";
/// Lifetime of a presigned download link minted by `train_status`.
pub const DOWNLOAD_URL_TTL_SECS: u64 = 3_600;
/// Largest request body the server accepts (HTTP 413 above it).
pub const MAX_REQUEST_BYTES: usize = 16_384;
/// Largest single dataset file (transport bound, rule `bound-file`).
pub const MAX_FILE_BYTES: usize = 33_554_432;
/// Largest dataset in total (transport bound, rule `bound-total`).
pub const MAX_TOTAL_BYTES: usize = 67_108_864;
/// Most rows in one dataset file (transport bound, rule `bound-rows`).
pub const MAX_ROWS_PER_FILE: usize = 200_000;

// -------------------------------------------------------------- retention

/// The object tag the lifecycle rule keys on (D-19). Written by every writer.
pub const RETENTION_TAG_KEY: &str = "laya-retention";
/// Every writer's default: the object expires with the lifecycle rule.
pub const RETENTION_EXPIRE: &str = "expire";
/// Set by the finalizer on a completed run whose gate passed (D-19), nowhere else.
pub const RETENTION_KEEP: &str = "keep";

// -------------------------------------------------------------- S3 layout

/// Where callers' datasets live in the server bucket (D-07).
pub const DATASETS_PREFIX: &str = "datasets/";
/// The pinned base-model archive the training container downloads.
pub const BASE_KEY: &str = "base/laya-55cf4c4e/laya-base.tar";
/// Where SageMaker writes its own job output.
pub const SAGEMAKER_OUTPUT_PREFIX: &str = "sagemaker-output/";
/// The trainer run dir, under a run prefix. Written by 14-06.
pub const RUN_DIR: &str = "run/";
/// The copy of the data dir the job trained on. Written by 14-06.
pub const DATA_DIR: &str = "data/";
/// The trainer's outcome record ([`Outcome`]). Written by 14-06.
pub const OUTCOME: &str = "outcome.json";
/// The complete inventory of what the trainer uploaded ([`TrainingManifest`]).
pub const TRAINING_MANIFEST: &str = "training-manifest.json";
/// The in-run inventory ([`TrainingManifest`] with `complete` false), rewritten as
/// files upload so an interrupted run stays inventoried. Written by 14-06.
pub const PARTIAL_MANIFEST: &str = "partial-manifest.json";
/// The training container's log.
pub const TRAIN_LOG: &str = "train.log";
/// The packer's outcome record ([`PackOutcome`]). Written by 14-07.
pub const PACK_OUTCOME: &str = "pack-outcome.json";
/// The packer's inventory (a JSON array of [`ManifestEntry`]). Written by 14-07.
pub const PACK_MANIFEST: &str = "pack-manifest.json";
/// The packer's log.
pub const PACK_LOG: &str = "pack.log";
/// The verify log.
pub const VERIFY_LOG: &str = "verify.log";
/// The finalizer's copy of the verdict. Written by 14-05.
pub const VERDICT: &str = "verdict.json";
/// The finalizer's merged inventory ([`Manifest`]). Written by 14-05.
pub const MANIFEST: &str = "manifest.json";

/// The prefix every output of one run lives under: `runs/<name>/<task_id>/`.
/// Built only from a validated name and a minted id (D-09).
#[must_use]
pub fn run_prefix(name: &str, task_id: &str) -> String {
    format!("runs/{name}/{task_id}/")
}

/// The key of the packed model: `<run_prefix><name>.apr`.
#[must_use]
pub fn apr_key(name: &str, task_id: &str) -> String {
    format!("{}{name}.apr", run_prefix(name, task_id))
}

/// The `.apr` entry's path relative to the run prefix.
#[must_use]
pub fn apr_entry_path(name: &str) -> String {
    format!("{name}.apr")
}

// ------------------------------------------------------------ schema ids

/// The one status payload (`train_status`, the stored terminal result, the working answer).
pub const SCHEMA_STATUS: &str = "laya-train-status-v1";
/// [`Outcome`].
pub const SCHEMA_OUTCOME: &str = "laya-train-outcome-v1";
/// [`TrainingManifest`].
pub const SCHEMA_TRAINING_MANIFEST: &str = "laya-train-training-manifest-v1";
/// [`PackOutcome`].
pub const SCHEMA_PACK_OUTCOME: &str = "laya-train-pack-outcome-v1";
/// [`Manifest`].
pub const SCHEMA_MANIFEST: &str = "laya-train-manifest-v1";
/// The verdict file the finalizer writes.
pub const SCHEMA_VERDICT: &str = "laya-train-verdict-v1";
/// [`ExecutionInput`].
pub const SCHEMA_EXECUTION_INPUT: &str = "laya-train-execution-input-v1";

/// Envelope version of the status payload.
pub const STATUS_SCHEMA_VERSION: u64 = 1;

// --------------------------------------------------------- inputs string

/// The dataset files, in the one order [`inputs_string`] writes them.
pub const INPUT_FILES: [&str; 4] = ["task.json", "train.jsonl", "eval.jsonl", "shift.jsonl"];

/// Serialize the validated files' sha256s as the canonical string
/// `task.json=<hex>;train.jsonl=<hex>;eval.jsonl=<hex>[;shift.jsonl=<hex>]`.
///
/// The order is fixed (not alphabetical), so the validating request function
/// and the verifying container build byte-identical strings from the same
/// bytes. Keys outside [`INPUT_FILES`] are not written.
#[must_use]
pub fn inputs_string(map: &BTreeMap<String, String>) -> String {
    INPUT_FILES
        .iter()
        .filter_map(|file| map.get(*file).map(|hex| format!("{file}={hex}")))
        .collect::<Vec<_>>()
        .join(";")
}

/// Parse [`inputs_string`] back. Refuses a missing required file, an unknown
/// file, a non-hex digest, a repeated file, or an order other than the canonical one.
///
/// # Errors
///
/// A message naming what is wrong (never a digest value).
pub fn parse_inputs_string(s: &str) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    let mut last_rank: Option<usize> = None;
    for part in s.split(';') {
        let (file, hex) = part
            .split_once('=')
            .ok_or_else(|| "an entry has no '='".to_string())?;
        let rank = INPUT_FILES
            .iter()
            .position(|f| *f == file)
            .ok_or_else(|| format!("unknown input file {file:?}"))?;
        if last_rank.is_some_and(|prev| rank <= prev) {
            return Err(format!("{file} is repeated or out of canonical order"));
        }
        last_rank = Some(rank);
        if !is_hex64(hex) {
            return Err(format!("{file} digest is not 64 lowercase hex characters"));
        }
        map.insert(file.to_string(), hex.to_string());
    }
    for required in &INPUT_FILES[..3] {
        if !map.contains_key(*required) {
            return Err(format!("{required} is missing"));
        }
    }
    Ok(map)
}

/// Exactly 64 lowercase hex characters.
#[must_use]
pub fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// One path segment: `^[a-z0-9][a-z0-9._-]{0,63}$`. A leading dot (so `.` and `..`)
/// cannot match, so a segment cannot climb out of its prefix.
#[must_use]
pub fn segment_ok(seg: &str) -> bool {
    let bytes = seg.as_bytes();
    bytes
        .first()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bytes.len() <= 64
        && bytes.iter().skip(1).all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(*b, b'.' | b'_' | b'-')
        })
}

// ------------------------------------------------- files the jobs write

/// How a training job ended, as the container classifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitClass {
    /// The trainer exited 0: the gate passed.
    Pass,
    /// The trainer finished and the gate failed (exit 3; `train.py` `EXIT_GATE_FAIL`).
    GateFail,
    /// The trainer refused its inputs before training (a `REFUSED` line).
    Refused,
    /// The trainer crashed.
    Crash,
    /// The job hit its runtime cap.
    Timeout,
}

/// `outcome.json`, written by the training container (14-06).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    /// [`SCHEMA_OUTCOME`].
    pub schema: String,
    /// The task this run belongs to.
    pub task_id: String,
    /// How the job ended.
    pub exit_class: ExitClass,
    /// The trainer's exit code, when it exited.
    pub train_rc: Option<i32>,
    /// The device the trainer reported using.
    pub device_used: String,
    /// False when the run fell back to a device the contract does not allow (CPU).
    pub device_ok: bool,
    /// The first `REFUSED` line of `train.log`, else null.
    pub refused_line: Option<String>,
    /// RFC 3339 start.
    pub started_at: String,
    /// RFC 3339 end.
    pub finished_at: String,
    /// Wall time of the trainer child.
    pub train_seconds: f64,
}

/// One uploaded object: `path` is relative to the run prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestEntry {
    /// Relative to the run prefix.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// Lowercase hex sha256 of the object.
    pub sha256: String,
}

/// `training-manifest.json` (complete) or `partial-manifest.json` (in-run),
/// written by the training container (14-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingManifest {
    /// [`SCHEMA_TRAINING_MANIFEST`].
    pub schema: String,
    /// The task this run belongs to.
    pub task_id: String,
    /// True for `training-manifest.json`, false for `partial-manifest.json`.
    pub complete: bool,
    /// `sha256:<64 hex>`: the image the training container was started from
    /// (14-06 writes it from the env the state machine sets).
    pub train_image_digest: String,
    /// sha256 of the gate contract the trainer read, hashed in the container.
    pub gate_contract_sha256: String,
    /// Every file uploaded so far.
    pub entries: Vec<ManifestEntry>,
}

/// `pack-outcome.json`, written by the pack container (14-07).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackOutcome {
    /// [`SCHEMA_PACK_OUTCOME`].
    pub schema: String,
    /// The task this run belongs to.
    pub task_id: String,
    /// `sha256:<64 hex>`: the image the pack container was started from.
    pub pack_image_digest: String,
    /// sha256 of the gate contract `pack_laya` read under `/src/contracts`.
    pub gate_contract_sha256: String,
    /// sha256 of the parity contract `pack_laya` read under `/src/contracts`.
    pub parity_contract_sha256: String,
    /// Whether pack_laya ran at all.
    pub ran: bool,
    /// When it did not run: the outcome's exit class that skipped it.
    pub skipped: Option<ExitClass>,
    /// The pack process's exit code (0 accepted, 3 refused, 2 error).
    pub exit_code: Option<i32>,
    /// True only when pack and verify both accepted the artifact.
    pub accepted: bool,
    /// The packed model's sha256; null unless accepted.
    pub apr_sha256: Option<String>,
    /// The seed pack_laya shipped.
    pub shipped_seed: Option<u64>,
    /// The pilot-waiver id pack_laya reported (`waived=<id>`), else null.
    pub waived: Option<String>,
    /// The `PACKED ...` line.
    pub packed_line: Option<String>,
    /// The verify JSON line.
    pub verify_line: Option<String>,
    /// verify's `deploy_eligible`.
    pub deploy_eligible: bool,
    /// Wall time of the pack child, null when it did not run.
    pub pack_seconds: Option<f64>,
    /// Wall time of the verify child, null when it did not run.
    pub verify_seconds: Option<f64>,
}

/// The images that produced a run, each `sha256:<64 hex>` or null.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Images {
    /// The training container image.
    pub train: Option<String>,
    /// The pack container image.
    pub pack: Option<String>,
}

/// The contract bytes that judged a run, each 64 hex or null.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contracts {
    /// The gate contract the request function's pre-filter embedded.
    pub prefilter_gate: Option<String>,
    /// The gate contract the trainer read.
    pub train_gate: Option<String>,
    /// The gate contract `pack_laya` read.
    pub pack_gate: Option<String>,
    /// The parity contract `pack_laya` read.
    pub pack_parity: Option<String>,
}

/// `manifest.json`, the finalizer's merged inventory (14-05 writes it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// [`SCHEMA_MANIFEST`].
    pub schema: String,
    /// The task this run belongs to.
    pub task_id: String,
    /// The caller's model name.
    pub name: String,
    /// The images that produced the run.
    pub images: Images,
    /// The contracts that judged the run.
    pub contracts: Contracts,
    /// Every listed object, sorted by path, de-duplicated.
    pub entries: Vec<ManifestEntry>,
    /// Objects under the run prefix that no writer manifest lists; set by the
    /// finalizer (14-05), 0 here.
    pub unlisted: u64,
}

/// What the state machine hands the job: the Step Functions execution input.
///
/// `owner` is the caller's auth subject: an identifier, not a secret. It
/// persists in Step Functions execution history for the machine's retention
/// (the same retention concern `task_store.rs` quotes for its envelope: a
/// StartExecution input is readable by anyone holding `states:GetExecutionHistory`).
/// No dataset content, token or URL ever travels in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionInput {
    /// [`SCHEMA_EXECUTION_INPUT`].
    pub schema: String,
    /// The minted task id (also the deterministic execution name).
    pub task_id: String,
    /// The caller's auth subject (see the type doc).
    pub owner: String,
    /// The validated model name.
    pub name: String,
    /// The normalized dataset key prefix, `datasets/<segs>/`.
    pub dataset_prefix: String,
    /// The run key prefix, `runs/<name>/<task_id>/`.
    pub run_prefix: String,
    /// `""` (not caller-set) or decimal digits.
    pub epochs: String,
    /// The canonical [`inputs_string`] of the validated files.
    pub inputs_sha256: String,
    /// sha256 (64 hex) of the embedded gate contract that judged the pre-filter.
    pub gate_contract_sha256: String,
}

// -------------------------------------------------------- status payload

/// The phases a task can report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Running (or about to).
    Working,
    /// Packed and verified.
    Completed,
    /// Trained to completion and the gate failed: a result, not an error.
    GateFailed,
    /// Did not produce a verified model.
    Failed,
    /// The client cancelled.
    Cancelled,
}

impl Phase {
    /// The wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Completed => "completed",
            Self::GateFailed => "gate_failed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// One manifest entry as `train_status` serves it. `url` is minted at read time
/// and is ALWAYS null in anything stored (T-14-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestLink {
    /// Relative to the run prefix.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// Lowercase hex sha256.
    pub sha256: String,
    /// A time-limited download link, read time only.
    pub url: Option<String>,
}

/// One seed's row, copied from `seeds.per_seed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedRow {
    /// The seed.
    pub seed: u64,
    /// Copied from the report.
    pub macro_f1: Option<f64>,
    /// Copied from the report.
    pub ece_post: Option<f64>,
    /// Copied from the report.
    pub margin: Option<f64>,
    /// Copied from the report.
    pub pass: Option<bool>,
}

/// What pack and verify said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackInfo {
    /// Whether the artifact was accepted.
    pub accepted: bool,
    /// The verify line (else the PACKED line), copied.
    pub line: Option<String>,
}

/// The verdict. Every field is COPIED from `gate-report.json` or
/// `pack-outcome.json` (D-05): a number absent from the report is `null`, never
/// a default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    /// The gate's own `pass`.
    pub pass: bool,
    /// The pilot-waiver id when a waived run was packed.
    pub waived: Option<String>,
    /// Top-level `margin`.
    pub margin: Option<f64>,
    /// `fine_tuned.ece_post`.
    pub ece_post: Option<f64>,
    /// `zero_shot.macro_f1`.
    pub zero_shot_macro_f1: Option<f64>,
    /// `fine_tuned.macro_f1`.
    pub ft_macro_f1: Option<f64>,
    /// `seeds.shipped` (null for a one-seed report).
    pub shipped_seed: Option<u64>,
    /// `seeds.per_seed` (empty for a one-seed report).
    pub per_seed: Vec<SeedRow>,
    /// `device_used`.
    pub device_used: Option<String>,
    /// `torch_version`.
    pub torch_version: Option<String>,
    /// `recipe_id`.
    pub recipe_id: Option<String>,
    /// `shift_probe`, copied as is.
    pub shift_probe: Option<serde_json::Value>,
    /// The packed model's sha256 (pack-outcome), null unless accepted.
    pub apr_sha256: Option<String>,
    /// What pack and verify said, when a pack ran.
    pub pack: Option<PackInfo>,
}

/// The one status shape (D-18): `train_status`, the stored terminal result and
/// the working answer all serialize this, so no two doors tell different stories.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusPayload {
    /// [`STATUS_SCHEMA_VERSION`].
    pub schema_version: u64,
    /// [`SCHEMA_STATUS`].
    pub schema: String,
    /// The minted task id.
    pub task_id: String,
    /// The caller's model name.
    pub name: String,
    /// The phase.
    pub phase: Phase,
    /// `s3://<bucket>/runs/<name>/<task_id>/`.
    pub run_prefix: String,
    /// The verdict, once a gate report exists.
    pub verdict: Option<Verdict>,
    /// Every listed object, with a read-time link.
    pub manifest: Vec<ManifestLink>,
    /// Why the run failed, or the refused rule.
    pub error: Option<String>,
    /// True when a trainer refusal followed a pre-filter acceptance (a defect to
    /// surface, D-08). False for `inputs-changed`: that is the dataset changing
    /// after validation, not a rule drift.
    pub prefilter_disagreement: bool,
    /// True when the inventory came from the in-run partial manifest.
    pub manifest_partial: bool,
    /// What the finalizer overrode or dropped to reach this phase (D-18): a
    /// fixed-shape sentence per event, such as the records deciding over a
    /// state-machine stage error, or manifest entries S3 does not hold. It never
    /// carries infrastructure text. Absent from the JSON when empty, so a payload
    /// with nothing to note serializes exactly as before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl StatusPayload {
    /// A payload with nothing but identity: the working answer and the base the
    /// read-time cancelled answer starts from.
    #[must_use]
    pub fn base(task_id: &str, name: &str, run_prefix_uri: &str, phase: Phase) -> Self {
        Self {
            schema_version: STATUS_SCHEMA_VERSION,
            schema: SCHEMA_STATUS.to_string(),
            task_id: task_id.to_string(),
            name: name.to_string(),
            phase,
            run_prefix: run_prefix_uri.to_string(),
            verdict: None,
            manifest: Vec::new(),
            error: None,
            prefilter_disagreement: false,
            manifest_partial: false,
            notes: Vec::new(),
        }
    }

    /// The payload as a JSON value (one serializer).
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// `entries` as links with no url.
#[must_use]
pub fn links_without_urls(entries: &[ManifestEntry]) -> Vec<ManifestLink> {
    entries
        .iter()
        .map(|e| ManifestLink {
            path: e.path.clone(),
            size: e.size,
            sha256: e.sha256.clone(),
            url: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(c: char) -> String {
        c.to_string().repeat(64)
    }

    #[test]
    fn inputs_string_round_trips() {
        let mut map = BTreeMap::new();
        map.insert("task.json".to_string(), hex('a'));
        map.insert("train.jsonl".to_string(), hex('b'));
        map.insert("eval.jsonl".to_string(), hex('c'));
        let s = inputs_string(&map);
        assert_eq!(
            s,
            format!(
                "task.json={};train.jsonl={};eval.jsonl={}",
                hex('a'),
                hex('b'),
                hex('c')
            )
        );
        assert_eq!(parse_inputs_string(&s).expect("round trip"), map);
        map.insert("shift.jsonl".to_string(), hex('d'));
        let with_shift = inputs_string(&map);
        assert!(with_shift.ends_with(&format!(";shift.jsonl={}", hex('d'))));
        assert_eq!(parse_inputs_string(&with_shift).expect("round trip"), map);
    }

    #[test]
    fn inputs_string_refuses_malformed() {
        let ok = format!(
            "task.json={};train.jsonl={};eval.jsonl={}",
            hex('a'),
            hex('b'),
            hex('c')
        );
        assert!(parse_inputs_string(&ok).is_ok());
        assert!(parse_inputs_string("").is_err());
        let missing = format!("task.json={};train.jsonl={}", hex('a'), hex('b'));
        assert!(parse_inputs_string(&missing).is_err());
        let out_of_order = format!(
            "train.jsonl={};task.json={};eval.jsonl={}",
            hex('b'),
            hex('a'),
            hex('c')
        );
        assert!(parse_inputs_string(&out_of_order).is_err());
        let unknown = format!("{ok};other.json={}", hex('d'));
        assert!(parse_inputs_string(&unknown).is_err());
        let upper = format!(
            "task.json={};train.jsonl={};eval.jsonl={}",
            hex('A'),
            hex('b'),
            hex('c')
        );
        assert!(parse_inputs_string(&upper).is_err());
    }

    #[test]
    fn keys_are_built_from_name_and_id_only() {
        assert_eq!(run_prefix("m-1", "t-9"), "runs/m-1/t-9/");
        assert_eq!(apr_key("m-1", "t-9"), "runs/m-1/t-9/m-1.apr");
        assert_eq!(apr_entry_path("m-1"), "m-1.apr");
    }

    #[test]
    fn status_payload_notes_are_optional_on_the_wire() {
        let base = StatusPayload::base("t-1", "m-1", "s3://b/runs/m-1/t-1/", Phase::Failed);
        let value = base.to_value();
        assert!(
            value.get("notes").is_none(),
            "an empty notes list is not serialized: {value}"
        );

        let mut noted = base.clone();
        noted.notes = vec![
            "stage-error-overridden: fixed text".to_string(),
            "listed-but-absent: 1 entry".to_string(),
        ];
        let text = noted.to_value().to_string();
        let back: StatusPayload = serde_json::from_str(&text).expect("round trip");
        assert_eq!(back, noted);

        // A payload stored before the field existed still reads, with no notes.
        let mut older = base.to_value();
        older.as_object_mut().expect("object").remove("notes");
        let back: StatusPayload = serde_json::from_value(older).expect("older payload");
        assert!(back.notes.is_empty());
        assert_eq!(back, base);
    }

    #[test]
    fn phase_wire_strings_match_serde() {
        for phase in [
            Phase::Working,
            Phase::Completed,
            Phase::GateFailed,
            Phase::Failed,
            Phase::Cancelled,
        ] {
            assert_eq!(serde_json::to_value(phase).expect("ser"), phase.as_str());
        }
    }
}
