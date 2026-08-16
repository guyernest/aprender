#![cfg(feature = "setfit")]
//! OPS-02 at the "a user can" tier: the SHIPPED BINARY, driven as a real process.
//!
//! # Why a spawned test exists at all when six modules already have unit tests
//!
//! Every rung of this chain is unit-tested inside `apr-cli`. None of those tests is
//! evidence about the binary an operator runs. A unit test calls `commands::eval::setfit::run`
//! with a struct it built itself; it cannot observe an argument that clap never routed, a
//! subcommand that was never registered, a feature gate that compiled the branch out, or an
//! exit code that `main` mapped differently from the `CliError` the module returned. Those are
//! exactly the wiring defects a per-module suite is structurally unable to see, and 04-REVIEWS
//! recorded their absence as a finding: *"There is no complete spawned CLI lifecycle proof for
//! OPS-02."*
//!
//! So every invocation in this file is a real `fork`/`exec` of
//! `env!("CARGO_BIN_EXE_apr")`, and every verdict is read from a **reaped `ExitStatus`**.
//!
//! # Three CLAUDE.md verification rules are load-bearing here, not decorative
//!
//! **Rule 1 — never read a status through a pipe.** A spawned-process test is the easiest
//! place in this repository to commit that defect: `Command::status()` on a shell pipeline,
//! or a `grep` on captured output used as the pass condition, both report the status of the
//! wrong thing. There is exactly one spawn site in this file, it pipes stdout and stderr to
//! this process rather than to another program, and the pass condition is always
//! `ExitStatus::success()` or a compared `ExitStatus::code()`.
//!
//! **Rule 3 — pin the binary.** `env!("CARGO_BIN_EXE_apr")` is an absolute path that cargo
//! computes for the binary it just built from THIS tree. It is never a `PATH` lookup and never
//! a hardcoded absolute path. Four `apr` binaries have coexisted on this dev box and a bare one
//! resolved to a 26-day-old copy; a lifecycle proof against the wrong binary is worse than no
//! proof, because it is a confident statement about code that is not running.
//!
//! **Rule 2 — prove the mechanism, do not label the run by intent.** The first rung is a
//! `--version` call whose output is asserted non-empty, so "the child process ran" is a
//! measurement rather than an assumption before anything is concluded from a later exit code.
//!
//! # This chain STOPS, on purpose, and says exactly where
//!
//! The plan this file implements asks for five green invocations ending in a classified
//! document. **That chain cannot close on this host, and no test here pretends otherwise.**
//! The cause is F-10, measured independently three times by 04-12 and again by 04-05 and 04-06:
//! `CALIBRATED_REGIMES` has exactly one entry, its architecture component is compared for exact
//! equality, and the only encoder that satisfies it is the phase-3 MiniLM slice — a 97-row
//! vocabulary closure that cannot compute the `setfit-apr-v1` contract's `probe_unicode`
//! (canonical id 5915). No `setfit-apr-v1` artifact exists in this repository and none can be
//! produced by the shipped commands.
//!
//! **What was deliberately NOT done.** `SetFitArtifactView`'s fields are public and
//! `aprender::setfit::write_setfit_apr` is public, so a synthetic APR-capable encoder would
//! have produced real `setfit-apr-v1` bytes from this very file, and the five-rung chain would
//! then have gone green and satisfied the plan's acceptance criteria to the letter. The model
//! classified would not be the model the `train` rung produced, and OPS-02 is a claim about the
//! JOIN between the rungs. 04-12 identified and refused that shortcut; this file refuses it
//! again, for the same reason.
//!
//! Instead, this file follows the convention 04-12 established and 04-17 re-confirmed: **a
//! blocked rung is a PASSING test that asserts the TYPED REFUSAL, and panics with restore
//! instructions the day the refusal stops happening.** An `#[ignore]`d test in this phase must
//! still pass under `--ignored` (04-10 gates them that way), so a full-chain test that cannot
//! pass would be a landmine rather than a placeholder.
//!
//! # What therefore IS proven, spawned, with real exit codes
//!
//! | rung | invocation | verdict |
//! |---|---|---|
//! | 0 | `--version` | 0 — the pinned child executes |
//! | 1 | `data tweet-eval-stance` | 0 — writes an attested benchmark directory |
//! | 2 | `data select` | 0 — consumes rung 1's directory, writes the selection manifest |
//! | 3 | `setfit train --dry-run` | 0 — consumes rungs 1+2, reports the merged config and both provenance fingerprints |
//! | 4 | `setfit train` | **6** — the typed `--model-dir` refusal; F-10's rung, and nothing is written |
//! | 5 | `inspect model.apr` | nonzero — the artifact rung 4 did not write is not there |
//!
//! Rungs 1 through 3 are a genuine end-to-end CLI chain in which each process consumes the
//! previous process's output FILES. That is the part of OPS-02 that is real today.
//!
//! The TRN-07 half — the durable selection-lock gate holding across process boundaries — is the
//! second test, and its own header states precisely which half of the workflow is reachable.

use std::fmt::Write as _;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// The binary under test: the one cargo built from THIS tree, by absolute path.
///
/// One definition rather than one per call site. Five copies of an environment lookup is five
/// places for a `PATH` lookup to be introduced later and go unnoticed; a single constant plus
/// the source guard below (exactly one `Command::new` site, and it takes this constant) is a
/// stronger statement than a high occurrence count would be.
const APR_BIN: &str = env!("CARGO_BIN_EXE_apr");

/// Nothing in this file may wait on a child forever. A hung child in CI is a job that burns its
/// whole timeout and reports nothing; a killed child reports which invocation hung.
const FAST_LIMIT: Duration = Duration::from_secs(120);

/// The rungs that read or write a whole benchmark directory.
const SLOW_LIMIT: Duration = Duration::from_secs(600);

// ==========================================================================================
// The one spawn site
// ==========================================================================================

/// One completed child process, with its status already reaped.
struct AprRun {
    /// The arguments, for failure messages.
    argv: Vec<String>,
    /// The REAPED exit status. Never a pipeline's status, never a `grep`'s.
    status: ExitStatus,
    /// Everything the child wrote to stdout.
    stdout: String,
    /// Everything the child wrote to stderr.
    stderr: String,
    /// How long it took, so a bound that is nearly hit is visible rather than silent.
    elapsed: Duration,
}

impl AprRun {
    /// The numeric exit code, refusing to guess when the child was signalled.
    fn code(&self) -> i32 {
        self.status.code().unwrap_or_else(|| {
            panic!(
                "the child was terminated by a signal and has no exit code — that is not a \
                 refusal, it is a crash:\n{}",
                self.transcript()
            )
        })
    }

    /// stdout and stderr together, for assertions about "the run said".
    fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }

    /// Everything a failing assertion needs to be actionable without a re-run.
    fn transcript(&self) -> String {
        format!(
            "  argv:    {:?}\n  status:  {:?} (code {:?})\n  elapsed: {:?}\n  \
             stdout:  {}\n  stderr:  {}",
            self.argv,
            self.status,
            self.status.code(),
            self.elapsed,
            self.stdout.trim(),
            self.stderr.trim()
        )
    }

    /// Require a zero exit, reading `ExitStatus::success` and nothing else.
    fn expect_success(&self, why: &str) -> &Self {
        assert!(
            self.status.success(),
            "{why}\nexpected a zero exit from the shipped binary:\n{}",
            self.transcript()
        );
        self
    }

    /// Require a NONZERO exit, and say what the refusal was supposed to be about.
    fn expect_refusal(&self, why: &str) -> &Self {
        assert!(
            !self.status.success(),
            "{why}\nexpected a NONZERO exit:\n{}",
            self.transcript()
        );
        self
    }

    /// Require the output to name something the operator has to act on.
    fn expect_mentions(&self, needle: &str, why: &str) -> &Self {
        assert!(
            self.combined().contains(needle),
            "{why}\nexpected the run to name `{needle}`:\n{}",
            self.transcript()
        );
        self
    }

    /// Require the output NOT to name something.
    ///
    /// This is an ORDER assertion, not a tidiness one: a refusal that names a path proves the
    /// path was reached, so a refusal that does NOT name it is evidence the run stopped before
    /// it. Used only in pairs, against an otherwise identical invocation that DOES name it.
    fn expect_silent_about(&self, needle: &str, why: &str) -> &Self {
        assert!(
            !self.combined().contains(needle),
            "{why}\ndid not expect the run to name `{needle}`:\n{}",
            self.transcript()
        );
        self
    }

    /// A refusal must be a refusal, not a crash.
    fn expect_no_panic(&self, why: &str) -> &Self {
        assert!(
            !self.combined().contains("panicked at"),
            "{why}\nthe command panicked instead of returning a typed error:\n{}",
            self.transcript()
        );
        self
    }
}

