//! Laya: a ModernBERT encoder with a typed decision head — the first (and today only)
//! [`DecisionMethod`](crate::DecisionMethod) (D-14).
//!
//! ```text
//! ids --ModernBertEncoder (core, prefix "encoder.")--> [l, d]
//!     + type_emb[qtype]
//!     --head_layers x HeadLayer (pre-norm TransformerEncoderLayer, nhead = max(1, d/64))-->
//!     gather at the [MASK] markers --Scorer (LayerNorm -> Linear -> GELU -> Linear)--> K logits
//!     --softmax(z / T), T = calibrated bucket temperature--> K probabilities
//! ```
//!
//! qtype indices are Laya's: `choice 0, score 1, noul 2`. A decision artifact serves
//! `choice` only (the task type); the other two exist so the parity ladder can
//! reproduce every row Laya's oracle recorded.
//!
//! The encoder is core's `aprender::models::modernbert` (D-13); head and scorer reuse
//! its `Linear`, `layer_norm`, `gelu_exact` and `attention` (OPS-03). Every head,
//! scorer and `type_emb` tensor goes through the same F16 widening
//! (`AprV2DequantExt::get_tensor_as_f32`) and is refused BY NAME when missing, of the
//! wrong shape, undecodable or non-finite.
//!
//! **Marker rule** (`contracts/decide-apr-v1.yaml` `marker_rule`): options longer than
//! `head_max_len` are SHRUNK exactly as Laya does and served. A task is refused only
//! when its built row keeps fewer markers than it has criteria
//! ([`LayaError::MarkersLost`]). Markers precede the state, so [`Laya::from_parts`]
//! checks this ONCE with an empty state: a task that fails it can never be loaded, and
//! it is never a per-request surprise.

pub mod builder;
pub mod head;
pub mod scorer;
pub mod temperature;

use crate::{DecideError, Decision, DecisionMethod, PreparedRow, Task};
use aprender::format::v2::AprV2ReaderRef;
use aprender::format::AprV2DequantExt;
use aprender::models::modernbert::{
    Linear, ModernBertConfig, ModernBertConfigError, ModernBertEncoder, ModernBertLoadError,
};
use builder::Builder;
use head::{head_geometry, HeadLayer};
use rayon::prelude::*;
use scorer::Scorer;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;

/// Laya's question type (index = Laya's qtype id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QType {
    /// Pick one of K named criteria (index 0).
    Choice,
    /// An ordinal level (index 1).
    Score,
    /// A yes/no statement (index 2).
    Noul,
}

impl QType {
    /// Laya's qtype index (the `type_emb` row).
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Choice => 0,
            Self::Score => 1,
            Self::Noul => 2,
        }
    }

    /// Laya's qtype name (`QTYPE_NAMES`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Choice => "choice",
            Self::Score => "score",
            Self::Noul => "noul",
        }
    }

    /// The qtype for Laya's index, if any.
    #[must_use]
    pub fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::Choice),
            1 => Some(Self::Score),
            2 => Some(Self::Noul),
            _ => None,
        }
    }
}

fn default_max_len() -> usize {
    512
}

fn default_head_max_len() -> usize {
    192
}

fn default_temperature() -> Vec<f64> {
    vec![1.0, 1.0, 1.0]
}

/// The fields of Laya's `rl_agent_config.json` inference uses. Laya's other fields
/// (`encoder`, `act_costs`, `amp_dtype`, ...) are ignored. Defaults are Laya's own
/// (`agent.py`: `max_len` 512, `head_max_len` 192, temperatures 1.0); `head_layers`
/// is required, as Laya requires it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AgentConfig {
    /// Number of decision-head layers.
    pub head_layers: usize,
    /// Row cap in tokens.
    #[serde(default = "default_max_len")]
    pub max_len: usize,
    /// Head + options budget in tokens.
    #[serde(default = "default_head_max_len")]
    pub head_max_len: usize,
    /// Per-qtype fallback temperature (`[choice, score, noul]`).
    #[serde(default = "default_temperature")]
    pub temperature: Vec<f64>,
    /// Calibrated temperature per bucket key (`"choice:3-5"`, ...).
    #[serde(default)]
    pub temperature_by_options: BTreeMap<String, f64>,
}

