//! ModernBERT `config.json`: a typed HF parse that validates the supported domain
//! BEFORE any shape is derived or any buffer allocated.
//!
//! The primitives downstream slice by `hidden_size`, `head_dim` and
//! `2 * intermediate_size` and chunk by the same, so a zero, an odd rotary dim or an
//! overflowing product would otherwise surface as an index panic deep in the forward.
//! Every such config is refused here with a [`ModernBertConfigError`] naming the field.

use serde::Deserialize;
use std::cmp::Ordering;
use std::fmt;

/// Layer-type strings HF writes in `layer_types`.
const FULL_ATTENTION: &str = "full_attention";
const SLIDING_ATTENTION: &str = "sliding_attention";

/// Why a ModernBERT `config.json` is outside the supported domain.
#[derive(Debug, Clone, PartialEq)]
pub enum ModernBertConfigError {
    /// The bytes are not a JSON object with the required fields and types.
    Json(String),
    /// A dimension that must be positive is 0.
    ZeroDimension {
        /// The HF field name.
        field: &'static str,
    },
    /// `hidden_size % num_attention_heads != 0`.
    HeadsDoNotDivideHidden {
        /// `hidden_size`.
        hidden_size: usize,
        /// `num_attention_heads`.
        num_attention_heads: usize,
    },
    /// The head dim is odd; rotate-half RoPE pairs dims `p` and `p + hd/2`.
    OddHeadDim {
        /// `hidden_size / num_attention_heads`.
        head_dim: usize,
    },
    /// `local_attention` is odd; the window is `local_attention / 2` on each side.
    OddLocalAttention {
        /// `local_attention`.
        local_attention: usize,
    },
    /// `layer_types` does not have one entry per layer.
    LayerTypesLength {
        /// `num_hidden_layers`.
        expected: usize,
        /// `layer_types.len()`.
        observed: usize,
    },
    /// A `layer_types` entry is neither `full_attention` nor `sliding_attention`.
    UnknownLayerType {
        /// Layer index.
        layer: usize,
        /// The unknown value.
        value: String,
    },
    /// `layer_types[layer]` disagrees with `layer % global_attn_every_n_layers == 0`.
    LayerTypesDisagree {
        /// Layer index.
        layer: usize,
    },
    /// `norm_eps` is not finite and positive.
    BadNormEps {
        /// The value parsed.
        value: f64,
    },
    /// A rope theta is absent.
    MissingTheta {
        /// Dotted HF path of the absent field.
        field: &'static str,
    },
    /// A rope theta is not finite and positive.
    BadTheta {
        /// Dotted HF path of the field.
        field: &'static str,
        /// The value parsed.
        value: f64,
    },
    /// A bias flag is true; this encoder has no biases.
    UnsupportedBias {
        /// The HF field name.
        field: &'static str,
    },
    /// A weight-shape product overflows `usize`.
    DimensionOverflow {
        /// The product, e.g. `"vocab_size x hidden_size"`.
        field: &'static str,
    },
}

impl fmt::Display for ModernBertConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "modernbert config: invalid JSON: {e}"),
            Self::ZeroDimension { field } => write!(f, "modernbert config: {field} must be > 0"),
            Self::HeadsDoNotDivideHidden {
                hidden_size,
                num_attention_heads,
            } => write!(
                f,
                "modernbert config: num_attention_heads {num_attention_heads} does not divide hidden_size {hidden_size}"
            ),
            Self::OddHeadDim { head_dim } => write!(
                f,
                "modernbert config: head dim {head_dim} is odd (rotate-half RoPE needs it even)"
            ),
            Self::OddLocalAttention { local_attention } => write!(
                f,
                "modernbert config: local_attention {local_attention} is odd (window is local_attention / 2 per side)"
            ),
            Self::LayerTypesLength { expected, observed } => write!(
                f,
                "modernbert config: layer_types has {observed} entries, num_hidden_layers is {expected}"
            ),
            Self::UnknownLayerType { layer, value } => write!(
                f,
                "modernbert config: layer_types[{layer}] = {value:?} is not {FULL_ATTENTION} or {SLIDING_ATTENTION}"
            ),
            Self::LayerTypesDisagree { layer } => write!(
                f,
                "modernbert config: layer_types[{layer}] disagrees with layer % global_attn_every_n_layers"
            ),
            Self::BadNormEps { value } => {
                write!(f, "modernbert config: norm_eps {value} must be finite and > 0")
            }
            Self::MissingTheta { field } => write!(f, "modernbert config: {field} is missing"),
            Self::BadTheta { field, value } => {
                write!(f, "modernbert config: {field} = {value} must be finite and > 0")
            }
            Self::UnsupportedBias { field } => write!(
                f,
                "modernbert config: {field} = true is unsupported (this encoder has no biases)"
            ),
            Self::DimensionOverflow { field } => {
                write!(f, "modernbert config: {field} overflows usize")
            }
        }
    }
}

