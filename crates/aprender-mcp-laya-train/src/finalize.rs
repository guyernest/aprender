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
