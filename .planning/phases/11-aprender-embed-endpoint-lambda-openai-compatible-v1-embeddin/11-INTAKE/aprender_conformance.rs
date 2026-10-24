//! Phase 205-06: the thirteen-check conformance probe for a deployed `aprender-embed` function.
//!
//! "Is the delivered Aprender server correct?" is one command:
//!
//! ```text
//! just aprender-embed-conformance lambda://<function-name>/v1/embeddings <reference-vectors.json>
//! ```
//!
//! which must print `CONFORMANCE RESULT PASS 13/13`. The thirteen checks are the ones named in the
//! hand-off brief's "Acceptance" section (`205-APRENDER-EMBED-HANDOFF.md`), in its order: shape,
//! l2-normalized, non-degenerate, deterministic, batch-order-invariant, long-input,
//! truncation-upper, truncation-lower, reference-vectors, semantic-sanity, model-echo,
//! empty-string-400, wrong-dimensions-400. The same probe is the aprender phase's own acceptance,
//! so there is no second, private standard.
//!
//! # What proves the probe itself
//!
//! A probe that cannot fail proves nothing. The hermetic tests below (no feature, no network, no
//! AWS) run every check against a contract-faithful fake and then against the known-broken server
//! shapes, and assert each broken shape FAILS the specific check that should catch it: `apr serve`'s
//! 128-zero stub, a server that panics on long input, a server with no truncation, one that cuts at
//! 128 content tokens (truncation-lower), one that cuts at 510 (truncation-upper), a reference file
//! whose vectors differ, a missing reference file, and a lenient server that accepts what the
//! contract says to refuse.
//!
//! # The live run
//!
//! The `#[ignore]` test at the bottom drives a deployed function through the real Lambda client. It
//! exists only with snapshot-build's `aprender-lambda` feature (the recipe enables it), so a plain
//! `cargo test -p snapshot-build` still compiles and runs every hermetic self-test. It reads:
//!
//! - `APRENDER_EMBED_BASE_URL` -- the `lambda://` form ONLY; anything else is refused with a named
//!   error (an HTTP URL would not exercise the production transport);
//! - `APRENDER_EMBED_REFERENCE` -- the delivered `aprender-embed-reference-vectors.json`.
//!
//! and refuses to run when any `AWS_ENDPOINT_URL*` variable is set. Output lines are
//! `CONFORMANCE <check> PASS|FAIL <detail>` and `CONFORMANCE RESULT PASS|FAIL <passed>/<total>`.

use std::sync::Arc;

use async_trait::async_trait;
use edgequake_llm::{EmbeddingProvider, LlmError};
use serde_json::{json, Value};
use snapshot_build::providers::aprender::{
    parse_endpoint, AprenderEndpoint, APRENDER_DIMENSION, APRENDER_MODEL,
};
use snapshot_build::providers::lambda_invoke::{
    invoke_json, InvokeOutcome, LambdaEmbedder, LambdaInvoker, EMBEDDINGS_PATH,
};

const MODEL: &str = APRENDER_MODEL;
const DIMENSION: usize = APRENDER_DIMENSION;
/// How many checks the brief names.
const TOTAL_CHECKS: usize = 13;

/// The brief's word list A (50 words).
const WORDS_A: &str = "river mountain forest valley stone cloud winter summer garden bridge ocean island \
desert meadow harbor lantern orchard canyon glacier prairie thunder autumn spring breeze shadow harvest \
village castle tower window mirror candle anchor compass feather violin ribbon basket blanket pillow \
kettle hammer ladder pencil wagon saddle fabric marble copper silver";
/// The brief's word list B (50 words).
const WORDS_B: &str = "budget profit revenue server network thread buffer socket packet router backup \
cluster signal sensor battery circuit voltage current engine pump valve wheel rocket orbit radio \
satellite salary audit tax loan share auction contract deposit premium currency ledger invoice database \
kernel program delay capacity software upgrade code protocol pipeline reactor market";

// ---------------------------------------------------------------------------------------------
// Fixtures, generated exactly as the brief defines them
// ---------------------------------------------------------------------------------------------

/// `the` repeated `n` times, space-joined (`the` is a single wordpiece in the uncased vocabulary).
fn the_times(n: usize) -> String {
    vec!["the"; n].join(" ")
}

/// A word list repeated `times` times, space-joined.
fn repeated(list: &str, times: usize) -> String {
    vec![list; times].join(" ")
}

/// T-UPPER-A / T-UPPER-B: 260 `the`, then a 300-word tail (a list six times). A server cutting at
/// 256 tokens returns the same vector for both; one cutting at 512, or not at all, does not.
fn t_upper(tail: &str) -> String {
    format!("{} {}", the_times(260), repeated(tail, 6))
}

/// T-LOWER-A / T-LOWER-B: 200 `the`, then a 50-word tail. 250 content wordpieces fit under the cap,
/// so a server cutting at 256 returns clearly different vectors; one cutting at 128 does not.
fn t_lower(tail: &str) -> String {
    format!("{} {}", the_times(200), tail)
}

