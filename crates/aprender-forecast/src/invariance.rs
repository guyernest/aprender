//! The D-19 no-argument bitwise invariance gate: a delivery that ADDS optional arguments
//! must be byte-identical for a caller who passes none.
//!
//! That is Forecast Coach's acceptance condition for a one-line tag bump, and it is the
//! only thing that makes "additive change" a checkable claim rather than a promise. The
//! mechanism is a bit-exact signature over the deterministic fields of a
//! [`ForecastResponse`], captured through the PUBLIC door on a commit at which the new
//! code does not exist, committed, and re-checked afterwards.
//!
//! # What the signature covers, and what it deliberately does not
//!
//! `fit_seconds` and `predict_seconds` are EXCLUDED. They are wall-clock: a signature that
//! included them could never reproduce, which would make the gate vacuous in the opposite
//! direction — permanently red, therefore permanently ignored. Everything else is included.
//!
//! Every f64 is hashed by `to_bits()`, never by a formatted decimal. A printed comparison
//! silently accepts any change below the print precision, which is exactly the class of
//! drift this gate exists to catch. Negative zero is normalised to positive zero so `-0.0`
//! and `0.0` do not read as a change; every other bit pattern, NaN payloads included, is
//! significant.
//!
//! `components` and `diagnostics` are walked as JSON. `serde_json::Map` is a `BTreeMap`,
//! so object iteration is KEY-SORTED and the signature does not depend on insertion order.
//! Both fields rely on that: the door builds `components` by inserting in computation
//! order, and a future reordering of those inserts must not read as a behaviour change.
//!
//! # Three parts, three DIFFERENT claims — and the exposure each one carries
//!
//! SC2 does not ask for a green invariance table. It asks for one that is *proven able to
//! fail*, and says in as many words that a green gate with no falsification probe beside it
//! does not satisfy the criterion. A `signature` that returned a constant would produce
//! exactly the same green table as a correct one. So the gate is four statements, not one,
//! and they are deliberately not four ways of saying the same thing:
//!
//! | Part | Claim it CARRIES | Claim it does NOT carry | Arch exposure |
//! |---|---|---|---|
//! | the committed baseline ([`every_baseline_case_reproduces_its_signature`]) | the eight door cases answer today what they answered at a commit where `regressors.rs` did not exist — the only INDEPENDENT HISTORICAL evidence in this gate | nothing about whether the signature can detect a change | **arch-keyed** (skips loudly on an unrecorded arch) |
//! | part A ([`part_a_every_case_is_deterministic_through_the_door`]) | the same host, twice, answers identically — so a baseline mismatch is a real difference and never run-to-run noise | nothing about the past: it compares now against now | **unconditional** |
//! | part B ([`part_b_the_signature_is_proven_able_to_fail`]) | the signature MOVES for a change of one unit in the last place, and for the band and `diagnostics` fields nothing else exercises | nothing about the forecast being correct — only that the detector detects | **unconditional** |
//! | part C ([`part_c_the_splice_is_inert_at_zero_regressors`]) | SPLICE INERTNESS: `regressors::splice` at zero regressors changes neither the design nor the forecast, measured bit for bit, bands included | it is NOT a pre-change comparison — both sides call the SAME post-change [`crate::prophet::predict`], so a regression common to the empty-regressor branch moves both sides equally and part C stays green | unconditional (but not independent of the current `predict`) |
//!
//! **Parts A and B must never be made conditional on the running architecture, and must
//! never be `#[ignore]`d.** Part A compares one host against ITSELF and part B mutates a
//! response in memory; neither touches libm, so neither can legitimately differ across
//! runners. Only the cross-commit baseline comparison is arch-keyed, for the libm reason
//! below. This distinction is written down because the tempting repair for a cross-platform
//! red — relaxing whatever is red — would remove exactly the two parts SC2 rests on.
//! [`parts_a_and_b_are_unconditional`] enforces it against this module's own source.
//!
//! # Why the baseline is keyed by architecture
//!
//! `by_arch` is keyed on [`std::env::consts::ARCH`], and this is a correctness requirement
//! rather than future-proofing. `prophet::feature_row` calls `f64::sin`/`f64::cos`, which
//! resolve to the platform libm, and the NeuralProphet arm trains on the f32 autograd whose
//! GEMM routing is arch-specific. CI's `workspace-test` job runs on self-hosted X64 Linux
//! and is a REQUIRED check on protected `main`, while these signatures were captured on
//! aarch64 macOS. A single-architecture baseline compared unconditionally would be red in
//! CI on day one, and a gate that is red for a reason nobody intended gets disabled.
//!
//! The repo already carries this precedent: `quantiles_abs_f32_nonaarch64` is a separate
//! bar precisely because x86 and aarch64 differ bitwise.
//!
//! Absence of the FILE is a DEFECT and panics. Absence of the running ARCH inside an
//! otherwise valid file is a skip WITH A STATED REASON and a stated way to close it — see
//! [`every_baseline_case_reproduces_its_signature`]. The companion test
//! [`the_baseline_records_at_least_one_architecture`] forbids a file that lost its content,
//! so "nothing to compare" can never be how this passes.

