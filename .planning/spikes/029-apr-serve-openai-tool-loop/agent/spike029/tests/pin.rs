//! Version-pin tripwire (D-05): the scaffolded `pmcp-agent` dependency must
//! stay pinned to the major.minor line this project was generated against. If a
//! future `cargo pmcp agent new` bumps the pin, update the expectation here
//! deliberately — a silent drift is a defect.

/// The major.minor line this project was generated against.
const EXPECTED_LINE: &str = "0.4";

#[test]
fn pmcp_agent_pin_matches_generated_major_minor() {
    let manifest = include_str!("../Cargo.toml");
    let parsed: toml::Value = toml::from_str(manifest).expect("parse Cargo.toml");
    let dep = parsed
        .get("dependencies")
        .and_then(|d| d.get("pmcp-agent"))
        .expect("Cargo.toml has [dependencies] pmcp-agent");
    // The dependency is a table: { version = "0.1.0", features = [...] }.
    let version = dep
        .get("version")
        .and_then(toml::Value::as_str)
        .expect("pmcp-agent dependency declares a version string");
    assert!(
        version.starts_with(EXPECTED_LINE),
        "scaffolded pmcp-agent pin `{version}` drifted from the generated `{EXPECTED_LINE}` line"
    );
}