/// The seven boundary texts the reference-vectors file covers, with their ids.
fn boundary_texts() -> Vec<(&'static str, String)> {
    vec![
        ("T-UPPER-A", t_upper(WORDS_A)),
        ("T-UPPER-B", t_upper(WORDS_B)),
        ("T-LOWER-A", t_lower(WORDS_A)),
        ("T-LOWER-B", t_lower(WORDS_B)),
        ("T-253", the_times(253)),
        ("T-254", the_times(254)),
        ("T-255", the_times(255)),
    ]
}

fn boundary(id: &str) -> String {
    boundary_texts()
        .into_iter()
        .find(|(name, _)| *name == id)
        .map(|(_, text)| text)
        .unwrap_or_else(|| panic!("no boundary text {id}"))
}

/// Three distinct sentences.
fn sentences() -> Vec<String> {
    [
        "The quick brown fox jumps over the lazy dog near the riverbank.",
        "Quarterly revenue grew by twelve percent driven by subscription renewals.",
        "A recipe for sourdough bread needs flour, water, salt and patience.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Three `(query, relevant, irrelevant)` triples about tables, orders and customers.
fn triples() -> Vec<(String, String, String)> {
    [
        (
            "Which table stores customer names?",
            "The customers table stores each customer's name and email address.",
            "The weather forecast predicts heavy rain across the northern hills tomorrow.",
        ),
        (
            "How are orders linked to customers?",
            "Each orders row has a foreign key linking it to the customers row that placed it.",
            "A marathon runner trains for months before the autumn race.",
        ),
        (
            "What columns does the orders table have?",
            "The orders table has columns for order id, order date, total amount and status.",
            "The orchestra rehearsed a symphony before the evening concert.",
        ),
    ]
    .iter()
    .map(|(q, r, i)| (q.to_string(), r.to_string(), i.to_string()))
    .collect()
}

/// An input of far more than the brief's 1,609 wordpieces: identifier-like tokens, which the
/// WordPiece vocabulary splits into several pieces each.
fn long_input() -> String {
    (0..2200)
        .map(|i| format!("order_line_item_{i}_total_amount"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let (mut dot, mut na, mut nb) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (x, y) in a.iter().zip(b) {
        dot += f64::from(*x) * f64::from(*y);
        na += f64::from(*x) * f64::from(*x);
        nb += f64::from(*y) * f64::from(*y);
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

// ---------------------------------------------------------------------------------------------
// The reference-vectors file
// ---------------------------------------------------------------------------------------------

/// One reference entry: the boundary text id, the text it was computed over (when present), and its
/// sentence-transformers vector.
struct ReferenceEntry {
    id: String,
    text: Option<String>,
    vector: Vec<f32>,
}

/// The delivered `aprender-embed-reference-vectors.json`.
struct Reference {
    entries: Vec<ReferenceEntry>,
}

fn parse_reference(raw: &str) -> Result<Reference, String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|e| format!("the reference file (APRENDER_EMBED_REFERENCE) is not JSON: {e}"))?;
    let texts = value
        .get("texts")
        .and_then(Value::as_array)
        .ok_or_else(|| "the reference file (APRENDER_EMBED_REFERENCE) has no \"texts\" array".to_string())?;
    let mut entries = Vec::new();
    for entry in texts {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| "a reference entry has no \"id\"".to_string())?
            .to_string();
        let vector = entry
            .get("vector")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("reference entry {id} has no \"vector\" array"))?
            .iter()
            .map(|v| v.as_f64().map(|n| n as f32))
            .collect::<Option<Vec<f32>>>()
            .ok_or_else(|| format!("reference entry {id} has a non-numeric vector value"))?;
        entries.push(ReferenceEntry {
            id,
            text: entry.get("text").and_then(Value::as_str).map(str::to_string),
            vector,
        });
    }
    Ok(Reference { entries })
}

/// Load the reference file named by `APRENDER_EMBED_REFERENCE`. Every error names that variable.
fn load_reference(path: Option<&str>) -> Result<Reference, String> {
    let Some(path) = path.map(str::trim).filter(|p| !p.is_empty()) else {
        return Err(
            "APRENDER_EMBED_REFERENCE is not set: the reference-vectors check needs the delivered \
             aprender-embed-reference-vectors.json"
                .to_string(),
        );
    };
    let raw = std::fs::read_to_string(path).map_err(|e| {
        format!("cannot read the reference file named by APRENDER_EMBED_REFERENCE ({path}): {e}")
    })?;
    parse_reference(&raw)
}

/// Whitespace-normalised, so a trailing newline in a delivered file is not a false failure.
fn normalised(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------------------------
// The probe
// ---------------------------------------------------------------------------------------------

/// One check's outcome.
#[derive(Debug, Clone)]
struct CheckResult {
    name: &'static str,
    ok: bool,
    detail: String,
}

struct Probe {
    invoker: Arc<dyn LambdaInvoker>,
    function_name: String,
    reference: Result<Reference, String>,
}

impl Probe {
    fn embedder(&self) -> LambdaEmbedder {
        LambdaEmbedder::new(&self.function_name, EMBEDDINGS_PATH, MODEL, DIMENSION, self.invoker.clone())
    }

    /// Embed through the production client (strict decode: count, indices, length, finite, non-zero).
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.embedder().embed(texts).await.map_err(|e| e.to_string())
    }

    /// A raw request: the function's `(statusCode, parsed body)`.
    async fn raw(&self, body: &Value) -> Result<(u16, Value), String> {
        invoke_json(self.invoker.as_ref(), &self.function_name, EMBEDDINGS_PATH, body)
            .await
            .map_err(|e| e.to_string())
    }

    fn request(inputs: &[String]) -> Value {
        json!({ "model": MODEL, "input": inputs })
    }

    async fn run(&self) -> Vec<CheckResult> {
        let mut results = Vec::with_capacity(TOTAL_CHECKS);
        let mut record = |name: &'static str, outcome: Result<String, String>| {
            let (ok, detail) = match outcome {
                Ok(detail) => (true, detail),
                Err(detail) => (false, detail),
            };
            results.push(CheckResult { name, ok, detail });
        };
        record("shape", self.shape().await);
        record("l2-normalized", self.l2_normalized().await);
        record("non-degenerate", self.non_degenerate().await);
        record("deterministic", self.deterministic().await);
        record("batch-order-invariant", self.batch_order_invariant().await);
        record("long-input", self.long_input().await);
        record("truncation-upper", self.truncation_upper().await);
        record("truncation-lower", self.truncation_lower().await);
        record("reference-vectors", self.reference_vectors().await);
        record("semantic-sanity", self.semantic_sanity().await);
        record("model-echo", self.model_echo().await);
        record("empty-string-400", self.empty_string_400().await);
        record("wrong-dimensions-400", self.wrong_dimensions_400().await);
        results
    }

    /// 1. Count matches the input, each embedding has 384 floats, all finite -- judged on the RAW
    /// body, independently of the client's own decoder.
    async fn shape(&self) -> Result<String, String> {
        let inputs = sentences();
        let (status, body) = self.raw(&Self::request(&inputs)).await?;
        if status != 200 {
            return Err(format!("status {status}, expected 200"));
        }
        let data = body
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| "the 200 body has no \"data\" array".to_string())?;
        if data.len() != inputs.len() {
            return Err(format!("{} embeddings for {} inputs", data.len(), inputs.len()));
        }
        for (position, entry) in data.iter().enumerate() {
            let values = entry
                .get("embedding")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("entry {position} has no embedding array"))?;
            if values.len() != DIMENSION {
                return Err(format!("entry {position} has {} floats, expected {DIMENSION}", values.len()));
            }
            for value in values {
                let finite = value.as_f64().is_some_and(|n| n.is_finite() && (n as f32).is_finite());
                if !finite {
                    return Err(format!("entry {position} has a non-finite or non-numeric value"));
                }
            }
        }
        Ok(format!("{} embeddings x {DIMENSION} finite floats", data.len()))
    }

    /// 2. Norm 1 within 1e-3.
    async fn l2_normalized(&self) -> Result<String, String> {
        let vectors = self.embed(&sentences()).await?;
        let mut worst = 0.0_f64;
        for (position, vector) in vectors.iter().enumerate() {
            let norm = vector.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>().sqrt();
            worst = worst.max((norm - 1.0).abs());
            if (norm - 1.0).abs() > 1e-3 {
                return Err(format!("embedding {position} has norm {norm:.6}, expected 1 +/- 0.001"));
            }
        }
        Ok(format!("max |norm - 1| = {worst:.2e}"))
    }

    /// 3. Three different texts, pairwise cosine < 0.99 (fails the zero-vector stub).
    async fn non_degenerate(&self) -> Result<String, String> {
        let vectors = self.embed(&sentences()).await?;
        let mut highest = f64::MIN;
        for i in 0..vectors.len() {
            for j in (i + 1)..vectors.len() {
                let c = cosine(&vectors[i], &vectors[j]);
                highest = highest.max(c);
                if c >= 0.99 {
                    return Err(format!("texts {i} and {j} have cosine {c:.6}, expected < 0.99"));
                }
            }
        }
        Ok(format!("highest pairwise cosine {highest:.4}"))
    }

    /// 4. The same text twice: cosine >= 0.99999.
    async fn deterministic(&self) -> Result<String, String> {
        let text = vec![sentences()[0].clone()];
        let first = self.embed(&text).await?;
        let second = self.embed(&text).await?;
        let c = cosine(&first[0], &second[0]);
        if c < 0.99999 {
            return Err(format!("two embeddings of one text have cosine {c:.7}, expected >= 0.99999"));
        }
        Ok(format!("cosine {c:.7}"))
    }

    /// 5. Batched (and reversed-batch) results equal the single-input results: cosine >= 0.9999.
    async fn batch_order_invariant(&self) -> Result<String, String> {
        let inputs = sentences();
        let batched = self.embed(&inputs).await?;
        let mut reversed_inputs = inputs.clone();
        reversed_inputs.reverse();
        let reversed = self.embed(&reversed_inputs).await?;
        let mut lowest = f64::MAX;
        for (position, text) in inputs.iter().enumerate() {
            let single = self.embed(std::slice::from_ref(text)).await?;
            let in_batch = cosine(&single[0], &batched[position]);
            let in_reversed = cosine(&single[0], &reversed[inputs.len() - 1 - position]);
            lowest = lowest.min(in_batch).min(in_reversed);
            if in_batch < 0.9999 || in_reversed < 0.9999 {
                return Err(format!(
                    "text {position}: single vs batch cosine {in_batch:.6}, vs reversed batch {in_reversed:.6}, expected >= 0.9999"
                ));
            }
        }
        Ok(format!("lowest single-vs-batch cosine {lowest:.6}"))
    }

    /// 6. An input of at least 1,609 wordpieces returns 200 (a real `apr embed` panics above 512).
    async fn long_input(&self) -> Result<String, String> {
        let (status, body) = self.raw(&Self::request(&[long_input()])).await?;
        if status != 200 {
            return Err(format!("status {status} for a 2,200-identifier input, expected 200"));
        }
        let entries = body.get("data").and_then(Value::as_array).map_or(0, Vec::len);
        if entries != 1 {
            return Err(format!("{entries} embeddings for one long input"));
        }
        Ok("200 with one embedding".to_string())
    }

    /// 7. T-UPPER-A vs T-UPPER-B: cosine >= 0.9999 (the tails past the 256-token cap are cut away).
    async fn truncation_upper(&self) -> Result<String, String> {
        let vectors = self.embed(&[boundary("T-UPPER-A"), boundary("T-UPPER-B")]).await?;
        let c = cosine(&vectors[0], &vectors[1]);
        if c < 0.9999 {
            return Err(format!(
                "T-UPPER-A vs T-UPPER-B cosine {c:.6}, expected >= 0.9999: the server does not cut at 256 tokens"
            ));
        }
        Ok(format!("cosine {c:.7}"))
    }

    /// 8. T-LOWER-A vs T-LOWER-B: cosine < 0.999 (250 content wordpieces are under the cap and must
    /// all be kept; a server cutting at 128 or 200 loses the tails).
    async fn truncation_lower(&self) -> Result<String, String> {
        let vectors = self.embed(&[boundary("T-LOWER-A"), boundary("T-LOWER-B")]).await?;
        let c = cosine(&vectors[0], &vectors[1]);
        if c >= 0.999 {
            return Err(format!(
                "T-LOWER-A vs T-LOWER-B cosine {c:.6}, expected < 0.999: the server cuts below 256 tokens"
            ));
        }
        Ok(format!("cosine {c:.6}"))
    }

    /// 9. Every boundary text within cosine 0.9999 of the delivered reference vector.
    async fn reference_vectors(&self) -> Result<String, String> {
        let reference = self.reference.as_ref().map_err(Clone::clone)?;
        let boundaries = boundary_texts();
        let texts: Vec<String> = boundaries.iter().map(|(_, text)| text.clone()).collect();
        let vectors = self.embed(&texts).await?;
        let mut lowest = f64::MAX;
        for ((id, text), vector) in boundaries.iter().zip(&vectors) {
            let Some(entry) = reference.entries.iter().find(|e| e.id == *id) else {
                return Err(format!("the reference file (APRENDER_EMBED_REFERENCE) has no entry for {id}"));
            };
            if entry.vector.len() != DIMENSION {
                return Err(format!(
                    "reference entry {id} has {} floats, expected {DIMENSION}",
                    entry.vector.len()
                ));
            }
            if let Some(reference_text) = &entry.text {
                if normalised(reference_text) != normalised(text) {
                    return Err(format!(
                        "reference entry {id} was computed over a different text than the brief defines"
                    ));
                }
            }
            let c = cosine(vector, &entry.vector);
            lowest = lowest.min(c);
            if c < 0.9999 {
                return Err(format!("{id}: cosine {c:.6} to the reference vector, expected >= 0.9999"));
            }
        }
        Ok(format!("{} texts, lowest cosine {lowest:.7}", boundaries.len()))
    }

    /// 10. Three query/relevant/irrelevant triples order correctly.
    async fn semantic_sanity(&self) -> Result<String, String> {
        let triples = triples();
        let flat: Vec<String> = triples
            .iter()
            .flat_map(|(q, r, i)| [q.clone(), r.clone(), i.clone()])
            .collect();
        let vectors = self.embed(&flat).await?;
        let mut smallest_margin = f64::MAX;
        for (n, chunk) in vectors.chunks(3).enumerate() {
            let relevant = cosine(&chunk[0], &chunk[1]);
            let irrelevant = cosine(&chunk[0], &chunk[2]);
            smallest_margin = smallest_margin.min(relevant - irrelevant);
            if relevant <= irrelevant {
                return Err(format!(
                    "triple {n}: relevant cosine {relevant:.4} is not above irrelevant {irrelevant:.4}"
                ));
            }
        }
        Ok(format!("{} triples ordered, smallest margin {smallest_margin:.4}", vectors.len() / 3))
    }

    /// 11. The response `model` equals `all-MiniLM-L6-v2`.
    async fn model_echo(&self) -> Result<String, String> {
        let (status, body) = self.raw(&Self::request(&sentences()[..1])).await?;
        if status != 200 {
            return Err(format!("status {status}, expected 200"));
        }
        match body.get("model").and_then(Value::as_str) {
            Some(MODEL) => Ok(format!("model {MODEL}")),
            Some(other) => Err(format!("response model is {other:?}, expected {MODEL:?}")),
            None => Err("the response has no \"model\" string".to_string()),
        }
    }

    /// 12. An empty-string input returns 400 with the OpenAI error envelope.
    async fn empty_string_400(&self) -> Result<String, String> {
        let (status, body) = self.raw(&json!({ "model": MODEL, "input": [""] })).await?;
        if status != 400 {
            return Err(format!("status {status} for an empty-string input, expected 400"));
        }
        let message = body.pointer("/error/message").and_then(Value::as_str);
        let kind = body.pointer("/error/type").and_then(Value::as_str);
        if message.is_none_or(str::is_empty) || kind.is_none_or(str::is_empty) {
            return Err("status 400 but the body is not the OpenAI error envelope {error:{message,type}}".to_string());
        }
        Ok("400 with the OpenAI error envelope".to_string())
    }

    /// 13. `dimensions` other than 384 returns 400.
    async fn wrong_dimensions_400(&self) -> Result<String, String> {
        let (status, _) = self
            .raw(&json!({ "model": MODEL, "input": ["a sentence"], "dimensions": 128 }))
            .await?;
        if status != 400 {
            return Err(format!("status {status} for dimensions=128, expected 400"));
        }
        Ok("400".to_string())
    }
}