use crate::test_support::{fixture_path, load_json, read_csv};
use crate::types::{ForecastArgs, ForecastResponse, HolidayArg};

// ------------------------------------------------------------- the hasher ----

/// FNV-1a 64. Chosen for being trivially reproducible in any language, not for strength:
/// this is a change detector, not a security primitive.
pub struct Hasher(u64);

impl Hasher {
    #[must_use]
    pub fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 ^= u64::from(x);
            self.0 = self.0.wrapping_mul(0x1000_0000_01b3);
        }
    }

    /// Hash a length prefix, so `["a", "bc"]` and `["ab", "c"]` cannot collide.
    fn len(&mut self, n: usize) {
        let n = u64::try_from(n).expect("a collection length fits in u64");
        self.bytes(&n.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        // Normalise the two zeros so -0.0 and 0.0 do not read as a change; every other
        // bit pattern, NaN payloads included, is significant.
        let v = if v == 0.0 { 0.0 } else { v };
        self.bytes(&v.to_bits().to_le_bytes());
    }

    pub fn f64s(&mut self, vs: &[f64]) {
        self.len(vs.len());
        for &v in vs {
            self.f64(v);
        }
    }

    pub fn str(&mut self, s: &str) {
        self.len(s.len());
        self.bytes(s.as_bytes());
    }

    #[must_use]
    pub fn finish(&self) -> u64 {
        self.0
    }
}

/// Walk a JSON value, tagging each variant so a string `"1"` and a number `1` differ.
fn json(h: &mut Hasher, v: &serde_json::Value) {
    match v {
        serde_json::Value::Null => h.bytes(b"n"),
        serde_json::Value::Bool(b) => {
            h.bytes(b"b");
            h.bytes(&[u8::from(*b)]);
        }
        serde_json::Value::Number(n) => {
            h.bytes(b"#");
            h.f64(n.as_f64().unwrap_or(f64::NAN));
        }
        serde_json::Value::String(s) => {
            h.bytes(b"s");
            h.str(s);
        }
        serde_json::Value::Array(a) => {
            h.bytes(b"[");
            h.len(a.len());
            for x in a {
                json(h, x);
            }
        }
        serde_json::Value::Object(o) => {
            // serde_json's default Map is a BTreeMap: iteration is key-sorted, so the
            // signature does not depend on insertion order.
            h.bytes(b"{");
            h.len(o.len());
            for (k, x) in o {
                h.str(k);
                json(h, x);
            }
        }
    }
}

/// A bit-exact signature over every deterministic field of a [`ForecastResponse`].
///
/// `fit_seconds` and `predict_seconds` are excluded — see the module docs.
#[must_use]
pub fn signature(r: &ForecastResponse) -> u64 {
    signature_with(r, &r.diagnostics)
}