impl AgentConfig {
    /// Parse `rl_agent_config.json`.
    ///
    /// # Errors
    ///
    /// [`LayaError::AgentConfig`] for malformed JSON, a missing `head_layers` or a
    /// non-positive `max_len`.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, LayaError> {
        let c: Self =
            serde_json::from_slice(bytes).map_err(|e| LayaError::AgentConfig(e.to_string()))?;
        if c.max_len == 0 {
            return Err(LayaError::AgentConfig("max_len must be positive".into()));
        }
        Ok(c)
    }
}

/// Why the Laya method refused.
#[derive(Debug, Clone, PartialEq)]
pub enum LayaError {
    /// The encoder `config.json` is outside core's supported domain.
    EncoderConfig(ModernBertConfigError),
    /// The encoder weights were refused by core's loader.
    EncoderLoad(ModernBertLoadError),
    /// `rl_agent_config.json` is malformed.
    AgentConfig(String),
    /// A head / scorer / `type_emb` tensor is absent.
    MissingTensor {
        /// Full tensor name.
        name: String,
    },
    /// A head / scorer / `type_emb` tensor's shape disagrees with `d`.
    ShapeMismatch {
        /// Full tensor name.
        name: String,
        /// Shape `d` implies.
        expected: Vec<usize>,
        /// Shape stored.
        observed: Vec<usize>,
    },
    /// A tensor's dtype cannot be widened to f32, or its data is truncated.
    Undecodable {
        /// Full tensor name.
        name: String,
    },
    /// A tensor holds a NaN or an infinity.
    NonFinite {
        /// Full tensor name.
        name: String,
    },
    /// Laya's `nhead = max(1, d / 64)` does not divide the hidden size.
    HeadDoesNotDivide {
        /// Hidden size.
        d: usize,
        /// The head count the rule gives.
        nhead: usize,
    },
    /// The tokenizer bytes or a text were refused by `tokenizers`.
    Tokenizer(String),
    /// `[CLS]`, `[SEP]` or `[MASK]` is not in the tokenizer's vocabulary.
    MissingSpecialToken(String),
    /// The task's built row keeps fewer `[MASK]` markers than the task has criteria
    /// (checked once, at load).
    MarkersLost {
        /// Criteria in the task.
        criteria: usize,
        /// Markers that survived the builder.
        markers: usize,
    },
    /// A row's marker points outside the row.
    MarkerOutOfRange {
        /// The marker position.
        marker: usize,
        /// Row length.
        tokens: usize,
    },
    /// A row's marker count differs from the task's criteria count.
    RowMarkerCount {
        /// Expected (task criteria).
        expected: usize,
        /// Observed in the row.
        observed: usize,
    },
}

impl fmt::Display for LayaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncoderConfig(e) => write!(f, "laya: encoder config: {e}"),
            Self::EncoderLoad(e) => write!(f, "laya: {e}"),
            Self::AgentConfig(e) => write!(f, "laya: rl_agent_config.json: {e}"),
            Self::MissingTensor { name } => write!(f, "laya: missing tensor {name}"),
            Self::ShapeMismatch {
                name,
                expected,
                observed,
            } => write!(
                f,
                "laya: tensor {name} has shape {observed:?}, expected {expected:?}"
            ),
            Self::Undecodable { name } => {
                write!(f, "laya: tensor {name} cannot be widened to f32")
            }
            Self::NonFinite { name } => write!(f, "laya: tensor {name} holds a non-finite value"),
            Self::HeadDoesNotDivide { d, nhead } => write!(
                f,
                "laya: head count {nhead} (max(1, d / 64)) does not divide hidden size {d}"
            ),
            Self::Tokenizer(e) => write!(f, "laya: tokenizer: {e}"),
            Self::MissingSpecialToken(t) => {
                write!(f, "laya: tokenizer has no {t} token")
            }
            Self::MarkersLost { criteria, markers } => write!(
                f,
                "laya: the task's row keeps {markers} of {criteria} option markers; \
                 its options do not fit max_len"
            ),
            Self::MarkerOutOfRange { marker, tokens } => {
                write!(f, "laya: marker {marker} is outside a {tokens}-token row")
            }
            Self::RowMarkerCount { expected, observed } => write!(
                f,
                "laya: row has {observed} markers, the task has {expected} criteria"
            ),
        }
    }
}

impl std::error::Error for LayaError {}