/// Run the pinned binary to completion, bounded, and hand back the reaped status.
///
/// # Why the pipes are drained on their own threads
///
/// A child that writes more than the pipe buffer holds blocks until somebody reads it. Polling
/// `try_wait` without draining would deadlock against exactly the verbose runs whose output this
/// file most wants to see. Two reader threads plus a polled `try_wait` gives both a bounded wait
/// and a complete transcript.
///
/// # Why not `Command::output()`
///
/// It waits forever. A hung `setfit train` would take the whole CI job's timeout and report
/// nothing about which rung hung.
fn run_apr(args: &[&str], limit: Duration) -> AprRun {
    let started = Instant::now();
    let mut child = Command::new(APR_BIN)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("could not spawn the pinned binary at {APR_BIN}: {error}"));

    let mut out_pipe = child.stdout.take().expect("stdout was piped at spawn");
    let mut err_pipe = child.stderr.take().expect("stderr was piped at spawn");
    let out_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = out_pipe.read_to_end(&mut buffer);
        buffer
    });
    let err_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = err_pipe.read_to_end(&mut buffer);
        buffer
    });

    let status = loop {
        match child.try_wait().expect("the spawned child is waitable") {
            Some(status) => break status,
            None => {
                if started.elapsed() > limit {
                    // Kill FIRST, then report. Panicking with the child still running would
                    // leave an orphan holding the pipes the reader threads are blocked on.
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!(
                        "the invocation {args:?} exceeded its {limit:?} bound and was killed — \
                         a lifecycle rung that hangs is a defect, not a slow test"
                    );
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    };

    let stdout = out_reader.join().map_or_else(
        |_| String::from("<stdout reader thread panicked>"),
        |bytes| String::from_utf8_lossy(&bytes).into_owned(),
    );
    let stderr = err_reader.join().map_or_else(
        |_| String::from("<stderr reader thread panicked>"),
        |bytes| String::from_utf8_lossy(&bytes).into_owned(),
    );

    AprRun {
        argv: args.iter().map(|a| (*a).to_string()).collect(),
        status,
        stdout,
        stderr,
        elapsed: started.elapsed(),
    }
}

// ==========================================================================================
// Fixtures — the Phase 2 recipe, and why it is spelled out here
// ==========================================================================================

/// The contracted canonical class counts.
///
/// These are replicated from `data_tweeteval`'s `TRAIN_COUNTS`/`VALIDATION_COUNTS`/
/// `TEST_COUNTS` because the fixture writer that owns them is `#[cfg(test)] pub(crate)` and is
/// therefore unreachable from an integration test, which is out-of-crate by construction. The
/// plan asked to reuse the recipe rather than invent a second one, and this is the closest
/// reachable form of that: the same six file names, the same row text, the same tag.
///
/// **A drift here cannot go unnoticed.** These counts are not asserted by this file; they are
/// handed to the shipped `data tweet-eval-stance`, which validates them against its own
/// constants. If the contracted counts ever change, rung 1 fails with the binary's own message
/// naming the expected counts — a self-diagnosing failure rather than a silent divergence.
const TRAIN_COUNTS: [usize; 3] = [159, 319, 109];
/// See [`TRAIN_COUNTS`].
const VALIDATION_COUNTS: [usize; 3] = [18, 36, 12];
/// See [`TRAIN_COUNTS`].
const TEST_COUNTS: [usize; 3] = [45, 189, 46];

/// The row-text prefix the in-crate fixture has always used, so the split digests this file
/// produces are the ones the in-crate tests produce.
const FIXTURE_TAG: &str = "authored fixture";

/// One of the ten contracted benchmark seeds. 42 is deliberately not one of them.
const FIXTURE_SEED: &str = "13";

/// Write one split of the synthetic canonical source tree.
fn write_source_split(root: &Path, split: &str, counts: [usize; 3]) {
    let mut texts = String::new();
    let mut labels = String::new();
    let mut index = 0_usize;
    for (label, count) in counts.into_iter().enumerate() {
        for _ in 0..count {
            writeln!(texts, "{FIXTURE_TAG} {split} sample {index}").expect("String is writable");
            writeln!(labels, "{label}").expect("String is writable");
            index += 1;
        }
    }
    fs::write(root.join(format!("{split}_text.txt")), texts).expect("fixture text is writable");
    fs::write(root.join(format!("{split}_labels.txt")), labels)
        .expect("fixture labels are writable");
}

/// The synthetic canonical TweetEval source tree `data tweet-eval-stance --source` accepts.
fn write_canonical_source(root: &Path) {
    fs::create_dir_all(root).expect("fixture source directory is creatable");
    write_source_split(root, "train", TRAIN_COUNTS);
    write_source_split(root, "val", VALIDATION_COUNTS);
    write_source_split(root, "test", TEST_COUNTS);
}

/// A training configuration carrying all twelve knobs, in the shape an operator types.
///
/// `max_length` is `aprender::setfit::MAX_SEQUENCE_LENGTH`, named through the public constant
/// rather than typed, so a contract change moves this file rather than silently disagreeing
/// with it.
fn write_train_config(dir: &Path) -> PathBuf {
    let path = dir.join("train.toml");
    let contents = format!(
        "encoder_lr = 2e-5\n\
         epochs = 1\n\
         batch_size = 16\n\
         warmup_ratio = 0.1\n\
         grad_clip_max_norm = 1.0\n\
         max_length = {max_length}\n\
         freeze_policy = []\n\
         root_seed = {FIXTURE_SEED}\n\
         device = \"cpu\"\n\
         lr_schedule = \"warmup_linear_decay\"\n\
         \n\
         [pair_config]\n\
         strategy = \"oversampling\"\n\
         singleton_policy = \"negatives_only\"\n\
         \n\
         [head_regularization]\n\
         kind = \"sklearn_equivalent_c\"\n\
         c = 1.0\n",
        max_length = aprender::setfit::MAX_SEQUENCE_LENGTH,
    );
    fs::write(&path, contents).expect("the training config is writable");
    path
}

/// The in-repo conformance slice — a real directory, and NOT a pinned checkout.
///
/// Resolved from this crate's manifest rather than the process working directory, which
/// `cargo test` does not guarantee.
fn slice_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../aprender-core/tests/fixtures/setfit")
}

/// Every path under `root`, so "nothing was written" is a set comparison rather than a spot
/// check on the one filename the test happened to think of.
fn listing(root: &Path) -> std::collections::BTreeSet<PathBuf> {
    fn walk(dir: &Path, into: &mut std::collections::BTreeSet<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, into);
            }
            into.insert(path);
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(root, &mut out);
    out
}

// ==========================================================================================
// Rung 4's restore instruction
// ==========================================================================================

