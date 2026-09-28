//! The classify tool boundary (`contracts/decide-tool-boundary-v1.yaml`,
//! FALSIFY-DECIDE-TOOL-001..008): every bound refuses at N+1 and accepts at N, the
//! refusals name their key and never echo text, admission counts real CPU work, and
//! the response is ordered by the artifact's task.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use aprender_decide::{DecideError, LayaError};
use proptest::prelude::*;

use super::*;

const TOOL_CONTRACT: &str = "decide-tool-boundary-v1";
/// Every wait in the admission tests is bounded, so a regression fails instead of hanging.
const WAIT: Duration = Duration::from_secs(10);
/// How long a probe waits to be sure something did NOT happen.
const QUIET: Duration = Duration::from_millis(300);

fn repo_path(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

fn contract_doc(name: &str) -> serde_yaml::Value {
    let path = repo_path(&format!("contracts/{name}.yaml"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));
    serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("parse {name}: {e}"))
}

/// `constants.<key>` of a contract as an integer — panics NAMING THE KEY when absent,
/// so a missing bound fails loudly instead of defaulting (copied from
/// aprender-forecast's `test_support::constant_u64`, which is `pub(crate)` there).
fn constant_u64(contract: &str, key: &str) -> u64 {
    contract_doc(contract)
        .get("constants")
        .and_then(|c| c.get(key))
        .and_then(serde_yaml::Value::as_u64)
        .unwrap_or_else(|| panic!("contract {contract} must define constants.{key}"))
}

fn constant_usize(key: &str) -> usize {
    usize::try_from(constant_u64(TOOL_CONTRACT, key)).expect("fits usize")
}

fn fixture_dir() -> std::path::PathBuf {
    repo_path("crates/aprender-decide/tests/fixtures/laya_tiny")
}

/// The tiny Laya fixture, packed by the production packer and loaded through the
/// ladder once per test binary.
fn model() -> Arc<Model> {
    static MODEL: OnceLock<Arc<Model>> = OnceLock::new();
    Arc::clone(MODEL.get_or_init(|| {
        let dir = fixture_dir();
        let bytes = aprender_decide::pack_run_dir(&dir, &dir.join("data")).expect("pack tiny");
        Arc::new(load_model_from_bytes(&bytes).expect("the ladder accepts the tiny fixture"))
    }))
}

/// The tiny fixture's `max_len` (the built-row window), from Laya's own oracle.
fn tiny_max_len() -> usize {
    let oracle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixture_dir().join("oracle.json")).expect("read oracle"),
    )
    .expect("parse oracle");
    usize::try_from(oracle["max_len"].as_u64().expect("max_len")).expect("fits")
}

fn args(texts: &[&str]) -> ClassifyArgs {
    ClassifyArgs {
        texts: texts.iter().map(|t| (*t).to_string()).collect(),
    }
}

/// The validation message of a refusal; any other error kind fails the test.
fn validation_message(error: &pmcp::Error) -> String {
    match error {
        pmcp::Error::Validation(message) => message.clone(),
        other => panic!("expected a validation-class refusal, got {other:?}"),
    }
}

fn assert_names_key(message: &str, key: &str) {
    assert!(
        message.contains(TOOL_CONTRACT) && message.contains(key),
        "the refusal must name {TOOL_CONTRACT} and {key}: {message}"
    );
}

fn shrunk(max_in_flight: usize, max_pending: usize) -> ClassifyLimits {
    ClassifyLimits {
        max_in_flight,
        max_pending,
        ..ClassifyLimits::CONTRACTED
    }
}

// ---------------------------------------------------------------- contract mirror

/// FALSIFY-DECIDE-TOOL-007: every Rust bound equals its contract constant.
#[test]
fn bounds_match_contract() {
    let c = ClassifyLimits::CONTRACTED;
    for (field, key) in [
        (c.min_texts, "classify_min_texts"),
        (c.max_texts, "classify_max_texts"),
        (c.max_text_bytes, "classify_max_text_bytes"),
        (c.max_total_tokens, "classify_max_total_tokens"),
        (c.max_in_flight, "classify_max_in_flight"),
        (c.max_pending, "classify_max_pending"),
    ] {
        assert_eq!(
            field,
            constant_usize(key),
            "ClassifyLimits::CONTRACTED vs {key}"
        );
    }
    assert!(
        CONTRACT.ends_with(&format!("{TOOL_CONTRACT}.yaml")),
        "refusals name the contract file this test reads"
    );
}