async fn run_probe(
    invoker: Arc<dyn LambdaInvoker>,
    function_name: &str,
    reference: Result<Reference, String>,
) -> Vec<CheckResult> {
    Probe {
        invoker,
        function_name: function_name.to_string(),
        reference,
    }
    .run()
    .await
}

/// The lines the probe prints, in order: one per check, then the result line.
fn report_lines(results: &[CheckResult]) -> Vec<String> {
    let passed = results.iter().filter(|r| r.ok).count();
    let mut lines: Vec<String> = results
        .iter()
        .map(|r| {
            let detail = r.detail.replace(['\n', '\r'], " ");
            format!("CONFORMANCE {} {} {detail}", r.name, if r.ok { "PASS" } else { "FAIL" })
        })
        .collect();
    let verdict = if passed == TOTAL_CHECKS && results.len() == TOTAL_CHECKS { "PASS" } else { "FAIL" };
    lines.push(format!("CONFORMANCE RESULT {verdict} {passed}/{TOTAL_CHECKS}"));
    lines
}

fn print_report(results: &[CheckResult]) -> String {
    let lines = report_lines(results);
    for line in &lines {
        println!("{line}");
    }
    lines.last().cloned().unwrap_or_default()
}

/// The names of every `AWS_ENDPOINT_URL*` variable among `names` (names only, never values).
fn endpoint_override_names(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut offenders: Vec<String> = names.filter(|n| n.starts_with("AWS_ENDPOINT_URL")).collect();
    offenders.sort();
    offenders.dedup();
    offenders
}

