//! FALSIFY-BUILTIN-007: the Rust constants EQUAL `contracts/aprender-builtin-v1.yaml`; the
//! file is the declaration, never a literal duplicated here.

use std::path::Path;

use crate::config::{DEFAULT_ROWS, DEFAULT_TOLERANCE_ABS};
use crate::precheck::{
    ARTIFACT_CAP, EVAL_CAP, EXIT_CONFIG_INVALID, EXIT_ELIGIBLE, EXIT_REFUSED,
    EXIT_STORE_UNREACHABLE, RECORD_CAP,
};
use crate::EXIT_USAGE;

fn constants() -> serde_yaml::Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/aprender-builtin-v1.yaml");
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).expect("the contract file"))
            .expect("yaml");
    doc.get("constants").cloned().expect("constants")
}

fn u64_of(key: &str) -> u64 {
    constants()
        .get(key)
        .and_then(serde_yaml::Value::as_u64)
        .unwrap_or_else(|| panic!("constants.{key} must be an integer"))
}

fn f64_of(key: &str) -> f64 {
    constants()
        .get(key)
        .and_then(serde_yaml::Value::as_f64)
        .unwrap_or_else(|| panic!("constants.{key} must be a number"))
}

#[test]
fn caps_equal_the_contract() {
    assert_eq!(ARTIFACT_CAP, u64_of("artifact_cap_bytes"));
    assert_eq!(RECORD_CAP, u64_of("record_cap_bytes"));
    assert_eq!(EVAL_CAP, u64_of("eval_cap_bytes"));
}

#[test]
fn exit_codes_equal_the_contract() {
    assert_eq!(
        i64::from(EXIT_ELIGIBLE),
        i64::try_from(u64_of("exit_eligible")).expect("fits")
    );
    assert_eq!(
        i64::from(EXIT_REFUSED),
        i64::try_from(u64_of("exit_refused")).expect("fits")
    );
    assert_eq!(
        i64::from(EXIT_CONFIG_INVALID),
        i64::try_from(u64_of("exit_config_invalid")).expect("fits")
    );
    assert_eq!(
        i64::from(EXIT_STORE_UNREACHABLE),
        i64::try_from(u64_of("exit_store_unreachable")).expect("fits")
    );
    assert_eq!(
        i64::from(EXIT_USAGE),
        i64::try_from(u64_of("exit_usage")).expect("fits")
    );
}

#[test]
fn verify_defaults_equal_the_contract() {
    assert!((DEFAULT_TOLERANCE_ABS - f64_of("verify_default_tolerance_abs")).abs() < f64::EPSILON);
    assert_eq!(u64::from(DEFAULT_ROWS), u64_of("verify_default_rows"));
    // The ceiling is enforced inside config::parse; its literal there must match the file.
    let with_tolerance = |t: f64| {
        format!(
            "{}\n[verify]\ntolerance_abs = {t:e}\n",
            super::cases::staging_config()
        )
    };
    let ceiling = f64_of("verify_tolerance_abs_max");
    let over = crate::config::parse(&with_tolerance(ceiling * 2.0), super::cases::SERVER_ID);
    assert!(over.is_err(), "twice the ceiling must refuse");
    let at = crate::config::parse(&with_tolerance(ceiling), super::cases::SERVER_ID);
    assert!(at.is_ok(), "the ceiling itself is admitted");
}
