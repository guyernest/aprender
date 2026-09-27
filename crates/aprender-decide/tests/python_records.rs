//! The cross-language key link (plan 08-15): a run dir WRITTEN BY THE PYTHON TRAINER
//! (`scripts/laya_train`, plan 08-14) parses under the Rust structs, and Rust's recomputation of
//! what Python recorded agrees with it bit-for-bit.
//!
//! ARMED only by BOTH `LAYA_PY_RUN_DIR` and `LAYA_PY_DATA_DIR`; otherwise it prints
//! `SKIP: python run dir not armed (LAYA_PY_RUN_DIR + LAYA_PY_DATA_DIR)` and passes (never
//! `#[ignore]`). Produce the input with the lifecycle's richest run (three seeds, the shift
//! probe, the float64 noise record) and point the test at it:
//!
//! ```text
//! K=$(mktemp -d); LAYA_LIFECYCLE_KEEP="$K" just laya-train-lifecycle
//! LAYA_PY_RUN_DIR="$K/run" LAYA_PY_DATA_DIR="$K/data" \
//!   cargo test -p aprender-decide --test python_records -- --nocapture
//! ```
//!
//! It prints one `NOISE which=<set> rust=<noise> python=<reported> bound=<bound>` line per set:
//! the noise Rust RECOMPUTES from the stored float64 rows and the torch probability file must
//! equal the `max_abs` Python reported, and the derived bound Python's `bound`, to the bit.

mod common;

use aprender_decide::pack::PackInputs;
use aprender_decide::verify::{read_data_dir, rescore_bounds, validate_probs, ProbsWhich};
use std::path::PathBuf;

const ENV_RUN: &str = "LAYA_PY_RUN_DIR";
const ENV_DATA: &str = "LAYA_PY_DATA_DIR";

#[test]
fn python_run_dir_records_match_rust() {
    let (Some(run), Some(data)) = (
        std::env::var_os(ENV_RUN).map(PathBuf::from),
        std::env::var_os(ENV_DATA).map(PathBuf::from),
    ) else {
        println!("SKIP: python run dir not armed ({ENV_RUN} + {ENV_DATA})");
        return;
    };
    let inputs = PackInputs::from_run_dir(&run, &data)
        .unwrap_or_else(|e| panic!("the Python run dir {} must parse: {e}", run.display()));
    let d = read_data_dir(&data).expect("the Python data dir parses");
    let labels = d.task.owned_labels();
    let ft = validate_probs(
        ProbsWhich::FineTuned,
        &inputs.eval_probs_json,
        &d.eval,
        &labels,
    )
    .expect("eval-probs.json validates");
    let zs = validate_probs(
        ProbsWhich::ZeroShot,
        &inputs.zero_shot_probs_json,
        &d.eval,
        &labels,
    )
    .expect("zero-shot-probs.json validates");
    let policy = common::policy();

    // A1: the noise record Python wrote, recomputed in Rust.
    assert!(
        inputs.gate_report.rescore_noise_sha256.is_some(),
        "a 1.4.0 Python run dir carries rescore_noise_sha256"
    );
    let bounds = rescore_bounds(&inputs, &ft, &zs, &policy)
        .unwrap_or_else(|e| panic!("Rust refuses Python's noise record: {e}"));
    let record: serde_json::Value = serde_json::from_slice(
        inputs
            .rescore_noise_json
            .as_deref()
            .expect("record bytes read"),
    )
    .expect("record parses");
    for (i, b) in bounds.iter().enumerate() {
        let set = &record["sets"][i];
        let python = set["max_abs"].as_f64().expect("reported max_abs");
        let python_bound = set["bound"].as_f64().expect("reported bound");
        let rust = b.noise.expect("a record yields a recomputed noise");
        println!(
            "NOISE which={} rust={rust:e} python={python:e} bound={:e}",
            b.which, b.bound
        );
        assert_eq!(set["which"].as_str(), Some(b.which.to_string().as_str()));
        assert_eq!(rust.to_bits(), python.to_bits(), "{}: noise", b.which);
        assert_eq!(
            b.bound.to_bits(),
            python_bound.to_bits(),
            "{}: bound",
            b.which
        );
    }
}
