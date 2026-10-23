//! `aprender-embed-graviton-probe`: the measurement harness for Phase 11's Graviton
//! gate (plan 11-01, R6). It is NOT the product. It puts one honest number on how
//! long the production engine takes on a real arm64 Lambda, and it is superseded by
//! the product crate once plan 11-06 lands.
//!
//! It calls the production engine the way the endpoint will:
//!   * `SetFitBert::from_pretrained_dir` ONCE, at start (the cold load cost);
//!   * the sha256 of `full_model.apr`, hashed at start the way the endpoint will
//!     hash its model files (plan 11-08), so the cold number includes that cost;
//!   * `encode_texts` in micro-batches inside ONE `no_grad` closure, on the thread
//!     that encodes (the grad flag and the tape are thread-local).
//!
//! Two modes: `--local <event.json>` runs one event on the host and prints the
//! response; with no arguments it is the Lambda custom runtime. A load failure is
//! DATA (an `error` key, exit 0), never a panic and never a process exit. No
//! request text is logged.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use aprender::autograd::{graph_tape_len, no_grad};
use aprender::setfit::SetFitBert;
use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const CORPUS: &str = include_str!("../corpus.txt");
const PROBE: &str = "aprender-embed-graviton-probe";
const DEFAULT_MODEL_DIR: &str = "/var/task/model";
/// Input i starts at word offset (i * WORD_STRIDE) mod the corpus length, so the n
/// inputs of one event differ from each other.
const WORD_STRIDE: usize = 53;
const ROOT_SEED: u64 = 0;
/// Upper bound on a synthetic tokenize sample (4 MiB); the recipe asks for 1 MiB.
const MAX_TOKENIZE_BYTES: usize = 4 * 1024 * 1024;

/// Invocations served by this process; the first one is the cold path of this process.
static INVOCATIONS: AtomicU64 = AtomicU64::new(0);

/// Everything measured once, at start. A load failure is kept as text and reported in
/// every response; the process does not exit on it.
struct Startup {
    model: Result<SetFitBert, String>,
    apr_sha256: Option<String>,
    load_ms: f64,
    identity_ms: f64,
    init_ms: f64,
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn startup(model_dir: &str) -> Startup {
    let t_init = Instant::now();
    let t_load = Instant::now();
    let loaded = SetFitBert::from_pretrained_dir(Path::new(model_dir), ROOT_SEED)
        .map_err(|error| error.to_string());
    let load_ms = ms(t_load);

    let t_identity = Instant::now();
    let apr = Path::new(model_dir).join("full_model.apr");
    let (model, apr_sha256) = match (loaded, sha256_file(&apr)) {
        (Ok(model), Ok(hash)) => (Ok(model), Some(hash)),
        (Ok(_), Err(error)) => (Err(format!("identity hash failed: {error}")), None),
        (Err(error), Ok(hash)) => (Err(format!("model load failed: {error}")), Some(hash)),
        (Err(error), Err(hash)) => (
            Err(format!("model load failed: {error}; identity hash failed: {hash}")),
            None,
        ),
    };
    let identity_ms = ms(t_identity);

    Startup {
        model,
        apr_sha256,
        load_ms,
        identity_ms,
        init_ms: ms(t_init),
    }
}

/// The value of `key` in the first core's block of a /proc/cpuinfo text.
fn cpuinfo_field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == key).then(|| value.trim().to_string())
    })
}

/// Graviton reports `CPU part` such as 0xd0c (Neoverse N1), 0xd40 (V1) or 0xd4f (V2).
#[must_use]
pub fn cpu_part(cpuinfo: &str) -> Option<String> {
    cpuinfo_field(cpuinfo, "CPU part")
}

/// Arm's implementer code is 0x41.
#[must_use]
pub fn cpu_implementer(cpuinfo: &str) -> Option<String> {
    cpuinfo_field(cpuinfo, "CPU implementer")
}