/// Resolve `APRENDER_EMBED_BASE_URL`: the `lambda://` form only.
fn parse_probe_target(base_url: &str) -> Result<(String, String), String> {
    match parse_endpoint(base_url)? {
        AprenderEndpoint::Lambda { function_name, path } => Ok((function_name, path)),
        AprenderEndpoint::Loopback { .. } => Err(format!(
            "APRENDER_EMBED_BASE_URL {base_url:?} is not a lambda:// URL: the conformance probe drives the \
             production transport only (lambda://<aprender-embed-function>/v1/embeddings)"
        )),
    }
}

// ---------------------------------------------------------------------------------------------
// Hermetic fakes
// ---------------------------------------------------------------------------------------------

/// How the fake server misbehaves (or does not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The contract: cuts at 254 content tokens (256 with [CLS]/[SEP]), validates requests.
    Faithful,
    /// `apr serve`'s Ollama-compat stub: ignores the request, returns 128 zeros.
    ZeroStub,
    /// A real `apr embed`: panics (an unhandled function error) above 512 wordpieces.
    PanicsOnLongInput,
    /// Never truncates.
    NoTruncation,
    /// Truncates at this many content tokens.
    TruncatesAt(usize),
    /// Accepts what the contract says to refuse and echoes the wrong model.
    Lenient,
}

