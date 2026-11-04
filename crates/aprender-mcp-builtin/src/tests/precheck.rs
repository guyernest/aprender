//! The build-time check (design §5): the 45-case table, the output grammar, the platform's
//! environment inputs.

use super::cases;
use crate::precheck::{self, Env};
use crate::store::MemStore;

#[tokio::test]
async fn the_case_table_holds() {
    for case in cases::all() {
        let out = precheck::run(&case.config, case.expected_id, &case.env, &case.store).await;
        assert_eq!(
            out.exit,
            case.expect_exit,
            "{}: {}\n{}",
            case.name,
            case.what,
            out.render()
        );
        assert_eq!(out.code.as_deref(), case.expect_code, "{}", case.name);
        let rendered = out.render();
        let last = rendered.lines().last().expect("a verdict line");
        match case.expect_exit {
            0 => assert!(last.starts_with("ELIGIBLE {"), "{}: {last}", case.name),
            4 => assert!(
                last.starts_with("ERROR: REFUSED s3-unreachable: "),
                "{}: {last}",
                case.name
            ),
            _ => assert!(
                last.starts_with(&format!(
                    "ERROR: REFUSED {}: ",
                    case.expect_code.expect("code")
                )),
                "{}: {last}",
                case.name
            ),
        }
        assert!(!last.contains("ERROR: ERROR"), "{}", case.name);
    }
}

#[tokio::test]
async fn warns_precede_the_verdict_and_are_echoed_in_eligible() {
    let case = cases::all()
        .into_iter()
        .find(|c| c.name == "P5")
        .expect("P5");
    let out = precheck::run(&case.config, case.expected_id, &case.env, &case.store).await;
    assert_eq!(
        out.warns,
        vec!["rollback-unknown-sha skipped: no published-sha history was passed"]
    );
    let r = out.render();
    assert!(r.starts_with("WARN rollback-unknown-sha skipped: "));
    let json: serde_json::Value = serde_json::from_str(
        r.lines()
            .last()
            .expect("line")
            .trim_start_matches("ELIGIBLE "),
    )
    .expect("json");
    assert_eq!(
        json["warns"][0],
        "rollback-unknown-sha skipped: no published-sha history was passed"
    );
    assert_eq!(json["publish_kind"], "rollback");
}

#[tokio::test]
async fn no_output_line_contains_a_record_body() {
    let case = cases::all()
        .into_iter()
        .find(|c| c.name == "P3")
        .expect("P3");
    let r = precheck::run(&case.config, case.expected_id, &case.env, &case.store)
        .await
        .render();
    // One field each that only the record BODIES carry (the summary names verified_by on purpose).
    assert!(
        !r.contains("PACKED waived=none")
            && !r.contains("difference_interval_90")
            && !r.contains("tolerance_abs"),
        "{r}"
    );
    let json: serde_json::Value = serde_json::from_str(
        r.lines()
            .last()
            .expect("line")
            .trim_start_matches("ELIGIBLE "),
    )
    .expect("json");
    assert_eq!(json["comparison"]["verdict"], "clearly_better");
    assert_eq!(json["staging_verification"]["max_abs_diff"], 2.03e-6);
    assert_eq!(
        json["eval_jsonl_sha256"],
        aprender_mcp_decide_lambda::sha256_hex(cases::EVAL_JSONL.as_bytes())
    );
}

#[test]
fn env_from_lookup_treats_empty_as_absent() {
    let e = Env::from_lookup(|k| match k {
        "APRENDER_PREVIOUS_ARTIFACT_SHA256" => Some(String::new()),
        "APRENDER_PUBLISHED_SHA256_HISTORY" => Some(" ".into()),
        "APRENDER_BUILD_ID" => Some("codebuild:1".into()),
        _ => None,
    });
    assert_eq!(e.previous_artifact_sha256, None);
    assert_eq!(e.published_sha256_history, None);
    assert_eq!(e.build_id.as_deref(), Some("codebuild:1"));
    let e = Env::from_lookup(|k| {
        (k == "APRENDER_PUBLISHED_SHA256_HISTORY").then(|| format!("{},{}", cases::B, cases::A))
    });
    assert_eq!(
        e.published_sha256_history,
        Some(vec![cases::B.to_string(), cases::A.to_string()])
    );
}

#[test]
fn config_invalid_line_names_the_key() {
    let out = futures::executor::block_on(precheck::run(
        &cases::staging_config().replace("[model]\n", "[model]\nmemory_mb = 10240\n"),
        cases::SERVER_ID,
        &Env::default(),
        &MemStore::default(),
    ));
    assert_eq!(out.exit, 3);
    assert!(
        out.last_line.starts_with("ERROR: REFUSED config-invalid: ")
            && out.last_line.contains("memory_mb"),
        "{}",
        out.last_line
    );
}

#[test]
fn a_multi_line_serde_message_still_renders_one_verdict_line() {
    let broken = cases::staging_config().replace(
        "reason = \"Phase 16 SageMaker control model; staging for the next experiment.\"",
        "reason = r",
    );
    let out = futures::executor::block_on(precheck::run(
        &broken,
        cases::SERVER_ID,
        &Env::default(),
        &MemStore::default(),
    ));
    assert_eq!(out.exit, 3);
    let r = out.render();
    assert_eq!(r.lines().count(), 1, "{r}");
    assert!(r.starts_with("ERROR: REFUSED config-invalid: "), "{r}");
}
