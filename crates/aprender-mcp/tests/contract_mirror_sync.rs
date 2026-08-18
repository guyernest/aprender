//! The crate-local tool-schemas contract must mirror the repo-root copy byte-for-byte.
//!
//! # Why two copies exist
//!
//! `build.rs` generates `schemas::APR_*_SCHEMA` / `APR_*_DESCRIPTION` from
//! `CARGO_MANIFEST_DIR/contracts/apr-mcp-tool-schemas-v1.yaml`, and FALSIFY-MCP-008
//! asserts the live `tools/list` response matches that YAML byte-for-byte. A
//! published `aprender-mcp` crate cannot reach `../../contracts/` — `cargo package`
//! only includes files under the crate directory — so the build input MUST live
//! inside the crate. Meanwhile the repo-root `contracts/` is the catalog that `pv`
//! and humans treat as the source of truth.
//!
//! Deduplicating is not available: a tracked symlink is rejected by
//! `scripts/check_publish_safety.sh` check 1 (PMAT-SQI — tracked symlinks broke
//! `cargo install` for every external user). So both copies stay, and this test is
//! what keeps them honest.
//!
//! # The failure this prevents
//!
//! The two files are indistinguishable by name. Editing the repo-root copy — the
//! obvious one, the one `pv lint contracts/` walks — changes NOTHING about the built
//! server, silently: codegen never reads it, so `tools/list` keeps serving the old
//! schema and every gate stays green. That is the shadowed-artifact failure mode
//! (CLAUDE.md: "a shadowed artifact is worse than a missing one — edits look
//! effective and change nothing"). It is not hypothetical: adding `apr.predict`
//! began by editing the root copy, and the build produced no such tool.
//!
//! # Scope
//!
//! Deliberately ONE file, not "every contract with a matching basename". Most
//! crate-local contracts are genuinely distinct documents that merely share a name
//! (three unrelated `matmul-v1.yaml`, most with no root counterpart at all), so a
//! blanket identity rule would be false-positive noise. The invariant here is
//! specific: this file is a build input mirrored from the catalog.

use std::path::PathBuf;

const CONTRACT_NAME: &str = "apr-mcp-tool-schemas-v1.yaml";

fn crate_local_copy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("contracts")
        .join(CONTRACT_NAME)
}

/// `crates/aprender-mcp` -> repo root. Absent when the crate is unpacked from a
/// published `.crate` tarball, which is a legitimate context, not a failure.
fn repo_root_copy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts")
        .join(CONTRACT_NAME)
}

#[test]
fn crate_contract_mirrors_repo_root_copy() {
    let local = crate_local_copy();
    let root = repo_root_copy();

    assert!(
        local.exists(),
        "build input missing: {} — codegen cannot run without it",
        local.display()
    );

    if !root.exists() {
        // Published-crate context: there is no catalog to mirror. Skipping is
        // correct; failing here would make the published crate's own test suite
        // red for a condition its consumers cannot fix.
        eprintln!(
            "skipping mirror check: no repo-root copy at {} (published-crate context)",
            root.display()
        );
        return;
    }

    let local_bytes = std::fs::read(&local)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", local.display()));
    let root_bytes =
        std::fs::read(&root).unwrap_or_else(|e| panic!("cannot read {}: {e}", root.display()));

    assert_eq!(
        local_bytes,
        root_bytes,
        "\n\
         {CONTRACT_NAME} has drifted between its two tracked copies.\n\
         \n\
           build input (codegen reads THIS): {}\n\
           repo-root catalog (pv/humans read THIS): {}\n\
         \n\
         Only the build input affects the built server, so if you edited the\n\
         repo-root copy your change has NOT taken effect and every gate is still\n\
         green against the old schema.\n\
         \n\
         Fix by copying whichever side you edited over the other, then rebuild so\n\
         codegen re-runs:\n\
           cp {} {}\n",
        local.display(),
        root.display(),
        root.display(),
        local.display(),
    );
}