/// The ONE hashing body. `signature` is this with the response's own `diagnostics`; part A's
/// `budget_hit` triage is this with a masked copy.
///
/// `ForecastResponse` is not `Clone` (it is a serialisation type), so the triage cannot
/// clone-and-edit. Factoring the body is the alternative to writing the field list twice —
/// and a second field list is precisely how a gate starts hashing less than it claims.
fn signature_with(r: &ForecastResponse, diagnostics: &serde_json::Value) -> u64 {
    let mut h = Hasher::new();
    h.str(&r.model);
    h.str(&r.freq);
    h.len(r.n_history);
    h.len(r.ds.len());
    for d in &r.ds {
        h.str(d);
    }
    h.f64s(&r.yhat);
    h.f64s(&r.yhat_lower);
    h.f64s(&r.yhat_upper);
    h.f64s(&r.trend);
    h.len(r.components.len());
    for (k, v) in &r.components {
        h.str(k);
        json(&mut h, v);
    }
    json(&mut h, diagnostics);
    h.finish()
}

// --------------------------------------------------------- the door cases ----

/// The shape of one door case, beyond the series and the horizon.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// Prophet, everything defaulted.
    ProphetDefault,
    /// Prophet on a monthly series (`freq: "MS"`).
    ProphetMonthly,
    /// Prophet, monthly, multiplicative seasonality.
    ProphetMonthlyMultiplicative,
    /// Prophet, logistic growth (the `cap` is derived from the series).
    ProphetLogistic,
    /// Prophet with one holiday carrying a `[-1, +1]` window.
    ProphetHolidayWindows,
    /// The NeuralProphet arm at a given lag count.
    NeuralProphet(usize),
}

/// The eight cases, ported from `sources/012-no-arg-bitwise-invariance/src/main.rs`.
///
/// There is deliberately NO `retail/neuralprophet/MS` case: `forecast.rs` refuses
/// `freq != "D"` on that arm (pinned by `neuralprophet_refuses_non_daily_freq`), so the
/// spike swapped it for a second DAILY series. A case that is refused rather than computed
/// would record the signature of an error path and prove nothing about invariance.
const CASES: [(&str, &str, usize, Shape); 8] = [
    (
        "peyton/prophet/default",
        "peyton_manning.csv",
        30,
        Shape::ProphetDefault,
    ),
    (
        "air/prophet/multiplicative",
        "air_passengers.csv",
        12,
        Shape::ProphetMonthlyMultiplicative,
    ),
    (
        "retail/prophet/default",
        "retail_sales.csv",
        12,
        Shape::ProphetMonthly,
    ),
    (
        "wp_log_R/prophet/logistic",
        "wp_log_R.csv",
        30,
        Shape::ProphetLogistic,
    ),
    (
        "peyton/prophet/holidays+windows",
        "peyton_manning.csv",
        30,
        Shape::ProphetHolidayWindows,
    ),
    (
        "peyton/neuralprophet/lag0",
        "peyton_manning.csv",
        30,
        Shape::NeuralProphet(0),
    ),
    (
        "peyton/neuralprophet/lag7",
        "peyton_manning.csv",
        30,
        Shape::NeuralProphet(7),
    ),
    (
        "wp_log_R/neuralprophet/lag0",
        "wp_log_R.csv",
        30,
        Shape::NeuralProphet(0),
    ),
];

/// Build one case's request through the PUBLIC argument type only.
///
/// `read_csv` already strips quoted fields and carriage returns and sorts/de-duplicates by
/// `ds`, which `wp_log_R.csv` needs because it is not chronological.
fn args_for(csv: &str, horizon: usize, shape: Shape) -> ForecastArgs {
    let (ds, y) = read_csv(csv);
    let mut args = ForecastArgs {
        ds,
        y,
        horizon,
        seed: Some(42),
        ..ForecastArgs::default()
    };
    match shape {
        Shape::ProphetDefault => {}
        Shape::ProphetMonthly => args.freq = Some("MS".into()),
        Shape::ProphetMonthlyMultiplicative => {
            args.freq = Some("MS".into());
            args.seasonality_mode = Some("multiplicative".into());
        }
        Shape::ProphetLogistic => {
            args.growth = Some("logistic".into());
            args.cap = Some(args.y.iter().fold(f64::MIN, |m, v| m.max(*v)) * 1.2);
        }
        Shape::ProphetHolidayWindows => {
            args.holidays = Some(vec![HolidayArg {
                name: "playoff".into(),
                dates: vec![
                    "2010-01-16".into(),
                    "2014-01-12".into(),
                    "2016-01-17".into(),
                ],
                lower_window: -1,
                upper_window: 1,
            }]);
        }
        Shape::NeuralProphet(n_lags) => {
            args.model = Some("neuralprophet".into());
            args.n_lags = Some(n_lags);
        }
    }
    args
}

