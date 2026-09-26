//! The decide-apr-v1 tracer on the tiny fixture: pack -> bytes -> ladder -> Decider ->
//! classify, with the identity the classify response will carry.

use super::{artifact_sha256_hex, ProbeRecord};
use crate::pack::{pack_run_dir, sha256_hex, ProbesFile};
use crate::test_support::{f32_list, fixture_dir, max_abs, oracle, read, tolerance, within};
use crate::{Decider, Task};

/// The committed tiny run dir (plan 08-02) packed with the shipped door.
pub(crate) fn pack_tiny() -> Vec<u8> {
    let dir = fixture_dir();
    pack_run_dir(&dir, &dir.join("data")).expect("the tiny run dir packs")
}

/// The run dir's `probes.json`, parsed with the same typed schema the packer uses.
pub(crate) fn fixture_probes() -> Vec<ProbeRecord> {
    serde_json::from_slice::<ProbesFile>(&read("probes.json"))
        .expect("probes.json parses")
        .probes
}

/// FALSIFY-DECIDE-APR-004/-005 on the tracer path: the packed tiny artifact loads only
/// through the ladder, classifies the oracle's task rows within laya-parity-v1
/// `probs_abs`, and reports the identity of the served bytes (D-11).
#[test]
fn tiny_roundtrip() {
    let bytes = pack_tiny();
    let decider = Decider::load_bytes(&bytes).expect("the ladder accepts the packed fixture");

    // Classification equals Laya's own oracle on the task rows.
    let bar = tolerance("probs_abs");
    let o = oracle();
    let rows: Vec<&serde_json::Value> = o["rows"]
        .as_array()
        .expect("oracle rows")
        .iter()
        .filter(|r| r["qid"] == "team")
        .collect();
    assert!(rows.len() >= 2, "the oracle has task rows");
    let texts: Vec<String> = rows
        .iter()
        .map(|r| r["state"].as_str().expect("state").to_string())
        .collect();
    let decisions = decider.classify(&texts).expect("classify");
    assert_eq!(decisions.len(), rows.len());
    let mut worst = 0.0f64;
    for (i, (r, d)) in rows.iter().zip(&decisions).enumerate() {
        let dp = max_abs(&d.probabilities, &f32_list(&r["probabilities"]));
        assert!(within(dp, bar), "task row {i}: probs max|d| {dp} > {bar}");
        let want = usize::try_from(r["argmax"].as_u64().expect("argmax")).expect("fits");
        assert_eq!(d.label_index, want, "task row {i}: argmax exact");
        worst = worst.max(dp);
    }

    // Identity (D-11): the whole-file sha256 and the recipe blob's sha256.
    let id = decider.identity();
    assert_eq!(id.artifact_sha256, artifact_sha256_hex(&bytes));
    assert_eq!(id.artifact_sha256, sha256_hex(&bytes));
    assert_eq!(id.artifact_sha256.len(), 64);
    assert_eq!(id.recipe_id, sha256_hex(&read("recipe.json")));
    assert_eq!(id.method, "laya");
    assert_eq!(id.base, "laya-tiny-synthetic@fixtures");

    // Labels in data/task.json document order (deliberately not sorted).
    let task = Task::from_slice(&read("data/task.json")).expect("data task parses");
    assert_eq!(decider.labels(), task.labels());
    assert_eq!(decider.labels(), ["shipping", "billing", "account"]);

    // The declared base and variant ride in the manifest (D-04).
    let m = decider.manifest();
    assert_eq!(m.base.checkpoint, "tiny-synthetic");
    assert_eq!(m.variant, "synthetic-fixture");
    assert_eq!(m.base, id.base_decl);

    // Probe expectations are the PYTHON values from probes.json, stored verbatim.
    assert_eq!(m.probes, fixture_probes());

    println!(
        "tiny_roundtrip: {} bytes, sha256 {}, {} task rows max|d| {worst:.3e} (bar {bar:e}), ARCH={}",
        bytes.len(),
        id.artifact_sha256,
        rows.len(),
        std::env::consts::ARCH
    );
}
