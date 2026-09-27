//! `pack_laya` — the back-office pack / verify CLI for Laya decision models (plan 08-09).
//!
//! ```text
//! pack_laya pack --run DIR --data DIR --base DIR --out FILE
//! ```
//!
//! `pack` verifies the run BEFORE anything is written (`aprender_decide::verify::pack_for_serving`):
//! production variant, the contract's base (declared and on disk), input hashes, the split,
//! both probability files, a Rust re-score of every eval row from the packed bytes and from
//! the base, and the gate recomputed from those verified probabilities. Only an accepted run
//! is written, atomically; it prints
//! `PACKED <path> sha256=<H> rescore_max_abs=<x> zs_rescore_max_abs=<y> argmax=<n>/<n>`.
//! A refusal prints ONE `REFUSED <Variant> <detail> (nothing written)` line.
//!
//! Exit codes (the scripts/laya_train convention): 0 accepted, 3 the recomputed gate failed,
//! 2 every other refusal (including a usage error).
//!
//! THE POLICY IS NOT AN ARGUMENT. Thresholds, `ece_bins`, `gate_metric_recompute_abs`,
//! `calibration_slice_min_per_class` and the base sha256 are read from
//! `contracts/laya-finetune-gate-v1.yaml`, and `pack_rescore_probs_abs` from
//! `contracts/laya-parity-v1.yaml`, at run time. The CLI accepts only the path arguments above
//! and reads no environment variable: there is no way to hand it another policy.

use aprender_decide::verify::{self, VerifyError, VerifyPolicy};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: pack_laya pack --run DIR --data DIR --base DIR --out FILE";

/// A contract file from the workspace root (resolved from this crate's manifest dir at
/// compile time — never from the environment).
fn contract(name: &str) -> Result<serde_yaml::Value, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts")
        .join(name);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_yaml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn at<'a>(v: &'a serde_yaml::Value, keys: &[&str]) -> &'a serde_yaml::Value {
    keys.iter().fold(v, |v, k| &v[*k])
}

fn f64_at(v: &serde_yaml::Value, keys: &[&str]) -> Result<f64, String> {
    at(v, keys).as_f64().ok_or_else(|| {
        format!(
            "contract value {} is missing or not a number",
            keys.join(".")
        )
    })
}

fn u64_at(v: &serde_yaml::Value, keys: &[&str]) -> Result<u64, String> {
    at(v, keys).as_u64().ok_or_else(|| {
        format!(
            "contract value {} is missing or not an integer",
            keys.join(".")
        )
    })
}

/// The verify policy, read from the contracts (never literals).
fn policy() -> Result<VerifyPolicy, String> {
    let gate = contract("laya-finetune-gate-v1.yaml")?;
    let parity = contract("laya-parity-v1.yaml")?;
    let c = |k: &'static str| ["constants", k];
    Ok(VerifyPolicy {
        min_macro_f1_margin: f64_at(&gate, &c("gate_min_macro_f1_margin"))?,
        max_ece: f64_at(&gate, &c("gate_max_ece"))?,
        ece_bins: u64_at(&gate, &c("ece_bins"))?,
        metric_recompute_abs: f64_at(&gate, &c("gate_metric_recompute_abs"))?,
        rescore_probs_abs: f64_at(
            &parity,
            &["equations", "pack_rescore_probs_abs", "float_tolerance"],
        )?,
        calibration_slice_min_per_class: u64_at(&gate, &c("calibration_slice_min_per_class"))?,
        base_sha256: at(&gate, &["base", "model_safetensors_sha256"])
            .as_str()
            .ok_or("contract value base.model_safetensors_sha256 is missing")?
            .to_string(),
    })
}

/// The ONLY arguments any subcommand takes.
#[derive(Default)]
struct Args {
    operand: Option<PathBuf>,
    run: Option<PathBuf>,
    data: Option<PathBuf>,
    base: Option<PathBuf>,
    out: Option<PathBuf>,
}

impl Args {
    fn parse(argv: &[String]) -> Result<Self, String> {
        let mut a = Self::default();
        let mut it = argv.iter();
        while let Some(arg) = it.next() {
            let slot = match arg.as_str() {
                "--run" => &mut a.run,
                "--data" => &mut a.data,
                "--base" => &mut a.base,
                "--out" => &mut a.out,
                s if s.starts_with('-') => return Err(format!("unknown argument {s}")),
                _ => &mut a.operand,
            };
            if slot.is_some() {
                return Err(format!("{arg} given twice"));
            }
            let value = if arg.starts_with("--") {
                it.next().ok_or_else(|| format!("{arg} needs a value"))?
            } else {
                arg
            };
            *slot = Some(PathBuf::from(value));
        }
        Ok(a)
    }

    fn need(v: Option<&PathBuf>, flag: &str) -> Result<PathBuf, String> {
        v.cloned().ok_or_else(|| format!("{flag} is required"))
    }
}

fn refused(e: &VerifyError) -> ExitCode {
    println!("REFUSED {} {e} (nothing written)", e.variant_name());
    ExitCode::from(u8::try_from(e.exit_code()).unwrap_or(2))
}

fn cmd_pack(a: &Args) -> Result<ExitCode, String> {
    if a.operand.is_some() {
        return Err("pack takes no operand".into());
    }
    let run = Args::need(a.run.as_ref(), "--run")?;
    let data = Args::need(a.data.as_ref(), "--data")?;
    let base = Args::need(a.base.as_ref(), "--base")?;
    let out = Args::need(a.out.as_ref(), "--out")?;
    let policy = policy()?;
    Ok(
        match verify::pack_for_serving(&run, &data, &base, &out, &policy) {
            Ok(r) => {
                println!(
                    "PACKED {} sha256={} rescore_max_abs={} zs_rescore_max_abs={} argmax={}/{}",
                    out.display(),
                    r.artifact_sha256,
                    r.rescore_max_abs,
                    r.zs_rescore_max_abs,
                    r.argmax_agree,
                    r.n
                );
                ExitCode::SUCCESS
            }
            Err(e) => refused(&e),
        },
    )
}

fn run(argv: &[String]) -> Result<ExitCode, String> {
    let (cmd, rest) = argv.split_first().ok_or(USAGE)?;
    let args = Args::parse(rest)?;
    match cmd.as_str() {
        "pack" => cmd_pack(&args),
        other => Err(format!("unknown subcommand {other}; {USAGE}")),
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    run(&argv).unwrap_or_else(|msg| {
        println!("REFUSED Usage {msg} (nothing written)");
        ExitCode::from(2)
    })
}