/// What to do the day `setfit train` starts succeeding.
///
/// Written as a panic message rather than a comment for the reason 04-12 gave: a comment ages
/// silently, and the person who closes F-10 is not the person who wrote this file.
const F10_CLOSED: &str = "\
`apr setfit train` SUCCEEDED. F-10 is CLOSED — an encoder now both passes CALIBRATED_REGIMES \
and computes the setfit-apr-v1 contract's six probes.\n\
RESTORE THE FULL CHAIN IN THIS FILE:\n\
  rung 4: assert exit 0, parse the --json report, capture `artifact_sha256`\n\
  rung 5: `inspect <APR> --json`   -> the SetFit section, same artifact hash\n\
  rung 6: `eval <APR> --task classify --split validation --lock-out lock.json --json`\n\
          -> assert lock.json EXISTS on disk, capture its lock_hash\n\
  rung 7: `eval <APR> --task classify --split test --selection-lock lock.json --json`\n\
          -> assert the SAME lock_hash, from a SEPARATE process reading that FILE\n\
  rung 8: `predict <APR> --input request.json --json`\n\
          -> parse into aprender::setfit::classify::ClassifyResponse, same artifact_sha256\n\
Also revisit the second test in this file: its positive half becomes reachable.";

// ==========================================================================================
// Test 1 — the OPS-02 chain, spawned
// ==========================================================================================

#[test]
#[ignore = "integration weight: spawns the shipped binary six times and builds a whole \
            benchmark directory and selection. 04-10 runs it as its own invocation — \
            `cargo test -p apr-cli --features setfit --test setfit_cli_lifecycle -- --ignored \
            lifecycle`. It PASSES: the chain is walked to the rung F-10 removes, and that \
            rung's refusal is what is asserted"]
#[allow(clippy::too_many_lines)]
fn setfit_cli_lifecycle_the_binary_walks_ops_02_to_the_rung_f_10_removes() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();

    // ------------------------------------------------------------------------------
    // RUNG 0 — the mechanism, before anything is concluded from an exit code.
    //
    // CLAUDE.md rule 2: a run must not be labelled by intent. Every later assertion in this
    // test reads a status from a child process, so "a child process ran at all, and it was
    // THIS binary" has to be a measurement first. A `--version` whose stdout is empty would
    // mean the exec silently produced nothing, and every subsequent nonzero exit would then
    // be indistinguishable from a broken harness.
    // ------------------------------------------------------------------------------
    let version = run_apr(&["--version"], FAST_LIMIT);
    version.expect_success("the pinned binary must execute before anything is read from it");
    assert!(
        !version.stdout.trim().is_empty(),
        "the pinned binary produced an empty --version — the harness, not the CLI, is the \
         thing under suspicion:\n{}",
        version.transcript()
    );
    assert!(
        Path::new(APR_BIN).is_file(),
        "CARGO_BIN_EXE_apr must name a real file — it is an absolute path cargo computed for \
         the binary it built from this tree, never a PATH lookup: {APR_BIN}"
    );

    // ------------------------------------------------------------------------------
    // RUNG 1 — `data tweet-eval-stance` writes an attested benchmark directory.
    //
    // `--source` is what makes this offline: the command downloads only when it is absent,
    // and a lifecycle test that reached the network would be measuring the network.
    // ------------------------------------------------------------------------------
    let source = root.join("srctree");
    write_canonical_source(&source);
    let benchmark = root.join("benchmark");

    let prepared = run_apr(
        &[
            "data",
            "tweet-eval-stance",
            "--output",
            &benchmark.display().to_string(),
            "--source",
            &source.display().to_string(),
        ],
        SLOW_LIMIT,
    );
    prepared.expect_success(
        "the canonical preparation is rung 1. If this fails naming class counts, the \
         contracted counts moved and the constants at the top of this file must follow — \
         that is the self-diagnosing failure they are documented to produce",
    );
    let manifest = benchmark.join("benchmark-manifest.json");
    assert!(
        manifest.is_file(),
        "rung 1 must leave the attestation on disk, because rung 2 is a SEPARATE PROCESS and \
         has nothing else to read"
    );

    // ------------------------------------------------------------------------------
    // RUNG 2 — `data select` CONSUMES rung 1's directory and writes the selection manifest.
    //
    // This is the first file-mediated hand-off: nothing is shared between the two processes
    // except the directory rung 1 wrote.
    // ------------------------------------------------------------------------------
    let selected = run_apr(
        &[
            "data",
            "select",
            "--data",
            &benchmark.display().to_string(),
            "--shots",
            "8",
            "--seed",
            FIXTURE_SEED,
        ],
        SLOW_LIMIT,
    );
    selected.expect_success("rung 2 consumes rung 1's attested directory");
    let selection = benchmark.join("selection-manifest.json");
    assert!(
        selection.is_file(),
        "rung 2 must leave the selection manifest on disk for rung 3 to read"
    );

    // ------------------------------------------------------------------------------
    // RUNG 3 — `setfit train --dry-run` CONSUMES rungs 1 and 2 and reports the merged config.
    //
    // This is the deepest GREEN rung of OPS-02 available today, and it is a real one: the
    // config is parsed and validated as a whole, the device is resolved on this host, the
    // output path is checked, and --data and --selection are replayed strictly against each
    // other. The provenance fingerprints in its report are computed FROM rung 1 and rung 2's
    // files — which is what makes this a chain rather than three unrelated commands.
    // ------------------------------------------------------------------------------
    let config = write_train_config(root);
    let output = root.join("model.apr");
    let model_dir = slice_fixture_dir();
    assert!(
        model_dir.is_dir(),
        "the conformance slice fixture must exist at {} — if the fixture estate moved this \
         test should be updated rather than deleted",
        model_dir.display()
    );

    let train_argv = |dry: bool| {
        let mut argv = vec![
            "--json".to_string(),
            "setfit".to_string(),
            "train".to_string(),
            "--config".to_string(),
            config.display().to_string(),
            "--data".to_string(),
            benchmark.display().to_string(),
            "--selection".to_string(),
            selection.display().to_string(),
            "--model-dir".to_string(),
            model_dir.display().to_string(),
            "--output".to_string(),
            output.display().to_string(),
        ];
        if dry {
            argv.push("--dry-run".to_string());
        }
        argv
    };

    let dry_argv = train_argv(true);
    let dry_refs: Vec<&str> = dry_argv.iter().map(String::as_str).collect();
    let dry = run_apr(&dry_refs, SLOW_LIMIT);
    dry.expect_success("rung 3 is the deepest green rung of OPS-02 available today");

    let report: serde_json::Value = serde_json::from_str(&dry.stdout).unwrap_or_else(|error| {
        panic!(
            "rung 3's --json output must be a single JSON document Phase 5 can consume: \
             {error}\n{}",
            dry.transcript()
        )
    });
    assert_eq!(
        report["dry_run"],
        serde_json::Value::Bool(true),
        "the report must say it was a dry run, or an operator cannot tell a pre-flight from a \
         completed training run:\n{}",
        dry.transcript()
    );

    // The two fingerprints are asserted to be DIFFERENT values. A renderer that read one path
    // twice would pass a presence check and fail this one — the same trap 04-07 closed in
    // `apr inspect`.
    let dataset_fp = report["provenance"]["dataset_fingerprint"]
        .as_str()
        .expect("the dry run reports the dataset fingerprint it computed from rung 1's files")
        .to_string();
    let split_fp = report["provenance"]["validation_split_fingerprint"]
        .as_str()
        .expect("and the validation split's own fingerprint")
        .to_string();
    assert_eq!(dataset_fp.len(), 64, "a hex SHA-256, not a truncation");
    assert_ne!(
        dataset_fp, split_fp,
        "the whole corpus and one split cannot have the same fingerprint; equal values would \
         mean one path was read twice"
    );
    assert_eq!(
        report["provenance"]["selection_root_seed"].as_u64(),
        Some(13),
        "the seed rung 2 selected under must survive into rung 3's provenance — that is the \
         file-mediated hand-off, observed"
    );
    assert_eq!(
        report["resolved"]["resolved_device"].as_str(),
        Some("cpu"),
        "the device must be the one resolved on this host, never the one requested"
    );

    // The dry run states what it did NOT check. A pre-flight that leaves the reader to assume
    // --model-dir was validated has told them something false by omission.
    let skipped = report["checks_skipped"].to_string();
    assert!(
        skipped.contains("--model-dir"),
        "the dry run must say --model-dir was not opened; got {skipped}"
    );
    assert!(
        !output.exists(),
        "a dry run writes NOTHING — not the artifact, not a temp file"
    );

    // ------------------------------------------------------------------------------
    // RUNG 4 — `setfit train` for real. THIS IS THE RUNG F-10 REMOVES.
    //
    // The refusal is at the ENCODER door, and that location is itself the evidence that
    // everything before it ran in this process: the config parsed and merged, --device cpu
    // resolved, --output was clear, the benchmark directory passed the attested boundary, and
    // the selection manifest replayed strictly against it. Any of those failing would produce
    // a DIFFERENT error, which is what makes this a stage measurement rather than a smoke test.
    // ------------------------------------------------------------------------------
    let real_argv = train_argv(false);
    let real_refs: Vec<&str> = real_argv.iter().map(String::as_str).collect();
    let trained = run_apr(&real_refs, SLOW_LIMIT);

    assert!(
        !trained.status.success(),
        "{F10_CLOSED}\n{}",
        trained.transcript()
    );
    assert_eq!(
        trained.code(),
        6,
        "the refusal must be the typed ModelLoadFailed exit code (6), not a panic and not a \
         generic failure — an operator scripting this needs to tell 'the pin is missing' from \
         'the CLI crashed':\n{}",
        trained.transcript()
    );
    trained
        .expect_refusal("rung 4 cannot complete on this host — F-10")
        .expect_mentions(
            "--model-dir",
            "the refusal must name the flag the operator has to change; the library cannot \
             know a CLI flag exists, so this is the adapter's half",
        )
        .expect_mentions(
            "NEVER downloads",
            "and it must state the offline prerequisite, because the obvious next assumption \
             is that the command fetches the pin itself",
        )
        .expect_mentions(
            "config.json",
            "and it must name the first missing pin file. Observed at authoring time: \
             SetFitError::ImportIo(config.json: No such file or directory). Naming it is what \
             turns 'it failed somewhere in the loader' into a stage measurement: the slice's \
             tokenizer.json satisfied the pinned-digest check first",
        );
    assert!(
        !output.exists(),
        "a run that refused must leave no artifact — a partial or empty model.apr would be \
         read by rung 5 as a real one"
    );

    // ------------------------------------------------------------------------------
    // RUNG 5 — the chain's next process, run anyway, so the STOP is demonstrated rather
    // than described.
    //
    // `inspect` is the generic command D-06 says a SetFit artifact uses. Pointed at the file
    // rung 4 did not write, it refuses. That is the honest end of the chain today: not
    // "we skipped the rest", but "the next process ran and had nothing to read".
    // ------------------------------------------------------------------------------
    let inspected = run_apr(
        &["inspect", &output.display().to_string(), "--json"],
        FAST_LIMIT,
    );
    inspected.expect_refusal(
        "rung 5 must refuse, because rung 4 wrote nothing. If this ever exits 0 something \
         produced a model.apr behind the chain's back and the whole file needs re-measuring",
    );

    // Nothing outside the tempdir was touched, and the only files in it are the ones the
    // rungs are documented to write.
    let final_listing = listing(root);
    assert!(
        final_listing.contains(&manifest) && final_listing.contains(&selection),
        "the two files the chain hands between processes must both still be on disk"
    );
    assert!(
        !final_listing.contains(&output),
        "and the artifact must not be, on any path"
    );
}

