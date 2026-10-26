//! Spike 033: the Phase 12 Kev-4B reference arm over `Qwen35Model::prefill_hidden` + Kev's pointer head.
//!
//! Modes:
//!   parity <gguf> <head.safetensors> <fixture.json> [--no-control]
//!       spike 020's behaviour: Rust prefill vs the torch fp32 oracle. Rows of >= 6000 tokens are reported as the "long"
//!       row; the last line is `PARITY_JSON {...}` for the evidence file.
//!   dump <gguf> <head.safetensors> <rows.json> <out.jsonl> --identity <identity.json> [--repeat N]
//!       for each row {row, split, ids, decide, opts}: prefill, the head's raw logits and softmax(z / T) (T from the head
//!       metadata), one JSON line {row, split, n_tokens, logits_raw, probs, ms, ms_all}. The first line of out.jsonl is
//!       the identity header; a resume is bound to it (a different header is `REFUSED dump-identity`, exit 2), a torn final
//!       line is truncated (`DROPPED torn final line (<n> bytes)`), rows already present are skipped. It prints no text:
//!       rows hold token ids only.
use realizar::gguf::forward_qwen35::Qwen35Model;
use realizar::gguf::MappedGGUFModel;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Instant;

const LONG_ROW_TOKENS: usize = 6000;

#[derive(Deserialize)]
struct Fixture {
    temperature: f32,
    records: Vec<Record>,
}
#[derive(Deserialize)]
struct Record {
    rows: Vec<FixRow>,
}
#[derive(Deserialize)]
struct FixRow {
    ids: Vec<u32>,
    decide: usize,
    opts: Vec<usize>,
    h_decide: Vec<f32>,
    logits_raw: Vec<f32>,
    probs: Vec<f32>,
}

#[derive(Deserialize)]
struct DumpRow {
    row: Value,
    split: String,
    ids: Vec<u32>,
    decide: usize,
    opts: Vec<usize>,
}

struct Head {
    qw: Vec<f32>,
    qb: Vec<f32>,
    kw: Vec<f32>,
    kb: Vec<f32>,
    dp: usize,
    d: usize,
    temperature: Option<f32>,
}
impl Head {
    fn load(path: &str) -> Head {
        let bytes = std::fs::read(path).expect("head");
        let st = safetensors::SafeTensors::deserialize(&bytes).expect("parse head");
        let get = |n: &str| -> (Vec<f32>, Vec<usize>) {
            let t = st.tensor(n).expect(n);
            (t.data().chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(), t.shape().to_vec())
        };
        let (qw, s) = get("q.weight");
        let (qb, _) = get("q.bias");
        let (kw, _) = get("k.weight");
        let (kb, _) = get("k.bias");
        let (_, meta) = safetensors::SafeTensors::read_metadata(&bytes).expect("head metadata");
        let temperature = meta.metadata().as_ref().and_then(|m| m.get("temperature")).and_then(|t| t.parse::<f32>().ok());
        Head { qw, qb, kw, kb, dp: s[0], d: s[1], temperature }
    }
    fn proj(&self, w: &[f32], b: &[f32], x: &[f32]) -> Vec<f32> {
        (0..self.dp).map(|o| b[o] + w[o * self.d..(o + 1) * self.d].iter().zip(x).map(|(a, c)| a * c).sum::<f32>()).collect()
    }
    fn logits(&self, hd: &[f32], ho: &[&[f32]]) -> Vec<f32> {
        let q = self.proj(&self.qw, &self.qb, hd);
        let s = 1.0 / (self.dp as f32).sqrt();
        ho.iter().map(|h| self.proj(&self.kw, &self.kb, h).iter().zip(&q).map(|(a, b)| a * b).sum::<f32>() * s).collect()
    }
}
fn softmax(z: &[f32], t: f32) -> Vec<f32> {
    let m = z.iter().fold(f32::MIN, |a, &b| a.max(b / t));
    let e: Vec<f32> = z.iter().map(|&v| (v / t - m).exp()).collect();
    let s: f32 = e.iter().sum();
    e.iter().map(|v| v / s).collect()
}
fn max_abs(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}
fn argmax(a: &[f32]) -> usize {
    a.iter().enumerate().fold((0, f32::MIN), |m, (i, &v)| if v > m.1 { (i, v) } else { m }).0
}

/// Compact JSON with object keys sorted at every level (independent of serde_json's `preserve_order` feature).
fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> =
                keys.iter().map(|k| format!("{}:{}", serde_json::to_string(k).expect("key"), canonical(&m[*k]))).collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonical).collect::<Vec<_>>().join(",")),
        other => serde_json::to_string(other).expect("scalar"),
    }
}