struct FakeServer {
    mode: Mode,
}

/// A stable hash (FNV-1a) so the fake's vectors are deterministic across runs and platforms.
fn fnv(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A normalised hashed bag-of-words vector over the first `limit` whitespace tokens. `the` is
/// down-weighted so the boundary texts' tails dominate once they are kept.
fn fake_vector(text: &str, limit: usize) -> Vec<f32> {
    let mut v = vec![0.0_f64; DIMENSION];
    for token in text.split_whitespace().take(limit) {
        let word: String = token
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .flat_map(char::to_lowercase)
            .collect();
        if word.is_empty() {
            continue;
        }
        let weight = if word == "the" { 0.1 } else { 1.0 };
        let hash = fnv(&word);
        let sign = if (hash >> 40) & 1 == 0 { 1.0 } else { -1.0 };
        v[(hash % DIMENSION as u64) as usize] += sign * weight;
    }
    let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm == 0.0 {
        v[0] = 1.0;
        return v.iter().map(|x| *x as f32).collect();
    }
    v.iter().map(|x| (x / norm) as f32).collect()
}

impl FakeServer {
    fn token_limit(&self) -> usize {
        match self.mode {
            Mode::NoTruncation | Mode::Lenient | Mode::PanicsOnLongInput | Mode::ZeroStub => usize::MAX,
            Mode::Faithful => 254,
            Mode::TruncatesAt(n) => n,
        }
    }

    fn reply(status: u16, body: &Value) -> InvokeOutcome {
        InvokeOutcome {
            status_code: Some(200),
            function_error: None,
            payload: serde_json::to_vec(&json!({
                "statusCode": status,
                "headers": {"content-type": "application/json"},
                "body": body.to_string(),
                "isBase64Encoded": false,
            }))
            .expect("serialize"),
        }
    }

    fn error(status: u16, message: &str, code: &str) -> InvokeOutcome {
        Self::reply(
            status,
            &json!({"error": {"message": message, "type": "invalid_request_error", "code": code}}),
        )
    }

    fn handle(&self, body: &Value) -> InvokeOutcome {
        let inputs: Vec<String> = match body.get("input") {
            Some(Value::String(one)) => vec![one.clone()],
            Some(Value::Array(many)) => many.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            _ => return Self::error(400, "input must be a string or a list of strings", "bad_input"),
        };
        if self.mode == Mode::ZeroStub {
            let data: Vec<Value> = (0..inputs.len())
                .map(|i| json!({"object": "embedding", "index": i, "embedding": vec![0.0_f32; 128]}))
                .collect();
            return Self::reply(200, &json!({"object": "list", "model": MODEL, "data": data}));
        }
        if self.mode != Mode::Lenient {
            if body.get("model").and_then(Value::as_str) != Some(MODEL) {
                return Self::error(400, "unknown model", "model_not_found");
            }
            if body.get("dimensions").is_some_and(|d| d.as_u64() != Some(DIMENSION as u64)) {
                return Self::error(400, "dimensions must be 384", "bad_dimensions");
            }
            if body.get("encoding_format").is_some_and(|f| f.as_str() != Some("float")) {
                return Self::error(400, "encoding_format must be float", "bad_encoding_format");
            }
            if inputs.len() > 128 {
                return Self::error(413, "too many inputs", "too_many_inputs");
            }
            if inputs.iter().any(|t| t.trim().is_empty()) {
                return Self::error(400, "input must not contain an empty string", "empty_input");
            }
        }
        if self.mode == Mode::PanicsOnLongInput && inputs.iter().any(|t| t.split_whitespace().count() > 512) {
            return InvokeOutcome {
                status_code: Some(200),
                function_error: Some("Unhandled".to_string()),
                payload: br#"{"errorMessage":"thread panicked: sequence length exceeds 512"}"#.to_vec(),
            };
        }
        let limit = self.token_limit();
        let data: Vec<Value> = inputs
            .iter()
            .enumerate()
            .map(|(i, text)| json!({"object": "embedding", "index": i, "embedding": fake_vector(text, limit)}))
            .collect();
        let model = if self.mode == Mode::Lenient { "some-other-model" } else { MODEL };
        Self::reply(200, &json!({"object": "list", "model": model, "data": data}))
    }
}

#[async_trait]
impl LambdaInvoker for FakeServer {
    async fn invoke(&self, _function_name: &str, payload: Vec<u8>) -> Result<InvokeOutcome, LlmError> {
        let event: Value = serde_json::from_slice(&payload).expect("the probe sends JSON events");
        assert_eq!(event["version"], "2.0");
        assert_eq!(event["rawPath"], EMBEDDINGS_PATH);
        let raw = event["body"].as_str().expect("the event body is a string");
        let body: Value = serde_json::from_str(raw).expect("the event body is JSON");
        Ok(self.handle(&body))
    }
}

/// The reference file a contract-faithful encoder (cut at 254 content tokens) would deliver.
fn faithful_reference_json() -> Value {
    let texts: Vec<Value> = boundary_texts()
        .into_iter()
        .map(|(id, text)| {
            let vector = fake_vector(&text, 254);
            json!({"id": id, "text": text, "vector": vector})
        })
        .collect();
    json!({
        "model": MODEL,
        "hf_revision": "fake",
        "tokenizer_sha256": "0".repeat(64),
        "max_seq_length": 256,
        "texts": texts,
    })
}

fn faithful_reference() -> Reference {
    parse_reference(&faithful_reference_json().to_string()).expect("the generated reference parses")
}

async fn probe(mode: Mode, reference: Result<Reference, String>) -> Vec<CheckResult> {
    run_probe(Arc::new(FakeServer { mode }), "aprender-embed-fake", reference).await
}

fn failed(results: &[CheckResult]) -> Vec<&'static str> {
    results.iter().filter(|r| !r.ok).map(|r| r.name).collect()
}

