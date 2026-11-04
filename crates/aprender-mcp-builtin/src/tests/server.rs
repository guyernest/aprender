//! The three tools over the loopback: `classify` loads lazily, `status` never loads,
//! `tools/list` is answered from the configuration (design §4).

use std::path::Path;
use std::sync::Arc;

use serde_json::json;

use super::{cases, fixture, rpc};
use crate::config;
use crate::load::ModelCell;
use crate::server::{self, Runtime};
use crate::store::MemStore;
use crate::tools;
use aprender_mcp_decide::ClassifyLimits;

/// The staging example over the tiny fixture: its sha and its labels.
pub(super) fn tiny_text(sha: &str) -> String {
    cases::staging_config().replace(cases::A, sha).replace(
        "labels = [\"not_antisemitic\", \"antisemitic\"]",
        &format!("labels = {:?}", fixture::tiny_labels()),
    )
}

fn runtime_over(path: &Path, sha: &str, store: Arc<MemStore>) -> (Runtime, String) {
    runtime_over_text(&tiny_text(sha), path, sha, store)
}

pub(super) fn runtime_over_text(
    text: &str,
    path: &Path,
    sha: &str,
    store: Arc<MemStore>,
) -> (Runtime, String) {
    let config = config::parse(text, cases::SERVER_ID).expect("parses");
    let cell: &'static ModelCell = Box::leak(Box::new(ModelCell::new()));
    (
        Runtime {
            config: Arc::new(config),
            source: fixture::local_source(path, sha),
            store,
            cell,
            config_sha256: config::config_sha256(text),
            build_id: Some("codebuild:test".into()),
        },
        text.to_string(),
    )
}

fn text_of(result: &serde_json::Value) -> serde_json::Value {
    serde_json::from_str(
        result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("no text content: {result}")),
    )
    .expect("json text")
}

#[tokio::test]
async fn tools_list_describes_classify_from_config_without_loading() {
    let (_d, path, sha) = fixture::tiny_apr();
    let (rt, _) = runtime_over(&path, &sha, Arc::new(MemStore::default()));
    let (url, _h) = rpc::serve(
        server::build_with(rt.clone(), "antisemitism-staging", "0.0.0").expect("server"),
    )
    .await;
    let list = rpc::call(&url, "tools/list", json!({}), &[]).await;
    let tools = list["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert!(
        names.contains(&"classify") && names.contains(&"status"),
        "{names:?}"
    );
    let desc = tools
        .iter()
        .find(|t| t["name"] == "classify")
        .expect("classify")["description"]
        .as_str()
        .expect("desc")
        .to_string();
    assert!(
        desc.starts_with("Antisemitism decider, GoldStandard2024"),
        "{desc}"
    );
    assert!(
        desc.contains("1..8 texts") && desc.contains("800 built tokens"),
        "{desc}"
    );
    assert!(rt.cell.get().is_none(), "tools/list must not load");
}

#[tokio::test]
async fn status_reports_not_loaded_then_loaded_after_classify() {
    let (_d, path, sha) = fixture::tiny_apr();
    let (rt, text) = runtime_over(&path, &sha, Arc::new(MemStore::default()));
    let (url, _h) = rpc::serve(
        server::build_with(rt.clone(), "antisemitism-staging", "0.0.0").expect("server"),
    )
    .await;
    let s = text_of(&rpc::tool(&url, "status", json!({}), &[]).await);
    assert_eq!(s["schema"], "aprender-status/v1");
    assert_eq!(s["loaded"], false);
    assert!(s["load"].is_null());
    assert_eq!(s["bounds"]["max_texts"], 8);
    assert_eq!(s["build"]["config_sha256"], config::config_sha256(&text));
    assert_eq!(s["build"]["build_id"], "codebuild:test");
    assert!(s["identity"].is_null());
    let c = text_of(
        &rpc::tool(
            &url,
            "classify",
            json!({"texts": ["The box is two weeks late and nobody answers."]}),
            &[],
        )
        .await,
    );
    assert_eq!(c["model"]["artifact_sha256"], sha);
    assert_eq!(c["results"].as_array().expect("results").len(), 1);
    let s = text_of(&rpc::tool(&url, "status", json!({}), &[]).await);
    assert_eq!(s["loaded"], true);
    assert_eq!(s["load"]["artifact_sha256"], sha);
    assert_eq!(s["load"]["source"], "local");
}

#[tokio::test]
async fn status_echoes_the_proxy_identity() {
    let (_d, path, sha) = fixture::tiny_apr();
    let (rt, _) = runtime_over(&path, &sha, Arc::new(MemStore::default()));
    let (url, _h) =
        rpc::serve(server::build_with(rt, "antisemitism-staging", "0.0.0").expect("server")).await;
    let s = text_of(
        &rpc::tool(
            &url,
            "status",
            json!({}),
            &[
                ("x-pmcp-user-id", "sub-1"),
                ("x-pmcp-user-groups", "admin,analysts"),
                ("x-pmcp-user-email", "a@example.org"),
            ],
        )
        .await,
    );
    assert_eq!(s["identity"]["subject"], "sub-1");
    assert_eq!(s["identity"]["groups"], json!(["admin", "analysts"]));
    assert_eq!(s["identity"]["email"], "a@example.org");
}