/// The second proof obligation, on paper before any deploy can violate it live: the
/// declared token budget fits the Lambda envelope it was derived from.
#[test]
fn token_budget_fits_the_envelope() {
    let tokens = constant_u64(TOOL_CONTRACT, "classify_max_total_tokens");
    let per_token = constant_u64(TOOL_CONTRACT, "tier_ms_per_token_at_512");
    let spent = tokens * per_token
        + constant_u64(TOOL_CONTRACT, "cold_start_budget_ms")
        + constant_u64(TOOL_CONTRACT, "probe_budget_ms")
        + constant_u64(TOOL_CONTRACT, "margin_ms");
    let cap = constant_u64(TOOL_CONTRACT, "api_gateway_timeout_ms");
    assert!(
        spent <= cap,
        "budget {tokens} tokens costs {spent} ms > {cap} ms"
    );
}

/// The count bound is reachable inside the budget: `classify_max_texts` rows of the
/// served task's SHORTEST built row fit `classify_max_total_tokens`. A larger count would
/// be admitted by precheck and always refused by the budget (v2.0.0, 3 008 MB tier:
/// 2 x 57 = 114 <= 120; 8 texts would need 456).
#[test]
fn max_texts_fit_the_budget_at_the_shortest_row() {
    let texts = constant_u64(TOOL_CONTRACT, "classify_max_texts");
    let min_row = constant_u64(TOOL_CONTRACT, "served_task_min_row_tokens");
    let budget = constant_u64(TOOL_CONTRACT, "classify_max_total_tokens");
    assert!(
        texts * min_row <= budget,
        "{texts} texts x {min_row}-token rows = {} > budget {budget}",
        texts * min_row
    );
    assert!(min_row > 0 && min_row <= constant_u64(TOOL_CONTRACT, "row_max_tokens"));
}

/// FALSIFY-DECIDE-TOOL-008 (sizing): admission is sized from the contract.
#[test]
fn admission_limits_match_contract() {
    let c = ClassifyLimits::CONTRACTED;
    assert_eq!(c.max_in_flight, constant_usize("classify_max_in_flight"));
    assert_eq!(c.max_pending, constant_usize("classify_max_pending"));
    let admission = Admission::new(&c);
    assert_eq!(admission.in_flight.available_permits(), c.max_in_flight);
    assert_eq!(admission.pending.available_permits(), c.max_pending);
}

/// The SERVED path runs under the contracted limits and nothing else.
#[tokio::test]
async fn served_service_uses_contracted_limits() {
    let service = ClassifyService::served(model());
    assert_eq!(service.limits(), &ClassifyLimits::CONTRACTED);
    assert_eq!(
        service.admission.pending.available_permits(),
        ClassifyLimits::CONTRACTED.max_pending
    );
    // Behavioural, not just a field read: the served call refuses the contracted N+1.
    let over = vec!["x"; ClassifyLimits::CONTRACTED.max_texts + 1];
    let error = service
        .call(args(&over))
        .await
        .expect_err("max_texts + 1 texts");
    assert_names_key(&validation_message(&error), "classify_max_texts");
}

// ---------------------------------------------------------------- count and bytes

/// `precheck` takes no model, so it cannot tokenize on the async path. The type
/// ascription is the proof: adding a model parameter stops this compiling.
#[test]
fn precheck_takes_no_model() {
    let check: fn(&ClassifyLimits, &ClassifyArgs) -> pmcp::Result<()> = precheck;
    check(&ClassifyLimits::CONTRACTED, &args(&["a"])).expect("one short text passes");
}

/// FALSIFY-DECIDE-TOOL-001: an empty list is refused naming classify_min_texts.
#[test]
fn empty_texts_refused_naming_min_texts() {
    let error = precheck(&ClassifyLimits::CONTRACTED, &args(&[])).expect_err("empty");
    assert_names_key(&validation_message(&error), "classify_min_texts");
}

/// FALSIFY-DECIDE-TOOL-001: N + 1 = classify_max_texts + 1 texts is refused naming
/// classify_max_texts and the observed count. The count comes from the contract, so the
/// test holds at every tier (v1.0.0's 9 texts at 10 240 MB, 3 at 3 008 MB).
#[test]
fn one_over_max_texts_refused_naming_max_texts() {
    let n = constant_usize("classify_max_texts") + 1;
    let error = precheck(&ClassifyLimits::CONTRACTED, &args(&vec!["x"; n])).expect_err("N + 1");
    let message = validation_message(&error);
    assert_names_key(&message, "classify_max_texts");
    assert!(
        message.contains(&format!("{n} texts")),
        "reports the observed count: {message}"
    );
}