/// The name of the committed baseline, read through `test_support::fixture_path`.
const BASELINE: &str = "invariance_baseline.json";

/// The recipe printed when the running architecture has no recorded entry. It is a single
/// line on purpose: a skip whose reason scrolls away is a silent skip.
const CAPTURE_RECIPE: &str = "INVARIANCE_BASELINE_MODE=capture cargo test -p aprender-forecast --lib invariance::capture_baseline -- --ignored --nocapture";

// --------------------------------------------------------------- capture ----

/// Write (or extend) the committed baseline for the RUNNING architecture.
///
/// `#[ignore]`d and additionally gated on `INVARIANCE_BASELINE_MODE=capture`, following
/// `np::wall`'s `NP_WALL_MODE` convention: this is a CAPTURE, not an assertion, and a
/// capture that can run by accident can overwrite the evidence it is supposed to preserve.
///
/// **It must be run on a commit at which the change under test does not yet exist.** A
/// baseline captured afterwards is circular evidence — it would record the new behaviour
/// and then congratulate the new behaviour for matching it. `captured_at_commit` is
/// `git rev-parse HEAD` taken BEFORE the capture is committed, so the recorded value is
/// already the parent of the commit that carries the file; that value IS the phase base and
/// must not have `^` applied to it again.
///
/// Capturing a SECOND architecture (the X64 CI runner) means checking out
/// `captured_at_commit` on that host and re-running this, then carrying the merged file
/// forward. Existing architectures are preserved, so the two captures compose.
#[test]
#[ignore = "capture, not an assertion; run with INVARIANCE_BASELINE_MODE=capture -- --ignored"]
fn capture_baseline() {
    let mode = std::env::var("INVARIANCE_BASELINE_MODE").unwrap_or_default();
    assert_eq!(
        mode, "capture",
        "refusing to overwrite the committed baseline without \
         INVARIANCE_BASELINE_MODE=capture; the recipe is: {CAPTURE_RECIPE}"
    );

    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse HEAD must run: the baseline's provenance is not optional");
    assert!(head.status.success(), "git rev-parse HEAD failed");
    let head = String::from_utf8(head.stdout)
        .expect("git rev-parse HEAD is utf-8")
        .trim()
        .to_string();

    // CLAUDE.md rule 2: ONE owner for this token. A second derivation here is exactly the
    // drift `profile_token`'s doc comment guards against — a debug run labelled `release`
    // is stamped into the committed baseline and turns it into a confident wrong answer.
    let profile = crate::sc1_wall::profile_token();
    let arch = std::env::consts::ARCH;

    let mut cases = serde_json::Map::new();
    for (label, csv, horizon, shape) in CASES {
        let args = args_for(csv, horizon, shape);
        let r = crate::forecast::forecast(&args)
            .unwrap_or_else(|e| panic!("case {label} must succeed through the door: {e}"));
        let sig = signature(&r);
        println!(
            "INVARIANCE CAPTURE: case={label} arch={arch} profile={profile} signature={sig:016x}"
        );
        // Built field by field rather than with `serde_json::json!`: that macro expands to
        // an internal `.unwrap()` for runtime values, which `.clippy.toml` bans outright
        // (GH-41). `Value::String` is the same result with no hidden unwrap.
        let mut entry = serde_json::Map::new();
        entry.insert(
            "signature".into(),
            serde_json::Value::String(format!("{sig:016x}")),
        );
        entry.insert("arch".into(), serde_json::Value::String(arch.to_string()));
        entry.insert(
            "profile".into(),
            serde_json::Value::String(profile.to_string()),
        );
        entry.insert(
            "captured_at_commit".into(),
            serde_json::Value::String(head.clone()),
        );
        cases.insert(label.to_string(), serde_json::Value::Object(entry));
    }

    let path = fixture_path(BASELINE);
    // Preserve any architecture already recorded, and keep the ORIGINAL top-level
    // provenance: the top-level `captured_at_commit` is the pre-change commit the whole
    // baseline describes, so a later capture on a second host must be taken at that same
    // commit rather than silently re-dating the file.
    let mut doc: serde_json::Map<String, serde_json::Value> = if path.is_file() {
        let raw = std::fs::read_to_string(&path).expect("existing baseline is readable");
        serde_json::from_str(&raw).expect("existing baseline is a JSON object")
    } else {
        serde_json::Map::new()
    };
    if !doc.contains_key("captured_at_commit") {
        doc.insert(
            "captured_at_commit".into(),
            serde_json::Value::String(head.clone()),
        );
        doc.insert(
            "captured_on".into(),
            serde_json::Value::String(iso_date_utc()),
        );
    }
    if let Some(recorded) = doc.get("captured_at_commit").and_then(|v| v.as_str()) {
        assert!(
            recorded == head,
            "this baseline describes commit {recorded}, but HEAD is {head}; check out \
             {recorded} before capturing another architecture, or the two halves of the \
             file would describe different trees"
        );
    }
    doc.insert(
        "note".into(),
        serde_json::Value::String(String::from(
            "Evidence for ONE tag bump, not a permanent certificate. These signatures \
             describe the tree at `captured_at_commit`; a later intentional behaviour \
             change re-captures them at its own pre-change commit. Keyed by \
             std::env::consts::ARCH because libm and the f32 GEMM routing differ bitwise \
             across architectures.",
        )),
    );
    let by_arch = doc
        .entry("by_arch")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .expect("by_arch is an object");
    by_arch.insert(arch.to_string(), serde_json::Value::Object(cases));

    let text = serde_json::to_string_pretty(&doc).expect("baseline serialises");
    std::fs::write(&path, text + "\n").expect("baseline is writable");
    println!("INVARIANCE CAPTURE: wrote {}", path.display());
}