#[tokio::test]
async fn an_oversized_request_is_refused_before_the_load() {
    let (_d, path, sha) = fixture::tiny_apr();
    let (rt, _) = runtime_over(&path, &sha, Arc::new(MemStore::default()));
    let (url, _h) = rpc::serve(
        server::build_with(rt.clone(), "antisemitism-staging", "0.0.0").expect("server"),
    )
    .await;
    let r = rpc::tool(&url, "classify", json!({"texts": vec!["x"; 9]}), &[]).await;
    assert_eq!(r["isError"], true, "{r}");
    assert!(
        r["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains("classify_max_texts"),
        "{r}"
    );
    assert!(rt.cell.get().is_none(), "a refused request never loads");
}

#[test]
fn build_derives_the_s3_source_from_config() {
    let text = cases::staging_config();
    let c = config::parse(&text, cases::SERVER_ID).expect("parses");
    assert!(server::build(
        &text,
        c,
        "antisemitism-staging",
        "0.0.0",
        None,
        Arc::new(MemStore::default())
    )
    .is_ok());
}

// ---------------------------------------------------------------------------
// verify (Task 6)
// ---------------------------------------------------------------------------

const ADMIN: [(&str, &str); 3] = [
    ("x-pmcp-user-id", "sub-1"),
    ("x-pmcp-user-groups", "admin"),
    ("x-pmcp-user-email", "ops@example.org"),
];

/// The fixture's eval records at the URIs the configuration names.
fn store_with_tiny_records(text: &str) -> Arc<MemStore> {
    let s = MemStore::default();
    let dir = fixture::dir();
    let cfg = config::parse(text, cases::SERVER_ID).expect("parses");
    s.put(
        &cfg.records.eval_jsonl_uri,
        std::fs::read(dir.join("data/eval.jsonl")).expect("eval"),
    );
    s.put(
        &cfg.records.eval_probs_uri,
        std::fs::read(dir.join("eval-probs.json")).expect("probs"),
    );
    Arc::new(s)
}

#[tokio::test]
async fn verify_passes_on_the_tiny_fixture_and_writes_a_record_once() {
    let (_d, path, sha) = fixture::tiny_apr();
    let text = tiny_text(&sha);
    let store = store_with_tiny_records(&text);
    let (rt, _) = runtime_over_text(&text, &path, &sha, store.clone());
    let (url, _h) =
        rpc::serve(server::build_with(rt, "antisemitism-staging", "0.0.0").expect("server")).await;
    let r = rpc::tool(&url, "verify", json!({"rows": 3, "seed": 0}), &ADMIN).await;
    let v = text_of(&r);
    assert_eq!(v["record"]["schema"], "aprender-verification/v1");
    assert_eq!(v["record"]["pass"], true, "{v}");
    assert_eq!(v["record"]["rows_scored"], 3);
    assert_eq!(v["record"]["tolerance_abs"], 5.0e-5);
    assert_eq!(v["record"]["verified_by"], "ops@example.org");
    assert_eq!(v["record"]["artifact_sha256"], sha);
    assert_eq!(v["written"], true, "{v}");
    let uri = v["record_uri"].as_str().expect("uri").to_string();
    assert!(
        uri.starts_with(
            "s3://aprender-laya-train-dev-123456789012/runs/antisemitism-v2/task-01jbx/verifications/"
        ) && uri.ends_with(&format!("-{}.json", &sha[..12])),
        "{uri}"
    );
    let stored: serde_json::Value =
        serde_json::from_slice(&store.get_sync(&uri).expect("stored")).expect("json");
    assert_eq!(stored["pass"], true);
    assert_eq!(stored["environment"], "staging");
}

#[tokio::test]
async fn verify_without_identity_is_the_documented_json_rpc_error() {
    let (_d, path, sha) = fixture::tiny_apr();
    let text = tiny_text(&sha);
    let (rt, _) = runtime_over_text(&text, &path, &sha, Arc::new(MemStore::default()));
    let (url, _h) = rpc::serve(
        server::build_with(rt.clone(), "antisemitism-staging", "0.0.0").expect("server"),
    )
    .await;
    let resp = rpc::call(
        &url,
        "tools/call",
        json!({"name": "verify", "arguments": {"rows": 8, "seed": 0}}),
        &[],
    )
    .await;
    let err = &resp["error"];
    assert!(!err.is_null(), "a JSON-RPC error, not a result: {resp}");
    assert_ne!(err["code"], -32602);
    assert!(
        err["message"]
            .as_str()
            .expect("msg")
            .contains("Permission denied: verify requires 'admin' role"),
        "{err}"
    );
    assert!(rt.cell.get().is_none(), "a denied verify never loads");
}

/// A two-row eval set cut from the fixture (its own has 9 rows, more than the door's 8):
/// `rows` over the set's size is refused in-band, naming both numbers, before any load.
#[tokio::test]
async fn verify_refuses_rows_over_eval_len() {
    let (_d, path, sha) = fixture::tiny_apr();
    let text = tiny_text(&sha);
    let cfg = config::parse(&text, cases::SERVER_ID).expect("parses");
    let dir = fixture::dir();
    let eval = std::fs::read_to_string(dir.join("data/eval.jsonl")).expect("eval");
    let two: Vec<&str> = eval.lines().take(2).collect();
    let mut probs: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("eval-probs.json")).expect("probs"))
            .expect("json");
    let rows = probs["rows"].as_array().expect("rows")[..2].to_vec();
    probs["rows"] = serde_json::Value::Array(rows);
    let store = Arc::new(MemStore::default());
    store.put(
        &cfg.records.eval_jsonl_uri,
        format!("{}\n", two.join("\n")).into_bytes(),
    );
    store.put(
        &cfg.records.eval_probs_uri,
        serde_json::to_vec(&probs).expect("json"),
    );
    let n = tools::read_eval_rows(&store.get_sync(&cfg.records.eval_jsonl_uri).expect("eval"))
        .expect("rows")
        .len();
    assert_eq!(n, 2);
    let (rt, _) = runtime_over_text(&text, &path, &sha, store);
    let (url, _h) = rpc::serve(
        server::build_with(rt.clone(), "antisemitism-staging", "0.0.0").expect("server"),
    )
    .await;
    let r = rpc::tool(&url, "verify", json!({"rows": 3, "seed": 0}), &ADMIN).await;
    assert_eq!(r["isError"], true, "{r}");
    let msg = r["content"][0]["text"].as_str().expect("text");
    assert!(
        msg.contains("verify rows 3 exceeds the eval set's 2 rows"),
        "{msg}"
    );
    assert!(rt.cell.get().is_none(), "refused before the load");
}