fn detail_of<'a>(results: &'a [CheckResult], name: &str) -> &'a str {
    &results.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no check {name}")).detail
}

// ---------------------------------------------------------------------------------------------
// Hermetic self-tests (always run; no feature, no network, no AWS)
// ---------------------------------------------------------------------------------------------

#[test]
fn the_fixtures_follow_the_briefs_definitions() {
    assert_eq!(WORDS_A.split_whitespace().count(), 50);
    assert_eq!(WORDS_B.split_whitespace().count(), 50);
    assert_eq!(boundary("T-UPPER-A").split_whitespace().count(), 260 + 300);
    assert_eq!(boundary("T-UPPER-B").split_whitespace().count(), 260 + 300);
    assert_eq!(boundary("T-LOWER-A").split_whitespace().count(), 200 + 50);
    assert_eq!(boundary("T-LOWER-B").split_whitespace().count(), 200 + 50);
    for (id, n) in [("T-253", 253), ("T-254", 254), ("T-255", 255)] {
        let text = boundary(id);
        assert_eq!(text.split_whitespace().count(), n, "{id}");
        assert!(text.split_whitespace().all(|w| w == "the"), "{id}");
    }
    assert!(boundary("T-UPPER-A").starts_with(&the_times(260)));
    assert!(boundary("T-UPPER-A").ends_with("copper silver"));
    assert!(boundary("T-UPPER-B").ends_with("pipeline reactor market"));
    // Three distinct sentences, three triples, and a long input of at least 2,000 identifiers.
    let s = sentences();
    assert!(s[0] != s[1] && s[1] != s[2] && s[0] != s[2]);
    assert_eq!(triples().len(), 3);
    assert!(long_input().split_whitespace().count() >= 2000);
}

