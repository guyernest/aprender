//! Tests for the `apr eval` SetFit branch.
//!
//! # The measured boundary of what these can reach
//!
//! `apr-cli` cannot construct a `setfit-apr-v1` artifact that passes the load ladder, and
//! therefore cannot construct a `SelectionLock` either: `SelectionCandidate::from_evaluation`
//! needs a `ValidationEvaluation`, whose only producers take a verified model. The blockers are
//! structural, not gaps in effort — `CALIBRATED_REGIMES` admits only the phase-3 MiniLM slice,
//! whose 97-row vocabulary closure cannot compute two of the six contract-resident probes
//! (F-10), and core's APR-capable view fixture is `#[cfg(test)] pub(crate)`.
//!
//! So the lock->token->grant chain is proven where an artifact exists: in `aprender-train`, by
//! `apr_evaluate_the_lock_travels_between_two_invocations_as_a_file` (TRN-07, IN-PROCESS,
//! file-mediated). 04-15 owns the SPAWNED cross-process proof.
//!
//! What IS reachable here — and is what this adapter is — is every refusal, every ordering
//! claim, and the structural guarantee that no path reaches the test split around `grant`.

use super::*;
use tempfile::TempDir;

/// This branch's own source, for the source assertions below.
const EVAL_SETFIT_SOURCE: &str = include_str!("setfit.rs");

/// Assemble a needle at RUNTIME so it cannot match the scan's own source.
fn needle(fragments: &[&str]) -> String {
    fragments.concat()
}

/// A minimal argument set; each test overrides the field it is about.
fn args<'a>(artifact: &'a Path, data: &'a Path, selection: &'a Path) -> SetFitEvalArgs<'a> {
    SetFitEvalArgs {
        artifact,
        data: Some(data),
        selection: Some(selection),
        split: Split::Validation,
        lock_out: None,
        selection_lock: None,
        candidates: &[],
        force: false,
        json: true,
    }
}

// ===========================================================================
// The split, and the flags that belong to each half
// ===========================================================================

#[test]
fn eval_setfit_split_is_a_closed_set_and_names_why_train_is_absent() {
    assert_eq!(
        "validation".parse::<Split>().expect("valid"),
        Split::Validation
    );
    assert_eq!("test".parse::<Split>().expect("valid"), Split::Test);
    let error = "train"
        .parse::<Split>()
        .expect_err("there is no third split");
    let rendered = error.to_string();
    assert!(
        rendered.contains("validation") && rendered.contains("test"),
        "the refusal must name the two legal values; got: {rendered}"
    );
    assert!(
        rendered.contains("memorisation"),
        "and it must say WHY the train split is not one of them; got: {rendered}"
    );
}

#[test]
fn eval_setfit_test_split_requires_a_lock_and_names_the_exact_prior_command() {
    // THE B4 REFUSAL. A test run cannot create the lock it needs — that is the whole point —
    // so the message must hand the operator the command that writes one.
    let temp = TempDir::new().expect("tempdir");
    let artifact = temp.path().join("model.apr");
    let data = temp.path().join("data");
    let selection = temp.path().join("selection-manifest.json");
    let mut a = args(&artifact, &data, &selection);
    a.split = Split::Test;

    let error = run(&a).expect_err("canonical test access requires a committed lock");
    assert!(
        matches!(error, CliError::ValidationFailed(_)),
        "an absent lock is a request error (exit 5); got: {error}"
    );
    let rendered = error.to_string();
    assert!(
        rendered.contains("--selection-lock"),
        "the refusal must name the missing flag; got: {rendered}"
    );
    assert!(
        rendered.contains("--split validation") && rendered.contains("--lock-out"),
        "the remedy must name the EXACT prior command that writes a lock; got: {rendered}"
    );
    // ORDERING: none of these paths exist. A run that opened `--data` first would have
    // reported a missing directory, sending the operator to look at a file that is fine.
    assert!(
        !rendered.contains("not found"),
        "the flag refusal must come BEFORE any file is opened; got: {rendered}"
    );
}

#[test]
fn eval_setfit_refuses_lock_out_on_a_test_run() {
    let temp = TempDir::new().expect("tempdir");
    let artifact = temp.path().join("model.apr");
    let data = temp.path().join("data");
    let selection = temp.path().join("sel.json");
    let lock_out = temp.path().join("lock.json");
    let mut a = args(&artifact, &data, &selection);
    a.split = Split::Test;
    a.lock_out = Some(&lock_out);

    let rendered = run(&a)
        .expect_err("a test run must not write its own lock")
        .to_string();
    assert!(
        rendered.contains("--lock-out") && rendered.contains("BEFORE"),
        "the refusal must say that a self-written lock defeats the ordering the lock exists \
         for; got: {rendered}"
    );
}