/// Load, shape-check, widen and finiteness-check one non-encoder tensor.
fn load_tensor(
    reader: &AprV2ReaderRef<'_>,
    name: &str,
    shape: &[usize],
) -> Result<Vec<f32>, LayaError> {
    let entry = reader
        .get_tensor(name)
        .ok_or_else(|| LayaError::MissingTensor {
            name: name.to_string(),
        })?;
    if entry.shape != shape {
        return Err(LayaError::ShapeMismatch {
            name: name.to_string(),
            expected: shape.to_vec(),
            observed: entry.shape.clone(),
        });
    }
    let undecodable = || LayaError::Undecodable {
        name: name.to_string(),
    };
    let data = reader.get_tensor_as_f32(name).ok_or_else(undecodable)?;
    if Some(data.len()) != shape.iter().try_fold(1usize, |a, &b| a.checked_mul(b)) {
        return Err(undecodable());
    }
    if !data.iter().all(|v| v.is_finite()) {
        return Err(LayaError::NonFinite {
            name: name.to_string(),
        });
    }
    Ok(data)
}

fn load_linear(
    reader: &AprV2ReaderRef<'_>,
    name: &str,
    out: usize,
    inp: usize,
) -> Result<Linear, LayaError> {
    Ok(Linear {
        w: load_tensor(reader, &format!("{name}.weight"), &[out, inp])?,
        b: Some(load_tensor(reader, &format!("{name}.bias"), &[out])?),
        out,
        inp,
    })
}

/// A loaded Laya model bound to one task.
#[derive(Debug)]
pub struct Laya {
    encoder: ModernBertEncoder,
    type_emb: Vec<f32>,
    head: Vec<HeadLayer>,
    scorer: Scorer,
    builder: Builder,
    agent: AgentConfig,
    task: Task,
    options: Vec<String>,
    temperature: f32,
}

impl Laya {
    /// Build Laya from an APR v2 reader (every tensor under `prefix`, the encoder under
    /// `{prefix}encoder.`), the encoder `config.json`, `rl_agent_config.json`, the
    /// byte-identical `tokenizer.json`, and the task it will answer.
    ///
    /// # Errors
    ///
    /// A [`LayaError`] naming the refused config, tensor or tokenizer, or
    /// [`LayaError::MarkersLost`] when the task's options do not all survive the
    /// builder.
    pub fn from_parts(
        reader: &AprV2ReaderRef<'_>,
        prefix: &str,
        encoder_config_bytes: &[u8],
        agent_config_bytes: &[u8],
        tokenizer_bytes: &[u8],
        task: Task,
    ) -> Result<Self, LayaError> {
        let config = ModernBertConfig::from_json_bytes(encoder_config_bytes)
            .map_err(LayaError::EncoderConfig)?;
        let agent = AgentConfig::from_json_bytes(agent_config_bytes)?;
        let d = config.hidden_size();
        let (nhead, hd) = head_geometry(d)?;
        // Tokenizer and marker rule first: they are cheap and refuse before any weight.
        let builder = Builder::from_bytes(tokenizer_bytes, agent.max_len, agent.head_max_len)?;
        let options = task.render_options();
        let probe = builder.build("", QType::Choice.name(), task.instructions(), &options)?;
        if probe.markers.len() != options.len() {
            return Err(LayaError::MarkersLost {
                criteria: options.len(),
                markers: probe.markers.len(),
            });
        }
        let encoder = ModernBertEncoder::from_apr(reader, &format!("{prefix}encoder."), &config)
            .map_err(LayaError::EncoderLoad)?;
        let ffn = 4 * d;
        let mut head = Vec::with_capacity(agent.head_layers);
        for i in 0..agent.head_layers {
            let p = format!("{prefix}head.layers.{i}.");
            head.push(HeadLayer {
                norm1: (
                    load_tensor(reader, &format!("{p}norm1.weight"), &[d])?,
                    load_tensor(reader, &format!("{p}norm1.bias"), &[d])?,
                ),
                in_proj: Linear {
                    w: load_tensor(reader, &format!("{p}self_attn.in_proj_weight"), &[3 * d, d])?,
                    b: Some(load_tensor(
                        reader,
                        &format!("{p}self_attn.in_proj_bias"),
                        &[3 * d],
                    )?),
                    out: 3 * d,
                    inp: d,
                },
                out_proj: load_linear(reader, &format!("{p}self_attn.out_proj"), d, d)?,
                norm2: (
                    load_tensor(reader, &format!("{p}norm2.weight"), &[d])?,
                    load_tensor(reader, &format!("{p}norm2.bias"), &[d])?,
                ),
                linear1: load_linear(reader, &format!("{p}linear1"), ffn, d)?,
                linear2: load_linear(reader, &format!("{p}linear2"), d, ffn)?,
                nhead,
                hd,
            });
        }
        let type_emb = load_tensor(reader, &format!("{prefix}type_emb.weight"), &[3, d])?;
        let scorer = Scorer {
            norm: (
                load_tensor(reader, &format!("{prefix}scorer.0.weight"), &[d])?,
                load_tensor(reader, &format!("{prefix}scorer.0.bias"), &[d])?,
            ),
            fc1: load_linear(reader, &format!("{prefix}scorer.1"), d, d)?,
            fc2: load_linear(reader, &format!("{prefix}scorer.3"), 1, d)?,
        };
        let temperature = temperature::temperature_for(&agent, QType::Choice, options.len());
        Ok(Self {
            encoder,
            type_emb,
            head,
            scorer,
            builder,
            agent,
            task,
            options,
            temperature,
        })
    }

