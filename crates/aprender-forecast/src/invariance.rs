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

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
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
    json(&mut h, &r.diagnostics);
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

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
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
        cases.insert(
            label.to_string(),
            serde_json::json!({
                "signature": format!("{sig:016x}"),
                "arch": arch,
                "profile": profile,
                "captured_at_commit": head,
            }),
        );
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
        doc.insert("captured_at_commit".into(), serde_json::json!(head));
        doc.insert("captured_on".into(), serde_json::json!(iso_date_utc()));
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
        serde_json::json!(
            "Evidence for ONE tag bump, not a permanent certificate. These signatures \
             describe the tree at `captured_at_commit`; a later intentional behaviour \
             change re-captures them at its own pre-change commit. Keyed by \
             std::env::consts::ARCH because libm and the f32 GEMM routing differ bitwise \
             across architectures."
        ),
    );
    let by_arch = doc
        .entry("by_arch")
        .or_insert_with(|| serde_json::json!({}))
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
    crate::dates::format_ymd(crate::dates::days_from_civil(1970, 1, 1) + days)
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

    r.components
        .insert("phantom".into(), serde_json::json!([0.0]));
    assert_ne!(signature(&r), clean, "an extra component key must show");
    r.components.remove("phantom");

    assert_eq!(
        signature(&r),
        clean,
        "reverting every mutation must return the original signature, or the detector is \
         reacting to something other than what was changed"
    );
}