impl std::error::Error for ModernBertConfigError {}

#[derive(Deserialize)]
struct RawRope {
    rope_theta: Option<f64>,
}

#[derive(Deserialize)]
struct RawRopeParameters {
    full_attention: Option<RawRope>,
    sliding_attention: Option<RawRope>,
}

/// The HF fields this encoder reads; every other HF field is ignored.
#[derive(Deserialize)]
struct RawConfig {
    vocab_size: usize,
    hidden_size: usize,
    intermediate_size: usize,
    num_hidden_layers: usize,
    num_attention_heads: usize,
    global_attn_every_n_layers: usize,
    local_attention: usize,
    norm_eps: f64,
    #[serde(default)]
    layer_types: Option<Vec<String>>,
    #[serde(default)]
    rope_parameters: Option<RawRopeParameters>,
    #[serde(default)]
    norm_bias: bool,
    #[serde(default)]
    attention_bias: bool,
    #[serde(default)]
    mlp_bias: bool,
}

/// A validated ModernBERT configuration. Only [`ModernBertConfig::from_json_bytes`]
/// constructs one, so every instance is inside the supported domain.
#[derive(Debug, Clone, PartialEq)]
pub struct ModernBertConfig {
    vocab_size: usize,
    hidden_size: usize,
    intermediate_size: usize,
    num_hidden_layers: usize,
    num_attention_heads: usize,
    global_attn_every_n_layers: usize,
    local_attention: usize,
    norm_eps: f64,
    rope_theta_global: f64,
    rope_theta_local: f64,
    layer_is_global: Vec<bool>,
}

fn positive_finite(v: f64) -> bool {
    v.is_finite() && matches!(v.partial_cmp(&0.0), Some(Ordering::Greater))
}

fn theta(rope: Option<&RawRope>, field: &'static str) -> Result<f64, ModernBertConfigError> {
    let value = rope
        .and_then(|r| r.rope_theta)
        .ok_or(ModernBertConfigError::MissingTheta { field })?;
    if positive_finite(value) {
        Ok(value)
    } else {
        Err(ModernBertConfigError::BadTheta { field, value })
    }
}

fn checked_product(dims: &[usize], field: &'static str) -> Result<usize, ModernBertConfigError> {
    dims.iter()
        .try_fold(1usize, |acc, &d| acc.checked_mul(d))
        .ok_or(ModernBertConfigError::DimensionOverflow { field })
}

impl ModernBertConfig {
    /// Parse and validate an HF ModernBERT `config.json`.
    ///
    /// # Errors
    ///
    /// [`ModernBertConfigError`] naming the field for every config outside the
    /// supported domain (see the module docs of `models::modernbert`).
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, ModernBertConfigError> {
        let raw: RawConfig = serde_json::from_slice(bytes)
            .map_err(|e| ModernBertConfigError::Json(e.to_string()))?;
        Self::validate(raw)
    }

