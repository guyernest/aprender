//! The tiny Laya fixture (`crates/aprender-decide/tests/fixtures/laya_tiny`), packed by the
//! production packer once per test binary, as `aprender-mcp-decide/src/tests.rs` does.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use aprender_mcp_decide_lambda::{ModelSource, ENV_MODEL, ENV_SHA256};

/// The fixture directory in the workspace.
pub fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/aprender-decide/tests/fixtures/laya_tiny")
}

fn tiny_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        let dir = dir();
        aprender_decide::pack_run_dir(&dir, &dir.join("data")).expect("pack tiny")
    })
}

/// The packed fixture written to a fresh temp dir: `(dir, path, sha256)`.
pub fn tiny_apr() -> (tempfile::TempDir, PathBuf, String) {
    let d = tempfile::tempdir().expect("tempdir");
    let path = d.path().join("tiny.apr");
    std::fs::write(&path, tiny_bytes()).expect("write tiny.apr");
    let sha = aprender_mcp_decide_lambda::sha256_hex(tiny_bytes());
    (d, path, sha)
}

/// The fixture's labels, read from its `eval-probs.json`.
pub fn tiny_labels() -> Vec<String> {
    let probs: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir().join("eval-probs.json")).expect("probs"))
            .expect("json");
    probs["labels"]
        .as_array()
        .expect("labels")
        .iter()
        .map(|v| v.as_str().expect("label").to_string())
        .collect()
}

/// A local-file source with a sha pin, through the same lookup the configuration uses.
pub fn local_source(path: &Path, sha: &str) -> ModelSource {
    let path = path.to_string_lossy().to_string();
    let sha = sha.to_string();
    ModelSource::from_lookup(|name| match name {
        ENV_MODEL => Some(path.clone()),
        ENV_SHA256 => Some(sha.clone()),
        _ => None,
    })
    .expect("local source")
}
