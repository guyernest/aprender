//! `apr setfit bench run` — ONE benchmark cell in, ONE verified row out (D-13, EVAL-03/EVAL-05).
//!
//! # This file is a FILESYSTEM ADAPTER, exactly like `setfit_train.rs`
//!
//! Every semantic decision lives behind a library door. The row schema and its digest
//! discipline are `entrenar::train::setfit::bench_row`; the metric assembly is
//! `bench_metrics::assemble_quality_block`; the predictions are
//! `apr_evaluate::evaluate_rows_from_artifact`; the training lifecycle is `SetFitRun`'s four
//! transitions; the test-split gate is Phase 3's `create_selection_lock` -> `mint_test_token`
//! -> `CanonicalTestAccess::grant` chain, consumed here and reimplemented nowhere. This module
//! reads files, spawns one child, times things, and writes files.
//!
//! # The three modes, and why they are three
//!
//! 1. **EXECUTION.** Train, write, RELOAD, measure, evaluate, emit. One process per cell.
//! 2. **RECORD** (`--record`). Ingest a row file that a DIFFERENT host executed, verifying its
//!    digest, schema, cell identity and filename agreement before recording it. Nothing is
//!    executed. This is the D-09 transport path: the LoRA cells run on a GPU host and their
//!    rows travel back as bytes.
//! 3. **COLD PROBE** (`--cold-probe`). A dedicated fresh child whose entire job is to load one
//!    artifact, classify once, print `COLD_LATENCY_MS=<f64>` and exit.
//!
//! # Why the cold probe is a CHILD and not the first classify here
//!
//! The contract's resource protocol forbids measuring cold latency in the process that just
//! finished training: that process's page cache is warm, its allocator arenas are populated
//! and the encoder is already resident, so its "first" classify is operationally WARM. The
//! same argument applies to peak RSS, and more sharply — a kernel high-water mark is process
//! CUMULATIVE, so `VmHWM` read in a process that trained and then inferred reports the
//! TRAINING peak while claiming to report the inference peak. Hence two fields with two
//! mechanisms, never one pooled number.
//!
//! The child is spawned under `/usr/bin/time` (`-l` on macOS, `-v` on Linux) and the parent
//! reads the child's TRUE kernel high-water mark out of that output. Both platforms therefore
//! report an exact HWM of an equivalently-scoped process, which is what makes a macOS SetFit
//! row and a Linux LoRA row comparable at all. `mach_task_basic_info` was proposed in review
//! and REJECTED: this workspace sets `unsafe_code = "forbid"`.
//!
//! # Bounds
//!
//! Every file this module reads is bounded from its stat'd length BEFORE the read
//! (T-05-09-05), on the reasoning `eval/setfit.rs` states: a bound applied after the
//! allocation is not a bound on the work an attacker can request.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use entrenar::train::setfit::bench_row::{
    sha256_hex, BenchRow, CellKey, Method, RecordOutcome, RunManifest, BENCH_SEEDS, BENCH_SHOTS,
    CLAIMS_CONTRACT_ID,
};

use crate::commands::setfit_train::{atomic_write, refuse_existing_output};
use crate::error::{CliError, Result};

// ==========================================================================================
// Bounds and layout
// ==========================================================================================

/// The house cap for every small file this module reads (04-18's 16 MiB pattern).
///
/// The `setfit-apr-v1` artifact is NOT read through this: it goes through
/// `setfit_io::read_setfit_apr_file_bounded`, the ONE bounded artifact door.
pub(crate) const MAX_BENCH_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// Row files live here, relative to `--bench-dir`.
pub(crate) const ROWS_DIR: &str = "rows";

/// Committed SetFit selection-lock records live here.
pub(crate) const LOCKS_DIR: &str = "locks";

/// Append-only LoRA candidate ledgers live here.
pub(crate) const LEDGER_DIR: &str = "ledger";

/// The pre-declared expectation set lives here.
pub(crate) const RUN_MANIFEST_FILE: &str = "run-manifest.json";

/// The machine-readable line the cold-probe child prints, and its parent parses.
///
/// A CONTRACT between two processes, so it is a constant rather than a format string typed
/// twice: the child writes `COLD_LATENCY_MS=<f64>` and the parent looks for exactly this
/// prefix. A drift between the two spellings would surface as "the probe produced no
/// latency", which reads like a probe failure and is not one.
pub(crate) const COLD_LATENCY_PREFIX: &str = "COLD_LATENCY_MS=";

// ==========================================================================================
// The request
// ==========================================================================================

/// Everything the three modes carry, resolved from clap.
#[derive(Debug, Clone)]
pub(crate) struct BenchRunArgs<'a> {
    /// `setfit` or `lora` (execution mode).
    pub(crate) method: Option<&'a str>,
    /// Examples per class (execution mode).
    pub(crate) shots: Option<u32>,
    /// The contracted seed (execution mode).
    pub(crate) seed: Option<u32>,
    /// The attested prepared dataset directory (execution mode).
    pub(crate) data: Option<&'a Path>,
    /// The selection manifest — the pairing key (execution mode).
    pub(crate) selection: Option<&'a Path>,
    /// Where rows, locks, ledgers and the run manifest live.
    pub(crate) bench_dir: Option<&'a Path>,
    /// The pinned encoder checkout (execution mode, `setfit`).
    pub(crate) model_dir: Option<&'a Path>,
    /// Optional training configuration (execution mode).
    pub(crate) config: Option<&'a Path>,
    /// Replace an existing row file or ledger.
    pub(crate) force: bool,
    /// Ingest a transported row file — no execution.
    pub(crate) record: Option<&'a Path>,
    /// The artifact the cold-probe child measures.
    pub(crate) cold_probe: Option<&'a Path>,
    /// The LoRA base model, when the cold probe measures a LoRA cell.
    pub(crate) cold_probe_base: Option<&'a Path>,
    /// The single text the cold probe classifies.
    pub(crate) probe_text: Option<&'a Path>,
    /// The global `--json`.
    pub(crate) json: bool,
}

/// Run one benchmark-matrix command.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] for a missing or contradictory flag set, an uncontracted
/// cell, an existing row file without `--force`, and every typed library refusal;
/// [`CliError::InvalidFormat`] for an over-cap file; [`CliError::Io`] for a read or write
/// failure.
pub(crate) fn run(args: &BenchRunArgs<'_>) -> Result<()> {
    // The COLD PROBE first, because it is the mode with the fewest obligations: it takes no
    // bench directory, writes nothing, and must not pay for any check the other two need.
    if let Some(artifact) = args.cold_probe {
        return cold_probe::run(artifact, args.cold_probe_base, args.probe_text);
    }

    // Both remaining modes need a bench directory. Checked here rather than in each, so the
    // message is one message.
    let bench_dir = args.bench_dir.ok_or_else(|| {
        CliError::ValidationFailed(
            "--bench-dir <DIR> is required. Rows are not a directory listing: completeness is \
             defined by the run manifest that lives there, and a listing can only report what \
             is present, never what is missing."
                .to_string(),
        )
    })?;

    if let Some(row_file) = args.record {
        return record_mode(bench_dir, row_file, args.force, args.json);
    }

    execute_mode(bench_dir, args)
}

// ==========================================================================================
// Cell identity — a non-contracted cell is a TYPED REFUSAL, never a run
// ==========================================================================================

