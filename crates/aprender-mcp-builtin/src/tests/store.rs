//! The object store the precheck and `verify` read through (design §5.4): HEAD, bounded GET,
//! create-only PUT; an in-memory stand-in and the S3 error classification.

use crate::store::{classify_s3_error, MemStore, ObjectStore, PutOutcome, SdkOutcome};

#[tokio::test]
async fn mem_store_head_get_and_declared_size() {
    let s = MemStore::default();
    s.put("s3://b/k", b"abc".to_vec());
    assert_eq!(s.head("s3://b/k").await.expect("ok"), Some(3));
    assert_eq!(
        s.get("s3://b/k", 10).await.expect("ok"),
        Some(b"abc".to_vec())
    );
    assert_eq!(
        s.get("s3://b/k", 2).await.expect("ok"),
        None,
        "over the cap reads as absent"
    );
    s.declare_size("s3://b/k", 1_342_177_281);
    assert_eq!(s.head("s3://b/k").await.expect("ok"), Some(1_342_177_281));
    assert_eq!(s.head("s3://b/none").await.expect("ok"), None);
}

#[tokio::test]
async fn mem_store_failures_look_like_network_errors() {
    let s = MemStore::default();
    s.fail("s3://b/k");
    let e = s.head("s3://b/k").await.expect_err("fails");
    assert!(e.0.starts_with("HeadObject s3://b/k: "), "{}", e.0);
    let e = s.get("s3://b/k", 10).await.expect_err("fails");
    assert!(e.0.starts_with("GetObject s3://b/k: "), "{}", e.0);
}

#[tokio::test]
async fn put_if_absent_is_create_only() {
    let s = MemStore::default();
    assert!(matches!(
        s.put_if_absent("s3://b/p/x.json", b"1".to_vec())
            .await
            .expect("ok"),
        PutOutcome::Written
    ));
    assert!(matches!(
        s.put_if_absent("s3://b/p/x.json", b"2".to_vec())
            .await
            .expect("ok"),
        PutOutcome::AlreadyExists
    ));
    assert_eq!(s.get_sync("s3://b/p/x.json"), Some(b"1".to_vec()));
    assert_eq!(
        s.keys_under("s3://b/p/"),
        vec!["s3://b/p/x.json".to_string()]
    );
}

#[test]
fn classify_s3_error_maps_404_and_codes_to_absent() {
    for (status, code) in [
        (Some(404), None),
        (None, Some("NotFound")),
        (None, Some("NoSuchKey")),
    ] {
        assert!(matches!(
            classify_s3_error("HeadObject", "s3://b/k", status, code, "x"),
            SdkOutcome::Absent
        ));
    }
}

#[test]
fn classify_s3_error_maps_403_to_failure_naming_both_causes() {
    let SdkOutcome::Failure(d) =
        classify_s3_error("HeadObject", "s3://b/k", Some(403), None, "service error")
    else {
        panic!("403 is a failure")
    };
    assert_eq!(
        d,
        "HeadObject s3://b/k: 403 Forbidden (the object is missing, or the builder role cannot read it)"
    );
}

#[test]
fn classify_s3_error_other_failures_carry_the_display() {
    let SdkOutcome::Failure(d) = classify_s3_error(
        "GetObject",
        "s3://b/k",
        Some(500),
        None,
        "dispatch failure: timeout",
    ) else {
        panic!("500 is a failure")
    };
    assert_eq!(d, "GetObject s3://b/k: dispatch failure: timeout");
}