/// Peak resident set size in kB, from the `VmHWM` line of /proc/self/status.
#[must_use]
pub fn vmhwm_kb(status: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let rest = line.strip_prefix("VmHWM:")?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

/// Host facts for the response. The /proc files are absent on macOS, so the host
/// run reports nulls there; the Lambda run reports the real values.
fn host_facts() -> Value {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    json!({
        "cpu_part": cpu_part(&cpuinfo),
        "cpu_implementer": cpu_implementer(&cpuinfo),
        "vmhwm_kb": vmhwm_kb(&status),
        "available_parallelism": std::thread::available_parallelism().map(|n| n.get()).ok(),
        "rayon_threads_env": std::env::var("RAYON_NUM_THREADS").ok(),
    })
}

fn bounded(event: &Value, key: &str, default: u64, lo: u64, hi: u64) -> u64 {
    event
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or(default)
        .clamp(lo, hi)
}

fn embedding_inputs(words: &[&str], n: usize, span: usize) -> Vec<String> {
    if words.is_empty() {
        return Vec::new();
    }
    (0..n)
        .map(|i| {
            let start = (i * WORD_STRIDE) % words.len();
            (0..span)
                .map(|k| words[(start + k) % words.len()])
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Fragments that exercise different tokenizer work than ordinary prose: CJK, accented
/// Latin, emoji and ZWJ sequences, symbol runs, control characters, zero-width
/// characters, and unusual spaces.
const ADVERSARIAL: [&str; 10] = [
    "中文字符测试 ",
    "café naïve résumé ",
    "😀🚀🔥 ",
    "!?;:(){}[]<>|&*^%$#@~` ",
    "\u{200B}\u{200D}\u{FEFF} ",
    "\u{0001}\u{0007}\t\n",
    "ﬁ ﬂ ۱۲۳ Ωμέγα ",
    "👨‍👩‍👧 \u{1F1EF}\u{1F1F5} ",
    "ꙮ ᚠᚢᚦ ",
    "\u{00A0}\u{2028}\u{3000} ",
];

/// One synthetic text of about `bytes` bytes, cut at a char boundary.
fn synthetic_text(bytes: usize, kind: &str) -> String {
    let mut text = String::with_capacity(bytes + 16);
    if kind == "adversarial" {
        let mut i = 0usize;
        while text.len() < bytes {
            text.push_str(ADVERSARIAL[i % ADVERSARIAL.len()]);
            i += 1;
        }
    } else {
        for word in CORPUS.split_whitespace().cycle() {
            if text.len() >= bytes {
                break;
            }
            text.push_str(word);
            text.push(' ');
        }
    }
    let mut cut = bytes.min(text.len());
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
    text
}

struct Encoded {
    rows: usize,
    dim: usize,
    first_row: Vec<f32>,
    pool_threads: usize,
}

/// Runs on the encoding thread, inside `no_grad`. The rayon width is read here, on the
/// thread that does the matrix work, so it is evidence of the pool that ran.
fn encode_chunks(model: &SetFitBert, texts: &[&str], micro_batch: usize) -> Result<Encoded, String> {
    let pool_threads = rayon::current_num_threads();
    let mut rows = 0usize;
    let mut dim = 0usize;
    let mut first_row = Vec::new();
    for chunk in texts.chunks(micro_batch) {
        let tensor = model.encode_texts(chunk).map_err(|error| error.to_string())?;
        let shape = tensor.shape();
        if shape.len() != 2 {
            return Err(format!("encoder returned rank {} (expected 2)", shape.len()));
        }
        let (batch_rows, width) = (shape[0], shape[1]);
        if first_row.is_empty() && batch_rows > 0 {
            first_row = tensor.data().get(..width).map(<[f32]>::to_vec).unwrap_or_default();
        }
        rows += batch_rows;
        dim = width;
    }
    Ok(Encoded {
        rows,
        dim,
        first_row,
        pool_threads,
    })
}

fn l2_norm(values: &[f32]) -> f64 {
    values
        .iter()
        .map(|v| f64::from(*v) * f64::from(*v))
        .sum::<f64>()
        .sqrt()
}

/// One event, one response. Every path returns a JSON object: a failure is an `error`
/// key next to the same host facts, never a panic.
fn run_event(st: &Startup, event: &Value) -> Value {
    let t_total = Instant::now();
    let invocation = INVOCATIONS.fetch_add(1, Ordering::SeqCst);
    let n = bounded(event, "n", 64, 1, 256) as usize;
    let micro_batch = bounded(event, "micro_batch", 8, 1, 64) as usize;
    let words_per_input = bounded(event, "words", 700, 1, 20_000) as usize;
    let tokenize_bytes = bounded(event, "tokenize_bytes", 0, 0, MAX_TOKENIZE_BYTES as u64) as usize;
    let tokenize_kind = event
        .get("tokenize_kind")
        .and_then(Value::as_str)
        .unwrap_or("prose")
        .to_string();

    let mut out = host_facts();
    out["probe"] = json!(PROBE);
    out["n"] = json!(n);
    out["micro_batch"] = json!(micro_batch);
    out["words"] = json!(words_per_input);
    out["first_invocation_in_process"] = json!(invocation == 0);
    out["load_ms"] = json!(st.load_ms);
    out["identity_ms"] = json!(st.identity_ms);
    out["init_ms"] = json!(st.init_ms);
    out["apr_sha256"] = json!(st.apr_sha256);

    let model = match &st.model {
        Ok(model) => model,
        Err(error) => {
            out["error"] = json!(error);
            out["total_ms"] = json!(ms(t_total));
            return out;
        }
    };

    if tokenize_bytes > 0 {
        if tokenize_kind != "prose" && tokenize_kind != "adversarial" {
            out["error"] = json!("tokenize_kind must be prose or adversarial");
            out["total_ms"] = json!(ms(t_total));
            return out;
        }
        let text = synthetic_text(tokenize_bytes, &tokenize_kind);
        let t_tokenize = Instant::now();
        let outcome = model.tokenize(&[text.as_str()]);
        let tokenize_bytes_ms = ms(t_tokenize);
        match outcome {
            Ok(batch) => {
                out["tokenize_bytes"] = json!(text.len());
                out["tokenize_bytes_ms"] = json!(tokenize_bytes_ms);
                out["tokenize_kind"] = json!(tokenize_kind);
                out["tokenize_truncated"] = json!(batch.truncation().first().map(|f| f.truncated));
            }
            Err(error) => out["error"] = json!(error.to_string()),
        }
        out["total_ms"] = json!(ms(t_total));
        return out;
    }

    let words = CORPUS.split_whitespace().collect::<Vec<_>>();
    let inputs = embedding_inputs(&words, n, words_per_input);
    let refs = inputs.iter().map(String::as_str).collect::<Vec<_>>();

    // A harness-only extra pass: the truncation and attention-mask facts come from a
    // tokenization of its own. encode_ms below contains exactly one tokenization.
    let t_tokenize = Instant::now();
    let facts = match model.tokenize(&refs) {
        Ok(batch) => {
            let truncated = batch.truncation().iter().filter(|f| f.truncated).count();
            let consumed: usize = batch.attention_mask().iter().map(|&m| usize::from(m)).sum();
            Ok((truncated, consumed))
        }
        Err(error) => Err(error.to_string()),
    };
    out["tokenize_ms"] = json!(ms(t_tokenize));
    let (truncated_count, consumed_positions) = match facts {
        Ok(facts) => facts,
        Err(error) => {
            out["error"] = json!(error);
            out["total_ms"] = json!(ms(t_total));
            return out;
        }
    };

    let t_encode = Instant::now();
    let encoded = no_grad(|| encode_chunks(model, &refs, micro_batch));
    let encode_ms = ms(t_encode);
    let tape_len_after = graph_tape_len();

    out["no_grad"] = json!(true);
    out["truncated_count"] = json!(truncated_count);
    out["consumed_positions"] = json!(consumed_positions);
    out["encode_ms"] = json!(encode_ms);
    out["tape_len_after"] = json!(tape_len_after);
    match encoded {
        Ok(encoded) => {
            out["rows"] = json!(encoded.rows);
            out["dim"] = json!(encoded.dim);
            out["l2_norm_first_row"] = json!(l2_norm(&encoded.first_row));
            out["rayon_pool_threads"] = json!(encoded.pool_threads);
        }
        Err(error) => out["error"] = json!(error),
    }
    out["total_ms"] = json!(ms(t_total));
    out
}

async fn handle(st: Arc<Startup>, payload: Value) -> Result<Value, Error> {
    // The grad flag and the tape are thread-local, so the whole event runs on one
    // blocking thread, the one that encodes.
    let joined = tokio::task::spawn_blocking(move || run_event(&st, &payload)).await;
    Ok(match joined {
        Ok(response) => response,
        Err(join) => json!({"probe": PROBE, "error": format!("handler task failed: {join}")}),
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    let model_dir =
        std::env::var("APRENDER_EMBED_MODEL_DIR").unwrap_or_else(|_| DEFAULT_MODEL_DIR.to_string());

    if args.get(1).map(String::as_str) == Some("--local") {
        let path = args.get(2).ok_or("usage: --local <event.json>")?;
        let event: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        // Load on this thread, then run the event on a blocking thread, exactly as the
        // Lambda path does. The tape is thread-local: a load that recorded ops on this
        // thread would otherwise be counted as encode-time tape growth.
        let st = Arc::new(startup(&model_dir));
        let response = tokio::task::spawn_blocking(move || run_event(&st, &event)).await?;
        println!("{response}");
        return Ok(());
    }

    let st = Arc::new(startup(&model_dir));
    run(service_fn(move |event: LambdaEvent<Value>| {
        let st = Arc::clone(&st);
        async move { handle(st, event.payload).await }
    }))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_part_parses_a_graviton_cpuinfo_fixture() {
        let fixture = "processor\t: 0\n\
BogoMIPS\t: 243.75\n\
Features\t: fp asimd evtstrm aes pmull sha1 sha2 crc32 atomics fphp asimdhp cpuid\n\
CPU implementer\t: 0x41\n\
CPU architecture: 8\n\
CPU variant\t: 0x1\n\
CPU part\t: 0xd40\n\
CPU revision\t: 1\n\
\n\
processor\t: 1\n\
CPU implementer\t: 0x41\n\
CPU part\t: 0xd40\n";
        assert_eq!(cpu_part(fixture).as_deref(), Some("0xd40"));
        assert_eq!(cpu_implementer(fixture).as_deref(), Some("0x41"));
        assert_eq!(cpu_part("model name\t: not an arm core\n"), None);
    }

    #[test]
    fn vmhwm_parses_a_proc_status_fixture() {
        let fixture = "Name:\tprobe\nVmPeak:\t 4000000 kB\nVmHWM:\t    3584512 kB\nVmRSS:\t 3500000 kB\n";
        assert_eq!(vmhwm_kb(fixture), Some(3_584_512));
        assert_eq!(vmhwm_kb("Name:\tprobe\n"), None);
    }

    #[test]
    fn synthetic_text_is_cut_at_a_char_boundary() {
        let text = synthetic_text(1001, "adversarial");
        assert!(text.len() <= 1001);
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
    }
}