/// Resolve `(method, shots, seed)` into a contracted cell, or refuse naming the contract.
///
/// The three components are checked SEPARATELY even though `CellKey::is_contracted` would
/// answer all three at once: an operator who typed `--seed 42` needs to be told that 42 is not
/// a contracted seed, not that "the cell is outside the matrix". The library's own combined
/// refusal remains the backstop at [`BenchRow::from_bytes`].
///
/// # Errors
///
/// [`CliError::ValidationFailed`] naming [`CLAIMS_CONTRACT_ID`] and the accepted values.
pub(crate) fn resolve_cell(
    method: Option<&str>,
    shots: Option<u32>,
    seed: Option<u32>,
) -> Result<CellKey> {
    let method_tag = method.ok_or_else(|| {
        CliError::ValidationFailed(format!(
            "--method <setfit|lora> is required by {CLAIMS_CONTRACT_ID}"
        ))
    })?;
    let method = Method::from_tag(method_tag).ok_or_else(|| {
        CliError::ValidationFailed(format!(
            "--method `{method_tag}` is not one of the two methods {CLAIMS_CONTRACT_ID} \
             compares: `setfit` or `lora`."
        ))
    })?;

    let shots = shots.ok_or_else(|| {
        CliError::ValidationFailed(format!(
            "--shots <SHOTS> is required; {CLAIMS_CONTRACT_ID} contracts {BENCH_SHOTS:?}"
        ))
    })?;
    if !BENCH_SHOTS.contains(&shots) {
        return Err(CliError::ValidationFailed(format!(
            "--shots {shots} is not a contracted shot count. {CLAIMS_CONTRACT_ID} declares \
             exactly {BENCH_SHOTS:?}, and a cell outside that set is not part of the claim the \
             40-cell matrix makes — running it would produce a row the report has no slot for."
        )));
    }

    let seed = seed.ok_or_else(|| {
        CliError::ValidationFailed(format!(
            "--seed <SEED> is required; {CLAIMS_CONTRACT_ID} contracts {BENCH_SEEDS:?}"
        ))
    })?;
    if !BENCH_SEEDS.contains(&seed) {
        return Err(CliError::ValidationFailed(format!(
            "--seed {seed} is not a contracted seed. {CLAIMS_CONTRACT_ID} declares exactly \
             {BENCH_SEEDS:?}. Note that 42 is deliberately NOT among them: a tool that defaults \
             a seed to 42 samples outside the protocol while appearing to honour it."
        )));
    }

    Ok(CellKey::new(method, shots, seed))
}

/// The row filename grammar, produced by ONE function.
///
/// `{method}-s{shots}-seed{seed}.json`. Two spellings of a filename are two filenames, and the
/// resume path compares a recorded digest against the file this name resolves to — so a drift
/// here would silently make every cell look un-run.
#[must_use]
pub(crate) fn row_file_name(cell: CellKey) -> String {
    format!(
        "{}-s{}-seed{}.json",
        cell.method.tag(),
        cell.shots,
        cell.seed
    )
}

/// The committed SetFit lock record's path, RELATIVE to the bench directory.
#[must_use]
pub(crate) fn lock_relative_path(cell: CellKey) -> String {
    format!(
        "{LOCKS_DIR}/{}-s{}-seed{}.lock.json",
        cell.method.tag(),
        cell.shots,
        cell.seed
    )
}

/// The LoRA candidate ledger's path, RELATIVE to the bench directory.
#[must_use]
pub(crate) fn ledger_relative_path(cell: CellKey) -> String {
    format!(
        "{LEDGER_DIR}/{}-s{}-seed{}.jsonl",
        cell.method.tag(),
        cell.shots,
        cell.seed
    )
}

// ==========================================================================================
// Bounded reads
// ==========================================================================================

/// Read a small file, refused from its DECLARED length before a byte is read.
///
/// # Errors
///
/// [`CliError::FileNotFound`], [`CliError::NotAFile`], [`CliError::InvalidFormat`] above the
/// cap, or [`CliError::Io`].
pub(crate) fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::FileNotFound(path.to_path_buf())
        } else {
            CliError::Io(error)
        }
    })?;
    if !metadata.is_file() {
        return Err(CliError::NotAFile(path.to_path_buf()));
    }
    if metadata.len() > MAX_BENCH_FILE_BYTES {
        return Err(CliError::InvalidFormat(format!(
            "{}: declares {} bytes against the bench cap of {MAX_BENCH_FILE_BYTES}; refused \
             from its declared length, before a byte is read.",
            path.display(),
            metadata.len()
        )));
    }
    let file = fs::File::open(path).map_err(CliError::Io)?;
    let mut bytes = Vec::new();
    // `+ 1` so a length that LIED is detected rather than silently truncated into a payload
    // that happens to parse.
    file.take(MAX_BENCH_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(CliError::Io)?;
    if bytes.len() as u64 > MAX_BENCH_FILE_BYTES {
        return Err(CliError::InvalidFormat(format!(
            "{}: the stream exceeded {MAX_BENCH_FILE_BYTES} bytes, so its declared length lied",
            path.display()
        )));
    }
    Ok(bytes)
}

// ==========================================================================================
// The run manifest — read, initialise-on-first-touch, record, write back
// ==========================================================================================

/// Load the bench directory's run manifest, DECLARING it on first touch.
///
/// Declaring rather than erroring is deliberate: the expectation set is derived in code from
/// the contract's constants, so a fresh directory has exactly one correct manifest and asking
/// the operator to run a separate `--declare` step would only create a way to skip it.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] for a manifest whose digest, schema or expectation set the
/// library refuses; [`CliError::Io`] or [`CliError::InvalidFormat`] for the read.
pub(crate) fn load_or_declare_manifest(bench_dir: &Path) -> Result<RunManifest> {
    let path = bench_dir.join(RUN_MANIFEST_FILE);
    if !path.exists() {
        return Ok(RunManifest::declare());
    }
    let bytes = read_bounded(&path)?;
    RunManifest::from_bytes(&bytes).map_err(|error| {
        CliError::ValidationFailed(format!(
            "{}: {error}\nThe run manifest defines what a complete run IS. It is not \
             regenerated on a mismatch, because regenerating it would erase the pre-declared \
             expectation set that makes an omitted cell visible at all.",
            path.display()
        ))
    })
}

/// Record `row_sha256` against `cell` and persist the manifest.
///
/// `RunManifest::record` is idempotent on an identical digest (the resume path after a crash
/// at cell 35) and a typed error on a differing one (the collision). Neither decision is made
/// here; this only surfaces the refusal with the path the library cannot know.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] for an unknown cell or a differing re-record;
/// [`CliError::Io`] for the write.
pub(crate) fn record_in_manifest(
    bench_dir: &Path,
    cell: CellKey,
    row_sha256: &str,
) -> Result<RecordOutcome> {
    let mut manifest = load_or_declare_manifest(bench_dir)?;
    let outcome = manifest.record(cell, row_sha256).map_err(|error| {
        CliError::ValidationFailed(format!(
            "{}: {error}",
            bench_dir.join(RUN_MANIFEST_FILE).display()
        ))
    })?;
    let bytes = manifest.to_file_bytes().map_err(|error| {
        CliError::ValidationFailed(format!("the run manifest did not serialize: {error}"))
    })?;
    // `force = true`: the manifest is a LIVE accumulator that every cell updates, so refusing
    // to replace it would make the second cell of any run fail. The no-clobber discipline
    // belongs to the ROW files, which are write-once evidence.
    atomic_write(&bench_dir.join(RUN_MANIFEST_FILE), &bytes, true)?;
    Ok(outcome)
}

// ==========================================================================================
// The resource protocol (EVAL-05) — three distinct measurement surfaces
// ==========================================================================================

/// The contracted resource measurements and the mechanisms that produced them.
///
/// Read `contracts/setfit-benchmark-claims-v1.yaml`'s `resource_protocol` beside this module:
/// every boundary here is implemented from that text, and the mechanism strings are mandatory
/// fields rather than a line in a methods section because a number without its measurement
/// boundary is not a measurement.
pub(crate) mod resource {
    use std::path::Path;
    use std::process::Command;
    use std::time::Instant;

    use entrenar::train::setfit::bench_row::{
        MECHANISM_CHILD_MAX_RSS_TIME_L, MECHANISM_CHILD_MAX_RSS_VM_HWM,
    };

    use crate::error::{CliError, Result};

    use super::COLD_LATENCY_PREFIX;

    // ---- The four mechanism strings, all named in this file --------------------------------

    /// Linux `/proc/self/status` `VmHWM`, read INSIDE the measured process.
    ///
    /// This value is labelled TRAIN peak and nothing else. `VmHWM` is process-CUMULATIVE, so
    /// in a process that trains, then reloads, then infers it reports the TRAINING peak.
    pub(crate) const MECHANISM_VM_HWM: &str = "vm_hwm";

