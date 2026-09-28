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

/// The tiny fixture's COMMITTED golden sha256 (aprender-decide FALSIFY-DECIDE-APR-002): an
/// identity oracle independent of the hash function under test, so the transport's identity
/// assertions never compare that function with itself.
pub(crate) fn tiny_golden_sha256() -> &'static str {
    static GOLDEN: OnceLock<String> = OnceLock::new();
    GOLDEN.get_or_init(|| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aprender-decide/tests/fixtures/laya_tiny.apr.sha256");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
            .trim()
            .to_string()
    })
}

/// The identity oracle is the committed golden, and the crate's re-exported hash agrees with
/// it on the packed tiny fixture.
#[test]
fn tiny_identity_oracle_is_the_committed_golden() {
    assert_eq!(tiny_golden_sha256().len(), 64);
    assert_eq!(sha256_hex(tiny_bytes()), tiny_golden_sha256());
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

/// WR-06: a loopback server that answered and then ended reaches `exit(1)` through the
/// watcher the bootstrap spawns — on a REAL loopback server, not a synthetic handle.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap()
async fn loopback_end_exits_the_process() {
    let server = build_server(tiny_model(), "decide-watch-test", "0.0.0").expect("build server");
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");
    let (bound, handle) = start_loopback(server, addr).await.expect("bind loopback");

    // The server is live: a POST initialize is answered.
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": crate::probe::PROBE_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "watch-test", "version": "0.0.0" }
        }
    });
    let resp = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client")
        .post(format!("http://{bound}/"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(init.to_string())
        .send()
        .await
        .expect("initialize is answered");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "the loopback answered initialize"
    );

    // The server task ends; the watcher must reach exit(1).
    handle.abort();
    let code = Arc::new(std::sync::Mutex::new(None::<i32>));
    let seen = Arc::clone(&code);
    watch_loopback(handle, move |c| {
        *seen.lock().expect("exit-code slot") = Some(c);
    })
    .await;
    assert_eq!(*code.lock().expect("exit-code slot"), Some(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loopback_identity_probe_over_real_http() {
    let model = tiny_model();
    let expected = tiny_golden_sha256().to_string();
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
    assert_eq!(sample.artifact_sha256, tiny_golden_sha256());
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
    let pin = Sha256Pin::parse(tiny_golden_sha256()).expect("pin");
    let (model, timeline) = resolve_local(&path, Some(&pin), contracted_cap())
        .await
        .expect("resolve");
    assert_eq!(model.identity().artifact_sha256, pin.as_str());
    assert_eq!(timeline.source, "local");
    assert_eq!(timeline.bytes, tiny_bytes().len() as u64);
    assert_eq!(timeline.artifact_sha256, pin.as_str());
}

// ---------------------------------------------------------------- the deployed tier

/// The one line `<indent><key><sep><integer>` of `text`, parsed. Panics naming the key and
/// the file when it is absent or appears more than once, so a renamed key fails loudly.
fn single_integer(text: &str, key: &str, sep: &str, file: &str) -> u64 {
    let hits: Vec<u64> = text
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix(key)?.strip_prefix(sep))
        .map(|v| {
            v.trim()
                .parse()
                .unwrap_or_else(|e| panic!("{file}: {key} is not an integer ({e})"))
        })
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "{file}: expected exactly one `{key}{sep}` line"
    );
    hits[0]
}

/// FALSIFY-DECIDE-TOOL-007 (tier): the memory the deploy template gives the function is
/// the memory decide-tool-boundary-v1 priced its token budget for. Lambda sells CPU in
/// proportion to memory, so any other size silently re-prices every request.
#[test]
fn deploy_memory_is_the_contract_tier() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let contract_path = root.join("../../contracts/decide-tool-boundary-v1.yaml");
    let template_path = root.join(".pmcp/deploy.toml.template");
    let contract = std::fs::read_to_string(&contract_path).expect("read the contract");
    let template = std::fs::read_to_string(&template_path).expect("read the deploy template");
    let tier = single_integer(
        &contract,
        "lambda_memory_mb",
        ": ",
        "decide-tool-boundary-v1",
    );
    let memory = single_integer(&template, "memory_mb", " = ", "deploy.toml.template");
    assert_eq!(
        memory, tier,
        "deploy.toml.template memory_mb {memory} != decide-tool-boundary-v1 lambda_memory_mb {tier}: \
         re-derive the budget for the new tier in the contract first"
    );
}

// ------------------------------------------------ the bootstrap's pure decisions (08-24)

/// V4-c: only a POST may trigger the cold model load; GET and OPTIONS are answered without
/// it, and every other method is a 405 without it.
#[test]
fn non_post_methods_do_not_load() {
    assert_eq!(route("POST"), Route::Load);
    assert_eq!(route("GET"), Route::Health);
    assert_eq!(route("OPTIONS"), Route::Preflight);
    for method in [
        "HEAD", "PUT", "DELETE", "PATCH", "TRACE", "CONNECT", "post", "",
    ] {
        assert_eq!(route(method), Route::MethodNotAllowed, "{method:?}");
    }
    assert_eq!(ALLOWED_METHODS, "POST, GET, OPTIONS");
}