/// FALSIFY-DECIDE-TOOL-001: N = classify_max_texts short texts pass every bound and are
/// classified by the SERVED service (contracted limits, the budget included).
#[tokio::test]
async fn max_texts_accepted_and_classified() {
    let pool = [
        "My parcel is late.",
        "I was charged twice.",
        "I cannot log in.",
        "Where is my order?",
        "Please refund the fee.",
        "Reset my password.",
        "The courier lost it.",
        "Update my profile.",
    ];
    let n = constant_usize("classify_max_texts");
    assert!(n <= pool.len(), "the pool covers classify_max_texts {n}");
    let texts = &pool[..n];
    assert_eq!(texts.len(), ClassifyLimits::CONTRACTED.max_texts);
    let response = ClassifyService::served(model())
        .call(args(texts))
        .await
        .expect("the maximal legal count is accepted");
    assert_eq!(response.results.len(), texts.len());
}

/// FALSIFY-DECIDE-TOOL-002 and -006: a 16385-byte text is refused before tokenization,
/// naming the key and the index, and the message does NOT contain the text.
#[test]
fn oversized_text_refused_without_echo() {
    let limit = ClassifyLimits::CONTRACTED.max_text_bytes;
    let text = "ZQXJ-secret-".repeat(limit / 12 + 1);
    let text = &text[..=limit];
    assert_eq!(text.len(), limit + 1);
    let error =
        precheck(&ClassifyLimits::CONTRACTED, &args(&["ok", text])).expect_err("16385 bytes");
    let message = validation_message(&error);
    assert_names_key(&message, "classify_max_text_bytes");
    assert!(
        message.contains("texts[1]") && message.contains("16385"),
        "{message}"
    );
    assert!(
        !message.contains("ZQXJ"),
        "the refusal must not echo the text: {message}"
    );
}

/// FALSIFY-DECIDE-TOOL-002: the bound is on BYTES — a text whose char count fits but
/// whose UTF-8 length does not is refused.
#[test]
fn multibyte_text_over_byte_limit_refused() {
    let limit = ClassifyLimits::CONTRACTED.max_text_bytes;
    let text = "é".repeat(limit / 2 + 1);
    assert!(text.chars().count() <= limit && text.len() > limit);
    let error = precheck(&ClassifyLimits::CONTRACTED, &args(&[&text])).expect_err("bytes");
    assert_names_key(&validation_message(&error), "classify_max_text_bytes");
}

/// FALSIFY-DECIDE-TOOL-002 and -005: a text of exactly 16384 bytes passes the byte
/// bound, is tokenized, and comes back truncated to the model's window.
#[test]
fn text_at_byte_limit_is_tokenized_and_truncated() {
    let limit = ClassifyLimits::CONTRACTED.max_text_bytes;
    let text = "a ".repeat(limit / 2);
    assert_eq!(text.len(), limit);
    let request = ClassifyArgs {
        texts: vec![text.clone()],
    };
    precheck(&ClassifyLimits::CONTRACTED, &request).expect("exactly-at the byte bound passes");
    let decisions =
        classify_blocking(&model(), &ClassifyLimits::CONTRACTED, &[text]).expect("classified");
    assert!(decisions[0].truncated, "a 16 KiB text overflows the window");
    assert_eq!(decisions[0].tokens, tiny_max_len());
}

// ---------------------------------------------------------------- token budget

fn built_lengths(texts: &[String]) -> Vec<usize> {
    model()
        .prepare(texts)
        .expect("prepare")
        .iter()
        .map(PreparedRow::tokens)
        .collect()
}

fn budget_texts() -> Vec<String> {
    vec![
        "My parcel is stuck at the depot.".to_string(),
        "I see two identical charges on my card.".to_string(),
    ]
}

/// FALSIFY-DECIDE-TOOL-003: rows summing to EXACTLY the (shrunk) budget are classified
/// through the whole handler path.
#[tokio::test]
async fn token_budget_accepts_exactly_at() {
    let texts = budget_texts();
    let sum: usize = built_lengths(&texts).iter().sum();
    let limits = ClassifyLimits {
        max_total_tokens: sum,
        ..ClassifyLimits::CONTRACTED
    };
    let response = ClassifyService::with_limits(model(), limits)
        .call(ClassifyArgs { texts })
        .await
        .expect("exactly-at the budget is accepted");
    let charged: usize = response.results.iter().map(|r| r.tokens).sum();
    assert_eq!(
        charged, sum,
        "tokens reports the built-row length the budget charged"
    );
}

