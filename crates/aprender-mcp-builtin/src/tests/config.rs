//! The strict `config.toml` parser (design §3.2).

use super::cases::{prod_config, staging_config, A, B, PROD_ID, SERVER_ID};
use crate::config::{config_sha256, parse, Environment, PublishKind};

#[test]
fn parses_the_staging_example() {
    let c = parse(&staging_config(), SERVER_ID).expect("parses");
    assert_eq!(c.environment, Environment::Staging);
    assert_eq!(c.publish.kind, PublishKind::Promote);
    assert_eq!(c.tolerance_abs(), 5.0e-5);
    assert_eq!(c.default_rows(), 8);
    assert_eq!(c.verify_role(), "admin");
}

#[test]
fn parses_the_prod_rollback_example() {
    let c = parse(&prod_config(A, "rollback", B), PROD_ID).expect("parses");
    assert!(c.is_prod());
    assert_eq!(c.publish.kind, PublishKind::Rollback);
    assert_eq!(c.tools.verify_allowed_principals, vec!["3k9f2m2mclientid"]);
}

#[test]
fn unknown_key_names_the_field() {
    let e = parse(
        &staging_config().replace("[model]\n", "[model]\nmemory_mb = 10240\n"),
        SERVER_ID,
    )
    .expect_err("refused");
    assert!(e.to_string().starts_with("config-invalid: "), "{e}");
    assert!(e.to_string().contains("memory_mb"), "{e}");
}

#[test]
fn unknown_section_is_refused() {
    assert!(parse(
        &(staging_config() + "\n[bounds]\nmax_texts = 4\n"),
        SERVER_ID
    )
    .is_err());
}

#[test]
fn id_must_be_lowercase_and_match_expected() {
    let upper = staging_config().replace(
        "id = \"antisemitism-staging\"",
        "id = \"Antisemitism-staging\"",
    );
    assert_eq!(
        parse(&upper, "Antisemitism-staging")
            .expect_err("refused")
            .key,
        "id"
    );
    assert_eq!(
        parse(&staging_config(), "antisemitism-other")
            .expect_err("refused")
            .key,
        "id"
    );
}

#[test]
fn every_named_rule_names_its_key() {
    for (edit, key) in [
        (("aprender-server/v1", "aprender-server/v2"), "schema"),
        (
            ("environment = \"staging\"", "environment = \"test\""),
            "environment",
        ),
        (("kind = \"decide\"", "kind = \"setfit\""), "[model].kind"),
    ] {
        let text = staging_config().replace(edit.0, edit.1);
        let e = parse(&text, SERVER_ID).expect_err(key);
        assert!(e.to_string().contains(key), "{e} should name {key}");
    }
    let e = parse(
        &staging_config().replace(
            &format!("artifact_sha256 = \"{A}\""),
            &format!("artifact_sha256 = \"{}\"", A.to_uppercase()),
        ),
        SERVER_ID,
    )
    .expect_err("sha");
    assert_eq!(e.key, "[model].artifact_sha256");
}

#[test]
fn prod_requires_baseline() {
    let e = parse(
        &prod_config("first_model", "promote", A)
            .replace("baseline_sha256 = \"first_model\"\n", ""),
        PROD_ID,
    )
    .expect_err("refused");
    assert_eq!(e.key, "[publish].baseline_sha256");
}

#[test]
fn verify_bounds_are_checked() {
    assert_eq!(
        parse(
            &(staging_config() + "\n[verify]\ntolerance_abs = 2.0e-3\n"),
            SERVER_ID
        )
        .expect_err("tol")
        .key,
        "[verify].tolerance_abs"
    );
    assert_eq!(
        parse(
            &(staging_config() + "\n[verify]\ndefault_rows = 9\n"),
            SERVER_ID
        )
        .expect_err("rows")
        .key,
        "[verify].default_rows"
    );
}

#[test]
fn verification_prefix_needs_trailing_slash() {
    let e = parse(
        &staging_config().replace("verifications/\"", "verifications\""),
        SERVER_ID,
    )
    .expect_err("refused");
    assert_eq!(e.key, "[records].verification_prefix");
}

#[test]
fn config_sha256_is_the_sha_of_the_bytes() {
    assert_eq!(
        config_sha256("x"),
        aprender_mcp_decide_lambda::sha256_hex(b"x")
    );
}

// ---------------------------------------------------------------------------
// Final-review fixes (Important #1–#3): the key a missing field or section names, a
// one-line detail for a multi-line serde message, and the v1-unsupported noise record.
// ---------------------------------------------------------------------------

#[test]
fn missing_top_level_field_or_section_names_it() {
    let no_id = staging_config().replace("id = \"antisemitism-staging\"\n", "");
    let e = parse(&no_id, SERVER_ID).expect_err("refused");
    assert_eq!(e.key, "id", "{e}");
    let start = staging_config().find("[model]").expect("model section");
    let end = staging_config().find("[records]").expect("records section");
    let no_model = format!("{}{}", &staging_config()[..start], &staging_config()[end..]);
    let e = parse(&no_model, SERVER_ID).expect_err("refused");
    assert_eq!(e.key, "model", "{e}");
    let no_reason = staging_config().replace(
        "reason = \"Phase 16 SageMaker control model; staging for the next experiment.\"\n",
        "",
    );
    let e = parse(&no_reason, SERVER_ID).expect_err("refused");
    assert_eq!(e.key, "[publish].reason", "{e}");
}

#[test]
fn serde_detail_is_always_one_line() {
    let broken = staging_config().replace(
        "reason = \"Phase 16 SageMaker control model; staging for the next experiment.\"",
        "reason = r",
    );
    let e = parse(&broken, SERVER_ID).expect_err("refused");
    assert!(!e.to_string().contains('\n'), "{e:?}");
    assert!(e.to_string().starts_with("config-invalid: "), "{e}");
}

#[test]
fn rescore_noise_uri_is_refused_in_v1() {
    let with_noise = staging_config().replace(
        "[records]\n",
        "[records]\nrescore_noise_uri = \"s3://b/rescore-noise.json\"\n",
    );
    let e = parse(&with_noise, SERVER_ID).expect_err("refused");
    assert_eq!(e.key, "[records].rescore_noise_uri", "{e}");
    assert!(e.detail.contains("v1"), "{e}");
}