    /// The sampled FALLBACK's prefix; the ACTUAL measured rate is appended.
    ///
    /// A poll at any finite rate can miss the peak entirely and the bias is one-directional
    /// (it can only understate), so the contract labels this a sampled LOWER BOUND and forbids
    /// comparing it against a `child_max_rss_*` figure.
    pub(crate) const MECHANISM_SAMPLED_PREFIX: &str = "sysinfo_sampled_";

    /// The two exact-kernel-high-water-mark mechanisms, re-exported so all four strings this
    /// module can emit are visible in one place.
    pub(crate) const MECHANISM_CHILD_TIME_L: &str = MECHANISM_CHILD_MAX_RSS_TIME_L;
    /// See [`MECHANISM_CHILD_TIME_L`].
    pub(crate) const MECHANISM_CHILD_VM_HWM: &str = MECHANISM_CHILD_MAX_RSS_VM_HWM;

    /// The warm measurement's sample count, after the contracted warmups.
    pub(crate) const WARM_MEASURED_COUNT: usize = 10;

    /// The batch size the throughput pass declares.
    ///
    /// RECORDED on the row because throughput without a batch size is not a comparable number
    /// (PF-008 warning sign 4).
    pub(crate) const THROUGHPUT_BATCH_SIZE: u32 = 32;

    /// The sampled fallback's target poll rate, in hertz.
    pub(crate) const SAMPLE_TARGET_HZ: u32 = 20;

    // ---- Peak RSS --------------------------------------------------------------------------