    /// The request builder (tokenizer + Laya's `build_sequence`).
    #[must_use]
    pub fn builder(&self) -> &Builder {
        &self.builder
    }

    /// The agent config this model was loaded with.
    #[must_use]
    pub fn agent_config(&self) -> &AgentConfig {
        &self.agent
    }

    /// The calibrated temperature applied to the task's `choice` rows.
    #[must_use]
    pub fn temperature(&self) -> f32 {
        self.temperature
    }

    /// Scorer logits for one built row: encoder -> `+ type_emb[qtype]` -> head layers
    /// -> hidden states at `markers` -> scorer. `tap(name, block)` sees the encoder's
    /// `emb` / `layer{i}` / `final`, then `head{i}` and `m_opts`.
    ///
    /// # Errors
    ///
    /// [`LayaError::MarkerOutOfRange`] for a marker outside the row, or an encoder /
    /// primitive refusal.
    #[provable_contracts_macros::contract("laya-parity-v1", equation = "logits_abs")]
    pub fn forward_row(
        &self,
        ids: &[u32],
        markers: &[usize],
        qtype: QType,
        mut tap: impl FnMut(&str, &[f32]),
    ) -> Result<Vec<f32>, DecideError> {
        let l = ids.len();
        if let Some(&marker) = markers.iter().find(|&&m| m >= l) {
            return Err(LayaError::MarkerOutOfRange { marker, tokens: l }.into());
        }
        let mut x = self.encoder.forward(ids, &mut tap)?;
        let d = self.encoder.config().hidden_size();
        let q = qtype.index();
        let te = &self.type_emb[q * d..(q + 1) * d];
        x.par_chunks_mut(d)
            .for_each(|r| r.iter_mut().zip(te).for_each(|(a, b)| *a += b));
        for (i, layer) in self.head.iter().enumerate() {
            layer.forward(&mut x, l)?;
            tap(&format!("head{i}"), &x);
        }
        let m: Vec<f32> = markers
            .iter()
            .flat_map(|&p| x[p * d..(p + 1) * d].iter().copied())
            .collect();
        tap("m_opts", &m);
        Ok(self.scorer.forward(&m, markers.len())?)
    }
}

/// Index of the first maximum (numpy `argmax`); NaN never wins.
fn argmax(p: &[f32]) -> usize {
    (0..p.len()).fold(0, |m, i| if p[i] > p[m] { i } else { m })
}

impl DecisionMethod for Laya {
    fn task(&self) -> &Task {
        &self.task
    }

    fn prepare(&self, texts: &[String]) -> Result<Vec<PreparedRow>, DecideError> {
        texts
            .iter()
            .map(|t| {
                let row = self.builder.build(
                    t,
                    QType::Choice.name(),
                    self.task.instructions(),
                    &self.options,
                )?;
                Ok(PreparedRow {
                    ids: row.ids,
                    markers: row.markers,
                    truncated: row.truncated,
                })
            })
            .collect()
    }

    fn classify_prepared(&self, rows: &[PreparedRow]) -> Result<Vec<Decision>, DecideError> {
        rows.iter()
            .map(|r| {
                if r.markers.len() != self.options.len() {
                    return Err(LayaError::RowMarkerCount {
                        expected: self.options.len(),
                        observed: r.markers.len(),
                    }
                    .into());
                }
                let z = self.forward_row(&r.ids, &r.markers, QType::Choice, |_, _| {})?;
                let probabilities = temperature::softmax_t(&z, self.temperature);
                Ok(Decision {
                    label_index: argmax(&probabilities),
                    probabilities,
                    tokens: r.ids.len(),
                    truncated: r.truncated,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
