//! Test plumbing: the tiny Laya fixture (plan 08-02) in an in-memory `.apr`, its
//! oracle, and contract values read from YAML at test time (never Rust literals).

use crate::laya::Laya;
use crate::Task;
use aprender::format::v2::{AprV2Metadata, AprV2ReaderRef, AprV2Writer, TensorDType};
use base64::Engine as _;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// `(name, dtype, shape, raw little-endian bytes)` as stored in the checkpoint.
pub(crate) type RawTensor = (String, TensorDType, Vec<usize>, Vec<u8>);

pub(crate) fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/laya_tiny")
}

pub(crate) fn read(rel: &str) -> Vec<u8> {
    std::fs::read(fixture_dir().join(rel)).unwrap_or_else(|e| panic!("read laya_tiny/{rel}: {e}"))
}

/// Every checkpoint tensor with its raw bytes: F16 stays F16, the F32 `temperature`
/// buffer stays F32.
pub(crate) fn checkpoint_tensors() -> Vec<RawTensor> {
    let bytes = read("checkpoint/model.safetensors");
    let st = safetensors::SafeTensors::deserialize(&bytes).expect("parse safetensors");
    let mut out: Vec<RawTensor> = st
        .tensors()
        .into_iter()
        .map(|(name, view)| {
            let dtype = match view.dtype() {
                safetensors::Dtype::F16 => TensorDType::F16,
                safetensors::Dtype::F32 => TensorDType::F32,
                other => panic!("{name}: unexpected fixture dtype {other:?}"),
            };
            (name, dtype, view.shape().to_vec(), view.data().to_vec())
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Write `tensors` into an in-memory `.apr` with `AprV2Writer::add_tensor`.
pub(crate) fn write_apr(tensors: &[RawTensor]) -> Vec<u8> {
    let mut w = AprV2Writer::new(AprV2Metadata::default());
    for (name, dtype, shape, raw) in tensors {
        w.add_tensor(name.clone(), *dtype, shape.clone(), raw.clone());
    }
    w.write().expect("write in-memory .apr")
}

/// The fixture checkpoint as an in-memory `.apr`.
pub(crate) fn fixture_apr() -> Vec<u8> {
    write_apr(&checkpoint_tensors())
}

/// Laya over `apr` for `task`, with the fixture's configs and tokenizer.
pub(crate) fn load_laya(apr: &[u8], task: Task) -> Result<Laya, crate::LayaError> {
    let reader = AprV2ReaderRef::from_bytes(apr).expect("open in-memory .apr");
    Laya::from_parts(
        &reader,
        "",
        &read("checkpoint/encoder/config.json"),
        &read("checkpoint/rl_agent_config.json"),
        &read("checkpoint/tokenizer/tokenizer.json"),
        task,
    )
}

/// The fixture's own `data/task.json`.
pub(crate) fn fixture_task() -> Task {
    Task::from_slice(&read("data/task.json")).expect("fixture task parses")
}

pub(crate) fn oracle() -> serde_json::Value {
    serde_json::from_slice(&read("oracle.json")).expect("parse oracle.json")
}

/// A decimal list of exact f32 values (carried as their f64 repr).
pub(crate) fn f32_list(v: &serde_json::Value) -> Vec<f32> {
    v.as_array()
        .expect("f32 list")
        .iter()
        .map(|x| x.as_f64().expect("f32 value") as f32)
        .collect()
}

pub(crate) fn u32_list(v: &serde_json::Value) -> Vec<u32> {
    v.as_array()
        .expect("id list")
        .iter()
        .map(|x| u32::try_from(x.as_u64().expect("id")).expect("id fits u32"))
        .collect()
}

pub(crate) fn usize_list(v: &serde_json::Value) -> Vec<usize> {
    v.as_array()
        .expect("position list")
        .iter()
        .map(|x| usize::try_from(x.as_u64().expect("position")).expect("fits usize"))
        .collect()
}

pub(crate) fn string_list(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .expect("string list")
        .iter()
        .map(|x| x.as_str().expect("string").to_string())
        .collect()
}

/// An `f32le_base64` block (standard padded base64 of little-endian f32 bytes).
pub(crate) fn f32_b64(v: &serde_json::Value) -> Vec<f32> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(v.as_str().expect("base64 block"))
        .expect("decode base64 block");
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn contract_yaml(name: &str) -> serde_yaml::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../contracts/{name}"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));
    serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("parse {name}: {e}"))
}

/// `equations.<equation>.float_tolerance` from `contracts/laya-parity-v1.yaml`.
pub(crate) fn tolerance(equation: &str) -> f64 {
    contract_yaml("laya-parity-v1.yaml")["equations"][equation]["float_tolerance"]
        .as_f64()
        .unwrap_or_else(|| panic!("laya-parity-v1 equations.{equation}.float_tolerance"))
}

/// NaN-visible `delta <= bound`: a NaN on either side never passes.
pub(crate) fn within(delta: f64, bound: f64) -> bool {
    matches!(
        delta.partial_cmp(&bound),
        Some(Ordering::Less | Ordering::Equal)
    )
}

/// `max |a - b|` in f64; NaN-propagating, and NaN on a length mismatch.
pub(crate) fn max_abs(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return f64::NAN;
    }
    a.iter().zip(b).fold(0.0f64, |m, (&x, &y)| {
        let d = (f64::from(x) - f64::from(y)).abs();
        if d.is_nan() || m.is_nan() {
            f64::NAN
        } else {
            m.max(d)
        }
    })
}