/// FALSIFY-DECIDE-TOOL-003 and -006: one token over is refused naming the key, the
/// observed sum and the per-text lengths, never the texts. The budget is shrunk to the
/// fixture's own sum minus one, so the test pins the exactly-at edge at any contracted
/// value instead of depending on the tier's number.
#[tokio::test]
async fn token_budget_refuses_one_over() {
    let texts = budget_texts();
    let per_text = built_lengths(&texts);
    let sum: usize = per_text.iter().sum();
    let limits = ClassifyLimits {
        max_total_tokens: sum - 1,
        ..ClassifyLimits::CONTRACTED
    };
    let error = ClassifyService::with_limits(model(), limits)
        .call(ClassifyArgs {
            texts: texts.clone(),
        })
        .await
        .expect_err("one token over is refused");
    let message = validation_message(&error);
    assert_names_key(&message, "classify_max_total_tokens");
    assert!(
        message.contains(&format!("total {sum} tokens")),
        "{message}"
    );
    assert!(message.contains(&format!("{per_text:?}")), "{message}");
    for text in &texts {
        assert!(!message.contains(text.as_str()), "echoed text: {message}");
    }
}

/// KANI-DECIDE-TOOL-001's runnable evidence: across each boundary, the request is
/// accepted iff the count, byte and token-sum bounds all hold, and the FIRST failing
/// bound in the order count -> bytes -> tokens is the one named.
fn first_failing_key(
    limits: &ClassifyLimits,
    bytes: &[usize],
    built: &[usize],
) -> Option<&'static str> {
    if bytes.len() < limits.min_texts {
        Some("classify_min_texts")
    } else if bytes.len() > limits.max_texts {
        Some("classify_max_texts")
    } else if bytes.iter().any(|&b| b > limits.max_text_bytes) {
        Some("classify_max_text_bytes")
    } else if built.iter().sum::<usize>() > limits.max_total_tokens {
        Some("classify_max_total_tokens")
    } else {
        None
    }
}

proptest! {
    #[test]
    fn bound_order_is_count_bytes_tokens(
        rows in proptest::collection::vec((0usize..=33, 0usize..=20), 0..=9),
    ) {
        // Every field that shapes the draw is pinned here, not inherited from CONTRACTED:
        // with the 3 008 MB tier's 2 texts, 2 x 20 built tokens never reaches 64 and the
        // token branch of the order would go untested.
        let limits = ClassifyLimits {
            max_texts: 8,
            max_text_bytes: 32,
            max_total_tokens: 64,
            ..ClassifyLimits::CONTRACTED
        };
        let bytes: Vec<usize> = rows.iter().map(|r| r.0).collect();
        let built: Vec<usize> = rows.iter().map(|r| r.1).collect();
        let request = ClassifyArgs { texts: bytes.iter().map(|&n| "x".repeat(n)).collect() };
        let outcome = precheck(&limits, &request)
            .and_then(|()| check_token_budget(&limits, &built).map_err(ClassifyFailure::into_pmcp));
        match first_failing_key(&limits, &bytes, &built) {
            None => prop_assert!(outcome.is_ok(), "{outcome:?}"),
            Some(key) => {
                let message = validation_message(&outcome.expect_err("refused"));
                prop_assert!(message.contains(key), "expected {key}: {message}");
            }
        }
    }
}

// ---------------------------------------------------------------- admission

/// A blocking job that reports when it starts and then waits for its release. If the
/// test panics, the release sender drops, `recv` errors, and the job ends — so a failing
/// test cannot leave a blocking thread that keeps the runtime from shutting down.
fn parked_job(
    started: tokio::sync::oneshot::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
) -> impl FnOnce() + Send + 'static {
    move || {
        let _ = started.send(());
        let _ = release.recv();
    }
}

