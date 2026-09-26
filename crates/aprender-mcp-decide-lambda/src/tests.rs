//! Transport tests: the stateless loopback server on the tiny fixture, probed over
//! real HTTP on 127.0.0.1 (the Lambda path minus the Lambda event wrapper), plus the
//! pure pieces of the cold-start evidence and the source parsing.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use super::*;
use crate::probe::{new_probe_id, run_cold_first, run_identity_probe};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../aprender-decide/tests/fixtures/laya_tiny")
}

/// The tiny Laya fixture, packed by the production packer (bytes, once per binary).
pub(crate) fn tiny_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        let dir = fixture_dir();
        aprender_decide::pack_run_dir(&dir, &dir.join("data")).expect("pack the tiny fixture")
    })
}

pub(crate) fn tiny_model() -> Arc<Model> {
    static MODEL: OnceLock<Arc<Model>> = OnceLock::new();
    Arc::clone(MODEL.get_or_init(|| {
        Arc::new(
            aprender_mcp_decide::load_model_from_bytes(tiny_bytes())
                .expect("the ladder accepts the tiny fixture"),
        )
    }))
}

/// A FRESH stateless loopback server over `model`, with the SAME config the bootstrap
/// uses ([`server_config`] via [`start_loopback`]).
pub(crate) async fn serve(model: Arc<Model>) -> String {
    let server = build_server(model, "decide-loopback-test", "0.0.0").expect("build server");
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");
    let (bound, _handle) = start_loopback(server, addr).await.expect("bind loopback");
    format!("http://{bound}")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_identity_probe_over_real_http() {
    let model = tiny_model();
    let expected = sha256_hex(tiny_bytes());
    assert_eq!(model.identity().artifact_sha256, expected);
    let labels = model.task().owned_labels();
    assert_eq!(labels, ["shipping", "billing", "account"]);

    let url = serve(Arc::clone(&model)).await;
    let id = new_probe_id();
    let report = run_identity_probe(&url, None, &expected, &labels, &id)
        .await
        .expect("identity probe completes");

    assert!(report.one_tool_named_classify, "{report:?}");
    assert!(report.description_has_labels_in_order, "{report:?}");
    assert!(report.identity_matches, "{report:?}");
    assert_eq!(report.artifact_sha256, expected);
    assert_eq!(report.labels, labels);
    assert!(report.tokens_total > 0);
    assert_eq!(report.probe_id, id);
    assert_eq!(report.calls.len(), 3);

    // A wrong expectation is reported, not hidden.
    let wrong = "0".repeat(64);
    let report = run_identity_probe(&url, None, &wrong, &labels, &id)
        .await
        .expect("probe completes");
    assert!(!report.identity_matches);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_cold_first_call_is_served_without_initialize() {
    // A FRESH server that has never seen an initialize: the shape a cold container
    // behind the gateway receives when the client's initialize landed elsewhere.
    let model = tiny_model();
    let url = serve(Arc::clone(&model)).await;
    let texts = vec![
        "My parcel never arrived.".to_string(),
        "I was charged twice this month.".to_string(),
    ];
    let sample = run_cold_first(&url, None, &texts, "cold-first-test-1")
        .await
        .expect("a tools/call with no preceding initialize is served");
    assert_eq!(sample.artifact_sha256, sha256_hex(tiny_bytes()));
    assert_eq!(sample.texts, 2);
    assert!(sample.tokens_total > 0);
    assert_eq!(sample.probe_id, "cold-first-test-1");
    // The loopback server itself never sets the load header (the bootstrap does).
    assert_eq!(sample.load_header, None);
}

/// The bootstrap's handler awaits `resolve_model`, and lambda_http requires that future
/// to be `Send` for every lifetime. `cargo test --lib` never builds the bin, so this
/// pins the property where the lib tests run.
#[test]
fn resolve_model_future_is_send() {
    fn assert_send<T: Send>(_: &T) {}
    let source = ModelSource::S3 {
        bucket: "b".into(),
        key: "k".into(),
        sha256: Sha256Pin::parse(&"a".repeat(64)).expect("pin"),
    };
    let future = resolve_model(&source);
    assert_send(&future);
    drop(future);
}

#[test]
fn server_config_is_stateless() {
    let config = server_config();
    assert!(config.session_id_generator.is_none());
    assert!(config.enable_json_response);
}

#[test]
fn probe_id_accepts_only_short_safe_ids() {
    assert_eq!(parse_probe_id(Some("abc-123-XYZ")), Some("abc-123-XYZ"));
    assert_eq!(
        parse_probe_id(Some(&"a".repeat(64))).map(str::len),
        Some(64)
    );
    assert_eq!(parse_probe_id(Some(&"a".repeat(65))), None);
    assert_eq!(parse_probe_id(Some("")), None);
    assert_eq!(parse_probe_id(Some("a b")), None);
    assert_eq!(parse_probe_id(Some("x\nperformed_load=true")), None);
    assert_eq!(parse_probe_id(Some("id=1")), None);
    assert_eq!(parse_probe_id(None), None);
    // Every generated id passes the server's filter.
    let id = new_probe_id();
    assert_eq!(parse_probe_id(Some(&id)), Some(id.as_str()));
}

#[test]
fn load_evidence_names_the_loading_request() {
    assert_eq!(load_header_value(true, 1234), "cold;load_ms=1234");
    assert_eq!(load_header_value(false, 1234), "warm");
    let timeline = LoadTimeline {
        source: "s3",
        bytes: 10,
        fetch_ms: 5,
        sha_ms: 1,
        build_ms: 7,
        artifact_sha256: "ab".repeat(32),
        rss_mb: None,
        peak_rss_mb: Some(2048.4),
        cpu_part: Some("0xd40".to_string()),
        graviton: graviton_generation(Some("0xd40")),
    };
    let line = load_log_line(true, Some("p-1"), 13, Some(&timeline));
    assert!(
        line.starts_with("decide.load performed_load=true probe_id=p-1 load_ms=13 download_ms=5"),
        "{line}"
    );
    assert!(line.contains("build_ms=7"), "{line}");
    assert!(line.contains("graviton=graviton3"), "{line}");
    assert!(line.contains("rss_mb=na peak_rss_mb=2048"), "{line}");
    assert_eq!(
        load_log_line(false, None, 0, None),
        "decide.load performed_load=false probe_id=none"
    );
    assert_eq!(graviton_generation(Some("0xd0c")), "graviton2");
    assert_eq!(graviton_generation(Some("0xd4f")), "graviton4");
    assert_eq!(graviton_generation(None), "unknown");
}

fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| (*v).to_string())
    }
}