/// `YYYY-MM-DD` for today, UTC, without adding a calendar dependency (D-17).
fn iso_date_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs();
    let days = i64::try_from(secs / 86_400).expect("days since epoch fit in i64");
    // `days_from_civil(1970, 1, 1)` is 0 by definition — the epoch is the origin.
    crate::dates::format_ymd(days)
}

// ------------------------------------------------------------ the gate ----

/// Read the committed baseline. Absence is a DEFECT, never a skip.
fn baseline() -> serde_json::Value {
    load_json(BASELINE)
}

/// Every recorded pre-change case reproduces its signature, on the architecture it was
/// recorded for.
///
/// When the running arch IS recorded, all eight cases are compared and the recorded count
/// is asserted to be exactly 8 — so a file that lost cases cannot pass by comparing fewer.
/// When it is NOT recorded, this prints one loud line naming the running arch, the archs
/// that are recorded and the exact capture recipe, and returns. That is a skip with a
/// stated reason and a stated way to close it; [`the_baseline_records_at_least_one_architecture`]
/// is what stops it from degenerating into a silent pass.
#[test]
fn every_baseline_case_reproduces_its_signature() {
    let doc = baseline();
    let by_arch = doc["by_arch"]
        .as_object()
        .expect("the baseline must carry a by_arch object");
    let arch = std::env::consts::ARCH;

    let Some(recorded) = by_arch.get(arch).and_then(|v| v.as_object()) else {
        let known: Vec<&str> = by_arch.keys().map(String::as_str).collect();
        println!(
            "INVARIANCE SKIP: no baseline recorded for arch={arch} (recorded: {known:?}); \
             this gate is NOT proving anything on this host. Capture one with: {CAPTURE_RECIPE}"
        );
        return;
    };

    assert_eq!(
        recorded.len(),
        CASES.len(),
        "the baseline for arch={arch} records {} cases; it must record all {} door cases, \
         or the gate is only watching part of the surface",
        recorded.len(),
        CASES.len()
    );

    for (label, csv, horizon, shape) in CASES {
        let want = recorded
            .get(label)
            .and_then(|v| v["signature"].as_str())
            .unwrap_or_else(|| panic!("baseline for arch={arch} must record case {label}"));
        let args = args_for(csv, horizon, shape);
        let r = crate::forecast::forecast(&args)
            .unwrap_or_else(|e| panic!("case {label} must succeed through the door: {e}"));
        let got = format!("{:016x}", signature(&r));
        assert_eq!(
            got, want,
            "case {label} on arch={arch} no longer reproduces its pre-change signature: \
             recorded {want}, got {got}. A caller who passes NO new argument is getting a \
             different answer, so this delivery is not a one-line tag bump for them"
        );
    }
}

