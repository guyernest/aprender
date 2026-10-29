//! The pure phase decision: a finished run's files in, one phase, status and
//! payload out.
//!
//! The finalizer (14-05) reads the run's files from S3, parses them into a
//! [`FinalizeInput`] and calls [`finalize_run`]; nothing here touches the network.
//!
//! # Nothing is recomputed (D-05)
//!
//! This function DECIDES from files the trainer and the packer wrote. It never
//! recomputes a metric, a gate clause or a probability: every verdict field is
//! copied from `gate-report.json` (at the locations `train.py` writes them:
//! `pass` and `margin` at the top, `ece_post` and `macro_f1` under `fine_tuned`,
//! `zero_shot.macro_f1`, `seeds.shipped`, `seeds.per_seed`) or from
//! `pack-outcome.json`. The server's verdict is therefore exactly as trustworthy
//! as `pack_laya verify`, which is the gate authority for completed runs.
//!
//! # The decision table
//!
//! | Situation | Phase | Notes |
//! |-----------|-------|-------|
//! | aborted | `cancelled` | entries minus any `.apr`; keep false |
//! | a manifest path with two different sha256 | `failed` | `integrity` |
//! | stage error, or no outcome | `failed` | the stage error text |
//! | refused `inputs-changed` | `failed` | `prefilter_disagreement` FALSE: the dataset changed after validation |
//! | any other refusal | `failed` | `prefilter_disagreement` TRUE (D-08: a defect to surface) |
//! | crash, timeout (D-12), or `device_ok` false | `failed` | |
//! | gate fail, no pilot-waiver claim | `gate_failed` | non-error result, no pack |
//! | gate fail, claim, pack accepted with `waived` = claim | `completed` | `verdict.pass` false, `.apr` listed, keep false |
//! | gate fail, claim, pack missing or refused | `failed` | D-15 |
//! | gate fail, claim, pack `waived` differs | `failed` | `integrity` |
//! | pass, pack accepted, `waived` null | `completed` | keep true (D-19: GATE PASS only) |
//! | pass, pack `waived` non-null | `failed` | `integrity`: a waiver cannot excuse a passing gate |
//! | pass, pack missing or refused | `failed` | D-15: every report entry kept, no `.apr` |
//!
//! A refused pack can never surface an `.apr`: every non-completed phase strips
//! the `.apr` entry from the manifest and `verdict.apr_sha256` is null.

use std::collections::BTreeMap;

use aprender_mcp_setfit_train::{BackendError, TaskBackend};
use pmcp::types::{CallToolResult, Content, TaskStatus};

use crate::layout::{
    apr_entry_path, links_without_urls, Contracts, ExecutionInput, ExitClass, Images, Manifest,
    ManifestEntry, Outcome, PackInfo, PackOutcome, Phase, SeedRow, StatusPayload, TrainingManifest,
    Verdict, SCHEMA_MANIFEST,
};

/// Longest free-text error the payload carries.
const MAX_ERROR_CHARS: usize = 1_024;

/// Everything the decision reads, already parsed.
#[derive(Debug, Clone)]
pub struct FinalizeInput {
    /// The server bucket, for the status payload's `s3://` run prefix.
    pub bucket: String,
    /// What the state machine started the run with.
    pub execution: ExecutionInput,
    /// A state-machine stage error, when one was recorded.
    pub stage_error: Option<String>,
    /// True when the execution was aborted (a client cancel).
    pub aborted: bool,
    /// `outcome.json`.
    pub outcome: Option<Outcome>,
    /// `gate-report.json`, as written.
    pub gate_report: Option<serde_json::Value>,
    /// `pack-outcome.json`.
    pub pack_outcome: Option<PackOutcome>,
    /// `training-manifest.json`, or `partial-manifest.json` when the complete
    /// one is absent: the caller passes whichever exists and
    /// [`TrainingManifest::complete`] records which.
    pub training_manifest: Option<TrainingManifest>,
    /// The packer's inventory (`pack-manifest.json`).
    pub pack_manifest: Vec<ManifestEntry>,
}

/// What the finalizer writes: the task's terminal status and result, the merged
/// manifest, and whether the run is retained (D-19).
#[derive(Debug, Clone, PartialEq)]
pub struct FinalizeDecision {
    /// The phase.
    pub phase: Phase,
    /// The task status to write: `completed` and `gate_failed` are both
    /// `TaskStatus::Completed` (the call succeeded), `failed` is `Failed`.
    pub status: TaskStatus,
    /// True only for `failed`.
    pub is_error: bool,
    /// The stored payload: every manifest `url` is null (links are minted at read time).
    pub payload: StatusPayload,
    /// The merged inventory, with the images and contracts that produced the run.
    pub manifest: Manifest,
    /// True only for `completed` with `verdict.pass` true (D-19).
    pub keep: bool,
    /// True when the inventory came from the in-run partial manifest.
    pub manifest_partial: bool,
}

/// A payload as the tool result the store persists: an error result for `failed`.
#[must_use]
pub fn payload_result(payload: &StatusPayload, is_error: bool) -> CallToolResult {
    let content = vec![Content::Text {
        text: payload.to_value().to_string(),
    }];
    if is_error {
        CallToolResult::error(content)
    } else {
        CallToolResult::new(content)
    }
}

/// The tool result for a decision.
#[must_use]
pub fn decision_result(decision: &FinalizeDecision) -> CallToolResult {
    payload_result(&decision.payload, decision.is_error)
}

fn f64_at(report: &serde_json::Value, pointer: &str) -> Option<f64> {
    report.pointer(pointer).and_then(serde_json::Value::as_f64)
}