#[test]
fn eval_setfit_refuses_candidates_on_a_test_run() {
    let temp = TempDir::new().expect("tempdir");
    let artifact = temp.path().join("model.apr");
    let data = temp.path().join("data");
    let selection = temp.path().join("sel.json");
    let candidates = vec![temp.path().join("other.apr")];
    let mut a = args(&artifact, &data, &selection);
    a.split = Split::Test;
    a.selection_lock = Some(&artifact);
    a.candidates = &candidates;

    let rendered = run(&a)
        .expect_err("the candidate set was fixed when the lock was written")
        .to_string();
    assert!(rendered.contains("--candidate"), "got: {rendered}");
}

#[test]
fn eval_setfit_refuses_a_selection_lock_on_a_validation_run() {
    let temp = TempDir::new().expect("tempdir");
    let artifact = temp.path().join("model.apr");
    let data = temp.path().join("data");
    let selection = temp.path().join("sel.json");
    let lock = temp.path().join("lock.json");
    let mut a = args(&artifact, &data, &selection);
    a.selection_lock = Some(&lock);

    let rendered = run(&a)
        .expect_err("a validation run commits a selection, it does not consume one")
        .to_string();
    assert!(rendered.contains("--selection-lock") && rendered.contains("--lock-out"));
}

#[test]
fn eval_setfit_requires_data_and_selection_before_anything_is_read() {
    let temp = TempDir::new().expect("tempdir");
    let artifact = temp.path().join("model.apr");
    let selection = temp.path().join("sel.json");

    let mut a = args(&artifact, &artifact, &selection);
    a.data = None;
    let rendered = run(&a).expect_err("--data is required").to_string();
    assert!(rendered.contains("--data"), "got: {rendered}");

    let mut a = args(&artifact, &artifact, &selection);
    a.selection = None;
    let rendered = run(&a).expect_err("--selection is required").to_string();
    assert!(
        rendered.contains("--selection") && rendered.contains("reload"),
        "the refusal must say what the selection is compared against; got: {rendered}"
    );
}

// ===========================================================================
// The lock file: bounded, typed, and never invented
// ===========================================================================

#[test]
fn eval_setfit_reports_an_absent_lock_by_naming_the_command_that_writes_one() {
    let temp = TempDir::new().expect("tempdir");
    let error =
        read_lock(&temp.path().join("absent.json")).expect_err("there is no lock at that path");
    let rendered = error.to_string();
    assert!(
        matches!(error, CliError::ValidationFailed(_)),
        "got: {rendered}"
    );
    assert!(
        rendered.contains("--lock-out"),
        "an absent lock must point at the command that writes one; got: {rendered}"
    );
}

#[test]
fn eval_setfit_refuses_an_over_cap_lock_file_before_reading_it() {
    // T-04-50 at this surface. Sparse, so the test costs no disk: an attacker does not have to
    // spend a megabyte to make a reader spend one.
    let temp = TempDir::new().expect("tempdir");
    let path = temp.path().join("huge.json");
    let file = std::fs::File::create(&path).expect("fixture is creatable");
    file.set_len(MAX_SELECTION_LOCK_BYTES + 1)
        .expect("a sparse over-cap file is creatable");
    drop(file);

    let error = read_lock(&path).expect_err("a lock past the contracted bound must be refused");
    assert!(
        matches!(error, CliError::InvalidFormat(_)),
        "an over-cap lock is a format refusal (exit 4); got: {error}"
    );
    let rendered = error.to_string();
    assert!(
        rendered.contains(&MAX_SELECTION_LOCK_BYTES.to_string())
            && rendered.contains(&(MAX_SELECTION_LOCK_BYTES + 1).to_string()),
        "the refusal must name BOTH the bound and the observed length; got: {rendered}"
    );
}