/// FALSIFY-DECIDE-TOOL-008: with in_flight 1 and pending 2 held busy, the 3rd request
/// is refused at once naming classify_max_pending; releasing one lets a new one in.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admission_refuses_over_pending() {
    let admission = Admission::new(&shrunk(1, 2));
    let (a_started, a_started_rx) = tokio::sync::oneshot::channel();
    let (a_release, a_release_rx) = std::sync::mpsc::channel();
    let (b_started, b_started_rx) = tokio::sync::oneshot::channel();
    let (b_release, b_release_rx) = std::sync::mpsc::channel();

    let a = admission.try_admit().expect("first is admitted");
    let a_task = tokio::spawn(a.run_blocking(parked_job(a_started, a_release_rx)));
    tokio::time::timeout(WAIT, a_started_rx)
        .await
        .expect("A runs")
        .expect("A signal");
    let b = admission
        .try_admit()
        .expect("second is admitted (waits for the slot)");
    let b_task = tokio::spawn(b.run_blocking(parked_job(b_started, b_release_rx)));

    let busy = admission.try_admit().expect_err("third is refused at once");
    assert_eq!(busy.max_pending, 2);
    assert_names_key(
        &validation_message(&busy.into_pmcp()),
        "classify_max_pending",
    );

    a_release.send(()).expect("release A");
    tokio::time::timeout(WAIT, a_task)
        .await
        .expect("A ends")
        .expect("A joins")
        .expect("A ok");
    tokio::time::timeout(WAIT, b_started_rx)
        .await
        .expect("B runs after A")
        .expect("B signal");
    let c = admission
        .try_admit()
        .expect("a slot freed when A's work ended");
    b_release.send(()).expect("release B");
    tokio::time::timeout(WAIT, b_task)
        .await
        .expect("B ends")
        .expect("B joins")
        .expect("B ok");
    tokio::time::timeout(WAIT, c.run_blocking(|| ()))
        .await
        .expect("C ends")
        .expect("C ok");
}

/// FALSIFY-DECIDE-TOOL-008: dropping the caller's future while its blocking work runs
/// does NOT free the in-flight slot; the next request runs only when that work ends.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admission_slot_held_until_blocking_ends() {
    let admission = Admission::new(&shrunk(1, 4));
    let (a_started, a_started_rx) = tokio::sync::oneshot::channel();
    let (a_release, a_release_rx) = std::sync::mpsc::channel();
    let a = admission.try_admit().expect("admitted");
    let a_task = tokio::spawn(a.run_blocking(parked_job(a_started, a_release_rx)));
    tokio::time::timeout(WAIT, a_started_rx)
        .await
        .expect("A runs")
        .expect("A signal");

    // The caller disconnects: its future is dropped mid-computation.
    a_task.abort();
    assert!(a_task.await.expect_err("aborted").is_cancelled());
    assert_eq!(
        admission.in_flight.available_permits(),
        0,
        "A's work still holds the slot"
    );
    assert_eq!(
        admission.pending.available_permits(),
        3,
        "and its pending slot"
    );

    let (b_started, mut b_started_rx) = tokio::sync::oneshot::channel();
    let b = admission.try_admit().expect("pending has room");
    let b_task = tokio::spawn(b.run_blocking(move || {
        let _ = b_started.send(());
    }));
    assert!(
        tokio::time::timeout(QUIET, &mut b_started_rx)
            .await
            .is_err(),
        "B must not start while A's blocking work still runs"
    );

    a_release.send(()).expect("release A");
    tokio::time::timeout(WAIT, b_started_rx)
        .await
        .expect("B runs once A ends")
        .expect("B");
    tokio::time::timeout(WAIT, b_task)
        .await
        .expect("B ends")
        .expect("B joins")
        .expect("B ok");
    assert_eq!(admission.in_flight.available_permits(), 1);
    assert_eq!(admission.pending.available_permits(), 4);
}

/// FALSIFY-DECIDE-TOOL-008: dropping a future still WAITING for the in-flight slot
/// releases its pending slot and never runs its work.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admission_waiting_cancel_frees_pending() {
    let admission = Admission::new(&shrunk(1, 2));
    let (a_started, a_started_rx) = tokio::sync::oneshot::channel();
    let (a_release, a_release_rx) = std::sync::mpsc::channel();
    let a = admission.try_admit().expect("admitted");
    let a_task = tokio::spawn(a.run_blocking(parked_job(a_started, a_release_rx)));
    tokio::time::timeout(WAIT, a_started_rx)
        .await
        .expect("A runs")
        .expect("A signal");

    let ran = Arc::new(AtomicBool::new(false));
    let ran_in_b = Arc::clone(&ran);
    let b = admission.try_admit().expect("admitted to wait");
    // Drive B's future ourselves so it is PROVABLY polled into its wait for the slot
    // (an unpolled future would drop its ticket trivially and prove nothing): the
    // bounded poll must time out, because A holds the only in-flight slot.
    let mut b_future = Box::pin(b.run_blocking(move || ran_in_b.store(true, Ordering::SeqCst)));
    assert!(
        tokio::time::timeout(QUIET, &mut b_future).await.is_err(),
        "B must be waiting for the slot A holds"
    );
    assert!(
        admission.try_admit().is_err(),
        "A running + B waiting fill pending 2"
    );

    // The caller disconnects while still waiting: its future is dropped.
    drop(b_future);
    let c = admission
        .try_admit()
        .expect("B's pending slot was released with its future");
    drop(c);

    a_release.send(()).expect("release A");
    tokio::time::timeout(WAIT, a_task)
        .await
        .expect("A ends")
        .expect("A joins")
        .expect("A ok");
    tokio::time::sleep(QUIET).await;
    assert!(
        !ran.load(Ordering::SeqCst),
        "a cancelled waiter never runs its work"
    );
    assert_eq!(admission.pending.available_permits(), 2);
    assert_eq!(admission.in_flight.available_permits(), 1);
}