fn str_at(report: &serde_json::Value, pointer: &str) -> Option<String> {
    report
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn seed_row(row: &serde_json::Value) -> Option<SeedRow> {
    Some(SeedRow {
        seed: row.get("seed")?.as_u64()?,
        macro_f1: row.get("macro_f1").and_then(serde_json::Value::as_f64),
        ece_post: row.get("ece_post").and_then(serde_json::Value::as_f64),
        margin: row.get("margin").and_then(serde_json::Value::as_f64),
        pass: row.get("pass").and_then(serde_json::Value::as_bool),
    })
}

/// The verdict, COPIED from a `gate-report.json` value (D-05): one function, so
/// the extraction cannot be done two ways. A field the report lacks is `null`,
/// never a default; `finalize_run` refuses a report that lacks a required one.
#[must_use]
pub fn verdict_from_report(report: &serde_json::Value) -> Verdict {
    Verdict {
        pass: report
            .get("pass")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        waived: None,
        margin: f64_at(report, "/margin"),
        ece_post: f64_at(report, "/fine_tuned/ece_post"),
        zero_shot_macro_f1: f64_at(report, "/zero_shot/macro_f1"),
        ft_macro_f1: f64_at(report, "/fine_tuned/macro_f1"),
        shipped_seed: report
            .pointer("/seeds/shipped")
            .and_then(serde_json::Value::as_u64),
        per_seed: report
            .pointer("/seeds/per_seed")
            .and_then(serde_json::Value::as_array)
            .map(|rows| rows.iter().filter_map(seed_row).collect())
            .unwrap_or_default(),
        device_used: str_at(report, "/device_used"),
        torch_version: str_at(report, "/torch_version"),
        recipe_id: str_at(report, "/recipe_id"),
        shift_probe: report.get("shift_probe").filter(|v| !v.is_null()).cloned(),
        apr_sha256: None,
        pack: None,
    }
}

/// The first field a verdict needs that the report lacks, if any.
fn report_defect(report: &serde_json::Value) -> Option<&'static str> {
    if report
        .get("pass")
        .and_then(serde_json::Value::as_bool)
        .is_none()
    {
        return Some("pass");
    }
    [
        "/margin",
        "/fine_tuned/ece_post",
        "/fine_tuned/macro_f1",
        "/zero_shot/macro_f1",
    ]
    .into_iter()
    .find(|pointer| f64_at(report, pointer).is_none())
}

/// The pilot-waiver claim's id, when the trainer attached one.
fn waiver_claim(report: &serde_json::Value) -> Option<String> {
    str_at(report, "/pilot_waiver/id")
}

fn clip(text: &str) -> String {
    text.chars().take(MAX_ERROR_CHARS).collect()
}

/// Union of the training (or partial) manifest and the pack manifest, sorted by
/// path and de-duplicated. A path listed by both with different content is
/// reported as a conflict (the first writer's entry is kept in the output).
fn merge_entries(
    training: Option<&TrainingManifest>,
    pack: &[ManifestEntry],
) -> (Vec<ManifestEntry>, Option<String>) {
    let mut by_path: BTreeMap<String, ManifestEntry> = BTreeMap::new();
    let mut conflict = None;
    let all = training
        .map(|m| m.entries.as_slice())
        .unwrap_or_default()
        .iter()
        .chain(pack.iter());
    for entry in all {
        match by_path.get(&entry.path) {
            Some(prev) if prev != entry => {
                conflict.get_or_insert_with(|| entry.path.clone());
            }
            Some(_) => {}
            None => {
                by_path.insert(entry.path.clone(), entry.clone());
            }
        }
    }
    (by_path.into_values().collect(), conflict)
}

struct Builder<'a> {
    input: &'a FinalizeInput,
    entries: Vec<ManifestEntry>,
    partial: bool,
}

impl Builder<'_> {
    fn manifest(&self, entries: Vec<ManifestEntry>) -> Manifest {
        let input = self.input;
        Manifest {
            schema: SCHEMA_MANIFEST.to_string(),
            task_id: input.execution.task_id.clone(),
            name: input.execution.name.clone(),
            images: Images {
                train: input
                    .training_manifest
                    .as_ref()
                    .map(|m| m.train_image_digest.clone()),
                pack: input
                    .pack_outcome
                    .as_ref()
                    .map(|p| p.pack_image_digest.clone()),
            },
            contracts: Contracts {
                prefilter_gate: Some(input.execution.gate_contract_sha256.clone()),
                train_gate: input
                    .training_manifest
                    .as_ref()
                    .map(|m| m.gate_contract_sha256.clone()),
                pack_gate: input
                    .pack_outcome
                    .as_ref()
                    .map(|p| p.gate_contract_sha256.clone()),
                pack_parity: input
                    .pack_outcome
                    .as_ref()
                    .map(|p| p.parity_contract_sha256.clone()),
            },
            entries,
            unlisted: 0,
        }
    }

    fn entries_without_apr(&self) -> Vec<ManifestEntry> {
        let apr = apr_entry_path(&self.input.execution.name);
        self.entries
            .iter()
            .filter(|e| e.path != apr)
            .cloned()
            .collect()
    }

    /// Build the decision. `entries` is what the manifest lists.
    fn decide(
        &self,
        phase: Phase,
        entries: Vec<ManifestEntry>,
        verdict: Option<Verdict>,
        error: Option<String>,
        prefilter_disagreement: bool,
    ) -> FinalizeDecision {
        let execution = &self.input.execution;
        let uri = format!("s3://{}/{}", self.input.bucket, execution.run_prefix);
        let mut payload = StatusPayload::base(&execution.task_id, &execution.name, &uri, phase);
        payload.manifest = links_without_urls(&entries);
        payload.error = error.map(|e| clip(&e));
        payload.prefilter_disagreement = prefilter_disagreement;
        payload.manifest_partial = self.partial;
        let keep = phase == Phase::Completed && verdict.as_ref().is_some_and(|v| v.pass);
        payload.verdict = verdict;
        let (status, is_error) = match phase {
            Phase::Failed => (TaskStatus::Failed, true),
            Phase::Cancelled => (TaskStatus::Cancelled, false),
            Phase::Working => (TaskStatus::Working, false),
            Phase::Completed | Phase::GateFailed => (TaskStatus::Completed, false),
        };
        FinalizeDecision {
            phase,
            status,
            is_error,
            payload,
            manifest: self.manifest(entries),
            keep,
            manifest_partial: self.partial,
        }
    }

    fn failed(
        &self,
        error: &str,
        verdict: Option<Verdict>,
        disagreement: bool,
    ) -> FinalizeDecision {
        self.decide(
            Phase::Failed,
            self.entries_without_apr(),
            verdict,
            Some(error.to_string()),
            disagreement,
        )
    }

    fn integrity(&self, what: &str, verdict: Option<Verdict>) -> FinalizeDecision {
        self.failed(&format!("integrity: {what}"), verdict, false)
    }
}