#[tokio::test]
async fn a_contract_faithful_server_passes_all_thirteen_checks() {
    let results = probe(Mode::Faithful, Ok(faithful_reference())).await;
    assert_eq!(print_report(&results), "CONFORMANCE RESULT PASS 13/13");
    let lines = report_lines(&results);
    assert_eq!(results.len(), TOTAL_CHECKS);
    assert_eq!(
        lines.last().map(String::as_str),
        Some("CONFORMANCE RESULT PASS 13/13"),
        "{}",
        lines.join("\n")
    );
    assert!(failed(&results).is_empty(), "{}", lines.join("\n"));
}

#[tokio::test]
async fn the_report_names_the_thirteen_checks_in_the_briefs_order() {
    let results = probe(Mode::Faithful, Ok(faithful_reference())).await;
    let lines = report_lines(&results);
    let names = [
        "shape",
        "l2-normalized",
        "non-degenerate",
        "deterministic",
        "batch-order-invariant",
        "long-input",
        "truncation-upper",
        "truncation-lower",
        "reference-vectors",
        "semantic-sanity",
        "model-echo",
        "empty-string-400",
        "wrong-dimensions-400",
    ];
    for (line, name) in lines.iter().zip(names) {
        assert!(line.starts_with(&format!("CONFORMANCE {name} PASS ")), "{line}");
    }
    assert_eq!(lines.len(), 14);
}

#[tokio::test]
async fn the_zero_vector_stub_fails_the_shape_and_non_degenerate_checks() {
    let results = probe(Mode::ZeroStub, Ok(faithful_reference())).await;
    let failures = failed(&results);
    assert!(failures.contains(&"shape"), "{failures:?}");
    assert!(failures.contains(&"non-degenerate"), "{failures:?}");
    assert!(detail_of(&results, "shape").contains("128 floats"), "{}", detail_of(&results, "shape"));
    let lines = report_lines(&results);
    assert!(lines.last().unwrap().starts_with("CONFORMANCE RESULT FAIL "), "{lines:?}");
}

#[tokio::test]
async fn a_server_that_panics_on_long_input_fails_the_long_input_check() {
    let results = probe(Mode::PanicsOnLongInput, Ok(faithful_reference())).await;
    let failures = failed(&results);
    assert!(failures.contains(&"long-input"), "{failures:?}");
    assert!(detail_of(&results, "long-input").contains("function error"), "{}", detail_of(&results, "long-input"));
    // The checks that need no long input still pass: the failure is attributed, not smeared.
    for name in ["shape", "deterministic", "semantic-sanity", "model-echo"] {
        assert!(!failures.contains(&name), "{name} should pass: {failures:?}");
    }
}

#[tokio::test]
async fn a_server_without_truncation_fails_truncation_upper() {
    let results = probe(Mode::NoTruncation, Ok(faithful_reference())).await;
    let failures = failed(&results);
    assert!(failures.contains(&"truncation-upper"), "{failures:?}");
    assert!(!failures.contains(&"truncation-lower"), "{failures:?}");
    assert!(!failures.contains(&"long-input"), "{failures:?}");
}

#[tokio::test]
async fn a_server_truncating_at_128_tokens_fails_truncation_lower() {
    let results = probe(Mode::TruncatesAt(128), Ok(faithful_reference())).await;
    let failures = failed(&results);
    assert!(failures.contains(&"truncation-lower"), "{failures:?}");
    assert!(!failures.contains(&"truncation-upper"), "{failures:?}");
}

#[tokio::test]
async fn a_server_truncating_at_510_tokens_fails_truncation_upper() {
    let results = probe(Mode::TruncatesAt(510), Ok(faithful_reference())).await;
    let failures = failed(&results);
    assert!(failures.contains(&"truncation-upper"), "{failures:?}");
    assert!(!failures.contains(&"truncation-lower"), "{failures:?}");
}

#[tokio::test]
async fn a_reference_file_whose_vectors_differ_fails_reference_vectors() {
    let mut reference = faithful_reference();
    // Deliver T-LOWER-A's vector under T-UPPER-A's id: a plausible copy-paste defect.
    let lower = reference.entries.iter().find(|e| e.id == "T-LOWER-A").unwrap().vector.clone();
    reference.entries.iter_mut().find(|e| e.id == "T-UPPER-A").unwrap().vector = lower;
    let results = probe(Mode::Faithful, Ok(reference)).await;
    assert_eq!(failed(&results), vec!["reference-vectors"], "{results:?}");
    assert!(detail_of(&results, "reference-vectors").contains("T-UPPER-A"));
}