// ---------------------------------------------------------------- refusals

/// FALSIFY-DECIDE-TOOL-006: every refusal class through the handler path is
/// validation-class, names the contract and its key, and omits the distinctive text.
#[tokio::test]
async fn every_refusal_names_key_without_text() {
    const SECRET: &str = "ZQXJ-distinctive-caller-text";
    let long = format!(
        "{SECRET} {}",
        "y".repeat(ClassifyLimits::CONTRACTED.max_text_bytes)
    );
    let service = ClassifyService::served(model());
    let cases: Vec<(ClassifyArgs, &str)> = vec![
        (args(&[]), "classify_min_texts"),
        (args(&[SECRET; 9]), "classify_max_texts"),
        (args(&[SECRET, &long]), "classify_max_text_bytes"),
    ];
    for (request, key) in cases {
        let message = validation_message(&service.call(request).await.expect_err(key));
        assert_names_key(&message, key);
        assert!(
            !message.contains(SECRET),
            "{key} echoed the text: {message}"
        );
    }

    let tight = ClassifyLimits {
        max_total_tokens: 1,
        ..ClassifyLimits::CONTRACTED
    };
    let error = ClassifyService::with_limits(model(), tight)
        .call(args(&[SECRET]))
        .await
        .expect_err("over budget");
    let message = validation_message(&error);
    assert_names_key(&message, "classify_max_total_tokens");
    assert!(
        !message.contains(SECRET),
        "budget echoed the text: {message}"
    );

    let saturated = ClassifyService::with_limits(model(), shrunk(1, 1));
    let _held = saturated.admission.try_admit().expect("take the only slot");
    let message = validation_message(&saturated.call(args(&[SECRET])).await.expect_err("busy"));
    assert_names_key(&message, "classify_max_pending");
    assert!(!message.contains(SECRET), "busy echoed the text: {message}");
}

/// The error taxonomy: the caller-fixable budget refusal is validation; a model
/// failure is internal and never carries the tokenizer's (possibly text-quoting) detail.
#[test]
fn error_taxonomy_budget_validation_model_internal() {
    let budget = ClassifyFailure::TokenBudget {
        total: 9,
        per_text: vec![9],
        limit: 8,
    };
    assert!(matches!(budget.into_pmcp(), pmcp::Error::Validation(_)));

    let tokenizer = ClassifyFailure::Model(DecideError::Laya(LayaError::Tokenizer(
        "cannot tokenize ZQXJ-caller-text".to_string(),
    )));
    match tokenizer.into_pmcp() {
        pmcp::Error::Internal(message) => {
            assert!(
                !message.contains("ZQXJ"),
                "tokenizer detail leaked: {message}"
            );
        }
        other => panic!("a model failure is internal, got {other:?}"),
    }
    let label = ClassifyFailure::LabelIndex {
        index: 7,
        labels: 3,
    };
    assert!(matches!(label.into_pmcp(), pmcp::Error::Internal(_)));
    assert!(matches!(
        Busy { max_pending: 4 }.into_pmcp(),
        pmcp::Error::Validation(_)
    ));
}

// ---------------------------------------------------------------- request schema

/// A caller-supplied `labels` key is refused: the labels are the artifact's (D-09).
#[test]
fn unknown_key_labels_refused() {
    let error = serde_json::from_value::<ClassifyArgs>(serde_json::json!({
        "texts": ["a"],
        "labels": ["x", "y"]
    }))
    .expect_err("an unmodeled key must be a rejection");
    assert!(error.to_string().contains("unknown field"), "{error}");
}