    /// One peak-RSS measurement with its boundary.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct PeakRss {
        /// The measured high-water mark, in bytes.
        pub(crate) bytes: u64,
        /// Which mechanism produced it.
        pub(crate) mechanism: String,
        /// Present exactly when `mechanism` is a sampled one.
        pub(crate) sample_interval_hz: Option<u32>,
    }

    /// Parse Linux `/proc/self/status`'s `VmHWM` line into BYTES.
    ///
    /// The kernel reports kilobytes; the row records bytes. Returns `None` when the field is
    /// absent or unparseable, which is a real state on a kernel that does not export it — and
    /// a caller that silently substituted zero would publish "this run used no memory".
    #[must_use]
    pub(crate) fn parse_vm_hwm_bytes(status_text: &str) -> Option<u64> {
        for line in status_text.lines() {
            // `strip_prefix` on the TRIMMED line, not `contains`: `VmHWMX:` and a line
            // mentioning VmHWM in prose must not match, and `contains` would accept both.
            let Some(rest) = line.trim_start().strip_prefix("VmHWM:") else {
                continue;
            };
            let mut fields = rest.split_whitespace();
            let value: u64 = fields.next()?.parse().ok()?;
            // The unit is part of the fact. A kernel that started reporting something other
            // than kB would otherwise be misread by a factor of 1024 in silence.
            if fields.next()? != "kB" {
                return None;
            }
            return value.checked_mul(1024);
        }
        None
    }

    /// Parse a `/usr/bin/time` block into `(bytes, mechanism)`.
    ///
    /// # The two platforms report DIFFERENT UNITS, and that is the whole hazard
    ///
    /// macOS `/usr/bin/time -l` prints `<value>  maximum resident set size` in **BYTES**.
    /// GNU `/usr/bin/time -v` prints `Maximum resident set size (kbytes): <value>` in
    /// **KILOBYTES**. The identical numeral therefore means two different quantities depending
    /// on which block it came from, so the mechanism is DERIVED FROM THE FORM THAT PARSED
    /// rather than from a `cfg!` — a parent that assumed its own platform would mislabel every
    /// number the moment the two ever ran on different hosts, which is exactly the D-09
    /// arrangement this benchmark uses.
    ///
    /// This parser ships a must-match / must-not-match case table (CLAUDE.md rule 7). The
    /// neighbouring lines in both blocks are the trap: macOS's `average shared memory size`
    /// and GNU's `Average resident set size (kbytes)` are one word away from matching.
    #[must_use]
    pub(crate) fn parse_child_max_rss(time_output: &str) -> Option<(u64, &'static str)> {
        for line in time_output.lines() {
            let trimmed = line.trim();
            // GNU form FIRST. It is prefix-anchored and unit-explicit, so it cannot be
            // confused with the macOS form (which ends with the label, not the number).
            if let Some(rest) = trimmed.strip_prefix("Maximum resident set size (kbytes):") {
                let kib: u64 = rest.trim().parse().ok()?;
                return kib.checked_mul(1024).map(|b| (b, MECHANISM_CHILD_VM_HWM));
            }
            // macOS form: the value PRECEDES a lowercase label that ends the line.
            if let Some(value) = trimmed.strip_suffix("maximum resident set size") {
                let bytes: u64 = value.trim().parse().ok()?;
                return Some((bytes, MECHANISM_CHILD_TIME_L));
            }
        }
        None
    }

    /// Parse the cold-probe child's one machine-readable line.
    #[must_use]
    pub(crate) fn parse_cold_latency_ms(stdout: &str) -> Option<f64> {
        stdout
            .lines()
            .find_map(|line| line.trim().strip_prefix(COLD_LATENCY_PREFIX))
            .and_then(|value| value.trim().parse().ok())
    }

    /// The median of a sample. Even counts average the two middles.
    ///
    /// Takes the samples by value and sorts in place: a median that mutated its caller's vector
    /// would reorder the latency series a reader might also want to print.
    #[must_use]
    pub(crate) fn median(mut samples: Vec<f64>) -> Option<f64> {
        if samples.is_empty() {
            return None;
        }
        samples.sort_by(f64::total_cmp);
        let mid = samples.len() / 2;
        if samples.len() % 2 == 1 {
            Some(samples[mid])
        } else {
            Some((samples[mid - 1] + samples[mid]) / 2.0)
        }
    }

    // ---- The TRAIN peak, measured inside THIS process ---------------------------------------

    /// A running peak-RSS observation over the training phase.
    ///
    /// On Linux this is a no-op that reads `VmHWM` at the end — the kernel already keeps the
    /// high-water mark, and a sampler would only produce a worse estimate of a number that is
    /// exact. Everywhere else a background thread polls `sysinfo` and reports the maximum it
    /// SAW, together with the rate it actually achieved.
    pub(crate) struct TrainRssSampler {
        #[cfg(not(target_os = "linux"))]
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        #[cfg(not(target_os = "linux"))]
        handle: Option<std::thread::JoinHandle<(u64, u32)>>,
    }

    impl TrainRssSampler {
        /// Begin observing. Call BEFORE the training invocation.
        #[cfg(target_os = "linux")]
        #[must_use]
        pub(crate) fn start() -> Self {
            Self {}
        }

        /// Begin observing. Call BEFORE the training invocation.
        #[cfg(not(target_os = "linux"))]
        #[must_use]
        pub(crate) fn start() -> Self {
            use std::sync::atomic::{AtomicBool, Ordering};
            use std::sync::Arc;

            let stop = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&stop);
            let handle = std::thread::spawn(move || {
                use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

                let interval =
                    std::time::Duration::from_micros(1_000_000 / u64::from(SAMPLE_TARGET_HZ));
                let pid = Pid::from_u32(std::process::id());
                let mut system = System::new();
                let mut peak = 0_u64;
                let mut samples = 0_u64;
                let started = Instant::now();
                while !flag.load(Ordering::Relaxed) {
                    system.refresh_processes_specifics(
                        ProcessesToUpdate::Some(&[pid]),
                        true,
                        ProcessRefreshKind::new().with_memory(),
                    );
                    if let Some(process) = system.process(pid) {
                        peak = peak.max(process.memory());
                    }
                    samples += 1;
                    std::thread::sleep(interval);
                }
                // The ACTUAL achieved rate, not the requested one. A thread that was starved
                // and polled at 3 Hz must not report 20: the mechanism string is the reader's
                // only handle on how much of the peak the sampler could have seen.
                let elapsed = started.elapsed().as_secs_f64();
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let achieved_hz = if elapsed > 0.0 {
                    ((samples as f64 / elapsed).round() as u32).max(1)
                } else {
                    SAMPLE_TARGET_HZ
                };
                (peak, achieved_hz)
            });
            Self {
                stop,
                handle: Some(handle),
            }
        }

        /// Stop observing and report the peak with its mechanism.
        #[cfg(target_os = "linux")]
        #[must_use]
        pub(crate) fn finish(self) -> PeakRss {
            let bytes = std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|text| parse_vm_hwm_bytes(&text))
                .unwrap_or(0);
            PeakRss {
                bytes,
                mechanism: MECHANISM_VM_HWM.to_string(),
                sample_interval_hz: None,
            }
        }

        /// Stop observing and report the peak with its mechanism.
        #[cfg(not(target_os = "linux"))]
        #[must_use]
        pub(crate) fn finish(mut self) -> PeakRss {
            use std::sync::atomic::Ordering;

            self.stop.store(true, Ordering::Relaxed);
            let (bytes, hz) = self
                .handle
                .take()
                .and_then(|handle| handle.join().ok())
                .unwrap_or((0, SAMPLE_TARGET_HZ));
            PeakRss {
                bytes,
                // The interval travels IN the mechanism string as well as in its own field, so
                // a reader looking at either one sees the boundary.
                mechanism: format!("{MECHANISM_SAMPLED_PREFIX}{hz}"),
                sample_interval_hz: Some(hz),
            }
        }
    }

    // ---- COLD latency + INFERENCE peak, in a dedicated fresh child ---------------------------

    /// What the cold-measurement child produced.
    #[derive(Debug, Clone, PartialEq)]
    pub(crate) struct ColdMeasurement {
        /// ONE classify, in a process that did nothing else.
        pub(crate) cold_latency_ms: f64,
        /// That process's TRUE kernel high-water mark.
        pub(crate) peak_rss_bytes: u64,
        /// Which of the two exact mechanisms produced it.
        pub(crate) peak_rss_mechanism: &'static str,
    }

    /// The `/usr/bin/time` invocation for this host.
    ///
    /// Absolute, never a bare `time`: `time` is a shell BUILTIN in bash and zsh and the
    /// builtin does not implement `-l` or `-v` at all. Resolving it through `PATH` would find
    /// whichever `time` a developer's environment happened to expose.
    const TIME_BIN: &str = "/usr/bin/time";

    /// `-l` reports the max RSS on macOS; `-v` does on GNU coreutils.
    #[cfg(target_os = "macos")]
    const TIME_FLAG: &str = "-l";
    /// See the macOS twin.
    #[cfg(not(target_os = "macos"))]
    const TIME_FLAG: &str = "-v";

    /// Spawn the dedicated cold-measurement child and read both numbers off it.
    ///
    /// `artifact` is the written production artifact; `base` is the LoRA base model when this
    /// is a LoRA cell and `None` for a standalone `setfit-apr-v1`.
    ///
    /// # Errors
    ///
    /// [`CliError::InferenceFailed`] when the child cannot be spawned, exits non-zero, or
    /// produces output neither parser recognises.
    pub(crate) fn measure_cold(
        artifact: &Path,
        base: Option<&Path>,
        probe_text: &Path,
    ) -> Result<ColdMeasurement> {
        // The child is THIS binary, resolved through `current_exe` — never a bare `apr`.
        // CLAUDE.md Verification Discipline rule 3: four `apr` binaries once coexisted on the
        // dev box and a bare `apr` resolved to a 26-day-old one. A benchmark row measured
        // against a different build than the one under test is worse than no row.
        let exe = std::env::current_exe().map_err(|error| {
            CliError::InferenceFailed(format!(
                "the cold-probe child could not be resolved: {error}. `current_exe` is the only \
                 spelling of \"this binary\" that cannot resolve to another build."
            ))
        })?;

        let mut command = Command::new(TIME_BIN);
        command
            .arg(TIME_FLAG)
            .arg(&exe)
            .arg("setfit")
            .arg("bench")
            .arg("run")
            .arg("--cold-probe")
            .arg(artifact);
        if let Some(base) = base {
            command.arg("--cold-probe-base").arg(base);
        }
        command.arg("--probe-text").arg(probe_text);

        let output = command.output().map_err(|error| {
            CliError::InferenceFailed(format!(
                "the cold-probe child could not be spawned under {TIME_BIN} {TIME_FLAG}: \
                 {error}"
            ))
        })?;
        // The status is read off the reaped `Output` ON ITS OWN LINE. CLAUDE.md Verification
        // Discipline rule 1: a status taken through a pipe is the LAST command's status, and
        // that defect has shipped twice in this repository.
        let status = output.status;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !status.success() {
            return Err(CliError::InferenceFailed(format!(
                "the cold-probe child exited with {status}. Its output follows, because the \
                 child's refusal is the finding:\n{stderr}"
            )));
        }

        let cold_latency_ms = parse_cold_latency_ms(&stdout).ok_or_else(|| {
            CliError::InferenceFailed(format!(
                "the cold-probe child exited 0 but printed no `{COLD_LATENCY_PREFIX}` line. A \
                 missing measurement is a failure, not a zero.\nstdout:\n{stdout}"
            ))
        })?;
        // `/usr/bin/time` writes its report to STDERR on both platforms, which is why the two
        // streams are parsed separately rather than merged.
        let (peak_rss_bytes, peak_rss_mechanism) =
            parse_child_max_rss(&stderr).ok_or_else(|| {
                CliError::InferenceFailed(format!(
                    "{TIME_BIN} {TIME_FLAG} reported no maximum resident set size, so the child's \
                 true high-water mark is unknown. Emitting a sampled figure in its place would \
                 put a lower bound in a field the contract declares exact.\nstderr:\n{stderr}"
                ))
            })?;

        Ok(ColdMeasurement {
            cold_latency_ms,
            peak_rss_bytes,
            peak_rss_mechanism,
        })
    }

    // ---- WARM latency + throughput, against the RELOADED model in this process ---------------

    /// Median of [`WARM_MEASURED_COUNT`] classifies after the contracted warmups.
    ///
    /// `classify_one` must perform ONE single-text classification against the RELOADED model.
    /// The warmups are discarded, which is the boundary `apr bench --warmup` already uses.
    ///
    /// # Errors
    ///
    /// Whatever `classify_one` returns.
    pub(crate) fn warm_latency_ms_median<F>(warmups: u32, mut classify_one: F) -> Result<f64>
    where
        F: FnMut() -> Result<()>,
    {
        for _ in 0..warmups {
            classify_one()?;
        }
        let mut samples = Vec::with_capacity(WARM_MEASURED_COUNT);
        for _ in 0..WARM_MEASURED_COUNT {
            let started = Instant::now();
            classify_one()?;
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        median(samples).ok_or_else(|| {
            CliError::InferenceFailed(
                "the warm-latency sample was empty, which cannot happen with a positive \
                 measured count — treat this as a defect rather than a zero"
                    .to_string(),
            )
        })
    }

    /// Rows per second over ONE full pass of the test split at the declared batch size.
    ///
    /// `classify_batch` is handed each batch's row count and returns how many rows it actually
    /// classified; the mismatch check belongs to the caller's door, not here.
    ///
    /// # Errors
    ///
    /// Whatever `classify_batch` returns.
    pub(crate) fn throughput_rows_per_sec<F>(
        n_rows: usize,
        batch_size: u32,
        mut classify_batch: F,
    ) -> Result<f64>
    where
        F: FnMut(usize, usize) -> Result<()>,
    {
        let batch = batch_size.max(1) as usize;
        let started = Instant::now();
        let mut offset = 0;
        while offset < n_rows {
            let end = (offset + batch).min(n_rows);
            classify_batch(offset, end)?;
            offset = end;
        }
        let elapsed = started.elapsed().as_secs_f64();
        if elapsed <= 0.0 {
            // A pass that measured zero wall time is a timer resolution artefact, not infinite
            // throughput. Reporting `inf` would render as `null` through serde_json.
            return Err(CliError::InferenceFailed(
                "the throughput pass measured zero wall time; the number this would produce is \
                 not a throughput"
                    .to_string(),
            ));
        }
        #[allow(clippy::cast_precision_loss)]
        Ok(n_rows as f64 / elapsed)
    }
}