/// One eval row whose text is over classify_max_text_bytes; the probs file names that row's sha.
#[tokio::test]
async fn verify_surfaces_classify_bound_refusal() {
    let (_d, path, sha) = fixture::tiny_apr();
    let text = tiny_text(&sha);
    let cfg = config::parse(&text, cases::SERVER_ID).expect("parses");
    let long = "x".repeat(ClassifyLimits::CONTRACTED.max_text_bytes + 1);
    let store = Arc::new(MemStore::default());
    store.put(
        &cfg.records.eval_jsonl_uri,
        format!("{{\"text\":{long:?},\"label\":\"shipping\"}}\n").into_bytes(),
    );
    store.put(
        &cfg.records.eval_probs_uri,
        serde_json::to_vec(&json!({
            "labels": fixture::tiny_labels(),
            "rows": [{
                "row": 0,
                "text_sha256": aprender_mcp_decide_lambda::sha256_hex(long.as_bytes()),
                "probabilities": [0.34, 0.33, 0.33],
            }],
        }))
        .expect("json"),
    );
    let (rt, _) = runtime_over_text(&text, &path, &sha, store);
    let (url, _h) =
        rpc::serve(server::build_with(rt, "antisemitism-staging", "0.0.0").expect("server")).await;
    let r = rpc::tool(&url, "verify", json!({"rows": 1, "seed": 0}), &ADMIN).await;
    assert_eq!(r["isError"], true, "{r}");
    let msg = r["content"][0]["text"].as_str().expect("text");
    assert!(msg.contains("classify_max_text_bytes"), "{r}");
    assert!(
        msg.starts_with("verify: ") && msg.contains("choose another seed or fewer rows"),
        "the refusal is worded for a verify caller, who sent no texts: {msg}"
    );
}

#[tokio::test]
async fn verify_write_record_false_returns_the_record_unwritten() {
    let (_d, path, sha) = fixture::tiny_apr();
    let text = tiny_text(&sha);
    let store = store_with_tiny_records(&text);
    let prefix = config::parse(&text, cases::SERVER_ID)
        .expect("parses")
        .records
        .verification_prefix
        .expect("prefix");
    let (rt, _) = runtime_over_text(&text, &path, &sha, store.clone());
    let (url, _h) =
        rpc::serve(server::build_with(rt, "antisemitism-staging", "0.0.0").expect("server")).await;
    let r = rpc::tool(
        &url,
        "verify",
        json!({"rows": 2, "seed": 0, "write_record": false}),
        &ADMIN,
    )
    .await;
    let v = text_of(&r);
    assert_eq!(v["record"]["rows_scored"], 2);
    assert_eq!(v["written"], false);
    assert_eq!(v["reason"], "write_record = false");
    assert!(v["record_uri"].is_null());
    assert!(store.keys_under(&prefix).is_empty());
}