    fn validate(raw: RawConfig) -> Result<Self, ModernBertConfigError> {
        for (field, v) in [
            ("vocab_size", raw.vocab_size),
            ("hidden_size", raw.hidden_size),
            ("intermediate_size", raw.intermediate_size),
            ("num_hidden_layers", raw.num_hidden_layers),
            ("num_attention_heads", raw.num_attention_heads),
            ("global_attn_every_n_layers", raw.global_attn_every_n_layers),
            ("local_attention", raw.local_attention),
        ] {
            if v == 0 {
                return Err(ModernBertConfigError::ZeroDimension { field });
            }
        }
        if raw.hidden_size % raw.num_attention_heads != 0 {
            return Err(ModernBertConfigError::HeadsDoNotDivideHidden {
                hidden_size: raw.hidden_size,
                num_attention_heads: raw.num_attention_heads,
            });
        }
        let head_dim = raw.hidden_size / raw.num_attention_heads;
        if head_dim % 2 != 0 {
            return Err(ModernBertConfigError::OddHeadDim { head_dim });
        }
        if raw.local_attention % 2 != 0 {
            return Err(ModernBertConfigError::OddLocalAttention {
                local_attention: raw.local_attention,
            });
        }
        let declared = match &raw.layer_types {
            None => None,
            Some(types) => {
                if types.len() != raw.num_hidden_layers {
                    return Err(ModernBertConfigError::LayerTypesLength {
                        expected: raw.num_hidden_layers,
                        observed: types.len(),
                    });
                }
                let mut globals = Vec::with_capacity(types.len());
                for (layer, t) in types.iter().enumerate() {
                    match t.as_str() {
                        FULL_ATTENTION => globals.push(true),
                        SLIDING_ATTENTION => globals.push(false),
                        _ => {
                            return Err(ModernBertConfigError::UnknownLayerType {
                                layer,
                                value: t.clone(),
                            })
                        }
                    }
                }
                Some(globals)
            }
        };
        if !positive_finite(raw.norm_eps) {
            return Err(ModernBertConfigError::BadNormEps {
                value: raw.norm_eps,
            });
        }
        for (field, on) in [
            ("norm_bias", raw.norm_bias),
            ("attention_bias", raw.attention_bias),
            ("mlp_bias", raw.mlp_bias),
        ] {
            if on {
                return Err(ModernBertConfigError::UnsupportedBias { field });
            }
        }
        let rope = raw.rope_parameters.as_ref();
        let rope_theta_global = theta(
            rope.and_then(|r| r.full_attention.as_ref()),
            "rope_parameters.full_attention.rope_theta",
        )?;
        let rope_theta_local = theta(
            rope.and_then(|r| r.sliding_attention.as_ref()),
            "rope_parameters.sliding_attention.rope_theta",
        )?;
        checked_product(
            &[raw.vocab_size, raw.hidden_size],
            "vocab_size x hidden_size",
        )?;
        checked_product(
            &[3, raw.hidden_size, raw.hidden_size],
            "3 x hidden_size x hidden_size",
        )?;
        checked_product(
            &[2, raw.intermediate_size, raw.hidden_size],
            "2 x intermediate_size x hidden_size",
        )?;
        let modulo: Vec<bool> = (0..raw.num_hidden_layers)
            .map(|i| i % raw.global_attn_every_n_layers == 0)
            .collect();
        if let Some(declared) = declared {
            if let Some(layer) = declared.iter().zip(&modulo).position(|(a, b)| a != b) {
                return Err(ModernBertConfigError::LayerTypesDisagree { layer });
            }
        }
        Ok(Self {
            vocab_size: raw.vocab_size,
            hidden_size: raw.hidden_size,
            intermediate_size: raw.intermediate_size,
            num_hidden_layers: raw.num_hidden_layers,
            num_attention_heads: raw.num_attention_heads,
            global_attn_every_n_layers: raw.global_attn_every_n_layers,
            local_attention: raw.local_attention,
            norm_eps: raw.norm_eps,
            rope_theta_global,
            rope_theta_local,
            layer_is_global: modulo,
        })
    }

    /// Vocabulary size (rows of `tok_embeddings`).
    pub fn vocab_size(&self) -> usize {
        self.vocab_size
    }

    /// Hidden size `d`.
    pub fn hidden_size(&self) -> usize {
        self.hidden_size
    }

    /// MLP intermediate size (`Wi` has `2 x` this many rows).
    pub fn intermediate_size(&self) -> usize {
        self.intermediate_size
    }

    /// Number of encoder layers.
    pub fn num_hidden_layers(&self) -> usize {
        self.num_hidden_layers
    }

    /// Number of attention heads.
    pub fn num_attention_heads(&self) -> usize {
        self.num_attention_heads
    }

    /// `hidden_size / num_attention_heads` (even by construction).
    pub fn head_dim(&self) -> usize {
        self.hidden_size / self.num_attention_heads
    }

    /// Global-attention interval.
    pub fn global_attn_every_n_layers(&self) -> usize {
        self.global_attn_every_n_layers
    }

    /// `local_attention` as configured (the full window width).
    pub fn local_attention(&self) -> usize {
        self.local_attention
    }

    /// The local half-window: a local layer keeps `|i - j| <= window()`.
    pub fn window(&self) -> usize {
        self.local_attention / 2
    }

    /// LayerNorm epsilon, honoured by every layer norm.
    pub fn norm_eps(&self) -> f64 {
        self.norm_eps
    }

    /// RoPE theta on global layers.
    pub fn rope_theta_global(&self) -> f64 {
        self.rope_theta_global
    }

    /// RoPE theta on local layers.
    pub fn rope_theta_local(&self) -> f64 {
        self.rope_theta_local
    }

    /// Per-layer global flag (`layer_types` reconciled with the modulo rule).
    pub fn layer_is_global(&self) -> &[bool] {
        &self.layer_is_global
    }
}