// ==========================================================================================
// The cold-probe child
// ==========================================================================================

/// The dedicated fresh child: load one artifact, classify once, print, exit.
mod cold_probe {
    use std::path::Path;
    use std::time::Instant;

    use crate::error::{CliError, Result};

    use super::{read_bounded, COLD_LATENCY_PREFIX};

    /// Run the probe.
    ///
    /// This process does NOTHING else. It has not trained, its page cache is cold, its
    /// allocator arenas are empty — which is what makes the one classify it performs a COLD
    /// measurement rather than a warm one wearing the word.
    ///
    /// # Errors
    ///
    /// [`CliError::ValidationFailed`] for a missing `--probe-text`;
    /// [`CliError::ModelLoadFailed`] for an artifact the load ladder rejects;
    /// [`CliError::InferenceFailed`] for a classify failure.
    pub(super) fn run(
        artifact: &Path,
        base: Option<&Path>,
        probe_text: Option<&Path>,
    ) -> Result<()> {
        let text_path = probe_text.ok_or_else(|| {
            CliError::ValidationFailed(
                "--probe-text <FILE> is required with --cold-probe: the probe classifies ONE \
                 text, and which text it is belongs to the measurement."
                    .to_string(),
            )
        })?;
        let text = String::from_utf8(read_bounded(text_path)?).map_err(|error| {
            CliError::ValidationFailed(format!(
                "--probe-text {}: not UTF-8 ({error})",
                text_path.display()
            ))
        })?;
        let text = text.trim_end_matches(['\n', '\r']).to_string();

        let cold_latency_ms = match base {
            Some(base) => lora_probe(artifact, base, &text)?,
            None => setfit_probe(artifact, &text)?,
        };

        // ONE machine-readable line on stdout. `/usr/bin/time`'s report goes to stderr, so the
        // parent parses two streams and neither can swallow the other.
        println!("{COLD_LATENCY_PREFIX}{cold_latency_ms}");
        Ok(())
    }

    /// A standalone `setfit-apr-v1`: the production loader, then one classify.
    fn setfit_probe(artifact: &Path, text: &str) -> Result<f64> {
        use aprender::setfit::{load_setfit_apr, ClassifyRequestDocument};

        // The timer opens BEFORE the read, because "cold" includes getting the artifact off
        // the platter and through the load ladder. A cold latency that excluded loading would
        // be a warm latency with extra steps.
        let started = Instant::now();
        let bytes = crate::setfit_io::read_setfit_apr_file_bounded(artifact)?;
        let model = load_setfit_apr(&bytes).map_err(|error| {
            CliError::ModelLoadFailed(format!("{}: {error}", artifact.display()))
        })?;
        let request = ClassifyRequestDocument::new(std::iter::once(text.to_string()));
        model
            .classify(&request)
            .map_err(|error| CliError::InferenceFailed(error.to_string()))?;
        Ok(started.elapsed().as_secs_f64() * 1000.0)
    }

    /// A LoRA cell: the base model plus the trained adapter, through the route 05-06 Task 3
    /// PROVED — `ClassifyPipeline::load_adapter` then `predict_proba_tokenized`.
    ///
    /// Named by those two symbols deliberately. 05-06's preflight recorded that no adapter
    /// load route existed before it, that `ClassifyPipeline::from_apr` builds FRESH LoRA
    /// layers, and that `resume_from_apr_checkpoint` installs tensors through `if let Ok(..)`
    /// — a silent partial load by construction. Inventing a reload here would re-open all of
    /// that against 40 remote 9B cells.
    fn lora_probe(adapter: &Path, base: &Path, text: &str) -> Result<f64> {
        let started = Instant::now();
        let mut pipeline = super::lora::reload_base_and_adapter(base, adapter)?;
        let token_ids = super::lora::tokenize_one(&pipeline, text)?;
        let probabilities = pipeline.predict_proba_tokenized(&token_ids);
        if probabilities.is_empty() {
            return Err(CliError::InferenceFailed(
                "the reloaded LoRA pipeline returned an empty probability vector".to_string(),
            ));
        }
        Ok(started.elapsed().as_secs_f64() * 1000.0)
    }
}

// ==========================================================================================
// `--record`: ingest a row another host executed. NO EXECUTION.
// ==========================================================================================

/// Ingest a transported row file into this bench directory.
///
/// # The verification order is the property
///
/// `BenchRow::from_bytes` verifies schema, digest, contracted cell and evidence-tag agreement
/// BEFORE returning a value, so nothing downstream can hold a row whose digest disagrees with
/// its payload. This function adds exactly one check the library cannot make: that the
/// FILENAME the operator handed over names the same cell the payload declares. A row whose
/// name and content disagree is a bookkeeping error that would otherwise land silently in the
/// slot named by whichever of the two the reader happened to trust.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] for a digest, schema, cell or filename disagreement, and for
/// a differing re-record; [`CliError::Io`] for the copy.
fn record_mode(bench_dir: &Path, row_file: &Path, force: bool, json: bool) -> Result<()> {
    let bytes = read_bounded(row_file)?;
    let row = BenchRow::from_bytes(&bytes).map_err(|error| {
        CliError::ValidationFailed(format!(
            "{}: {error}\nThese bytes are not recorded. Re-export the row on the host that \
             produced it rather than editing it here — the digest is over the payload, so an \
             edit that makes the row parse also makes it a different measurement.",
            row_file.display()
        ))
    })?;

    let cell = row.payload.cell();
    let expected_name = row_file_name(cell);
    let observed_name = row_file
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    if observed_name != expected_name {
        return Err(CliError::ValidationFailed(format!(
            "{}: the payload declares cell {cell} whose row file is `{expected_name}`, but the \
             file handed over is named `{observed_name}`. The name and the content must agree: \
             a row filed under another cell's name would be counted as that cell by every \
             later reader.",
            row_file.display()
        )));
    }

    let row_sha256 = sha256_hex(&row.payload.to_canonical_bytes().map_err(|error| {
        CliError::ValidationFailed(format!("the row payload did not re-serialize: {error}"))
    })?);

    // RECORD FIRST, COPY SECOND. `RunManifest::record` is the door that refuses a differing
    // re-record, and refusing AFTER the copy would leave the differing row on disk beside a
    // manifest that never accepted it.
    let outcome = record_in_manifest(bench_dir, cell, &row_sha256)?;

    let destination = bench_dir.join(ROWS_DIR).join(&expected_name);
    match outcome {
        // The idempotent path: the manifest already carries THIS digest, so the file on disk
        // is the same measurement. Re-writing it is permitted without `--force` precisely
        // because nothing can change — this is the resume-after-a-dropped-ssh case.
        RecordOutcome::AlreadyRecorded => atomic_write(&destination, &bytes, true)?,
        RecordOutcome::Recorded => atomic_write(&destination, &bytes, force)?,
    }

    if json {
        println!(
            "{}",
            serde_json::json!({
                "command": "setfit-bench-record",
                "cell": cell.render(),
                "row": destination.display().to_string(),
                "row_sha256": row_sha256,
                "outcome": match outcome {
                    RecordOutcome::Recorded => "recorded",
                    RecordOutcome::AlreadyRecorded => "already_recorded",
                },
                "executed": false,
            })
        );
    } else {
        println!(
            "recorded {cell} -> {} ({})",
            destination.display(),
            match outcome {
                RecordOutcome::Recorded => "new",
                RecordOutcome::AlreadyRecorded => "identical digest, idempotent",
            }
        );
    }
    Ok(())
}

// ==========================================================================================
// Execution — the per-method cell paths
// ==========================================================================================