/// The baseline records at least one architecture, and every architecture it records is
/// complete.
///
/// This is the anti-vacuity companion to [`every_baseline_case_reproduces_its_signature`]:
/// that test skips when the running arch is absent, so without this one a baseline that was
/// emptied — or written with `by_arch: {}` — would sail through on every host.
#[test]
fn the_baseline_records_at_least_one_architecture() {
    let doc = baseline();
    let by_arch = doc["by_arch"]
        .as_object()
        .expect("the baseline must carry a by_arch object");
    assert!(
        !by_arch.is_empty(),
        "by_arch is empty: the gate would skip on every host and prove nothing"
    );
    for (arch, cases) in by_arch {
        let cases = cases
            .as_object()
            .unwrap_or_else(|| panic!("by_arch.{arch} must be an object of cases"));
        assert_eq!(
            cases.len(),
            CASES.len(),
            "by_arch.{arch} records {} cases; every recorded arch must carry all {}",
            cases.len(),
            CASES.len()
        );
        for (label, entry) in cases {
            let sig = entry["signature"]
                .as_str()
                .unwrap_or_else(|| panic!("by_arch.{arch}.{label}.signature must be a string"));
            assert_eq!(
                sig.len(),
                16,
                "by_arch.{arch}.{label}.signature must be 16 lowercase hex digits, got {sig:?}"
            );
            assert!(
                sig.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "by_arch.{arch}.{label}.signature must be lowercase hex, got {sig:?}"
            );
        }
    }
    assert!(
        doc.get("captured_at_commit")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| !s.trim().is_empty()),
        "the baseline must record captured_at_commit: a signature with no provenance \
         cannot be traced to the tree it describes"
    );
}

/// The signature is able to FAIL — a guard that cannot go red is theatre.
///
/// Three mutations at the smallest representable scale (1 ULP) plus a structural one, then
/// a restore. Plan 06.1-04 owns the full mutation ladder; this is the in-plan floor that
/// stops the gate from being committed vacuous.
#[test]
fn the_signature_detects_a_one_ulp_change() {
    let args = args_for("peyton_manning.csv", 30, Shape::ProphetDefault);
    let mut r = crate::forecast::forecast(&args).expect("the default case must succeed");
    let clean = signature(&r);

    let orig = r.yhat[0];
    r.yhat[0] = f64::from_bits(orig.to_bits() + 1);
    assert_ne!(signature(&r), clean, "a 1-ULP change in yhat[0] must show");
    r.yhat[0] = orig;

    let last = r.trend.len() - 1;
    let orig_t = r.trend[last];
    r.trend[last] = f64::from_bits(orig_t.to_bits() + 1);
    assert_ne!(
        signature(&r),
        clean,
        "a 1-ULP change in trend[last] must show"
    );
    r.trend[last] = orig_t;

    r.components.insert(
        "phantom".into(),
        serde_json::Value::Array(vec![serde_json::Value::Null]),
    );
    assert_ne!(signature(&r), clean, "an extra component key must show");
    r.components.remove("phantom");

    assert_eq!(
        signature(&r),
        clean,
        "reverting every mutation must return the original signature, or the detector is \
         reacting to something other than what was changed"
    );
}