#[test]
fn eval_setfit_refuses_a_lock_file_that_is_not_a_lock() {
    let temp = TempDir::new().expect("tempdir");
    let path = temp.path().join("not-a-lock.json");
    std::fs::write(&path, br#"{"schema_version":99}"#).expect("fixture is writable");
    let error = read_lock(&path).expect_err("an arbitrary JSON object is not a selection lock");
    assert!(
        matches!(error, CliError::ValidationFailed(_)),
        "the library's typed refusal must surface, not a panic; got: {error}"
    );
    assert!(
        error.to_string().contains("not a usable selection lock"),
        "the message must name the path and carry the library's diagnosis"
    );
}

#[test]
fn eval_setfit_refuses_a_directory_as_a_lock() {
    let temp = TempDir::new().expect("tempdir");
    let error = read_lock(temp.path()).expect_err("a directory is not a lock");
    assert!(matches!(error, CliError::NotAFile(_)), "got: {error}");
}

// ===========================================================================
// Structural: this adapter gates nothing itself, and bypasses nothing
// ===========================================================================

/// The CODE LINES of this branch, so the scans below do not fail on their own explanations.
fn production_code_lines() -> String {
    let code: String = EVAL_SETFIT_SOURCE
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            !(t.starts_with("//") || t.starts_with("/// ") || t.starts_with("///"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        code.contains("fn run(args: &SetFitEvalArgs"),
        "non-vacuity: the comment filter must not have eaten the module"
    );
    code
}

#[test]
fn eval_setfit_drives_the_three_library_doors_and_defines_no_gating_type() {
    let code = production_code_lines();
    for door in [
        "reload_verified_run_from_apr(",
        "create_selection_lock(",
        "SelectionLock::from_canonical_bytes(",
        "mint_test_token(",
        "CanonicalTestAccess::grant(",
    ] {
        assert!(
            code.contains(door),
            "the durable-lock workflow must drive `{door}` — this adapter re-implements none \
             of them"
        );
    }
    // A gating type defined HERE would be a second answer to "may this process read the test
    // split", and the second answer is the one nobody audits.
    for forged in [
        "struct CanonicalTestToken",
        "struct CanonicalTestGrant",
        "struct SelectionLock",
        "struct SelectionCandidate",
        "impl SetFitCredential",
    ] {
        assert!(
            !code.contains(forged),
            "`{forged}` here would be a CLI-side gate; every gate lives in aprender-train"
        );
    }
}

#[test]
fn eval_setfit_has_no_path_to_the_test_split_that_bypasses_the_grant() {
    // The T-04-21 assertion. `grant.test()` is the ONLY way this file names the test rows;
    // a direct `dataset.test()` would read them with the lock chain merely alongside.
    let code = production_code_lines();
    let direct = needle(&["dataset.", "test()"]);
    assert_eq!(
        code.matches(&direct).count(),
        0,
        "the test split must come OUT of the grant, never off the dataset directly"
    );
    assert_eq!(
        code.matches("grant.test()").count(),
        1,
        "exactly one route to the canonical test rows"
    );
    // And the reverse control: the grant is obtained from a token, which is obtained from a
    // lock READ OFF DISK. A run that minted from a lock it had just built in memory would
    // satisfy every needle above and prove nothing about ordering.
    let mint = code
        .find("mint_test_token(")
        .expect("the mint call must exist");
    let read = code
        .find("let lock = read_lock(")
        .expect("the lock read must exist");
    assert!(
        read < mint,
        "the lock must be READ from disk before a token is minted from it; in `--split test` \
         there is no other source for one"
    );
}

#[test]
fn eval_setfit_reads_artifacts_only_through_the_one_bounded_door() {
    let code = production_code_lines();
    let banned = needle(&["fs::", "read("]);
    assert_eq!(
        code.matches(&banned).count(),
        0,
        "no unbounded whole-file read may appear in this branch"
    );
    assert!(
        code.contains("setfit_io::read_setfit_apr_file_bounded"),
        "artifact bytes come through the ONE bounded door (review B5)"
    );
    assert!(
        code.contains("MAX_SELECTION_LOCK_BYTES"),
        "the lock file carries its own contracted bound, applied before the read"
    );
}

#[test]
fn eval_setfit_writes_the_lock_atomically_through_exactly_one_rename() {
    let code = production_code_lines();
    assert_eq!(
        code.matches("fs::rename(").count(),
        1,
        "exactly one rename site: a lock half-written by an interrupted run would look like a \
         committed selection and not be one"
    );
    assert!(
        code.contains("sync_all()"),
        "the bytes must be on disk before the rename makes them visible"
    );
    assert!(
        code.contains("already exists"),
        "an existing lock must not be clobbered without --force"
    );
}

#[test]
fn eval_setfit_uses_the_librarys_evaluator_and_computes_no_validation_metric_itself() {
    let code = production_code_lines();
    assert!(
        code.contains("evaluate_validation_from_artifact("),
        "the validation metric must be the library's, so the number the lock records is the \
         one trusted code computed"
    );
    for banned in ["fn accuracy(", "fn macro_f1(", "ValidationEvaluation {"] {
        assert!(
            !code.contains(banned),
            "`{banned}` here would be a CLI-side validation metric — the caller-asserted number \
             the library removed"
        );
    }
}