/// Execute one cell.
///
/// The check order follows `setfit_train.rs`'s module header: the whole REQUEST first (the
/// cell key, the required flags), then the output refusal, then the inputs. A run that spends
/// a training pass and then says "I will not overwrite that row" has told the operator
/// something it knew before it started.
fn execute_mode(bench_dir: &Path, args: &BenchRunArgs<'_>) -> Result<()> {
    let cell = resolve_cell(args.method, args.shots, args.seed)?;

    let data = args.data.ok_or_else(|| {
        CliError::ValidationFailed(
            "--data <DIR> is required: the canonical splits live in the Phase 2 prepared \
             dataset directory."
                .to_string(),
        )
    })?;
    let selection = args.selection.ok_or_else(|| {
        CliError::ValidationFailed(
            "--selection <FILE> is required. It is the PAIRING KEY: EVAL-02's identical-\
             sampled-ID guarantee is that both methods consumed the same manifest for this \
             (shots, seed), and the row records its hash."
                .to_string(),
        )
    })?;

    // The output refusal, before any input is read.
    let row_path = bench_dir.join(ROWS_DIR).join(row_file_name(cell));
    refuse_existing_output(&row_path, args.force).map_err(|_| {
        CliError::ValidationFailed(format!(
            "{} already exists, so cell {cell} has already been executed. Re-running a \
             completed cell is idempotent-or-refused, never silently duplicated: pass --force \
             only if you intend to replace the run that produced the published number.",
            row_path.display()
        ))
    })?;

    let request = CellRequest {
        cell,
        data,
        selection,
        bench_dir,
        model_dir: args.model_dir,
        config: args.config,
        force: args.force,
        json: args.json,
    };

    match cell.method {
        Method::Setfit => setfit_cell::execute(&request),
        Method::Lora => lora::execute(&request),
    }
}

/// One resolved cell-execution request, shared by both method paths.
pub(crate) struct CellRequest<'a> {
    /// The contracted cell.
    pub(crate) cell: CellKey,
    /// The attested prepared dataset directory.
    pub(crate) data: &'a Path,
    /// The selection manifest — the pairing key.
    pub(crate) selection: &'a Path,
    /// Where rows, locks, ledgers and the run manifest live.
    pub(crate) bench_dir: &'a Path,
    /// The pinned encoder checkout (`setfit` only).
    pub(crate) model_dir: Option<&'a Path>,
    /// Optional training configuration.
    pub(crate) config: Option<&'a Path>,
    /// Replace an existing row file or ledger.
    pub(crate) force: bool,
    /// The global `--json`.
    pub(crate) json: bool,
}