// ------------------------------------------------- part A: determinism ----

/// The four hashed f64 arrays, in the signature's own order, so the divergence walk and the
/// signature cannot disagree about which fields or which order they are looking at.
fn float_fields(r: &ForecastResponse) -> [(&'static str, &[f64]); 4] {
    [
        ("yhat", r.yhat.as_slice()),
        ("yhat_lower", r.yhat_lower.as_slice()),
        ("yhat_upper", r.yhat_upper.as_slice()),
        ("trend", r.trend.as_slice()),
    ]
}

/// This response's `diagnostics` with `lbfgs.budget_hit` forced to a fixed value.
///
/// Used ONLY by the mismatch triage below, never by the gate itself: the key stays inside
/// the signature (see [`mismatch_report`] for why removing it would be a permanent
/// weakening).
fn diagnostics_with_budget_hit_masked(r: &ForecastResponse) -> serde_json::Value {
    let mut d = r.diagnostics.clone();
    if let Some(flag) = d.get_mut("lbfgs").and_then(|l| l.get_mut("budget_hit")) {
        *flag = serde_json::Value::Bool(false);
    }
    d
}

/// A diagnostic for two runs of one case whose signatures disagree.
///
/// A bare "signatures differ" sends the reader back to the whole response. This is written
/// for the place the failure is hardest to chase — CI, where nobody can re-run it
/// interactively — so it names the first divergent BIT, the scale of the divergence per
/// field, and whether the one wall-clock-dependent flag inside `diagnostics` accounts for
/// it on its own.
fn mismatch_report(label: &str, a: &ForecastResponse, b: &ForecastResponse) -> String {
    let mut s = format!(
        "case {label} produced two different signatures on the SAME host, from two calls \
         made back to back with the same arguments: {:016x} then {:016x}.\n",
        signature(a),
        signature(b)
    );

    // (1) The FIRST divergent element, by field, index and bit pattern.
    let mut first: Option<String> = None;
    for ((field, xa), (_, xb)) in float_fields(a).into_iter().zip(float_fields(b)) {
        if xa.len() != xb.len() {
            first = Some(format!(
                "  first divergence: {field} has {} values in run 1 and {} in run 2 — a \
                 STRUCTURAL break, not arithmetic drift\n",
                xa.len(),
                xb.len()
            ));
            break;
        }
        if let Some(i) = xa
            .iter()
            .zip(xb)
            .position(|(p, q)| p.to_bits() != q.to_bits())
        {
            let (p, q) = (xa[i], xb[i]);
            first = Some(format!(
                "  first divergence: {field}[{i}]  run1={p:e} 0x{:016x}  run2={q:e} \
                 0x{:016x}  abs diff {:e}\n",
                p.to_bits(),
                q.to_bits(),
                (p - q).abs()
            ));
            break;
        }
    }
    s.push_str(&first.unwrap_or_else(|| {
        String::from(
            "  first divergence: NONE of yhat, yhat_lower, yhat_upper or trend differs by a \
             single bit, so the difference is in ds, components or diagnostics — look \
             there, not at the arithmetic\n",
        )
    }));

    // (2) The whole-response max absolute difference, per field, so a one-ULP libm
    //     difference reads differently from a structural break.
    s.push_str("  max abs difference per field:");
    for ((field, xa), (_, xb)) in float_fields(a).into_iter().zip(float_fields(b)) {
        let m = xa
            .iter()
            .zip(xb)
            .map(|(p, q)| (p - q).abs())
            .fold(0.0_f64, f64::max);
        s.push_str(&format!(" {field}={m:e}"));
    }
    s.push('\n');

    // (3) The budget_hit note, as a POINTER and not as a subtraction.
    let masked_a = signature_with(a, &diagnostics_with_budget_hit_masked(a));
    let masked_b = signature_with(b, &diagnostics_with_budget_hit_masked(b));
    let verdict = if masked_a == masked_b {
        "EQUAL"
    } else {
        "STILL UNEQUAL"
    };
    s.push_str(&format!(
        "  with diagnostics.lbfgs.budget_hit masked to a fixed value, the two signatures \
         are {verdict}.\n\
         \x20   READ THAT AS A POINTER, NOT AS A SUBTRACTION. budget_hit records whether \
         fit::fit_prophet's cooperative round-boundary budget (FIT_BUDGET_SECS, \
         fit.rs:104 and fit.rs:112) was hit, and hitting it BREAKS the optimisation loop \
         — so when the flag flips, the parameters, the iteration count, the objective, \
         the predictions and the bands all differ too.\n\
         \x20   MASKED-EQUAL means: the ONLY difference is the flag, so the two fits \
         genuinely agreed and this is a loaded machine rather than a regression.\n\
         \x20   MASKED-STILL-UNEQUAL does NOT mean the opposite. A REAL budget hit changes \
         far more than the flag, so this line cannot rule an environment effect out; it \
         can only ever rule one IN.\n\
         \x20   budget_hit is deliberately NOT carved out of the signature. SC2 puts \
         diagnostics inside the comparison, and removing one key to dodge a failure mode \
         that has never been observed weakens the gate permanently in exchange for a \
         convenience.\n"
    ));
    s
}

/// Part A: every recorded door case is DETERMINISTIC — the same host, twice, same answer.
///
/// This is the claim that makes the committed baseline mean something. Without it, a
/// baseline mismatch has two readings — "the code changed" and "this run was noisy" — and
/// the second reading is always available to whoever does not want to believe the first.
///
/// UNCONDITIONAL on every architecture, by construction: it compares one host against
/// itself and never against a recorded value, so libm differences cannot reach it. See the
/// module docs for which part carries which exposure.
#[test]
fn part_a_every_case_is_deterministic_through_the_door() {
    let mut exercised = 0usize;
    for (label, csv, horizon, shape) in CASES {
        let args = args_for(csv, horizon, shape);
        let first = crate::forecast::forecast(&args)
            .unwrap_or_else(|e| panic!("case {label} must succeed through the door: {e}"));
        let second = crate::forecast::forecast(&args)
            .unwrap_or_else(|e| panic!("case {label} must succeed on its second call: {e}"));
        assert!(
            signature(&first) == signature(&second),
            "{}",
            mismatch_report(label, &first, &second)
        );
        exercised += 1;
    }
    // Non-vacuity, pinned and PRINTED: a determinism table that silently shrank would
    // otherwise report green while proving less. Same discipline as the poisson sweep's
    // checked-lambda count.
    assert!(
        exercised == CASES.len() && exercised == 8,
        "part A exercised {exercised} cases; it must exercise all 8 recorded door cases \
         (CASES.len() = {})",
        CASES.len()
    );
}

/// The two wall-clock fields are proven EXCLUDED, not merely documented as excluded.
///
/// Without this, "we left the timings out" is a claim about the code rather than a property
/// of the function. And the failure it guards against is vacuity in the OPPOSITE direction:
/// a signature carrying wall-clock can never reproduce, so the gate would be permanently
/// red, therefore permanently ignored, which is exactly as useless as permanently green.
#[test]
fn the_signature_ignores_the_two_wall_clock_fields() {
    let args = args_for("retail_sales.csv", 12, Shape::ProphetMonthly);
    let mut r = crate::forecast::forecast(&args).expect("the retail case must succeed");
    let before = signature(&r);
    let (fit0, predict0) = (r.fit_seconds, r.predict_seconds);
    assert!(
        fit0.is_finite() && predict0.is_finite(),
        "the door must report finite timings, got fit_seconds={fit0} \
         predict_seconds={predict0}"
    );

    r.fit_seconds = fit0 + 1.0;
    r.predict_seconds = predict0 + 2.0;
    assert!(
        r.fit_seconds.to_bits() != fit0.to_bits()
            && r.predict_seconds.to_bits() != predict0.to_bits(),
        "the mutation must actually change both fields, or this test proves nothing"
    );
    assert!(
        signature(&r) == before,
        "replacing BOTH wall-clock fields changed the signature, so the gate is hashing \
         time; it could never reproduce and would be red on every host forever"
    );
}