// ==========================================================================================
// The tagged DECOY — what it is, and the two things it is deliberately not
// ==========================================================================================

/// The exit code `CliError::ValidationFailed` maps to.
const EXIT_VALIDATION_FAILED: i32 = 5;

/// The exit code `CliError::ModelLoadFailed` maps to.
const EXIT_MODEL_LOAD_FAILED: i32 = 6;

/// An APR v2 container carrying the SCHEMA-OWNED entry names and the SetFit tag.
///
/// # This is NOT a manufactured `setfit-apr-v1` artifact, and the difference is the point
///
/// The shortcut this phase forbids is building a *valid* artifact out of a synthetic
/// APR-capable encoder so a lifecycle test goes GREEN — a green light that reads as "OPS-02
/// holds" while the model classified was never produced by the `train` rung. 04-12 identified
/// and refused exactly that.
///
/// This container is the opposite thing. It is written by core's PRODUCTION writer
/// (`AprV2Writer`), so it is a real APR v2 file with real entries — and it is not a model at
/// all: `load_setfit_apr` refuses it, which the second test below asserts **by spawning
/// `predict` against it and requiring a nonzero exit**. Nothing in this file goes green because
/// of it. It exists for two questions that are about the CONTAINER rather than about a model:
///
/// 1. **TRN-07's gate** — `apr eval` routes on the typed tag (D-04) and refuses the
///    `--split test` flag set *before* it opens the corpus or the artifact. Reaching that gate
///    from a spawned process needs a file carrying the tag, and needs nothing else.
/// 2. **D-01 / research assumption A3** — do the generic tools survive the three schema-owned
///    entries, including the U8 `tokenizer.blob` pseudo-tensor? That is a question about entry
///    names and dtypes in a container, and this container has exactly those.
///
/// The tensor payloads are shaped, not trained: `setfit.head.weight` is `[num_labels, hidden]`
/// and `setfit.head.bias` is `[num_labels]` per `contracts/setfit-apr-v1.yaml` items 1 and 5,
/// and `tokenizer.blob` is a U8 tensor of `[len]` as the same contract specifies.
fn write_tagged_decoy(dir: &Path, name: &str) -> PathBuf {
    use aprender::format::v2::{AprV2Metadata, AprV2Writer, TensorDType};

    let mut custom: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();
    custom.insert(
        "setfit".to_string(),
        serde_json::from_str(r#"{"schema":"setfit-apr-v1","schema_version":1}"#)
            .expect("the decoy document is valid JSON"),
    );
    let metadata = AprV2Metadata {
        model_type: "setfit".to_string(),
        created_at: None,
        custom,
        ..Default::default()
    };

    let mut writer = AprV2Writer::new(metadata);
    writer.add_f32_tensor("setfit.head.weight".to_string(), vec![3, 8], &[0.5_f32; 24]);
    writer.add_f32_tensor("setfit.head.bias".to_string(), vec![3], &[0.0_f32; 3]);
    // The U8 entry assumption A3 is about. A tool that assumed every entry is numeric-and-
    // dequantizable would crash, omit it, or report a nonsense statistic here.
    let blob: Vec<u8> = (0..64_u8).collect();
    writer.add_tensor(
        "tokenizer.blob".to_string(),
        TensorDType::U8,
        vec![blob.len()],
        blob,
    );

    let bytes = writer.write().expect("the decoy container is writable");
    let path = dir.join(name);
    fs::write(&path, bytes).expect("the decoy file is writable");
    path
}

// ==========================================================================================
// Test 2 — TRN-07 across a PROCESS boundary
// ==========================================================================================

#[test]
#[ignore = "integration weight: spawns the shipped binary seven times. 04-10 runs it with the \
            same `-- --ignored lifecycle` filter as the chain test above. It PASSES: the half \
            of the durable-lock workflow that is reachable without an artifact is the GATE, \
            and the gate is what this asserts"]
#[allow(clippy::too_many_lines)]
fn setfit_cli_lifecycle_trn_07_the_test_split_gate_holds_across_processes() {
    // ------------------------------------------------------------------------------
    // WHAT THIS IS, AND WHAT IT IS NOT — read before citing it
    //
    // 04-07 shipped `apr_evaluate_the_lock_travels_between_two_invocations_as_a_file` and
    // labelled it IN-PROCESS: two scopes in one process, mediated by a file. Its summary
    // assigns the SPAWNED citation to this plan. This is that citation, and it is honest
    // about which half of the workflow it covers.
    //
    // The POSITIVE half — a validation process writes lock.json, a separate test process
    // reads that FILE and is admitted — requires a real `setfit-apr-v1` artifact, because
    // `create_selection_lock` needs a credential, which needs `load_setfit_apr`, which needs
    // bytes no shipped command can produce (F-10). It is unreachable, it is not faked here,
    // and rung 4 of the chain test above is where it stops.
    //
    // The NEGATIVE half is fully reachable and is the half that actually guards the canonical
    // test split. `commands::eval::setfit::run` checks the split flag set at step (1) —
    // BEFORE the Phase 2 ingest at step (2) and before the artifact reload at step (3). So a
    // fresh process can be observed refusing test access with nothing on disk but a tagged
    // file. That is a stronger claim than it sounds: it says the gate cannot be reached
    // around, cannot be satisfied by anything in this process's memory, and costs nothing to
    // enforce.
    //
    // Every pair below holds ALL other arguments constant and varies ONE flag, so each
    // refusal is attributable to that flag rather than to the invocation being broken in
    // general (CLAUDE.md rule 6: one failing input is an anecdote).
    // ------------------------------------------------------------------------------
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    let decoy = write_tagged_decoy(root, "tagged.apr");

    // Deliberately absent. If the gate is reached, neither is ever opened — and the ONLY way
    // to tell "not opened" from "opened and happened to be fine" is to make opening it fail
    // loudly, then check whether the failure was reported.
    let absent_data = root.join("no-such-corpus");
    let absent_selection = root.join("no-such-selection.json");
    let absent_data_s = absent_data.display().to_string();
    let absent_selection_s = absent_selection.display().to_string();
    let decoy_s = decoy.display().to_string();
    let lock = root.join("lock.json");
    let lock_s = lock.display().to_string();
    assert!(
        !absent_data.exists() && !absent_selection.exists(),
        "both Phase 2 inputs must genuinely be absent, or 'the corpus was never opened' is \
         indistinguishable from 'the corpus was opened and was fine'"
    );

    let base = |split: &'static str| {
        vec![
            "eval".to_string(),
            decoy_s.clone(),
            "--task".to_string(),
            "classify".to_string(),
            "--data".to_string(),
            absent_data_s.clone(),
            "--selection".to_string(),
            absent_selection_s.clone(),
            "--split".to_string(),
            split.to_string(),
        ]
    };
    let spawn = |argv: &[String], limit: Duration| {
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        run_apr(&refs, limit)
    };

    // ------------------------------------------------------------------------------
    // L1 — a fresh process REFUSES the canonical test split with no lock, and does so
    //      before it looks at the corpus.
    // ------------------------------------------------------------------------------
    let no_lock = spawn(&base("test"), FAST_LIMIT);
    no_lock
        .expect_refusal(
            "canonical test rows must not be reachable from a process that was handed no \
             selection lock. If this ever exits 0, T-04-21 is live: the split the whole \
             benchmark protocol protects is readable on demand",
        )
        .expect_no_panic("a missing flag is a refusal, not a crash")
        .expect_mentions(
            "--selection-lock",
            "the refusal must name the flag that is missing",
        )
        .expect_mentions(
            "--split validation",
            "and it must name the PRIOR command that writes one, because a test run cannot \
             create what it needs — that is the entire point of the workflow",
        )
        .expect_mentions("--lock-out", "including the flag that commits the decision")
        .expect_silent_about(
            &absent_data_s,
            "and it must NOT name the corpus, because the corpus was never opened. This is \
             the order assertion: the gate is at step (1), the ingest at step (2)",
        );
    assert_eq!(
        no_lock.code(),
        EXIT_VALIDATION_FAILED,
        "the refusal is a typed ValidationFailed (exit 5), which an operator's script can \
         tell from a missing file (3) or a broken artifact (6):\n{}",
        no_lock.transcript()
    );

    // ------------------------------------------------------------------------------
    // L2 — THE MECHANISM PROOF. The same invocation plus `--selection-lock` gets PAST the
    //      gate and dies on the corpus instead.
    //
    // Without this, L1's silence about the corpus would be indistinguishable from "this
    // invocation is broken in some way that never reaches anything". The two runs differ in
    // exactly one flag and produce two DIFFERENT refusals; that difference is the evidence
    // the gate is what fired (CLAUDE.md rule 2).
    // ------------------------------------------------------------------------------
    let mut with_lock_argv = base("test");
    with_lock_argv.push("--selection-lock".to_string());
    with_lock_argv.push(lock_s.clone());
    let with_lock = spawn(&with_lock_argv, FAST_LIMIT);
    with_lock
        .expect_refusal("the corpus really is absent, so this must fail too — but LATER")
        .expect_no_panic("and still as a typed error")
        .expect_mentions(
            &absent_data_s,
            "supplying --selection-lock must carry the run PAST the gate and into the ingest, \
             which is what names the corpus. If this run is silent about the corpus too, the \
             two refusals are the same refusal and L1 proved nothing about ordering",
        );
    assert_ne!(
        no_lock.combined().trim(),
        with_lock.combined().trim(),
        "two invocations differing in one flag must not produce the same message"
    );

    // ------------------------------------------------------------------------------
    // L3 — a test run may not write its own lock, and does not leave one behind.
    // ------------------------------------------------------------------------------
    let mut self_lock_argv = base("test");
    self_lock_argv.push("--lock-out".to_string());
    self_lock_argv.push(lock_s.clone());
    let self_lock = spawn(&self_lock_argv, FAST_LIMIT);
    self_lock
        .expect_refusal(
            "a test run that could write its own lock would defeat the requirement the lock \
             exists for: that the selection be COMMITTED BEFORE test access is taken",
        )
        .expect_mentions("--lock-out", "the refused flag is named")
        .expect_mentions("--split validation", "and the split it belongs to");
    assert!(
        !lock.exists(),
        "and no lock file may appear on disk from a refused run — the file IS the commitment"
    );

    // ------------------------------------------------------------------------------
    // L4 — the mirror: a validation run may not CONSUME a lock.
    //
    // A flag silently ignored is worse than a flag refused. An operator who passes
    // --selection-lock to a validation run and sees a green report has every reason to
    // believe the lock gated something; it would have gated nothing.
    // ------------------------------------------------------------------------------
    let mut wrong_side_argv = base("validation");
    wrong_side_argv.push("--selection-lock".to_string());
    wrong_side_argv.push(lock_s.clone());
    let wrong_side = spawn(&wrong_side_argv, FAST_LIMIT);
    wrong_side
        .expect_refusal("a validation run COMMITS a selection, it does not consume one")
        .expect_mentions("--selection-lock", "the refused flag is named")
        .expect_mentions("--split test", "and the split it belongs to");

    // ------------------------------------------------------------------------------
    // L5 — the candidate set is the DECISION, and it was fixed when the lock was written.
    // ------------------------------------------------------------------------------
    let mut candidate_argv = base("test");
    candidate_argv.push("--selection-lock".to_string());
    candidate_argv.push(lock_s.clone());
    candidate_argv.push("--candidate".to_string());
    candidate_argv.push(decoy_s.clone());
    let candidate = spawn(&candidate_argv, FAST_LIMIT);
    candidate
        .expect_refusal("a test run may not re-open the candidate set")
        .expect_mentions("--candidate", "the refused flag is named");

    // ------------------------------------------------------------------------------
    // L6 — a validation run that never reached the decision COMMITS NOTHING.
    //
    // The lock file is the durable record of a decision. A run that died in the ingest never
    // made one, so a lock on disk afterwards would be a commitment to a decision nobody took.
    // ------------------------------------------------------------------------------
    let mut commit_argv = base("validation");
    commit_argv.push("--lock-out".to_string());
    commit_argv.push(lock_s.clone());
    let committed = spawn(&commit_argv, FAST_LIMIT);
    committed
        .expect_refusal("the corpus is absent, so no decision can be taken")
        .expect_mentions(
            &absent_data_s,
            "and the refusal names the corpus it could not read",
        );
    assert!(
        !lock.exists(),
        "a validation run that failed before the decision must leave NO lock file. A lock \
         written here would later admit a test run on the strength of a decision that was \
         never made"
    );

    // ------------------------------------------------------------------------------
    // L7 — there is no third split, and the refusal says why.
    // ------------------------------------------------------------------------------
    let bogus = spawn(&base("train"), FAST_LIMIT);
    bogus
        .expect_refusal("the train split is what the model was fitted on")
        .expect_mentions(
            "memorisation",
            "and the refusal must say why reading it as an evaluation is wrong, not merely \
             that the value is not in a list",
        );

    // ------------------------------------------------------------------------------
    // THE DECOY IS NOT A MODEL — asserted by the binary, not promised in a comment.
    //
    // Everything above needed a file carrying the tag and nothing more. This proves that is
    // all it is: the generic `predict` command routes on the same tag, hands the file to
    // `load_setfit_apr`'s full ladder, and the ladder refuses it. If this ever exits 0, the
    // decoy has become a real artifact and every claim in this file needs re-reading.
    // ------------------------------------------------------------------------------
    let predicted = run_apr(&["predict", &decoy_s, "--text", "hola"], FAST_LIMIT);
    predicted
        .expect_refusal(
            "the decoy must NOT load as a model. A green predict here would mean this file \
             manufactured a setfit-apr-v1 artifact, which is precisely the shortcut 04-12 \
             refused and this plan was told not to take",
        )
        .expect_no_panic("and the refusal is typed, not a crash");
    assert_eq!(
        predicted.code(),
        EXIT_MODEL_LOAD_FAILED,
        "the tagged-but-invalid container is a ModelLoadFailed (exit 6) — the tag ROUTED and \
         the loader then refused. Exit 4 would mean it was never recognised as a classifier \
         at all, which would make the gate assertions above vacuous:\n{}",
        predicted.transcript()
    );
}

