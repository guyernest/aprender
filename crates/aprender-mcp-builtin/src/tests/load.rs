//! The once-per-process model load from the configuration (design §4.1).

use super::{cases, fixture};
use crate::config;
use crate::load::{load, source_from_config, ModelCell};
use aprender_mcp_decide_lambda::ModelSource;

#[tokio::test]
async fn loads_the_tiny_fixture_once() {
    let (_d, path, sha) = fixture::tiny_apr();
    let cell: &'static ModelCell = Box::leak(Box::new(ModelCell::new()));
    assert!(cell.get().is_none());
    let (first, performed) = cell
        .get_or_load(
            &fixture::local_source(&path, &sha),
            Some(&fixture::tiny_labels()),
        )
        .await
        .expect("loads");
    assert!(performed);
    assert_eq!(first.labels, fixture::tiny_labels());
    assert_eq!(first.timeline.artifact_sha256, sha);
    let (_again, performed) = cell
        .get_or_load(&fixture::local_source(&path, &sha), None)
        .await
        .expect("cached");
    assert!(!performed);
}

#[tokio::test]
async fn labels_mismatch_refuses_before_serving() {
    let (_d, path, sha) = fixture::tiny_apr();
    let wrong = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let e = load(&fixture::local_source(&path, &sha), Some(&wrong))
        .await
        .expect_err("refused");
    assert_eq!(e.kind, "labels_mismatch");
    assert!(e.detail.contains("shipping"), "{}", e.detail);
}

#[tokio::test]
async fn sha_mismatch_is_refused_before_the_ladder() {
    let (_d, path, _sha) = fixture::tiny_apr();
    let e = load(&fixture::local_source(&path, &"0".repeat(64)), None)
        .await
        .expect_err("refused");
    assert_eq!(e.kind, "hash_mismatch");
}

#[test]
fn source_from_config_maps_the_uri_and_pin() {
    let c = config::parse(&cases::staging_config(), cases::SERVER_ID).expect("parses");
    let s = source_from_config(&c.model).expect("source");
    assert!(matches!(s, ModelSource::S3 { .. }));
    assert_eq!(s.kind(), "s3");
}
