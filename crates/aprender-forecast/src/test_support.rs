//! Shared `#[cfg(test)]` helpers: fixture and contract readers.
//!
//! Plans 06-03 through 06-06 build their parity ladders on these, so the gotchas live
//! here once instead of in every test: fixtures are `expect`ed (absence is a defect, never
//! a skip — CLAUDE.md Verification Discipline #5), CSVs are sorted and de-duplicated by
//! `ds` because `wp_log_R.csv` is not chronological, and tolerances are READ from the
//! contract rather than written as literals so the contract stays the source of truth.

use std::path::PathBuf;

/// Absolute path to a committed fixture under `tests/fixtures/`.
pub(crate) fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Read and parse a committed JSON oracle fixture.
///
/// # Panics
///
/// Panics if the fixture is missing or unparseable. Both are DEFECTS, not conditions to
/// skip on: a parity test that silently does not run proves nothing.
pub(crate) fn load_json(name: &str) -> serde_json::Value {
    let path = fixture_path(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture {name} is committed; absence is a defect ({e})"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("fixture {name} is not valid JSON: {e}"))
}

/// Read a two-column `ds,y` CSV fixture, sorted ascending and de-duplicated by `ds`.
///
/// Handles both the quoted (`"ds","y"`) and bare (`ds,y`) header forms, and strips a
/// trailing `\r` so a CRLF checkout cannot change a parsed value.
///
/// # Panics
///
/// Panics if the fixture is missing or a `y` cell is not a number.
pub(crate) fn read_csv(name: &str) -> (Vec<String>, Vec<f64>) {
    let path = fixture_path(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("fixture {name} is committed; absence is a defect ({e})"));
    let cell = |s: &str| s.trim().trim_matches('"').to_string();
    let mut rows: Vec<(String, f64)> = Vec::new();
    for line in raw.lines().skip(1) {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let mut it = line.split(',');
        let (Some(d), Some(v)) = (it.next(), it.next()) else {
            panic!("fixture {name}: line {line:?} is not `ds,y`")
        };
        let y: f64 = cell(v)
            .parse()
            .unwrap_or_else(|e| panic!("fixture {name}: {v:?} is not a number: {e}"));
        rows.push((cell(d), y));
    }
    // wp_log_R.csv is NOT chronological; every consumer wants ascending, unique `ds`.
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.dedup_by(|a, b| a.0 == b.0);
    rows.into_iter().unzip()
}

/// Absolute path to a repository contract, `contracts/<name>.yaml`.
pub(crate) fn contract_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts")
        .join(format!("{name}.yaml"))
}

/// Parse a contract once per process and hand out cheap clones of the parsed tree.
///
/// `serde_yaml` on a 30-50 KB contract costs ~6.5 ms, and the parity ladders call the two
/// readers below ~87 times for Prophet alone — re-reading and re-parsing per call was
/// roughly 0.5 s of pure repeat work. The contracts are immutable for the life of a test
/// binary, so one parse each is enough. D-15's contract-as-source-of-truth is untouched:
/// this memoizes the file plumbing under it, not the lookup.
///
/// # Panics
///
/// Panics if the contract is missing or unparseable.
fn contract_doc(contract: &str) -> std::sync::Arc<serde_yaml::Value> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock, PoisonError};
    static DOCS: OnceLock<Mutex<HashMap<String, Arc<serde_yaml::Value>>>> = OnceLock::new();
    let cache = DOCS.get_or_init(|| Mutex::new(HashMap::new()));

    // Read the cache, then RELEASE the lock. Parsing under the guard would hold a global
    // mutex across ~6.5 ms of file I/O plus deserialisation — serialising every test thread
    // on first touch, the opposite of what this memo is for — and, worse, a missing or
    // malformed contract panics inside that critical section. Unwinding out of a held guard
    // poisons the mutex, so the ONE accurate "contract X must exist at <path>" failure
    // would be followed by dozens of "contract cache mutex poisoned" panics in unrelated
    // tests, none of which name the real defect.
    if let Some(hit) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(contract)
    {
        return Arc::clone(hit);
    }
    let path = contract_path(contract);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("contract {contract} must exist at {}: {e}", path.display()));
    let doc = Arc::new(
        serde_yaml::from_str(&raw)
            .unwrap_or_else(|e| panic!("contract {contract} is not valid YAML: {e}")),
    );
    // A racing thread may have parsed the same contract meanwhile; either Arc is equally
    // correct, so keep whichever landed first and hand back that one.
    Arc::clone(
        cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(contract.to_string())
            .or_insert(doc),
    )
}

/// Read `equations.<equation>.float_tolerance` from a contract.
///
/// Tolerances belong to the contract, not to a test literal: a test that hardcodes one can
/// be loosened without the contract ever noticing.
///
/// # Panics
///
/// Panics if the contract is missing, unparseable, or does not carry that key.
pub(crate) fn equation_tolerance(contract: &str, equation: &str) -> f64 {
    let doc = contract_doc(contract);
    doc.get("equations")
        .and_then(|e| e.get(equation))
        .and_then(|e| e.get("float_tolerance"))
        .and_then(serde_yaml::Value::as_f64)
        .unwrap_or_else(|| {
            panic!("contract {contract} must define equations.{equation}.float_tolerance")
        })
}