// ==========================================================================================
// Test 2b — WR-10 across a PROCESS boundary: the no-clobber gate precedes the work
// ==========================================================================================

#[test]
#[ignore = "integration weight: spawns the shipped binary three times. It is selected by the \
            SAME `-- --ignored lifecycle` filter `make setfit-cli-lifecycle` drives (verified \
            by reading the recipe, not assumed), and that gate's `assert_tests_ran` floor of 2 \
            is a MINIMUM, so this case is covered without touching the Makefile"]
fn setfit_cli_lifecycle_wr_10_the_lock_refusal_precedes_the_corpus_read() {
    // ------------------------------------------------------------------------------
    // WHY THIS IS A SPAWNED TEST AND NOT A SHELL PROBE
    //
    // The obvious way to check this by hand — run the pinned binary against some `*.apr` in
    // the repo with an occupied `--lock-out` — CANNOT REACH the code under test, and that was
    // measured rather than guessed. `dispatch_analysis.rs:759` reads the SetFit tag BEFORE
    // `commands::eval::setfit::run` is called, so an UNTAGGED artifact is refused at the flag
    // table (`:787`) with "--lock-out applies only to a setfit-apr-v1 artifact, and {path}
    // does not carry the SetFit tag". No `*.apr` in this repository carries the tag — the
    // nearest, `tests/fixtures/setfit/slice_model.apr`, has `model_type: "Bert"` — and F-10
    // guarantees none can be produced. A probe run that way would report a refusal that has
    // nothing to do with the ordering it claims to measure.
    //
    // `write_tagged_decoy` exists for exactly this: a real APR v2 container carrying the tag
    // and nothing else, which is all the routing gate needs. The second test above establishes
    // that a `--split validation --lock-out` invocation on the decoy reaches step (2) and
    // names the corpus (its L6 leg), so the pre-flight this test is about IS on the path.
    //
    // WHAT THIS ADDS OVER THE TWO IN-PROCESS ORDERING TESTS. Those call
    // `commands::eval::setfit::run` with a struct they built themselves. They cannot observe
    // an argument clap never routed, a `--force` flag that reached the dispatcher but not the
    // adapter, or an exit code `main` mapped differently from the `CliError` the module
    // returned. This does, from a reaped `ExitStatus`.
    // ------------------------------------------------------------------------------
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    let decoy = write_tagged_decoy(root, "tagged.apr");
    let decoy_s = decoy.display().to_string();

    // ABSENT on purpose, and asserted absent: "the corpus was never opened" is only
    // distinguishable from "the corpus was opened and was fine" if opening it fails loudly.
    let absent_data = root.join("no-such-corpus");
    let absent_selection = root.join("no-such-selection.json");
    let absent_data_s = absent_data.display().to_string();
    let absent_selection_s = absent_selection.display().to_string();
    assert!(
        !absent_data.exists() && !absent_selection.exists(),
        "both Phase 2 inputs must genuinely be absent, or this test measures nothing"
    );

    // The OCCUPIED destination — a committed lock a prior validation run left behind.
    const PRIOR_COMMITMENT: &[u8] = br#"{"committed":"by a prior validation run"}"#;
    let occupied = root.join("committed-lock.json");
    fs::write(&occupied, PRIOR_COMMITMENT).expect("the pre-existing destination is writable");
    let occupied_s = occupied.display().to_string();

    // And a VACANT one, for the control below.
    let vacant = root.join("vacant-lock.json");
    let vacant_s = vacant.display().to_string();
    assert!(!vacant.exists(), "the control's destination must be vacant");

    // Every leg holds ALL other arguments constant and varies ONE thing, so each verdict is
    // attributable to that one thing rather than to the invocation being broken in general
    // (CLAUDE.md rule 6: one failing input is an anecdote).
    let invocation = |lock_out: &str, force: bool| {
        let mut argv = vec![
            "eval".to_string(),
            decoy_s.clone(),
            "--task".to_string(),
            "classify".to_string(),
            "--data".to_string(),
            absent_data_s.clone(),
            "--selection".to_string(),
            absent_selection_s.clone(),
            "--split".to_string(),
            "validation".to_string(),
            "--lock-out".to_string(),
            lock_out.to_string(),
        ];
        if force {
            argv.push("--force".to_string());
        }
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        run_apr(&refs, FAST_LIMIT)
    };

    // ------------------------------------------------------------------------------
    // W1 — THE ORDER ASSERTION. An occupied destination is refused, and the corpus is
    //      never named, because it was never opened.
    // ------------------------------------------------------------------------------
    let occupied_run = invocation(&occupied_s, false);
    occupied_run
        .expect_refusal(
            "a committed selection lock must not be replaced without --force. If this ever \
             exits 0, a test measurement taken under the old lock silently stops describing \
             the selection its file records",
        )
        .expect_no_panic("an occupied destination is a refusal, not a crash")
        .expect_mentions(
            "A selection lock is a COMMITMENT",
            "and the operator must get the BESPOKE wording, not the generic no-clobber \
             message — the generic one does not say what replacing a lock costs",
        )
        .expect_mentions(&occupied_s, "the refusal names the destination it refused")
        .expect_silent_about(
            &absent_data_s,
            "THE WR-10 ASSERTION: the refusal must NOT name the corpus, because the gate now \
             runs before the Phase 2 ingest. Pre-fix this invocation reported the missing \
             corpus — a true statement about the wrong problem, arrived at only after the \
             whole multi-candidate sweep",
        );
    assert_eq!(
        occupied_run.code(),
        EXIT_VALIDATION_FAILED,
        "the refusal is a typed ValidationFailed (exit 5), which a script can tell from a \
         missing file (3) or a broken artifact (6):\n{}",
        occupied_run.transcript()
    );
    // Printed so a SUMMARY transcribes the refusal from a RUN rather than from the format
    // string in the source — the convention the tooling test below established. Visible under
    // `-- --ignored lifecycle --nocapture`.
    println!(
        "[04-20] W1 spawned refusal (exit {}): {}",
        occupied_run.code(),
        occupied_run.combined().trim()
    );

    // ------------------------------------------------------------------------------
    // W2 — THE MECHANISM PROOF. The same invocation with a VACANT destination gets past
    //      the pre-flight and dies on the corpus instead.
    //
    // Without this, W1's silence about the corpus would be indistinguishable from "this
    // invocation is broken in some way that never reaches anything". Two runs differing in
    // one path produce two DIFFERENT refusals; that difference is the evidence.
    // ------------------------------------------------------------------------------
    let vacant_run = invocation(&vacant_s, false);
    vacant_run
        .expect_refusal("the corpus really is absent, so this must fail too — but LATER")
        .expect_no_panic("and still as a typed error")
        .expect_mentions(
            &absent_data_s,
            "a vacant destination must carry the run PAST the pre-flight and into the ingest, \
             which is what names the corpus. If this run is silent about the corpus too, the \
             two refusals are the same refusal and W1 proved nothing about ordering",
        );
    assert_ne!(
        occupied_run.combined().trim(),
        vacant_run.combined().trim(),
        "two invocations differing only in the --lock-out path must not produce the same \
         message"
    );
    assert!(
        !vacant.exists(),
        "and a run that failed before the decision must leave NO lock file — the file IS the \
         commitment"
    );

    // ------------------------------------------------------------------------------
    // W3 — --force overrides the pre-flight, at the process tier too.
    //
    // The pre-flight has to be gated on --force exactly like the two checks it joins, or an
    // operator who asked for the replacement would be refused by the new gate and the flag
    // would have stopped working. Same occupied destination, one extra flag.
    // ------------------------------------------------------------------------------
    let forced = invocation(&occupied_s, true);
    forced
        .expect_refusal("the corpus is still absent, so a forced run must still fail — LATER")
        .expect_mentions(
            &absent_data_s,
            "--force must carry the run past the pre-flight and into the ingest",
        )
        .expect_silent_about(
            "A selection lock is a COMMITMENT",
            "and the no-clobber refusal must NOT have fired: --force is the operator saying \
             they intend to replace the committed decision",
        );
    assert_eq!(
        fs::read(&occupied).expect("the occupied destination is readable"),
        PRIOR_COMMITMENT,
        "the prior commitment must be BYTE-IDENTICAL after all three runs: none of them \
         reached the decision, so none of them had anything to write"
    );
}