#[tokio::test]
async fn a_reference_missing_an_entry_or_computed_over_other_text_fails_naming_it() {
    let mut reference = faithful_reference();
    reference.entries.retain(|e| e.id != "T-255");
    let results = probe(Mode::Faithful, Ok(reference)).await;
    assert_eq!(failed(&results), vec!["reference-vectors"]);
    assert!(detail_of(&results, "reference-vectors").contains("T-255"));

    let mut reference = faithful_reference();
    reference.entries.iter_mut().find(|e| e.id == "T-254").unwrap().text = Some("the the".to_string());
    let results = probe(Mode::Faithful, Ok(reference)).await;
    assert_eq!(failed(&results), vec!["reference-vectors"]);
    assert!(detail_of(&results, "reference-vectors").contains("T-254"));
}

#[tokio::test]
async fn a_missing_reference_file_fails_reference_vectors_naming_the_variable() {
    let missing = load_reference(None);
    let results = probe(Mode::Faithful, missing).await;
    assert_eq!(failed(&results), vec!["reference-vectors"]);
    assert!(
        detail_of(&results, "reference-vectors").contains("APRENDER_EMBED_REFERENCE"),
        "{}",
        detail_of(&results, "reference-vectors")
    );

    let dir = tempfile::TempDir::new().unwrap();
    let nowhere = dir.path().join("absent.json");
    let err = load_reference(nowhere.to_str()).err().expect("an unreadable path is an error");
    assert!(err.contains("APRENDER_EMBED_REFERENCE"), "{err}");

    let garbage = dir.path().join("garbage.json");
    std::fs::write(&garbage, "{ not json").unwrap();
    let err = load_reference(garbage.to_str()).err().expect("garbage is an error");
    assert!(err.contains("APRENDER_EMBED_REFERENCE"), "{err}");
}

#[tokio::test]
async fn a_reference_file_on_disk_round_trips_into_a_pass() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("aprender-embed-reference-vectors.json");
    std::fs::write(&path, faithful_reference_json().to_string()).unwrap();
    let results = probe(Mode::Faithful, load_reference(path.to_str())).await;
    assert!(failed(&results).is_empty(), "{results:?}");
}

#[tokio::test]
async fn a_lenient_server_fails_the_model_echo_and_the_two_400_checks() {
    let results = probe(Mode::Lenient, Ok(faithful_reference())).await;
    let failures = failed(&results);
    for name in ["model-echo", "empty-string-400", "wrong-dimensions-400"] {
        assert!(failures.contains(&name), "{name} should fail: {failures:?}");
    }
}

#[test]
fn the_live_gate_refuses_any_aws_endpoint_override() {
    let names = ["HOME", "AWS_REGION", "AWS_ENDPOINT_URL_LAMBDA", "AWS_ENDPOINT_URL", "AWS_ENDPOINT_URL"];
    let offenders = endpoint_override_names(names.iter().map(|n| n.to_string()));
    assert_eq!(offenders, vec!["AWS_ENDPOINT_URL", "AWS_ENDPOINT_URL_LAMBDA"]);
    assert!(endpoint_override_names(["AWS_REGION", "AWS_IGNORE_CONFIGURED_ENDPOINT_URLS"].iter().map(|n| n.to_string()))
        .is_empty());
}

#[test]
fn only_the_lambda_form_is_an_accepted_probe_target() {
    assert_eq!(
        parse_probe_target("lambda://aprender-embed-endpoint-dev/v1/embeddings"),
        Ok(("aprender-embed-endpoint-dev".to_string(), "/v1/embeddings".to_string()))
    );
    let http = parse_probe_target("http://127.0.0.1:18420/v1/embeddings").expect_err("loopback http");
    assert!(http.contains("lambda://") && http.contains("APRENDER_EMBED_BASE_URL"), "{http}");
    assert!(parse_probe_target("lambda://other-fn/v1/embeddings").is_err());
    assert!(parse_probe_target("https://example.com/v1/embeddings").is_err());
}

// ---------------------------------------------------------------------------------------------
// The live run (needs the real Lambda client)
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "aprender-lambda")]
#[tokio::test]
#[ignore = "invokes a deployed aprender-embed function in the operator's account -- run via just aprender-embed-conformance <base_url> <reference>"]
async fn the_deployed_aprender_function_passes_thirteen_of_thirteen() {
    use snapshot_build::providers::lambda_invoke::AwsLambdaInvoker;
    use snapshot_build::providers::RetryOwner;

    let Some(base_url) = std::env::var("APRENDER_EMBED_BASE_URL").ok().filter(|u| !u.trim().is_empty()) else {
        println!("SKIP: APRENDER_EMBED_BASE_URL not set");
        return;
    };
    let offenders = endpoint_override_names(std::env::vars_os().filter_map(|(name, _)| name.into_string().ok()));
    assert!(
        offenders.is_empty(),
        "a live conformance run must not target an endpoint override; remove {offenders:?}"
    );
    let (function_name, _path) = parse_probe_target(&base_url).unwrap_or_else(|e| panic!("{e}"));
    let reference = load_reference(std::env::var("APRENDER_EMBED_REFERENCE").ok().as_deref());

    let invoker = Arc::new(AwsLambdaInvoker::new(RetryOwner::Sdk));
    let results = run_probe(invoker, &function_name, reference).await;
    let verdict = print_report(&results);
    assert_eq!(verdict, "CONFORMANCE RESULT PASS 13/13", "the deployed function is not conformant");
}