/// A malformed argument shape is refused through [`parse_args`] as a validation error that
/// names the contract and NEVER echoes caller text: serde's own message quotes a string
/// sent where the list belongs, and an unknown key, verbatim.
#[test]
fn malformed_arguments_are_refused_without_echo() {
    const SECRET: &str = "ZQXJ-caller-document-4111";
    let mut unknown_key = serde_json::Map::new();
    unknown_key.insert("texts".to_string(), serde_json::Value::from(vec!["a"]));
    unknown_key.insert(SECRET.to_string(), serde_json::Value::from(1));
    for (label, bad) in [
        (
            "string for the list",
            serde_json::Value::from(serde_json::Map::from_iter([(
                "texts".to_string(),
                serde_json::Value::from(SECRET),
            )])),
        ),
        ("unknown key", serde_json::Value::Object(unknown_key)),
        ("not an object", serde_json::Value::from(SECRET)),
    ] {
        let raw = serde_json::from_value::<ClassifyArgs>(bad.clone()).expect_err(label);
        assert!(
            raw.to_string().contains(SECRET),
            "{label}: serde itself echoes, which is why its message is withheld"
        );
        let message = validation_message(&parse_args(bad).expect_err(label));
        assert!(message.contains(TOOL_CONTRACT), "{label}: {message}");
        assert!(!message.contains(SECRET), "{label} echoed: {message}");
    }
    let ok = parse_args(serde_json::Value::from(serde_json::Map::from_iter([(
        "texts".to_string(),
        serde_json::Value::from(vec!["a", "b"]),
    )])))
    .expect("the one accepted shape");
    assert_eq!(ok.texts, ["a", "b"]);
}

/// decide-tool-boundary-v1 `classify_token_budget`: the contracted count must fit the budget
/// for the SERVED task's shortest row, or the server refuses to build (instead of
/// advertising a count every call at which is refused).
#[test]
fn served_task_must_fit_the_contracted_count() {
    let min_row = check_served_task_fits(&model(), &ClassifyLimits::CONTRACTED)
        .expect("the tiny task fits the contracted tier");
    assert!(min_row > 0);
    assert!(
        min_row * ClassifyLimits::CONTRACTED.max_texts
            <= ClassifyLimits::CONTRACTED.max_total_tokens
    );
    let tight = ClassifyLimits {
        max_total_tokens: min_row * ClassifyLimits::CONTRACTED.max_texts - 1,
        ..ClassifyLimits::CONTRACTED
    };
    let error = check_served_task_fits(&model(), &tight).expect_err("one token short");
    assert!(
        error.to_string().contains("classify_max_total_tokens"),
        "{error}"
    );
    build_server(model(), "decide-fit-test", "0.0.0").expect("the tiny task builds a server");
}

/// The advertised schema is strict and `texts` is its only, required field.
#[test]
fn input_schema_is_strict() {
    let schema =
        serde_json::to_value(schemars::schema_for!(ClassifyArgs)).expect("schema serializes");
    assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    assert_eq!(schema["required"], serde_json::json!(["texts"]));
    let properties = schema["properties"].as_object().expect("properties");
    assert_eq!(properties.keys().collect::<Vec<_>>(), ["texts"]);
}

// ---------------------------------------------------------------- response