fn pack_info(pack: &PackOutcome) -> PackInfo {
    PackInfo {
        accepted: pack.accepted,
        line: pack
            .verify_line
            .clone()
            .or_else(|| pack.packed_line.clone())
            .map(|l| clip(&l)),
    }
}

/// The pack-stage branch shared by a passing gate and a waived gate failure.
/// `claim` is the pilot-waiver id for the waived branch, `None` for a pass.
fn decide_packed(b: &Builder<'_>, mut verdict: Verdict, claim: Option<&str>) -> FinalizeDecision {
    let Some(pack) = b.input.pack_outcome.as_ref() else {
        verdict.pack = None;
        return b.failed(
            "pack-outcome.json is missing: the pack stage left no record",
            Some(verdict),
            false,
        );
    };
    verdict.pack = Some(pack_info(pack));
    if !pack.accepted {
        let detail = verdict
            .pack
            .as_ref()
            .and_then(|p| p.line.clone())
            .unwrap_or_else(|| "no detail recorded".to_string());
        let code = pack
            .exit_code
            .map_or_else(|| "none".to_string(), |c| c.to_string());
        return b.failed(
            &format!("pack refused (exit {code}): {detail}"),
            Some(verdict),
            false,
        );
    }
    match (claim, pack.waived.as_deref()) {
        (None, Some(_)) => {
            return b.integrity("a passing gate cannot carry a waiver", Some(verdict));
        }
        (Some(id), waived) if waived != Some(id) => {
            return b.integrity(
                "the pack's waiver does not match the trainer's claim",
                Some(verdict),
            );
        }
        _ => {}
    }
    let Some(apr_sha256) = pack.apr_sha256.clone() else {
        return b.integrity("pack accepted without an apr sha256", Some(verdict));
    };
    let apr_path = apr_entry_path(&b.input.execution.name);
    match b.entries.iter().find(|e| e.path == apr_path) {
        Some(entry) if entry.sha256 == apr_sha256 => {}
        _ => {
            return b.integrity(
                "the packed model is not listed with the sha256 pack reported",
                Some(verdict),
            );
        }
    }
    verdict.apr_sha256 = Some(apr_sha256);
    verdict.waived = claim.map(str::to_string);
    b.decide(
        Phase::Completed,
        b.entries.clone(),
        Some(verdict),
        None,
        false,
    )
}

/// Decide a finished run's phase, status, payload and manifest from its files.
/// See the module doc for the table; this function recomputes nothing.
#[must_use]
pub fn finalize_run(input: &FinalizeInput) -> FinalizeDecision {
    let (entries, conflict) = merge_entries(input.training_manifest.as_ref(), &input.pack_manifest);
    let b = Builder {
        input,
        entries,
        partial: input
            .training_manifest
            .as_ref()
            .is_some_and(|m| !m.complete),
    };

    if input.aborted {
        return b.decide(Phase::Cancelled, b.entries_without_apr(), None, None, false);
    }
    if let Some(path) = conflict {
        return b.integrity(
            &format!("{path} is listed by two writers with different content"),
            None,
        );
    }
    if let Some(error) = &input.stage_error {
        return b.failed(error, None, false);
    }
    let Some(outcome) = input.outcome.as_ref() else {
        return b.failed(
            "outcome.json is missing: the training stage left no record",
            None,
            false,
        );
    };

    match outcome.exit_class {
        ExitClass::Refused => {
            let line = outcome
                .refused_line
                .as_deref()
                .unwrap_or("REFUSED (no line recorded)");
            return if line.starts_with("REFUSED inputs-changed") {
                b.failed(
                    &format!("dataset changed after validation (inputs-changed): {line}"),
                    None,
                    false,
                )
            } else {
                b.failed(
                    &format!("the trainer refused a dataset the pre-filter accepted: {line}"),
                    None,
                    true,
                )
            };
        }
        ExitClass::Crash => return b.failed("the trainer crashed", None, false),
        ExitClass::Timeout => {
            return b.failed(
                "the training job hit its runtime cap and was stopped",
                None,
                false,
            )
        }
        ExitClass::Pass | ExitClass::GateFail => {}
    }
    if !outcome.device_ok {
        return b.failed(
            &format!(
                "the trainer ran on {:?}, not a device the contract allows",
                outcome.device_used
            ),
            None,
            false,
        );
    }

    let Some(report) = input.gate_report.as_ref() else {
        return b.failed("gate-report.json is missing", None, false);
    };
    if let Some(field) = report_defect(report) {
        return b.failed(&format!("gate-report.json lacks `{field}`"), None, false);
    }
    let verdict = verdict_from_report(report);
    let claim = waiver_claim(report);

    match outcome.exit_class {
        ExitClass::Pass => {
            if !verdict.pass {
                return b.integrity(
                    "the outcome says pass but the gate report does not",
                    Some(verdict),
                );
            }
            decide_packed(&b, verdict, None)
        }
        _ => {
            if verdict.pass {
                return b.integrity(
                    "the outcome says gate_fail but the gate report passes",
                    Some(verdict),
                );
            }
            match claim {
                None => b.decide(
                    Phase::GateFailed,
                    b.entries_without_apr(),
                    Some(verdict),
                    None,
                    false,
                ),
                Some(id) => decide_packed(&b, verdict, Some(&id)),
            }
        }
    }
}

