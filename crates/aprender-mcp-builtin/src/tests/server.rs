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