/// FALSIFY-DECIDE-TOOL-004: labels follow task.json (shipping, billing, account — not
/// alphabetical); every probability array has K entries in that order, sums to 1, and
/// its argmax is the label. pmcp links serde_json/preserve_order into this crate, so
/// this is the ON backing; aprender-decide's own task tests carry the OFF leg.
#[tokio::test]
async fn response_labels_follow_task_order() {
    let model = model();
    let texts = [
        "My parcel never arrived.",
        "Why was I billed twice?",
        "I cannot reset my password.",
    ];
    // As many as the SERVED count bound admits (tier policy: 2 at 3 008 MB).
    let n = texts.len().min(ClassifyLimits::CONTRACTED.max_texts);
    let response = ClassifyService::served(Arc::clone(&model))
        .call(args(&texts[..n]))
        .await
        .expect("classified");
    assert_eq!(response.results.len(), n);
    assert_eq!(response.labels, ["shipping", "billing", "account"]);
    assert_eq!(response.labels, model.task().owned_labels());
    for (i, r) in response.results.iter().enumerate() {
        assert_eq!(
            r.probabilities.len(),
            response.labels.len(),
            "result {i}: K"
        );
        let sum: f64 = r.probabilities.iter().map(|&p| f64::from(p)).sum();
        assert!((sum - 1.0).abs() <= 1e-6, "result {i}: sums to {sum}");
        let argmax = r.probabilities.iter().enumerate().fold(0, |best, (k, &p)| {
            if p > r.probabilities[best] {
                k
            } else {
                best
            }
        });
        assert_eq!(
            r.label, response.labels[argmax],
            "result {i}: label is the argmax"
        );
    }

    // The wire shape: arrays, never a label-keyed map; identity on every response.
    let wire = serde_json::to_value(&response).expect("serializes");
    assert!(wire["labels"].is_array());
    assert!(wire["results"][0]["probabilities"].is_array());
    let id = model.identity();
    assert_eq!(
        wire["model"]["artifact_sha256"],
        id.artifact_sha256.as_str()
    );
    assert_eq!(wire["model"]["recipe_id"], id.recipe_id.as_str());
    assert_eq!(wire["model"]["method"], "laya");
    assert_eq!(wire["model"]["base"], id.base.as_str());
    let keys: Vec<&String> = wire["results"][0]
        .as_object()
        .expect("result")
        .keys()
        .collect();
    for key in ["label", "probabilities", "tokens", "truncated"] {
        assert!(keys.iter().any(|k| *k == key), "result lacks {key}");
    }
}

/// Grow a text one filler word at a time and return the built row at `words` words.
fn filler_row(words: usize) -> PreparedRow {
    let text = vec!["a"; words].join(" ");
    model().prepare(&[text]).expect("prepare").remove(0)
}

/// FALSIFY-DECIDE-TOOL-005: a text exactly filling the room the builder leaves is NOT
/// truncated; one filler word more is, and both build to exactly the window.
#[test]
fn truncation_flag_flips_one_past_the_room() {
    let window = tiny_max_len();
    let first_cut = (1..=window)
        .find(|&n| filler_row(n).truncated())
        .expect("some filler length overflows the window");
    let fits = filler_row(first_cut - 1);
    let cut = filler_row(first_cut);
    assert!(
        !fits.truncated(),
        "exactly filling the room is not truncated"
    );
    assert_eq!(
        fits.tokens(),
        window,
        "the last fitting text fills the window exactly"
    );
    assert!(cut.truncated(), "one token more is truncated (D-12)");
    assert_eq!(cut.tokens(), window, "a truncated row is cut to the window");
}

/// D-12 through the response: a long text reports truncated, a short one does not.
#[tokio::test]
async fn long_text_truncated_short_text_not() {
    let long = "The customer has written several times about the delayed parcel. ".repeat(20);
    let response = ClassifyService::served(model())
        .call(ClassifyArgs {
            texts: vec!["Short and sweet.".to_string(), long],
        })
        .await
        .expect("classified");
    assert!(!response.results[0].truncated);
    assert!(response.results[1].truncated);
    assert_eq!(response.results[1].tokens, tiny_max_len());
}

/// The description is built from the artifact: its question, its labels in order, the
/// bounds and the one-document guidance.
#[test]
fn tool_description_lists_labels_in_order_with_guidance() {
    let model = model();
    let description = tool_description(&model);
    assert!(description.contains(model.task().instructions()));
    let mut last = 0usize;
    for label in model.task().labels() {
        let at = description[last..]
            .find(label)
            .unwrap_or_else(|| panic!("description lacks {label} after byte {last}"));
        last += at + label.len();
    }
    // The bounds are read from the contract, so the description is checked at the tier
    // the contract prices (a stale literal here once hid a tier change).
    let bounds = [
        format!(
            "{}..={} texts",
            constant_u64(TOOL_CONTRACT, "classify_min_texts"),
            constant_u64(TOOL_CONTRACT, "classify_max_texts")
        ),
        format!(
            "{} UTF-8 bytes",
            constant_u64(TOOL_CONTRACT, "classify_max_text_bytes")
        ),
        format!(
            "{} model tokens",
            constant_u64(TOOL_CONTRACT, "classify_max_total_tokens")
        ),
    ];
    for phrase in [
        "ONE complete document",
        "NEVER split",
        "never join",
        "truncated: true",
        bounds[0].as_str(),
        bounds[1].as_str(),
        bounds[2].as_str(),
        TOOL_CONTRACT,
    ] {
        assert!(
            description.contains(phrase),
            "description lacks {phrase:?}: {description}"
        );
    }
}