/// A4-6: the health body says ok only when the model-source config parses; a broken or
/// missing config is a 503 naming the config error (never the pin's value).
#[test]
fn health_is_not_ok_on_invalid_config() {
    let sha = "a".repeat(64);
    let good_s3 = ModelSource::from_lookup(lookup(&[
        (ENV_S3_URI, "s3://bucket/decide/abc.apr"),
        (ENV_SHA256, &sha),
    ]));
    let good_local = ModelSource::from_lookup(lookup(&[(ENV_MODEL, "/m.apr")]));
    for (source, loaded) in [(&good_s3, false), (&good_local, true)] {
        let (status, body) = health(source, loaded);
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["ok"], true, "{body}");
        assert_eq!(body["loaded"], loaded, "{body}");
        assert_eq!(body["package"], PACKAGE, "{body}");
        assert!(body.get("config_error").is_none(), "{body}");
    }

    let secret_ish = "Z".repeat(64);
    let cases = [
        (ModelSource::from_lookup(lookup(&[])), SourceError::Missing),
        (
            ModelSource::from_lookup(lookup(&[(ENV_S3_URI, "s3://b/k"), (ENV_MODEL, "/m")])),
            SourceError::Ambiguous,
        ),
        (
            ModelSource::from_lookup(lookup(&[(ENV_S3_URI, "s3://b/k")])),
            SourceError::MissingSha256,
        ),
        (
            ModelSource::from_lookup(lookup(&[(ENV_MODEL, "/m"), (ENV_SHA256, &secret_ish)])),
            SourceError::BadSha256 { len: 64 },
        ),
    ];
    for (source, expected) in &cases {
        assert_eq!(source.as_ref().err(), Some(expected));
        let (status, body) = health(source, false);
        assert_eq!(status, 503, "{expected:?}: {body}");
        assert_eq!(body["ok"], false, "{expected:?}: {body}");
        assert_eq!(body["loaded"], false, "{expected:?}: {body}");
        assert_eq!(
            body["config_error"],
            expected.to_string(),
            "{expected:?}: {body}"
        );
        assert!(!body.to_string().contains(&secret_ish), "{body}");
    }
}

/// IN-06: a proxied response carries exactly one access-control-allow-origin, whatever
/// the upstream sent; framing headers are not forwarded either.
#[test]
fn proxied_response_has_one_cors_origin() {
    use reqwest::header::{HeaderMap, HeaderValue};
    let mut upstream = HeaderMap::new();
    upstream.insert(
        "access-control-allow-origin",
        HeaderValue::from_static("https://evil.example"),
    );
    upstream.insert(
        "access-control-allow-credentials",
        HeaderValue::from_static("true"),
    );
    upstream.insert("content-type", HeaderValue::from_static("application/json"));
    upstream.insert("content-length", HeaderValue::from_static("12"));
    upstream.insert("transfer-encoding", HeaderValue::from_static("chunked"));
    upstream.insert("mcp-session-id", HeaderValue::from_static("s-1"));

    let mut builder = lambda_http::Response::builder().status(200);
    for (name, value) in proxied_headers(&upstream, load_header_value(false, 0)) {
        builder = builder.header(name, value);
    }
    let response = builder.body(lambda_http::Body::Empty).expect("a response");
    let headers = response.headers();
    let origins: Vec<_> = headers
        .get_all("access-control-allow-origin")
        .iter()
        .collect();
    assert_eq!(origins, ["*"], "exactly the bootstrap's own origin");
    assert!(headers.get("access-control-allow-credentials").is_none());
    assert!(headers.get("content-length").is_none());
    assert!(headers.get("transfer-encoding").is_none());
    assert_eq!(headers.get_all(LOAD_HEADER).iter().count(), 1);
    assert_eq!(
        headers.get("content-type").map(|v| v.as_bytes()),
        Some(&b"application/json"[..])
    );
    assert_eq!(
        headers.get("mcp-session-id").map(|v| v.as_bytes()),
        Some(&b"s-1"[..])
    );
}

/// V3-d: a REAL local file one byte over a shrunk cap is refused as too_large on its
/// declared length — before any byte is read (the only path a static file can reach).
#[tokio::test]
async fn local_over_cap_is_too_large() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("over.apr");
    let cap = tiny_bytes().len() as u64 - 1;
    std::fs::write(&path, tiny_bytes()).expect("write a real file");
    let err = resolve_local(&path, None, cap)
        .await
        .expect_err("one byte over the shrunk cap");
    assert_eq!(err.kind(), "too_large", "{err}");
    match err {
        ResolveError::TooLarge {
            what,
            observed,
            cap: c,
        } => assert_eq!((what, observed, c), ("declared_length", cap + 1, cap)),
        other => panic!("expected TooLarge, got {other:?}"),
    }
    // At the cap exactly, the same real file is served.
    let (model, _) = resolve_local(&path, None, cap + 1)
        .await
        .expect("at the cap");
    assert_eq!(model.identity().artifact_sha256, tiny_golden_sha256());
}

/// V3-d/R2: rung 1's own stream refusal (ArtifactTooLarge) surfaces as too_large, not as
/// a generic read failure; every other bounded-read error stays `read`. Proven on the
/// mapping, because a static file cannot reach the stream check once its declared length
/// passed.
#[test]
fn artifact_too_large_maps_to_too_large() {
    let cap = contracted_cap();
    let err = map_bounded_read_error(aprender_decide::ArtifactError::ArtifactTooLarge {
        what: "stream",
        observed: cap + 1,
        cap,
    });
    assert_eq!(err.kind(), "too_large", "{err}");
    assert!(
        matches!(
            err,
            ResolveError::TooLarge { what: "stream", observed, cap: c } if observed == cap + 1 && c == cap
        ),
        "{err:?}"
    );
    let err = map_bounded_read_error(aprender_decide::ArtifactError::Read {
        reason: "disk gone".into(),
    });
    assert_eq!(err.kind(), "read", "{err}");
}