// ==========================================================================================
// Test 3 — D-01's generic-tooling promise, executed (research assumption A3)
// ==========================================================================================

#[test]
#[ignore = "integration weight: spawns the shipped binary four times. 04-10 runs it as \
            `cargo test -p apr-cli --features setfit --test setfit_cli_lifecycle -- --ignored \
            tooling`"]
fn setfit_cli_tooling_generic_apr_commands_survive_the_schema_owned_entries() {
    // ------------------------------------------------------------------------------
    // THE HONESTY RULE THIS TEST EXISTS TO SERVE
    //
    // D-01's entire justification for canonical tensor names is that `apr tensors`, `apr qa`
    // and `apr diff` keep working on a SetFit artifact unmodified — that is why the artifact
    // ships as an APR instead of as a format with its own tooling. 04-RESEARCH records this as
    // assumption A3 and flags the U8 `tokenizer.blob` entry as the thing most likely to break
    // it, because a tool written for LLM weights may assume every entry is numeric and
    // dequantizable. **No plan had ever executed it.** An assumption that has never been run
    // is not evidence, and a justification resting on one is an argument, not a measurement.
    //
    // WHAT THIS TEST CAN AND CANNOT SAY. The subject is a container carrying the three
    // schema-owned entries — `setfit.head.weight`, `setfit.head.bias` and the U8
    // `tokenizer.blob` — written by core's production `AprV2Writer`. It is NOT a trained
    // model, and cannot be, because no `setfit-apr-v1` artifact can be produced on this host
    // (F-10). For A3 that distinction does not matter: the assumption is about ENTRY NAMES and
    // DTYPES surviving generic tooling, and those are present and real here. It does mean this
    // test says nothing about tooling that reads the artifact DOCUMENT, and it says so rather
    // than leaving the reader to assume otherwise.
    // ------------------------------------------------------------------------------
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path();
    // Rebuilt in this test's own tempdir so the two tests stay independent.
    let decoy = write_tagged_decoy(root, "model.apr");
    let decoy_s = decoy.display().to_string();

    // ------------------------------------------------------------------------------
    // (1) `apr tensors` — the A3 falsification attempt.
    //
    // All three schema-owned entries must be NAMED in the output. Omitting the U8 blob would
    // be the failure mode A3 predicts, and it is a silent one: a tool that skipped it would
    // still exit 0 and still look like it worked.
    // ------------------------------------------------------------------------------
    let tensors = run_apr(&["tensors", &decoy_s], FAST_LIMIT);
    tensors
        .expect_success(
            "D-01 promises `apr tensors` works on a SetFit artifact unmodified. A nonzero exit \
             here is a FINDING against D-01, not a test to relax",
        )
        .expect_no_panic("and it must not crash on the U8 entry")
        .expect_mentions(
            "tokenizer.blob",
            "THE A3 ASSERTION: the U8 pseudo-tensor must be NAMED. A tool that assumed every \
             entry is numeric would omit it here and still exit 0",
        )
        .expect_mentions(
            "setfit.head.weight",
            "and the head weight, which the schema reserves",
        )
        .expect_mentions("setfit.head.bias", "and the head bias");

    // Printed so this plan's SUMMARY transcribes the rows from a run rather than from memory.
    // Visible under `-- --ignored tooling --nocapture`.
    for line in tensors.stdout.lines().filter(|line| {
        line.contains("tokenizer.blob")
            || line.contains("setfit.head.weight")
            || line.contains("setfit.head.bias")
    }) {
        println!("[04-15] apr tensors row: {}", line.trim());
    }

    // ------------------------------------------------------------------------------
    // (2) `apr inspect`, human mode — the metadata path, with no --json to shape it.
    // ------------------------------------------------------------------------------
    let inspected = run_apr(&["inspect", &decoy_s], FAST_LIMIT);
    inspected
        .expect_success("`apr inspect` is the generic metadata command D-06 points a user at")
        .expect_no_panic("and it must not crash on the SetFit tag");
    assert!(
        !inspected.stdout.trim().is_empty(),
        "an inspect that exits 0 and prints nothing has not inspected anything:\n{}",
        inspected.transcript()
    );

    // ------------------------------------------------------------------------------
    // (3) `apr qa` — RECORDED, not asserted green.
    //
    // qa's gates were written for LLM artifacts and may legitimately not apply to a
    // classifier. The phase's requirement is to KNOW which, so the verdict is recorded
    // verbatim in this plan's SUMMARY and a nonzero exit is surfaced as a finding rather than
    // suppressed. What is asserted here is only what must hold either way: the command
    // terminates within its bound and returns a typed verdict instead of panicking.
    // ------------------------------------------------------------------------------
    let qa = run_apr(&["qa", &decoy_s, "--json"], FAST_LIMIT);
    qa.expect_no_panic(
        "whatever qa's verdict is, a panic is a defect: an operator running the first tool in \
         CLAUDE.md's debugging table must get an answer, not a backtrace",
    );
    assert!(
        !qa.combined().trim().is_empty(),
        "qa must SAY something — a silent exit is not a verdict, and the phase cannot record \
         what it did not report:\n{}",
        qa.transcript()
    );

    // ------------------------------------------------------------------------------
    // (4) THE CONTROL, and the finding it turns from an anecdote into a cause.
    //
    // qa refuses the container above with `APR missing embedded tokenizer` (exit 5). The
    // tempting reading is "the U8 tokenizer.blob broke qa" — assumption A3, falsified. That
    // reading is WRONG, and one input could not have told the difference (CLAUDE.md rule 6).
    //
    // The control is the committed plain-Bert `slice_model.apr`: no SetFit tag, no
    // setfit.head.* entries, no U8 blob. It gets the IDENTICAL exit code and the IDENTICAL
    // message. So the cause is not the SetFit entries at all — `apr qa`'s first gate is
    // `load_embedded_bpe_tokenizer()` followed by `run_inference` with a token budget and a
    // top-k. qa is a GENERATIVE-model gate, and it does not apply to any encoder-only APR,
    // classifier or not.
    //
    // Recorded, not suppressed: this is a real limit on D-01's "generic tooling keeps working
    // unmodified", it is narrower than "the U8 entry breaks tooling", and it is asserted here
    // so that the day either verdict changes, this test turns red and hands its author the
    // measurement rather than a stale sentence in a summary.
    // ------------------------------------------------------------------------------
    let plain_apr = slice_fixture_dir().join("slice_model.apr");
    assert!(
        plain_apr.is_file(),
        "the control fixture must exist at {} — without it the qa verdict above is an \
         anecdote about one file",
        plain_apr.display()
    );
    let qa_control = run_apr(
        &["qa", &plain_apr.display().to_string(), "--json"],
        FAST_LIMIT,
    );
    qa_control.expect_no_panic("the control must fail the same WAY, not differently");

    const QA_GATE: &str = "APR missing embedded tokenizer";
    assert_eq!(
        qa.status.code(),
        qa_control.status.code(),
        "the SetFit-shaped container and a plain encoder-only APR must get the SAME qa \
         verdict. If they ever diverge, the SetFit entries have started to matter to qa and \
         assumption A3 needs re-measuring — which is the whole point of keeping this control:\
         \n  decoy:   {}\n  control: {}",
        qa.transcript(),
        qa_control.transcript()
    );
    assert!(
        qa.combined().contains(QA_GATE) && qa_control.combined().contains(QA_GATE),
        "and both must name the SAME gate. qa's first check is an embedded BPE tokenizer plus \
         generation; neither file has one, and neither is a generative model:\n  decoy:   {}\n\
           control: {}",
        qa.transcript(),
        qa_control.transcript()
    );

    // Printed so the recorded verdict in the SUMMARY is transcribed from a run rather than
    // remembered. Visible under `-- --ignored tooling --nocapture`.
    println!(
        "[04-15] apr qa verdict, recorded verbatim:\n  \
         setfit-shaped container -> exit {:?}: {}\n  \
         plain-Bert APR control  -> exit {:?}: {}",
        qa.status.code(),
        qa.stderr.trim(),
        qa_control.status.code(),
        qa_control.stderr.trim()
    );
}