/// What [`attach_result_if_absent`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachOutcome {
    /// The result was written.
    Attached,
    /// A result was already stored; nothing was written.
    AlreadyPresent,
    /// The task is not terminal yet; nothing was written.
    NotTerminal,
    /// No such task for this owner; nothing was written.
    NotFound,
}

/// Give a terminal task that has no stored result its payload, without changing
/// anything else.
///
/// It exists because the store's own writers cannot do this: `TaskStore::set_result`
/// overwrites unconditionally, and `AprenderTaskStore::finish` refuses a terminal
/// record. A client `tasks/cancel` moves a task to `Cancelled` and writes NO
/// result, so until something attaches one, `tasks/result` answers "task not
/// completed". This is the finalizer's (14-05) only way to give a cancelled task
/// its stored payload.
///
/// It writes `result` only when none is stored and only on a terminal record, by
/// compare-and-swap on the version, never touches the status or the timestamps,
/// and never replaces a result: first writer wins is preserved. On a lost CAS it
/// re-reads once and applies the same rules again.
///
/// # Errors
///
/// A backend failure other than the one retried conflict.
pub async fn attach_result_if_absent(
    backend: &dyn TaskBackend,
    owner: &str,
    task_id: &str,
    result: CallToolResult,
) -> Result<AttachOutcome, BackendError> {
    for attempt in 0..2 {
        let Some(mut record) = backend.get(owner, task_id).await? else {
            return Ok(AttachOutcome::NotFound);
        };
        if record.result.is_some() {
            return Ok(AttachOutcome::AlreadyPresent);
        }
        if !record.task.status.is_terminal() {
            return Ok(AttachOutcome::NotTerminal);
        }
        let expected = record.version;
        record.result = Some(result.clone());
        record.version = expected + 1;
        match backend.cas(owner, task_id, expected, record).await {
            Ok(()) => return Ok(AttachOutcome::Attached),
            Err(BackendError::Conflict) if attempt == 0 => {}
            Err(e) => return Err(e),
        }
    }
    Err(BackendError::Conflict)
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
pub(crate) mod fixtures {
    use aprender_decide::digest::sha256_hex;

    use super::FinalizeInput;
    use crate::layout::{
        inputs_string, ExecutionInput, ManifestEntry, Outcome, PackOutcome, TrainingManifest,
        SCHEMA_EXECUTION_INPUT,
    };

    pub const OUTCOME: &str = include_str!("../fixtures/outcome-v1.json");
    pub const TRAINING_MANIFEST: &str = include_str!("../fixtures/training-manifest-v1.json");
    pub const PACK_OUTCOME: &str = include_str!("../fixtures/pack-outcome-v1.json");
    pub const MANIFEST: &str = include_str!("../fixtures/manifest-v1.json");
    pub const GATE_REPORT: &str = include_str!("../fixtures/gate-report-v1.json");
    /// The REAL report the trainer's own tiny run wrote (one seed, no pilot waiver).
    pub const REAL_TINY_REPORT: &str =
        include_str!("../../aprender-decide/tests/fixtures/laya_tiny/gate-report.json");

    pub const TASK_ID: &str = "00000000-0000-4000-8000-000000000001";
    pub const NAME: &str = "fixture-model";
    pub const BUCKET: &str = "fixture-bucket";

    pub fn sha(label: &str) -> String {
        sha256_hex(label.as_bytes())
    }

    pub fn outcome() -> Outcome {
        serde_json::from_str(OUTCOME).expect("outcome fixture")
    }

    pub fn training_manifest() -> TrainingManifest {
        serde_json::from_str(TRAINING_MANIFEST).expect("training manifest fixture")
    }

    pub fn pack_outcome() -> PackOutcome {
        serde_json::from_str(PACK_OUTCOME).expect("pack outcome fixture")
    }

    pub fn gate_report() -> serde_json::Value {
        serde_json::from_str(GATE_REPORT).expect("gate report fixture")
    }

    /// The packer's inventory, spelled out here (not read back from the golden
    /// manifest), so the golden-equality test checks the merge for real.
    pub fn pack_entries() -> Vec<ManifestEntry> {
        [
            ("fixture-model.apr", 4_194_304, "apr"),
            ("pack-outcome.json", 640, "pack-outcome"),
            ("pack.log", 1_024, "pack-log"),
            ("verify.log", 2_048, "verify-log"),
        ]
        .into_iter()
        .map(|(path, size, label)| ManifestEntry {
            path: path.to_string(),
            size,
            sha256: sha(label),
        })
        .collect()
    }

    pub fn execution() -> ExecutionInput {
        let inputs = [
            ("task.json", "data-task"),
            ("train.jsonl", "data-train"),
            ("eval.jsonl", "data-eval"),
        ]
        .into_iter()
        .map(|(file, label)| (file.to_string(), sha(label)))
        .collect();
        ExecutionInput {
            schema: SCHEMA_EXECUTION_INPUT.to_string(),
            task_id: TASK_ID.to_string(),
            owner: "alice".to_string(),
            name: NAME.to_string(),
            dataset_prefix: "datasets/fixture/".to_string(),
            run_prefix: format!("runs/{NAME}/{TASK_ID}/"),
            epochs: String::new(),
            inputs_sha256: inputs_string(&inputs),
            gate_contract_sha256: sha("prefilter-gate"),
        }
    }

    /// A passing, accepted run, built only from the committed fixtures.
    pub fn input() -> FinalizeInput {
        FinalizeInput {
            bucket: BUCKET.to_string(),
            execution: execution(),
            stage_error: None,
            aborted: false,
            outcome: Some(outcome()),
            gate_report: Some(gate_report()),
            pack_outcome: Some(pack_outcome()),
            training_manifest: Some(training_manifest()),
            pack_manifest: pack_entries(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
mod tests {
    use super::fixtures::{self, input, sha};
    use super::*;
    use crate::layout::{ExitClass, Manifest, PackOutcome, TrainingManifest};

    fn gate_fail_input() -> FinalizeInput {
        let mut i = input();
        i.outcome.as_mut().expect("outcome").exit_class = ExitClass::GateFail;
        i.gate_report.as_mut().expect("report")["pass"] = serde_json::json!(false);
        i.pack_outcome = None;
        i.pack_manifest = Vec::new();
        i
    }

    fn waived_input(id: &str) -> FinalizeInput {
        let mut i = gate_fail_input();
        i.gate_report.as_mut().expect("report")["pilot_waiver"] =
            serde_json::json!({ "id": id, "waives": "ece_post" });
        let mut pack = fixtures::pack_outcome();
        pack.waived = Some(id.to_string());
        i.pack_outcome = Some(pack);
        i.pack_manifest = fixtures::pack_entries();
        i
    }

    fn paths(d: &FinalizeDecision) -> Vec<&str> {
        d.manifest.entries.iter().map(|e| e.path.as_str()).collect()
    }

    fn has_apr(d: &FinalizeDecision) -> bool {
        d.manifest.entries.iter().any(|e| e.path.ends_with(".apr"))
            || d.payload.manifest.iter().any(|l| l.path.ends_with(".apr"))
    }

    fn all_report_entries_present(d: &FinalizeDecision) {
        for entry in &fixtures::training_manifest().entries {
            assert!(
                paths(d).contains(&entry.path.as_str()),
                "report entry {} is missing",
                entry.path
            );
        }
    }

    fn error_of(d: &FinalizeDecision) -> String {
        d.payload.error.clone().unwrap_or_default()
    }

    #[test]
    fn fixtures_round_trip() {
        fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(text: &str) {
            let value: serde_json::Value = serde_json::from_str(text).expect("json");
            let parsed: T = serde_json::from_value(value.clone()).expect("parses into its struct");
            assert_eq!(serde_json::to_value(parsed).expect("ser"), value);
        }
        round_trip::<Outcome>(fixtures::OUTCOME);
        round_trip::<TrainingManifest>(fixtures::TRAINING_MANIFEST);
        round_trip::<PackOutcome>(fixtures::PACK_OUTCOME);
        round_trip::<Manifest>(fixtures::MANIFEST);

        // The gate-report fixture mirrors the REAL report's nesting exactly.
        let report = fixtures::gate_report();
        let keys = |v: &serde_json::Value| -> Vec<String> {
            let mut k: Vec<String> = v.as_object().expect("object").keys().cloned().collect();
            k.sort();
            k
        };
        assert_eq!(
            keys(&report),
            [
                "calibration",
                "device_is_cpu",
                "device_used",
                "eval_probs_sha256",
                "fine_tuned",
                "inputs_sha256",
                "margin",
                "pass",
                "probes_sha256",
                "recipe_id",
                "rescore_noise_sha256",
                "schema",
                "seeds",
                "thresholds",
                "torch_version",
                "zero_shot",
                "zero_shot_probs_sha256"
            ]
        );
        assert_eq!(
            keys(&report["calibration"]),
            [
                "bucket",
                "clamp_hit",
                "slice_ids",
                "slice_ids_sha256",
                "slice_size",
                "t_applied",
                "t_fitted"
            ]
        );
        assert!(report["fine_tuned"].get("ece_post").is_some());
        assert!(report["calibration"].get("ece_post").is_none());
        assert_eq!(
            keys(&report["inputs_sha256"]),
            [
                "base_model",
                "eval_jsonl",
                "task_json",
                "tokenizer_json",
                "train_jsonl"
            ]
        );
        assert_eq!(
            report["seeds"]["per_seed"].as_array().expect("rows").len(),
            3
        );
        let real: serde_json::Value =
            serde_json::from_str(fixtures::REAL_TINY_REPORT).expect("real");
        assert_eq!(keys(&report["fine_tuned"]), keys(&real["fine_tuned"]));
        assert_eq!(keys(&report["calibration"]), keys(&real["calibration"]));
    }

    #[test]
    fn finalize_completed() {
        let d = finalize_run(&input());
        assert_eq!(d.phase, Phase::Completed);
        assert_eq!(d.status, TaskStatus::Completed);
        assert!(!d.is_error);
        assert!(d.keep);
        assert!(has_apr(&d));
        assert_eq!(d.payload.error, None);
        assert!(!d.payload.prefilter_disagreement);
        assert!(!d.manifest_partial);
        let verdict = d.payload.verdict.as_ref().expect("verdict");
        assert!(verdict.pass);
        assert_eq!(verdict.apr_sha256, Some(sha("apr")));
        assert_eq!(verdict.waived, None);
        assert_eq!(verdict.pack.as_ref().map(|p| p.accepted), Some(true));
        assert_eq!(
            d.payload.run_prefix,
            format!(
                "s3://fixture-bucket/runs/fixture-model/{}/",
                fixtures::TASK_ID
            )
        );
        // The merge, the images and the contracts equal the committed golden manifest.
        let golden: serde_json::Value = serde_json::from_str(fixtures::MANIFEST).expect("golden");
        assert_eq!(serde_json::to_value(&d.manifest).expect("ser"), golden);
    }

    #[test]
    fn finalize_gate_fail() {
        let mut i = gate_fail_input();
        // Even a stray .apr in a pack manifest must not surface.
        i.pack_manifest = vec![fixtures::pack_entries().remove(0)];
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::GateFailed);
        assert_eq!(d.status, TaskStatus::Completed);
        assert!(!d.is_error);
        assert!(!d.keep);
        assert!(!has_apr(&d));
        assert!(!d.payload.verdict.as_ref().expect("verdict").pass);
        all_report_entries_present(&d);
    }

    #[test]
    fn finalize_gate_fail_no_claim() {
        let i = gate_fail_input();
        assert!(i
            .gate_report
            .as_ref()
            .expect("report")
            .get("pilot_waiver")
            .is_none());
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::GateFailed, "no claim: gate_failed, no pack");
        assert_eq!(d.payload.verdict.as_ref().expect("verdict").waived, None);
        assert!(!d.keep);
    }

    fn pack_refused(code: i32) -> FinalizeDecision {
        let mut i = input();
        let pack = i.pack_outcome.as_mut().expect("pack");
        pack.accepted = false;
        pack.apr_sha256 = None;
        pack.exit_code = Some(code);
        pack.deploy_eligible = false;
        // The worst case: the packer wrote the model, then verify refused it.
        i.pack_manifest = fixtures::pack_entries();
        finalize_run(&i)
    }

    fn assert_refused_pack(d: &FinalizeDecision) {
        assert_eq!(d.phase, Phase::Failed);
        assert_eq!(d.status, TaskStatus::Failed);
        assert!(d.is_error);
        assert!(!d.keep);
        let verdict = d
            .payload
            .verdict
            .as_ref()
            .expect("the report verdict is kept");
        assert_eq!(verdict.apr_sha256, None);
        assert_eq!(verdict.pack.as_ref().map(|p| p.accepted), Some(false));
        assert!(!has_apr(d), "a refused pack can never surface an .apr");
        all_report_entries_present(d);
        assert!(error_of(d).contains("pack refused"), "{}", error_of(d));
    }

    #[test]
    fn finalize_pack_refused_3() {
        let d = pack_refused(3);
        assert_refused_pack(&d);
        assert!(error_of(&d).contains("exit 3"), "{}", error_of(&d));
    }

    #[test]
    fn finalize_pack_refused_2() {
        let d = pack_refused(2);
        assert_refused_pack(&d);
        assert!(error_of(&d).contains("exit 2"), "{}", error_of(&d));
    }

    #[test]
    fn finalize_pack_missing() {
        let mut i = input();
        i.pack_outcome = None;
        i.pack_manifest = Vec::new();
        i.stage_error = Some("pack stage failed: ECS task exited 137".to_string());
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(
            error_of(&d).contains("ECS task exited 137"),
            "{}",
            error_of(&d)
        );
        // Without a recorded stage error the missing record is itself the finding.
        i.stage_error = None;
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(
            error_of(&d).contains("pack-outcome.json is missing"),
            "{}",
            error_of(&d)
        );
        assert!(!has_apr(&d));
        all_report_entries_present(&d);
    }

    #[test]
    fn finalize_refused() {
        let mut i = input();
        let outcome = i.outcome.as_mut().expect("outcome");
        outcome.exit_class = ExitClass::Refused;
        outcome.refused_line =
            Some("REFUSED eval-class-coverage: eval.jsonl holds no row".to_string());
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(d.is_error);
        assert!(
            d.payload.prefilter_disagreement,
            "D-08: a defect to surface"
        );
        assert!(
            error_of(&d).contains("REFUSED eval-class-coverage"),
            "{}",
            error_of(&d)
        );
    }

    #[test]
    fn finalize_inputs_changed() {
        let mut i = input();
        let outcome = i.outcome.as_mut().expect("outcome");
        outcome.exit_class = ExitClass::Refused;
        outcome.refused_line = Some("REFUSED inputs-changed: train.jsonl".to_string());
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(
            !d.payload.prefilter_disagreement,
            "the dataset changed after validation; the rules did not drift"
        );
        assert!(error_of(&d).contains("inputs-changed"), "{}", error_of(&d));
    }

    #[test]
    fn finalize_crash() {
        let mut i = input();
        i.outcome.as_mut().expect("outcome").exit_class = ExitClass::Crash;
        let d = finalize_run(&i);
        assert_eq!(
            (d.phase, d.status, d.is_error),
            (Phase::Failed, TaskStatus::Failed, true)
        );
        assert!(!d.keep);
    }

    #[test]
    fn finalize_timeout() {
        let mut i = input();
        i.outcome.as_mut().expect("outcome").exit_class = ExitClass::Timeout;
        let d = finalize_run(&i);
        assert_eq!(
            (d.phase, d.status, d.is_error),
            (Phase::Failed, TaskStatus::Failed, true)
        );
        assert!(error_of(&d).contains("runtime cap"), "{}", error_of(&d));
    }

    #[test]
    fn finalize_cpu() {
        let mut i = input();
        let outcome = i.outcome.as_mut().expect("outcome");
        outcome.device_ok = false;
        outcome.device_used = "cpu".to_string();
        let d = finalize_run(&i);
        assert_eq!(
            (d.phase, d.status, d.is_error),
            (Phase::Failed, TaskStatus::Failed, true)
        );
        assert!(error_of(&d).contains("cpu"), "{}", error_of(&d));
    }

    #[test]
    fn finalize_aborted() {
        let mut i = input();
        i.aborted = true;
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Cancelled);
        assert_eq!(d.status, TaskStatus::Cancelled);
        assert!(!d.is_error);
        assert!(!d.keep);
        assert!(d.payload.verdict.is_none());
        assert_eq!(d.payload.error, None);
        assert!(!has_apr(&d));
        all_report_entries_present(&d);
    }

    #[test]
    fn finalize_no_outcome() {
        let mut i = input();
        i.outcome = None;
        i.stage_error = Some("train stage failed: SageMaker job Failed".to_string());
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(
            error_of(&d).contains("SageMaker job Failed"),
            "{}",
            error_of(&d)
        );
        i.stage_error = None;
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(
            error_of(&d).contains("outcome.json is missing"),
            "{}",
            error_of(&d)
        );
    }

    #[test]
    fn finalize_integrity() {
        let mut i = input();
        let mut clash = i.training_manifest.as_ref().expect("tm").entries[0].clone();
        clash.sha256 = sha("somebody-else");
        i.pack_manifest.push(clash);
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(error_of(&d).contains("integrity"), "{}", error_of(&d));
        // The same path with the same content is simply de-duplicated.
        let mut i = input();
        i.pack_manifest
            .push(i.training_manifest.as_ref().expect("tm").entries[0].clone());
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Completed);
        assert_eq!(d.manifest.entries.len(), 11);
    }

    #[test]
    fn finalize_images() {
        let d = finalize_run(&input());
        let tm = fixtures::training_manifest();
        let pack = fixtures::pack_outcome();
        assert_eq!(
            d.manifest.images.train.as_deref(),
            Some(tm.train_image_digest.as_str())
        );
        assert_eq!(
            d.manifest.images.pack.as_deref(),
            Some(pack.pack_image_digest.as_str())
        );
        let d = finalize_run(&gate_fail_input());
        assert_eq!(
            d.manifest.images.train.as_deref(),
            Some(tm.train_image_digest.as_str())
        );
        assert_eq!(
            d.manifest.images.pack, None,
            "no pack-outcome, no pack image"
        );
    }

    #[test]
    fn finalize_contracts() {
        let mut i = input();
        i.execution.gate_contract_sha256 = "e".repeat(64);
        let d = finalize_run(&i);
        let tm = fixtures::training_manifest();
        let pack = fixtures::pack_outcome();
        let c = &d.manifest.contracts;
        assert_eq!(c.prefilter_gate.as_deref(), Some("e".repeat(64).as_str()));
        assert_eq!(
            c.train_gate.as_deref(),
            Some(tm.gate_contract_sha256.as_str())
        );
        assert_eq!(
            c.pack_gate.as_deref(),
            Some(pack.gate_contract_sha256.as_str())
        );
        assert_eq!(
            c.pack_parity.as_deref(),
            Some(pack.parity_contract_sha256.as_str())
        );
        let d = finalize_run(&gate_fail_input());
        assert!(d.manifest.contracts.prefilter_gate.is_some());
        assert_eq!(
            d.manifest.contracts.pack_gate, None,
            "null when the file is absent"
        );
        assert_eq!(d.manifest.contracts.pack_parity, None);
    }

    #[test]
    fn finalize_partial_manifest() {
        let mut i = input();
        let tm = i.training_manifest.as_mut().expect("tm");
        tm.complete = false;
        tm.entries.truncate(4);
        i.outcome.as_mut().expect("outcome").exit_class = ExitClass::Timeout;
        i.pack_outcome = None;
        i.pack_manifest = Vec::new();
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert_eq!(
            d.manifest.entries.len(),
            4,
            "the four uploaded files stay inventoried"
        );
        assert!(d.manifest_partial);
        assert!(
            d.payload.manifest_partial,
            "the payload records the inventory as partial"
        );
        // A complete manifest is not partial.
        assert!(!finalize_run(&input()).payload.manifest_partial);
    }

    #[test]
    fn finalize_waived_accepted() {
        let d = finalize_run(&waived_input("W-1"));
        assert_eq!(d.phase, Phase::Completed);
        assert_eq!(d.status, TaskStatus::Completed);
        assert!(!d.is_error);
        let verdict = d.payload.verdict.as_ref().expect("verdict");
        assert!(!verdict.pass, "a waiver never changes the gate's own pass");
        assert_eq!(verdict.waived.as_deref(), Some("W-1"));
        assert_eq!(verdict.apr_sha256, Some(sha("apr")));
        assert!(has_apr(&d), ".apr listed");
        assert!(
            !d.keep,
            "D-19 keeps GATE PASS only; a waived acceptance expires"
        );
    }

    #[test]
    fn finalize_waived_pack_refused() {
        let mut i = waived_input("W-1");
        let pack = i.pack_outcome.as_mut().expect("pack");
        pack.accepted = false;
        pack.apr_sha256 = None;
        pack.exit_code = Some(3);
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(d.is_error);
        assert!(!has_apr(&d));
        all_report_entries_present(&d);
        assert_eq!(
            d.payload.verdict.as_ref().expect("verdict").apr_sha256,
            None
        );
    }

    #[test]
    fn finalize_waived_mismatch() {
        let mut i = waived_input("W-1");
        i.pack_outcome.as_mut().expect("pack").waived = Some("W-2".to_string());
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(error_of(&d).contains("integrity"), "{}", error_of(&d));
        assert!(!has_apr(&d));
        // An accepted pack that reports no waiver at all is a mismatch too.
        let mut i = waived_input("W-1");
        i.pack_outcome.as_mut().expect("pack").waived = None;
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(error_of(&d).contains("integrity"), "{}", error_of(&d));
    }

    #[test]
    fn finalize_pass_waived_integrity() {
        let mut i = input();
        i.pack_outcome.as_mut().expect("pack").waived = Some("W-1".to_string());
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(error_of(&d).contains("integrity"), "{}", error_of(&d));
        assert!(!d.keep);
    }

    #[test]
    fn finalize_without_a_listed_model_is_an_integrity_failure() {
        let mut i = input();
        i.pack_manifest.retain(|e| !e.path.ends_with(".apr"));
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(error_of(&d).contains("integrity"), "{}", error_of(&d));
        let mut i = input();
        i.pack_manifest[0].sha256 = sha("a different model");
        assert!(error_of(&finalize_run(&i)).contains("integrity"));
    }

    #[test]
    fn finalize_malformed_report_is_refused_not_defaulted() {
        let mut i = input();
        i.gate_report.as_mut().expect("report")["fine_tuned"]
            .as_object_mut()
            .expect("object")
            .remove("ece_post");
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Failed);
        assert!(error_of(&d).contains("ece_post"), "{}", error_of(&d));
        let mut i = input();
        i.gate_report = None;
        assert_eq!(finalize_run(&i).phase, Phase::Failed);
        // The outcome and the report must agree about the gate.
        let mut i = input();
        i.gate_report.as_mut().expect("report")["pass"] = serde_json::json!(false);
        assert!(error_of(&finalize_run(&i)).contains("integrity"));
    }

    #[test]
    fn verdict_from_real_report() {
        let real: serde_json::Value =
            serde_json::from_str(fixtures::REAL_TINY_REPORT).expect("real");
        let v = verdict_from_report(&real);
        assert_eq!(v.ece_post, real["fine_tuned"]["ece_post"].as_f64());
        assert!(v.ece_post.is_some());
        assert_eq!(v.margin, real["margin"].as_f64());
        assert_eq!(v.zero_shot_macro_f1, real["zero_shot"]["macro_f1"].as_f64());
        assert_eq!(v.ft_macro_f1, real["fine_tuned"]["macro_f1"].as_f64());
        assert_eq!(v.pass, real["pass"].as_bool().expect("pass"));
        assert_eq!(
            v.shipped_seed, None,
            "a one-seed report has no shipped seed"
        );
        assert!(v.per_seed.is_empty());
        assert_eq!(v.device_used.as_deref(), real["device_used"].as_str());
        assert_eq!(v.torch_version.as_deref(), real["torch_version"].as_str());
        assert_eq!(v.recipe_id.as_deref(), real["recipe_id"].as_str());
        assert!(v.shift_probe.is_none());
        assert_eq!(
            report_defect(&real),
            None,
            "the real report has every required field"
        );

        // Three seeds, and a `calibration` block with no ece key.
        let report = fixtures::gate_report();
        let v = verdict_from_report(&report);
        assert_eq!(v.ece_post, report["fine_tuned"]["ece_post"].as_f64());
        assert_eq!(v.shipped_seed, report["seeds"]["shipped"].as_u64());
        assert_eq!(v.per_seed.len(), 3);
        for (row, source) in v
            .per_seed
            .iter()
            .zip(report["seeds"]["per_seed"].as_array().expect("rows"))
        {
            assert_eq!(Some(row.seed), source["seed"].as_u64());
            assert_eq!(row.ece_post, source["ece_post"].as_f64());
            assert_eq!(row.macro_f1, source["macro_f1"].as_f64());
            assert_eq!(row.margin, source["margin"].as_f64());
            assert_eq!(row.pass, source["pass"].as_bool());
        }
        // The shipped seed's own row is the report's fine_tuned block.
        let shipped = v
            .per_seed
            .iter()
            .find(|r| Some(r.seed) == v.shipped_seed)
            .expect("shipped row");
        assert_eq!(shipped.ece_post, v.ece_post);
    }

    #[test]
    fn verdict_copies_the_shift_probe_as_is() {
        let mut report = fixtures::gate_report();
        report["shift_probe"] =
            serde_json::json!({ "n": 12, "ece": 0.19, "note": "kept verbatim" });
        let v = verdict_from_report(&report);
        assert_eq!(v.shift_probe, Some(report["shift_probe"].clone()));
        report["shift_probe"] = serde_json::Value::Null;
        assert!(verdict_from_report(&report).shift_probe.is_none());
    }

    fn every_decision() -> Vec<FinalizeDecision> {
        let mut crash = input();
        crash.outcome.as_mut().expect("outcome").exit_class = ExitClass::Crash;
        let mut aborted = input();
        aborted.aborted = true;
        vec![
            finalize_run(&input()),
            finalize_run(&gate_fail_input()),
            finalize_run(&waived_input("W-1")),
            finalize_run(&crash),
            finalize_run(&aborted),
            pack_refused(3),
        ]
    }

    #[test]
    fn payload_no_links_stored() {
        for d in every_decision() {
            assert!(
                d.payload.manifest.iter().all(|l| l.url.is_none()),
                "{:?} stored a link",
                d.phase
            );
            let stored = serde_json::to_value(&d.payload).expect("ser");
            for entry in stored["manifest"].as_array().expect("manifest") {
                assert!(entry["url"].is_null());
            }
            let text = decision_result(&d);
            let Some(Content::Text { text }) = text.content.first() else {
                panic!("no text");
            };
            assert!(
                !text.contains("https://"),
                "a stored payload carries no link: {text}"
            );
        }
    }

    #[test]
    fn payload_size() {
        let mut i = input();
        let tm = i.training_manifest.as_mut().expect("tm");
        tm.entries = (0..64)
            .map(|n| ManifestEntry {
                path: format!("run/seeds/seed-20260925/checkpoint/shard-{n:04}.safetensors"),
                size: 1 << 28,
                sha256: sha(&format!("shard-{n}")),
            })
            .collect();
        let d = finalize_run(&i);
        assert_eq!(d.phase, Phase::Completed);
        assert_eq!(d.payload.manifest.len(), 64 + 4);
        assert_eq!(
            d.payload.verdict.as_ref().expect("verdict").per_seed.len(),
            3
        );
        let size = decision_result(&d)
            .content
            .iter()
            .map(|c| match c {
                Content::Text { text } => text.len(),
                _ => 0,
            })
            .sum::<usize>();
        assert!(
            size < 64 * 1024,
            "{size} bytes: far under the 380 KB item ceiling"
        );
    }
}
