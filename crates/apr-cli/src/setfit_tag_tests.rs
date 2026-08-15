//! Tests for the typed-tag detector (D-04's negative, stated as tests).

use super::test_support::write_setfit_shaped_apr;
use super::*;
use tempfile::TempDir;

/// This module's subject, for the source assertions below.
const SETFIT_TAG_SOURCE: &str = include_str!("setfit_tag.rs");

/// Assemble a needle at RUNTIME so it cannot match the scan's own source.
fn needle(fragments: &[&str]) -> String {
    fragments.concat()
}

#[test]
fn setfit_tag_recognizes_a_tagged_artifact_and_returns_its_document() {
    let temp = TempDir::new().expect("tempdir");
    let path = write_setfit_shaped_apr(
        temp.path(),
        "tagged.apr",
        SETFIT_MODEL_TYPE,
        Some(r#"{"schema":"setfit-apr-v1","schema_version":1}"#),
    );

    let tag = read_setfit_tag(&path)
        .expect("a well-formed container is readable")
        .expect("a container tagged `setfit` must be recognized");
    let doc = tag.doc.expect("the fixture carries the one custom key");
    assert_eq!(
        doc.get("schema").and_then(serde_json::Value::as_str),
        Some("setfit-apr-v1"),
        "the detector must hand back the custom key's value verbatim"
    );
}

#[test]
fn setfit_tag_refuses_to_sniff_a_setfit_shaped_but_untagged_apr() {
    // D-04's NEGATIVE. This file carries `setfit.head.weight` and `setfit.head.bias`
    // and the artifact document at the very key a SetFit artifact uses. The ONLY
    // difference from the positive above is `model_type`. A detector that looked at
    // tensor names — or at the presence of the custom key — would say yes here.
    let temp = TempDir::new().expect("tempdir");
    let path = write_setfit_shaped_apr(
        temp.path(),
        "untagged.apr",
        "",
        Some(r#"{"schema":"setfit-apr-v1","schema_version":1}"#),
    );

    assert!(
        read_setfit_tag(&path)
            .expect("a well-formed container is readable")
            .is_none(),
        "an APR whose model_type is empty is a PLAIN APR, whatever its tensors are named"
    );
}

#[test]
fn setfit_tag_treats_a_differently_tagged_apr_as_plain() {
    let temp = TempDir::new().expect("tempdir");
    let path = write_setfit_shaped_apr(temp.path(), "qwen.apr", "qwen2", None);
    assert!(
        read_setfit_tag(&path)
            .expect("a well-formed container is readable")
            .is_none(),
        "only the exact tag routes to the SetFit path"
    );
}

#[test]
fn setfit_tag_returns_none_for_a_non_apr_file() {
    let temp = TempDir::new().expect("tempdir");
    let path = temp.path().join("notes.txt");
    std::fs::write(&path, b"this is not a model at all").expect("fixture is writable");
    assert!(
        read_setfit_tag(&path)
            .expect("a readable file is not an error")
            .is_none(),
        "a file with the wrong magic is not a SetFit artifact"
    );
}

#[test]
fn setfit_tag_reports_an_absent_path_and_a_directory_typed() {
    let temp = TempDir::new().expect("tempdir");
    let absent = read_setfit_tag(&temp.path().join("nope.apr"))
        .expect_err("an absent path cannot be classified");
    assert!(
        matches!(absent, CliError::FileNotFound(_)),
        "an absent path must be FileNotFound (exit 3); got: {absent}"
    );
    let dir = read_setfit_tag(temp.path()).expect_err("a directory cannot be classified");
    assert!(
        matches!(dir, CliError::NotAFile(_)),
        "a directory must be NotAFile (exit 3); got: {dir}"
    );
}

#[test]
fn setfit_tag_refuses_a_metadata_block_longer_than_its_own_file() {
    // The T-04-50 half this module owns. `metadata_size` is a u32 read out of the
    // file under judgement, so a hostile container can declare up to 4 GiB and make
    // a naive reader allocate it. The refusal must land BEFORE the allocation, which
    // is why it is expressed against the stat'd length.
    //
    // The hostile file is produced by TRUNCATING an honest one rather than by
    // patching a byte at a guessed header offset: truncation is layout-independent,
    // so this test cannot silently stop testing anything the day a header field
    // moves. (A first draft did patch offset 28 — the detector accepted the file, so
    // the test was measuring nothing.)
    let temp = TempDir::new().expect("tempdir");
    let honest = write_setfit_shaped_apr(temp.path(), "honest.apr", SETFIT_MODEL_TYPE, None);
    let bytes = std::fs::read(&honest).expect("the fixture is readable");
    assert!(
        read_setfit_tag(&honest)
            .expect("the honest fixture is readable")
            .is_some(),
        "non-vacuity: the UNtruncated fixture must be recognized, or this test proves nothing \
         about truncation"
    );

    // Long enough to hold the header, far too short to hold the metadata block the
    // header declares.
    let truncated_len = HEADER_SIZE_V2 + 8;
    assert!(
        bytes.len() > truncated_len,
        "the honest fixture must be longer than the truncation point"
    );
    let path = temp.path().join("truncated.apr");
    std::fs::write(&path, &bytes[..truncated_len]).expect("fixture is writable");

    let error = read_setfit_tag(&path)
        .expect_err("a metadata block that runs past the end of the file is malformed");
    assert!(
        matches!(error, CliError::InvalidFormat(_)),
        "an impossible metadata length is a format refusal (exit 4); got: {error}"
    );
    let rendered = error.to_string();
    assert!(
        rendered.contains(&truncated_len.to_string()),
        "the refusal must name the file length that bounds it; got: {rendered}"
    );
}

/// The CODE LINES of the production half of `setfit_tag.rs`.
///
/// Two filters, and each was earned by a real red:
///
/// * the file is truncated at the test-support boundary, because the fixture builder
///   there deliberately writes tensors NAMED `setfit.head.weight` — a whole-file scan
///   turns red on the very fixture that proves the detector ignores tensor names;
/// * comment lines are dropped, because the module header EXPLAINS the rule using the
///   forbidden names, and a guard that fails on its own documentation is the F-05
///   defect. The explanation is worth more than the names' absence from prose.
///
/// Both filters carry a non-vacuity assertion, because a filter that ate the module
/// would make every claim below trivially true.
fn production_code_lines() -> String {
    let boundary = needle(&["#[cfg(test)]\n", "pub(crate) mod test_support"]);
    let end = SETFIT_TAG_SOURCE
        .find(&boundary)
        .expect("the test-support boundary marker must exist; if it moved, fix this scan");
    let half = &SETFIT_TAG_SOURCE[..end];
    assert!(
        half.contains("pub(crate) fn read_setfit_tag"),
        "non-vacuity: the truncation must leave the function under scan in place"
    );
    let code: String = half
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        code.contains("AprV2Metadata::from_json"),
        "non-vacuity: the comment filter must not have eaten the module's code"
    );
    code
}

#[test]
fn setfit_tag_detection_never_consults_a_tensor_name() {
    // The source half of D-04. The behavioural half is
    // `setfit_tag_refuses_to_sniff_a_setfit_shaped_but_untagged_apr`; this one
    // catches a future edit that adds a name check the fixtures happen not to cover.
    let production = production_code_lines();
    let reader = needle(&["AprV2", "Reader"]);
    assert_eq!(
        production.matches(&reader).count(),
        0,
        "detection must not open the tensor index: the tag is in the metadata record"
    );
    for banned in ["tensor_names", "head.weight", "tokenizer.blob"] {
        assert!(
            !production.contains(banned),
            "detection must not name a tensor ({banned}) — D-04 is tag-only"
        );
    }
}
