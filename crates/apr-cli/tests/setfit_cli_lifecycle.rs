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