/// Where the cell ran, read from the host rather than declared.
#[must_use]
pub(crate) fn host_identity() -> entrenar::train::setfit::bench_row::HostIdentity {
    entrenar::train::setfit::bench_row::HostIdentity {
        hostname: sysinfo::System::host_name().unwrap_or_else(|| "unknown".to_string()),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

/// Seal a payload into a row and land both the row file and the manifest entry.
///
/// Atomic rename, so an interrupted cell leaves either NO row or one complete digest-valid
/// row — never a partial. The manifest is updated AFTER the row lands, so a manifest entry
/// always has a file behind it.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] for a serialization or manifest refusal; [`CliError::Io`]
/// for the write.
pub(crate) fn emit_row(
    bench_dir: &Path,
    payload: entrenar::train::setfit::bench_row::BenchRowPayload,
    force: bool,
) -> Result<(PathBuf, String)> {
    let cell = payload.cell();
    let row = BenchRow::new(payload);
    let file_bytes = row.to_file_bytes().map_err(|error| {
        CliError::ValidationFailed(format!("the benchmark row did not serialize: {error}"))
    })?;
    let path = bench_dir.join(ROWS_DIR).join(row_file_name(cell));
    atomic_write(&path, &file_bytes, force)?;
    record_in_manifest(bench_dir, cell, &row.semantic_hash)?;
    Ok((path, row.semantic_hash))
}

/// Write the single probe text this cell's cold measurement classifies.
///
/// Taken from the test split's first row rather than invented, so the cold measurement is over
/// the same kind of input the throughput pass sees.
///
/// # Errors
///
/// [`CliError::Io`] for the write.
pub(crate) fn write_probe_text(bench_dir: &Path, cell: CellKey, text: &str) -> Result<PathBuf> {
    let path = bench_dir
        .join("probe")
        .join(format!("{}.txt", row_file_name(cell).replace(".json", "")));
    atomic_write(&path, text.as_bytes(), true)?;
    Ok(path)
}

// ==========================================================================================
// The SetFit cell path
// ==========================================================================================

/// The SetFit cell: train, write, RELOAD, measure, lock, evaluate, emit.
///
/// # Pure library orchestration
///
/// Not one semantic decision is made in this module. `SetFitRun`'s four transitions carry
/// every training gate; `load_setfit_apr` (reached through `reload_verified_run_from_apr`'s
/// load ladder) is the production loader, so only a VERIFIED artifact proceeds;
/// `evaluate_rows_from_artifact` is the ONE prediction door a benchmark cell may call;
/// `create_selection_lock` -> `mint_test_token` -> `CanonicalTestAccess::grant` is the D-16
/// workflow `apr eval` uses, consumed rather than reimplemented; `assemble_quality_block`
/// computes every published number. This module reads files, times things and writes files.
mod setfit_cell {
    use std::path::{Path, PathBuf};

    use aprender::setfit::{ClassifyRequestDocument, PINNED_REVISION};
    use aprender_contrastive_data::ledger::AccessLedger;
    use aprender_contrastive_data::prepared::{Canonical, PreparedDataset};
    use aprender_contrastive_data::select::Selection;
    use entrenar::train::setfit::apr_codec::AprCodec;
    use entrenar::train::setfit::apr_evaluate::{
        evaluate_rows_from_artifact, evaluate_validation_from_artifact, EvaluatedSplit,
        RowPredictions,
    };
    use entrenar::train::setfit::apr_reload::{
        reload_verified_run_from_apr, ReloadedSetFitCredential,
    };
    use entrenar::train::setfit::bench_metrics::assemble_quality_block;
    use entrenar::train::setfit::bench_row::{
        sha256_hex, BenchLockRef, BenchRowPayload, MethodEvidence, ResourceBlock, SetfitEvidence,
        BENCH_ROW_SCHEMA_VERSION, CLAIMS_CONTRACT_ID, WARMUP_COUNT,
    };
    use entrenar::train::setfit::evaluate::ValidationMetricKind;
    use entrenar::train::setfit::lock::{
        create_selection_lock, CanonicalTestAccess, SelectionCandidate, SelectionRule,
    };
    use entrenar::train::setfit::SetFitRun;

    use crate::commands::{data_contrastive, data_tweeteval, setfit_train};

    use super::resource::{self, TrainRssSampler, THROUGHPUT_BATCH_SIZE};
    use super::{
        emit_row, host_identity, lock_relative_path, read_bounded, write_probe_text, CellRequest,
        CliError, Result,
    };

    /// The metric the lock orders candidates by — the SAME fixed choice `apr eval` makes.
    ///
    /// Fixed rather than a flag for the reason `eval/setfit.rs` records: a candidate set whose
    /// metric kinds disagree cannot form a lock, so a per-invocation choice would let an
    /// operator build a lock that cannot be created and discover it at the end.
    const EVAL_METRIC: ValidationMetricKind = ValidationMetricKind::Accuracy;

    /// The selection rule this command commits under — again `apr eval`'s.
    const EVAL_RULE: SelectionRule = SelectionRule::MaxMetricLowestIndexTieBreak;

    /// Everything the Phase 2 ingest produces, replayed against itself.
    struct Phase2 {
        dataset: PreparedDataset<Canonical>,
        selection: Selection,
        manifest_semantic_hash: String,
        dataset_revision: String,
    }

    /// Read `--data` and `--selection` through the doors `data_contrastive` owns.
    ///
    /// The same three calls `commands/eval/setfit.rs` and `apr finetune --selection-manifest`
    /// make — `read_attested_canonical`, `read_selection_manifest`, `Selection::replay`. This
    /// is EVAL-02's identical-sampled-ID guarantee: both methods walk one code path rather
    /// than trusting an exporter.
    fn read_phase2(request: &CellRequest<'_>) -> Result<Phase2> {
        let mut ledger = AccessLedger::new();
        let dataset = data_contrastive::read_attested_canonical(request.data, &mut ledger)?;
        let manifest = data_contrastive::read_selection_manifest(request.selection)?;
        let selection = Selection::replay(&manifest, &dataset, &mut ledger).map_err(|error| {
            CliError::ValidationFailed(format!(
                "--selection {} does not replay against --data {}: {error}",
                request.selection.display(),
                request.data.display()
            ))
        })?;

        // THE CELL KEY AND THE SELECTION MUST DESCRIBE THE SAME DRAW. Nothing else checks
        // this: `Selection::replay` proves the manifest describes THIS dataset, and the row
        // records `shots`/`seed` from the FLAGS. A cell run with `--seed 13` against a
        // manifest drawn at seed 17 would publish a row filed under seed 13 whose rows are
        // seed 17's, and every paired-delta in the report would be comparing two different
        // draws while looking correctly paired.
        if selection.root_seed() != u64::from(request.cell.seed) {
            return Err(CliError::ValidationFailed(format!(
                "--seed {} disagrees with the selection manifest, which was drawn at root seed \
                 {}. The row would be filed under a seed its rows do not come from.",
                request.cell.seed,
                selection.root_seed()
            )));
        }
        if selection.shots_per_class() != request.cell.shots {
            return Err(CliError::ValidationFailed(format!(
                "--shots {} disagrees with the selection manifest, which carries {} per class.",
                request.cell.shots,
                selection.shots_per_class()
            )));
        }

        let manifest_bytes = read_bounded(&request.data.join(data_tweeteval::MANIFEST_FILE))
            .map_err(|error| {
                CliError::ValidationFailed(format!(
                    "{}: {error}",
                    request.data.join(data_tweeteval::MANIFEST_FILE).display()
                ))
            })?;
        let dataset_revision = data_tweeteval::dataset_revision_from_manifest(&manifest_bytes)?;

        Ok(Phase2 {
            dataset,
            selection,
            manifest_semantic_hash: manifest.semantic_hash.clone(),
            dataset_revision,
        })
    }

    /// Execute one SetFit cell.
    #[allow(clippy::too_many_lines)]
    pub(super) fn execute(request: &CellRequest<'_>) -> Result<()> {
        let model_dir = request.model_dir.ok_or_else(|| {
            CliError::ValidationFailed(
                "--model-dir <DIR> is required for a setfit cell: it names the pinned \
                 all-MiniLM-L6-v2 checkout. This command NEVER downloads."
                    .to_string(),
            )
        })?;

        // (1) THE REQUEST, in full, before anything expensive. The seed is the CELL's, and it
        //     goes through the library's validated merge door whether or not a file was given.
        let config = setfit_train::resolve_config(request.config, u64::from(request.cell.seed))?;

        // (2) Phase 2's artifacts, replayed strictly against each other and against the cell.
        let training_inputs = read_phase2(request)?;
        let dataset_revision = training_inputs.dataset_revision.clone();
        let selection_manifest_hash = training_inputs.manifest_semantic_hash.clone();
        let dataset_fingerprint = training_inputs
            .dataset
            .validation_witness()
            .dataset_fingerprint_hex();

        // (3) THE ENCODER, then the shipped lifecycle. Every gate — device probe, pair budget,
        //     selection/dataset agreement, calibration regime, evidence thresholds, head fit,
        //     artifact round trip — is inside those four transitions.
        let encoder =
            aprender::setfit::SetFitMiniLm::from_pretrained_dir(model_dir, config.root_seed())
                .map_err(|error| {
                    CliError::ModelLoadFailed(format!(
                        "--model-dir {}: {error}",
                        model_dir.display()
                    ))
                })?;

        // The TRAIN peak sampler opens here and closes the moment training ends, so what it
        // observes is the training process's footprint and not the reload's.
        let sampler = TrainRssSampler::start();
        let started = std::time::Instant::now();
        let prepared = SetFitRun::prepare(
            encoder,
            training_inputs.dataset,
            training_inputs.selection,
            config,
        )
        .map_err(train_error)?;
        let tuned = prepared.tune_encoder().map_err(train_error)?;
        let fitted = tuned.fit_head().map_err(train_error)?;
        let verified = fitted
            .verify_artifact(&AprCodec::new())
            .map_err(train_error)?;
        #[allow(clippy::cast_possible_truncation)]
        let train_wall_ms = started.elapsed().as_millis() as u64;
        let train_peak = sampler.finish();

        let evidence_table_hash = verified.evidence_table_hash().to_string();
        let apr_artifact_sha256 = verified.artifact_hash();

        // (4) THE WRITE. These are the bytes the trusted policy hashed and round-trip-closed,
        //     so `artifact_bytes` below counts the artifact `apr_artifact_sha256` describes.
        //     SetFit ships ONE file, so `deployable_total_bytes == artifact_bytes` — stated
        //     rather than assumed, because on the LoRA side they deliberately differ.
        let bytes = verified.into_artifact_bytes();
        let artifact_bytes = bytes.len() as u64;
        let artifact_path = artifact_path(request.bench_dir, request);
        super::atomic_write(&artifact_path, &bytes, true)?;
        drop(bytes);

        // (5) THE RELOAD. Through the production door, so only a VERIFIED artifact proceeds —
        //     and against a FRESH Phase 2 ingest, because the lifecycle consumed the first.
        //     Reading the directory twice is the same discipline `apr eval` runs under: the
        //     evaluation's inputs pass the attested boundary in their own right.
        let eval_inputs = read_phase2(request)?;
        let artifact_file_bytes = crate::setfit_io::read_setfit_apr_file_bounded(&artifact_path)?;
        let credential = reload_verified_run_from_apr(
            &artifact_file_bytes,
            &eval_inputs.dataset,
            &eval_inputs.selection,
        )
        .map_err(|error| {
            CliError::ValidationFailed(format!(
                "{}: {error}\nThe artifact this cell just wrote did not reload against the \
                 inputs it was trained on, so no number measured from it would describe the \
                 run that produced it.",
                artifact_path.display()
            ))
        })?;
        drop(artifact_file_bytes);

        // (6) COLD LATENCY + INFERENCE PEAK, in a dedicated fresh child. NEVER the first
        //     classify in this process: it has just trained and is operationally warm.
        let probe_text = eval_inputs
            .dataset
            .test()
            .rows()
            .first()
            .map(|row| row.input.clone())
            .ok_or_else(|| {
                CliError::ValidationFailed(
                    "the canonical test split has no rows, so there is nothing to probe with"
                        .to_string(),
                )
            })?;
        let probe_path = write_probe_text(request.bench_dir, request.cell, &probe_text)?;
        let cold = resource::measure_cold(&artifact_path, None, &probe_path)?;

        // (7) WARM + THROUGHPUT, against the RELOADED model in this process.
        let warm_request = ClassifyRequestDocument::new(std::iter::once(probe_text.clone()));
        let mut backend_identity = String::new();
        let warm_latency_ms_median = resource::warm_latency_ms_median(WARMUP_COUNT, || {
            let response = credential
                .model()
                .classify(&warm_request)
                .map_err(|error| CliError::InferenceFailed(error.to_string()))?;
            // BACKEND IDENTITY, READ FROM EXECUTION (Ph4 D-12). `ClassifyResponse::backend`
            // is `ExecutionBackend::identity` called on the value the encode invocation
            // RETURNED — there is no parameter, no setter and no configuration path that
            // reaches it. Echoing a device string from `--config` here would produce a row
            // that says GPU because somebody typed GPU.
            backend_identity = response.backend().to_string();
            Ok(())
        })?;

        let test_rows_data = eval_inputs.dataset.test().rows();
        let throughput_rows_per_sec = resource::throughput_rows_per_sec(
            test_rows_data.len(),
            THROUGHPUT_BATCH_SIZE,
            |from, to| {
                let batch = ClassifyRequestDocument::new(
                    test_rows_data[from..to].iter().map(|row| row.input.clone()),
                );
                credential
                    .model()
                    .classify(&batch)
                    .map_err(|error| CliError::InferenceFailed(error.to_string()))?;
                Ok(())
            },
        )?;

        // (8) THE EVALUATION, through the Phase 3 lock chain. Validation FIRST (it is what a
        //     selection may be made on), then the lock is COMMITTED TO DISK, then the token,
        //     then the grant, then the test rows.
        let validation_rows = evaluate_rows_from_artifact(
            &credential,
            &eval_inputs.dataset,
            EvaluatedSplit::Validation,
        )
        .map_err(|error| CliError::ValidationFailed(error.to_string()))?;

        let scalar =
            evaluate_validation_from_artifact(&credential, &eval_inputs.dataset, EVAL_METRIC)
                .map_err(|error| CliError::ValidationFailed(error.to_string()))?;
        let config_hash = config_hash_of(&credential);
        let lock = create_selection_lock(
            &credential,
            vec![SelectionCandidate::from_evaluation(&config_hash, scalar)],
            EVAL_RULE,
        )
        .map_err(|error| {
            CliError::ValidationFailed(format!(
                "the selection lock could not be committed: {error}"
            ))
        })?;

        // COMMIT THE LOCK BYTES. The row's `lock_hash` is a CLAIM; this file is the evidence,
        // and 05-10's gate recomputes the digest from these bytes rather than trusting the
        // field. `force = true` because the row file is the write-once artifact — a re-run
        // that got past the row's no-clobber gate is entitled to rewrite its own lock.
        let lock_bytes = lock.to_canonical_bytes();
        let lock_rel = lock_relative_path(request.cell);
        super::atomic_write(&request.bench_dir.join(&lock_rel), &lock_bytes, true)?;
        let committed_lock_hash = sha256_hex(&lock_bytes);

        let token = lock.mint_test_token(&credential).map_err(|error| {
            CliError::ValidationFailed(format!(
                "the selection lock does not admit this artifact: {error}"
            ))
        })?;
        let grant = CanonicalTestAccess::grant(token, &credential, &eval_inputs.dataset).map_err(
            |error| {
                CliError::ValidationFailed(format!("canonical test access was refused: {error}"))
            },
        )?;
        let test_rows = evaluate_rows_from_artifact(
            &credential,
            &eval_inputs.dataset,
            EvaluatedSplit::Test(&grant),
        )
        .map_err(|error| CliError::ValidationFailed(error.to_string()))?;

        // (9) THE QUALITY BLOCK. Test rows supply the accuracy family, validation rows the
        //     calibration diagnostics — SEPARATE PARAMETERS, so there is no argument order
        //     that feeds test probabilities to the calibration functions (D-07).
        let ordered_labels: Vec<String> = credential.model().ordered_labels().to_vec();
        let quality = assemble_quality_block(&test_rows, &validation_rows, &ordered_labels)
            .map_err(|error| CliError::ValidationFailed(error.to_string()))?;

        // (10) THE ROW.
        let payload = BenchRowPayload {
            schema_version: BENCH_ROW_SCHEMA_VERSION,
            contract_id: CLAIMS_CONTRACT_ID.to_string(),
            method: request.cell.method,
            shots: request.cell.shots,
            seed: request.cell.seed,
            dataset_revision,
            dataset_fingerprint,
            model_revision: PINNED_REVISION.to_string(),
            selection_manifest_hash,
            backend_identity,
            host: host_identity(),
            quality,
            resource: ResourceBlock {
                train_wall_ms,
                cold_latency_ms: cold.cold_latency_ms,
                // Always true on a row this adapter writes: `measure_cold` has no in-process
                // path, so there is no branch here that could set it false.
                cold_measured_in_child_process: true,
                warm_latency_ms_median,
                throughput_rows_per_sec,
                throughput_batch_size: THROUGHPUT_BATCH_SIZE,
                warmup_count: WARMUP_COUNT,
                train_peak_rss_bytes: train_peak.bytes,
                train_peak_rss_mechanism: train_peak.mechanism.clone(),
                inference_peak_rss_bytes: cold.peak_rss_bytes,
                inference_peak_rss_mechanism: cold.peak_rss_mechanism.to_string(),
                peak_rss_sample_interval_hz: train_peak.sample_interval_hz,
                artifact_bytes,
                // SetFit ships ONE standalone file: the encoder, the tokenizer identity, the
                // pooling policy and the head are all inside it. The equality is therefore a
                // FACT about the format, not a copy-paste.
                deployable_total_bytes: artifact_bytes,
            },
            evidence: MethodEvidence::Setfit(SetfitEvidence {
                evidence_table_hash,
                apr_artifact_sha256,
                lock: BenchLockRef {
                    lock_hash: committed_lock_hash,
                    role: "written".to_string(),
                    rule: lock.rule().to_string(),
                    lock_record_path: lock_rel,
                },
            }),
        };

        let (row_path, row_hash) = emit_row(request.bench_dir, payload, request.force)?;
        report(request.json, request.cell, &row_path, &row_hash)
    }

    /// Where this cell's artifact lands inside the bench directory.
    fn artifact_path(bench_dir: &Path, request: &CellRequest<'_>) -> PathBuf {
        bench_dir.join("artifacts").join(format!(
            "{}-s{}-seed{}.apr",
            request.cell.method.tag(),
            request.cell.shots,
            request.cell.seed
        ))
    }

    /// The hex SHA-256 over the artifact document's canonical `requested_config` sub-document.
    ///
    /// The SAME derivation `apr eval` records as `CONFIG_HASH_DERIVATION`, so a lock this
    /// command writes and a lock `apr eval` writes carry comparable candidate identities.
    fn config_hash_of(credential: &ReloadedSetFitCredential) -> String {
        use sha2::Digest as _;
        let requested = &credential.model().doc_view().requested_config;
        let bytes = serde_json::to_vec(requested).unwrap_or_default();
        aprender_contrastive_data::hash::hex(&sha2::Sha256::digest(&bytes).into())
    }

    /// Map a lifecycle failure onto the CLI surface.
    fn train_error(error: entrenar::train::setfit::SetFitTrainError) -> CliError {
        CliError::ValidationFailed(format!("setfit bench cell: {error}"))
    }

    /// What the command says when a cell lands.
    fn report(
        json: bool,
        cell: entrenar::train::setfit::bench_row::CellKey,
        row_path: &Path,
        row_hash: &str,
    ) -> Result<()> {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "command": "setfit-bench-run",
                    "cell": cell.render(),
                    "row": row_path.display().to_string(),
                    "row_sha256": row_hash,
                    "executed": true,
                })
            );
        } else {
            println!("{cell} -> {} ({row_hash})", row_path.display());
        }
        Ok(())
    }

    /// Silence the unused-import warning when a helper is only used by one arm.
    #[allow(dead_code)]
    fn _row_predictions_type_is_named(_: &RowPredictions) {}
}