// ==========================================================================================
// Source guards — cheap, and they run in the DEFAULT invocation
// ==========================================================================================

/// This file's own source. Read from disk rather than `include_str!` of a literal path so the
/// needles below cannot match a path literal in this very file.
fn lifecycle_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("setfit_cli_lifecycle.rs");
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("this file must be readable at {}: {error}", path.display()))
}

/// Assemble a needle at RUNTIME so it cannot match the scan's own source.
fn needle(fragments: &[&str]) -> String {
    fragments.concat()
}

/// CODE lines only — every comment and doc comment stripped.
///
/// # This filter is not fastidiousness, it is F-05
///
/// Three guards in this phase turned red on their own documentation, because a module header
/// that EXPLAINS a rule necessarily spells the thing the rule forbids. This file's header names
/// the pinned-binary macro twice while explaining why it is the only admissible spelling, and a
/// scan over the whole file would count three occurrences of something that appears once in the
/// code. Every filtered scan below therefore carries a non-vacuity assertion, because a filter
/// that ate the module would make all of them pass.
fn code_lines(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn setfit_cli_the_binary_under_test_is_pinned_and_spawned_from_exactly_one_site() {
    let whole = lifecycle_source();
    // Non-vacuity, twice over. A wrong path or an empty read would make every count below zero
    // and every assertion pass for the wrong reason; and a filter that swallowed the module
    // would do the same while looking like it worked.
    assert!(
        whole.len() > 10_000,
        "the self-scan must actually have read this file; got {} bytes",
        whole.len()
    );
    let source = code_lines(&whole);
    assert!(
        source.len() > whole.len() / 4,
        "the comment filter ate the module: {} code bytes out of {} total. Every count below \
         would then be a vacuous zero",
        source.len(),
        whole.len()
    );
    let harness = needle(&["fn run_", "apr(args: &[&str]"]);
    assert!(
        source.contains(&harness),
        "and the filter must have KEPT the code — the spawn harness's own signature is the \
         positive control"
    );

    // ONE spawn site. The plan's criterion asks for five occurrences of the cargo env var; one
    // constant used everywhere is a stronger guarantee than five copies, because five copies
    // are five places a PATH lookup could later be introduced without anyone noticing. What
    // makes that true is this pair of counts, not the doc comment claiming it.
    let spawn_site = needle(&["Command::", "new(APR_BIN)"]);
    assert_eq!(
        source.matches(&spawn_site).count(),
        1,
        "exactly one process-spawning site, and it takes the pinned constant"
    );
    let any_spawn = needle(&["Command::", "new("]);
    assert_eq!(
        source.matches(&any_spawn).count(),
        1,
        "and no OTHER command is constructed anywhere in this file — a second one could name \
         a shell, a PATH lookup, or a different binary"
    );

    // The constant is the cargo-computed absolute path, not a name resolved at runtime.
    let pinned = needle(&["env!(\"CARGO_BIN_EXE_", "apr\")"]);
    assert_eq!(
        source.matches(&pinned).count(),
        1,
        "the binary is pinned by cargo's absolute path exactly once (CLAUDE.md rule 3)"
    );

    // No bare command-name string anywhere. A `\"apr\"` literal would be a PATH lookup waiting
    // to happen, and four `apr` binaries have coexisted on this dev box.
    // The fragments are split mid-token on purpose: `needle(&["\"", "apr", "\""])` would
    // itself contain the literal it forbids, and the scan would report its own guard.
    let bare = needle(&["\"a", "pr\""]);
    assert_eq!(
        source.matches(&bare).count(),
        0,
        "no bare command-name string literal may appear in this file — that is a PATH lookup \
         waiting to happen, and four such binaries have coexisted on this dev box"
    );

    // And no status may be read through a pipe. `Stdio::piped()` sends the child's output to
    // THIS process; a `Stdio::from` of another child's stdout would be a pipeline, whose exit
    // status is the last command's (CLAUDE.md rule 1).
    let pipeline = needle(&["Stdio::", "from("]);
    assert_eq!(
        source.matches(&pipeline).count(),
        0,
        "no child's output is wired into another child — that would make every status read \
         here the status of the wrong process"
    );
}