#[test]
fn source_from_env_parses_both_kinds_and_refuses_the_rest() {
    let sha = "a".repeat(64);
    let s3 = ModelSource::from_lookup(lookup(&[
        (ENV_S3_URI, "s3://bucket/decide/srv/abc.apr"),
        (ENV_SHA256, &sha),
    ]))
    .expect("s3 source");
    assert_eq!(
        s3,
        ModelSource::S3 {
            bucket: "bucket".into(),
            key: "decide/srv/abc.apr".into(),
            sha256: Sha256Pin::parse(&sha).expect("pin"),
        }
    );
    let local = ModelSource::from_lookup(lookup(&[(ENV_MODEL, "/m.apr")])).expect("local");
    assert_eq!(
        local,
        ModelSource::Local {
            path: "/m.apr".into(),
            sha256: None
        }
    );
    assert_eq!(
        ModelSource::from_lookup(lookup(&[])),
        Err(SourceError::Missing)
    );
    assert_eq!(
        ModelSource::from_lookup(lookup(&[(ENV_S3_URI, "s3://b/k")])),
        Err(SourceError::MissingSha256)
    );
    assert_eq!(
        ModelSource::from_lookup(lookup(&[(ENV_S3_URI, "s3://b/k"), (ENV_MODEL, "/m")])),
        Err(SourceError::Ambiguous)
    );
    let upper = "A".repeat(64);
    assert_eq!(
        ModelSource::from_lookup(lookup(&[(ENV_MODEL, "/m"), (ENV_SHA256, &upper)])),
        Err(SourceError::BadSha256 { len: 64 })
    );
    assert_eq!(
        ModelSource::from_lookup(lookup(&[(ENV_MODEL, "/m"), (ENV_SHA256, "abc")])),
        Err(SourceError::BadSha256 { len: 3 })
    );
}

#[tokio::test]
async fn local_source_resolves_the_tiny_fixture_with_its_pin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("tiny.apr");
    std::fs::write(&path, tiny_bytes()).expect("write");
    let pin = Sha256Pin::parse(&sha256_hex(tiny_bytes())).expect("pin");
    let (model, timeline) = resolve_local(&path, Some(&pin), contracted_cap())
        .await
        .expect("resolve");
    assert_eq!(model.identity().artifact_sha256, pin.as_str());
    assert_eq!(timeline.source, "local");
    assert_eq!(timeline.bytes, tiny_bytes().len() as u64);
    assert_eq!(timeline.artifact_sha256, pin.as_str());
}