/// Resume state of an output file: bytes to keep, the dropped torn tail (if any) and the complete lines.
struct Resume {
    keep: usize,
    dropped: Option<usize>,
    lines: Vec<String>,
}
/// A final line without a trailing newline, or one that is not valid JSON, is torn and dropped.
fn scan_resume(bytes: &[u8]) -> Resume {
    let mut keep = bytes.len();
    let mut dropped = None;
    if !bytes.is_empty() && *bytes.last().expect("non-empty") != b'\n' {
        let start = bytes.iter().rposition(|&b| b == b'\n').map(|i| i + 1).unwrap_or(0);
        dropped = Some(bytes.len() - start);
        keep = start;
    } else if !bytes.is_empty() {
        let body = &bytes[..bytes.len() - 1];
        let start = body.iter().rposition(|&b| b == b'\n').map(|i| i + 1).unwrap_or(0);
        if serde_json::from_slice::<Value>(&body[start..]).is_err() {
            dropped = Some(bytes.len() - start);
            keep = start;
        }
    }
    let text = String::from_utf8_lossy(&bytes[..keep]);
    let lines = text.lines().map(|l| l.to_string()).collect();
    Resume { keep, dropped, lines }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn load_model(gguf: &str) -> (MappedGGUFModel, Instant) {
    let t0 = Instant::now();
    (MappedGGUFModel::from_path(gguf).expect("gguf"), t0)
}

fn parity(args: &[String]) {
    let control = !args.iter().any(|a| a == "--no-control");
    let fixture: Fixture = serde_json::from_str(&std::fs::read_to_string(&args[4]).expect("fixture")).expect("parse");
    let (mapped, t0) = load_model(&args[2]);
    let base = Qwen35Model::create_base_model(&mapped.model, mapped.data()).expect("base");
    let qwen = Qwen35Model::from_model_and_layers(&base, &mapped.model, mapped.data()).expect("layers");
    let head = Head::load(&args[3]);
    let d = base.config().hidden_dim;
    println!("load {:.0} ms, rayon threads {}\n", t0.elapsed().as_secs_f64() * 1e3, rayon::current_num_threads());
    println!("| row | tokens | prefill ms | token-at-a-time ms | max|dh| decide vs torch | max|dp| vs torch | argmax |");
    println!("|---|---|---|---|---|---|---|");
    // short rows and the long row are aggregated separately: the 1e-4 bar applies to each
    let (mut s_worst, mut s_agree, mut s_n) = (0f32, 0usize, 0usize);
    let (mut l_worst, mut l_agree, mut l_n, mut l_tokens) = (0f32, 0usize, 0usize, 0usize);
    let mut n = 0;
    for rec in &fixture.records {
        for row in &rec.rows {
            n += 1;
            let mut st = qwen.new_state(row.ids.len() + 1);
            let s = Instant::now();
            let h = qwen.prefill_hidden(&row.ids, &mut st).expect("prefill");
            let pre_ms = s.elapsed().as_secs_f64() * 1e3;
            let step_ms = if control {
                let mut st = qwen.new_state(row.ids.len() + 1);
                let s = Instant::now();
                for (p, &tok) in row.ids.iter().enumerate() {
                    let hs = qwen.forward_single_qwen35_hidden(tok, &mut st, p).expect("step");
                    assert!(max_abs(&hs, &h[p * d..(p + 1) * d]) < 1e-2, "prefill vs step diverged");
                }
                s.elapsed().as_secs_f64() * 1e3
            } else {
                f64::NAN
            };
            let hd = &h[row.decide * d..(row.decide + 1) * d];
            let ho: Vec<&[f32]> = row.opts.iter().map(|&o| &h[o * d..(o + 1) * d]).collect();
            let z = head.logits(hd, &ho);
            let p = softmax(&z, fixture.temperature);
            let dp = max_abs(&p, &row.probs);
            let ok = argmax(&p) == argmax(&row.probs);
            let _ = max_abs(&z, &row.logits_raw);
            if row.ids.len() >= LONG_ROW_TOKENS {
                l_worst = l_worst.max(dp);
                l_agree += ok as usize;
                l_n += 1;
                l_tokens = l_tokens.max(row.ids.len());
            } else {
                s_worst = s_worst.max(dp);
                s_agree += ok as usize;
                s_n += 1;
            }
            println!(
                "| {n} | {} | {pre_ms:.0} | {step_ms:.0} | {:.2e} | {dp:.2e} | {} |",
                row.ids.len(),
                max_abs(hd, &row.h_decide),
                if ok { "same" } else { "FLIP" }
            );
        }
    }
    println!("\nshort rows: worst |dp| vs torch {s_worst:.3e}, argmax {s_agree}/{s_n}");
    println!("long rows: worst |dp| vs torch {l_worst:.3e}, argmax {l_agree}/{l_n}, tokens {l_tokens}");
    println!(
        "PARITY_JSON {}",
        serde_json::json!({
            "short": {"n_rows": s_n, "max_abs_dprob": s_worst, "argmax_agree": s_agree},
            "long": {"n_rows": l_n, "n_tokens": l_tokens, "max_abs_dprob": l_worst, "argmax_agree": l_agree}
        })
    );
}

fn dump(args: &[String]) {
    let identity_path = flag_value(args, "--identity").expect("dump needs --identity FILE");
    let repeat: usize = flag_value(args, "--repeat").map(|r| r.parse().expect("--repeat N")).unwrap_or(1).max(1);
    let identity: Value = serde_json::from_str(&std::fs::read_to_string(&identity_path).expect("identity file")).expect("identity json");
    let header = format!("{{\"identity\":{}}}", canonical(&identity));
    let rows: Vec<DumpRow> = serde_json::from_str(&std::fs::read_to_string(&args[4]).expect("rows")).expect("parse rows");
    let out_path = &args[5];

    // identity-bound resume (T-12-63): inspect the output before any model work
    let mut done: HashSet<(String, String)> = HashSet::new();
    let existing = std::fs::read(out_path).unwrap_or_default();
    let r = scan_resume(&existing);
    if let Some(n) = r.dropped {
        eprintln!("DROPPED torn final line ({n} bytes)");
        let f = OpenOptions::new().write(true).open(out_path).expect("open for truncate");
        f.set_len(r.keep as u64).expect("truncate");
        f.sync_all().expect("sync");
    }
    let fresh = r.lines.is_empty();
    if !fresh {
        if r.lines[0] != header {
            eprintln!("REFUSED dump-identity: the first line of {out_path} is not this run's identity header");
            std::process::exit(2);
        }
        for l in &r.lines[1..] {
            let v: Value = serde_json::from_str(l).expect("complete line parses");
            done.insert((v["row"].to_string(), v["split"].as_str().unwrap_or("").to_string()));
        }
    }
    let mut out = OpenOptions::new().create(true).append(true).open(out_path).expect("open out");
    if fresh {
        writeln!(out, "{header}").expect("header");
        out.sync_all().expect("sync");
    }
    let todo: Vec<&DumpRow> = rows.iter().filter(|r| !done.contains(&(r.row.to_string(), r.split.clone()))).collect();
    eprintln!("dump: {} rows in file, {} already present, {} to score", rows.len(), done.len(), todo.len());
    if todo.is_empty() {
        return;
    }

    let (mapped, t0) = load_model(&args[2]);
    let base = Qwen35Model::create_base_model(&mapped.model, mapped.data()).expect("base");
    let qwen = Qwen35Model::from_model_and_layers(&base, &mapped.model, mapped.data()).expect("layers");
    let head = Head::load(&args[3]);
    let t = head.temperature.expect("head.safetensors metadata has no temperature");
    let d = base.config().hidden_dim;
    eprintln!("load {:.0} ms, rayon threads {}, T {t}", t0.elapsed().as_secs_f64() * 1e3, rayon::current_num_threads());
    for r in todo {
        let mut ms_all = Vec::with_capacity(repeat);
        let mut last: Vec<f32> = Vec::new();
        for _ in 0..repeat {
            let mut st = qwen.new_state(r.ids.len() + 1);
            let s = Instant::now();
            last = qwen.prefill_hidden(&r.ids, &mut st).expect("prefill");
            ms_all.push(s.elapsed().as_secs_f64() * 1e3);
        }
        let hd = &last[r.decide * d..(r.decide + 1) * d];
        let ho: Vec<&[f32]> = r.opts.iter().map(|&o| &last[o * d..(o + 1) * d]).collect();
        let z = head.logits(hd, &ho);
        let p = softmax(&z, t);
        let line = serde_json::json!({
            "row": r.row, "split": r.split, "n_tokens": r.ids.len(), "logits_raw": z, "probs": p,
            "ms": ms_all.last().copied().unwrap_or(0.0), "ms_all": ms_all
        });
        writeln!(out, "{line}").expect("row");
        out.sync_all().expect("sync");
        eprintln!("row {} {} : {} tokens, {:.0} ms", r.row, r.split, r.ids.len(), ms_all.last().copied().unwrap_or(0.0));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("parity") if args.len() >= 5 => parity(&args),
        Some("dump") if args.len() >= 6 => dump(&args),
        _ => {
            eprintln!("usage: parity <gguf> <head> <fixture> [--no-control] | dump <gguf> <head> <rows.json> <out.jsonl> --identity FILE [--repeat N]");
            std::process::exit(64);
        }
    }
}