// ==========================================================================================
// The LoRA cell path
// ==========================================================================================

/// Wave-4 task 3 fills this module. See [`setfit_cell`].
pub(crate) mod lora {
    use std::path::Path;

    use super::{CellRequest, CliError, Result};

    /// The reloaded base + adapter pipeline, through the route 05-06 Task 3 proved.
    pub(crate) fn reload_base_and_adapter(
        _base: &Path,
        _adapter: &Path,
    ) -> Result<entrenar::finetune::ClassifyPipeline> {
        Err(CliError::ValidationFailed(
            "the LoRA reload path lands in plan 05-09 task 3".to_string(),
        ))
    }

    /// Tokenize one text through the pipeline's own tokenizer.
    pub(crate) fn tokenize_one(
        _pipeline: &entrenar::finetune::ClassifyPipeline,
        _text: &str,
    ) -> Result<Vec<u32>> {
        Err(CliError::ValidationFailed(
            "the LoRA tokenize path lands in plan 05-09 task 3".to_string(),
        ))
    }

    pub(super) fn execute(request: &CellRequest<'_>) -> Result<()> {
        Err(CliError::ValidationFailed(format!(
            "cell {} cannot be executed by this build yet: `apr setfit bench run`'s LoRA \
             execution path lands in plan 05-09 task 3.",
            request.cell
        )))
    }
}

#[cfg(test)]
#[path = "setfit_bench_tests.rs"]
mod setfit_bench_tests;