/// The whole parsed contract tree, for a test that must walk arbitrary structure.
///
/// [`constant_u64`] and [`constant_f64`] answer "what is the value of ONE named scalar?".
/// The `door_surface` completeness tests ask a different question — "does this LIST agree
/// with what the code actually declares?" — and there is no scalar path that expresses it.
/// Handing out the memoized `Arc` keeps those tests on the same single parse as every other
/// contract reader here rather than opening the file a second time.
pub(crate) fn contract_value(contract: &str) -> std::sync::Arc<serde_yaml::Value> {
    contract_doc(contract)
}

/// Read a top-level `constants.<key>` REAL from a contract.
///
/// [`constant_u64`] cannot express a threshold that is conceptually a real number, and
/// rounding one to fit it would make the mirror assert something weaker than the constant
/// it mirrors. YAML writes `30` and `20000` as integers, so this falls back to the integer
/// accessors and casts — the contract stays readable without forcing `30.0` on every value.
///
/// # Panics
///
/// Panics NAMING THE KEY if the contract is missing, unparseable, or does not carry it —
/// the same failure shape [`constant_u64`] has, because an absent bound must fail loudly
/// rather than default to something plausible.
pub(crate) fn constant_f64(contract: &str, key: &str) -> f64 {
    let doc = contract_doc(contract);
    let v = doc
        .get("constants")
        .and_then(|c| c.get(key))
        .unwrap_or_else(|| panic!("contract {contract} must define constants.{key}"));
    v.as_f64()
        .or_else(|| {
            #[allow(clippy::cast_precision_loss)]
            v.as_u64().map(|n| n as f64)
        })
        .or_else(|| {
            #[allow(clippy::cast_precision_loss)]
            v.as_i64().map(|n| n as f64)
        })
        .unwrap_or_else(|| panic!("contract {contract} constants.{key} must be a number"))
}

/// Read a top-level `constants.<key>` integer from a contract.
///
/// # Panics
///
/// Panics if the contract is missing, unparseable, or does not carry that key.
pub(crate) fn constant_u64(contract: &str, key: &str) -> u64 {
    let doc = contract_doc(contract);
    doc.get("constants")
        .and_then(|c| c.get(key))
        .and_then(serde_yaml::Value::as_u64)
        .unwrap_or_else(|| panic!("contract {contract} must define constants.{key}"))
}

#[cfg(test)]
mod tests {
    use super::{fixture_path, load_json, read_csv};

    #[test]
    fn every_committed_fixture_is_readable() {
        for name in [
            "peyton_manning_prophet140.json",
            "air_passengers_prophet140.json",
            "retail_sales_prophet140.json",
            "np_oracle_peyton.json",
            "peyton_default_prophet140.json",
            "peyton_holidays_prophet140.json",
            "wp_log_R_logistic_prophet140.json",
            "air_multiplicative_prophet140.json",
            "chronos_bolt_tiny_fixture.json",
            "chronos_probes.json",
            "weights_index.json",
            "chronos_holdout_oracle.json",
            "peyton_tiny_oracle.json",
        ] {
            assert!(fixture_path(name).is_file(), "{name} must be committed");
            assert!(load_json(name).is_object(), "{name} must be a JSON object");
        }
    }

    #[test]
    fn read_csv_sorts_and_dedups_the_non_chronological_fixture() {
        for name in [
            "peyton_manning.csv",
            "air_passengers.csv",
            "wp_log_R.csv",
            "retail_sales.csv",
        ] {
            let (ds, y) = read_csv(name);
            assert_eq!(ds.len(), y.len(), "{name}: parallel arrays");
            assert!(ds.len() > 100, "{name}: {} rows is too few", ds.len());
            assert!(
                ds.windows(2).all(|w| w[0] < w[1]),
                "{name}: read_csv must return strictly ascending, unique ds"
            );
            assert!(y.iter().all(|v| v.is_finite()), "{name}: y is finite");
        }
    }

    #[test]
    fn the_peyton_csv_and_the_peyton_oracle_agree_on_the_history() {
        let (ds, y) = read_csv("peyton_manning.csv");
        let fx = load_json("peyton_manning_prophet140.json");
        let fx_ds = fx["history"]["ds"].as_array().expect("history.ds");
        assert_eq!(
            ds.len(),
            fx_ds.len(),
            "the CSV and the Prophet 1.4.0 oracle must describe the same series"
        );
        assert_eq!(ds[0], fx_ds[0].as_str().expect("ds[0]"));
        let fx_y = fx["history"]["y"].as_array().expect("history.y");
        assert!(
            (y[0] - fx_y[0].as_f64().expect("y[0]")).abs() < 1e-12,
            "first y must match the oracle"
        );
    }
}
