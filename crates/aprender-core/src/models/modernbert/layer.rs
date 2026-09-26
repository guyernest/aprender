//! The ModernBERT layer and its reusable primitives: `layer_norm`, `gelu_exact`,
//! `rope_rotate_half`, `attention`.
//!
//! These are public because aprender-decide's Laya head and scorer reuse them (one
//! implementation per operation, OPS-03) rather than copying them. Every function
//! validates its buffer lengths and returns a typed [`ModernBertError`] instead of
//! panicking on an inconsistent call. Numerics and their order are spike 025's.

use super::{check_len, Linear, ModernBertConfig, ModernBertError};
use rayon::prelude::*;

/// Row-wise LayerNorm over `[rows, d]` with f64 mean/variance accumulation.
///
/// `eps` is the caller's configured epsilon (`norm_eps` for the encoder). It is
/// rounded to f32 before use, exactly as torch's CPU LayerNorm casts its double
/// `eps` to the f32 accumulation type, so `1e-5` here is torch's `1e-5f`.
///
/// # Errors
///
/// [`ModernBertError::InputShape`] when `d == 0`, `x` is not a whole number of rows,
/// or `w` / `b` are not `[d]`.
pub fn layer_norm(
    x: &[f32],
    d: usize,
    w: &[f32],
    b: Option<&[f32]>,
    eps: f64,
) -> Result<Vec<f32>, ModernBertError> {
    if d == 0 || x.len() % d != 0 {
        return Err(ModernBertError::InputShape {
            what: "layer_norm.x",
            expected: None,
            observed: x.len(),
        });
    }
    check_len("layer_norm.w", w.len(), &[d])?;
    if let Some(b) = b {
        check_len("layer_norm.b", b.len(), &[d])?;
    }
    let eps = f64::from(eps as f32);
    let mut y = vec![0.0f32; x.len()];
    y.par_chunks_mut(d).zip(x.par_chunks(d)).for_each(|(o, r)| {
        let mean = r.iter().map(|&v| f64::from(v)).sum::<f64>() / d as f64;
        let var = r
            .iter()
            .map(|&v| (f64::from(v) - mean).powi(2))
            .sum::<f64>()
            / d as f64;
        let inv = 1.0 / (var + eps).sqrt();
        for (i, (oi, &ri)) in o.iter_mut().zip(r).enumerate() {
            let v = ((f64::from(ri) - mean) * inv) as f32 * w[i];
            *oi = b.map_or(v, |b| v + b[i]);
        }
    });
    Ok(y)
}

/// Exact (erf) GELU: `0.5 * x * (1 + erf(x / sqrt 2))`, evaluated in f64 as
/// `0.5 * x * erfc(-x / sqrt 2)` through the house `erfc_precise` (the erfc form
/// avoids the negative-tail cancellation).
pub fn gelu_exact(x: f32) -> f32 {
    let x = f64::from(x);
    (0.5 * x * batuta_common::math::erfc_precise(-x / std::f64::consts::SQRT_2)) as f32
}

/// Rotate-half RoPE in place on `[l, heads * hd]`, position = row index.
///
/// torch order: `inv_freq = 1 / theta^(2p / hd)` narrowed to f32, `angle = pos * inv_freq`
/// in f32, `(x_p, x_{p+hd/2}) -> (x_p cos - x_{p+hd/2} sin, x_{p+hd/2} cos + x_p sin)`.
///
/// # Errors
///
/// [`ModernBertError::InputShape`] when `hd` is 0 or odd, or `x` is not `[l, heads * hd]`.
pub fn rope_rotate_half(
    x: &mut [f32],
    l: usize,
    heads: usize,
    hd: usize,
    theta: f64,
) -> Result<(), ModernBertError> {
    if hd == 0 || hd % 2 != 0 || heads == 0 {
        return Err(ModernBertError::InputShape {
            what: "rope.head_dim",
            expected: None,
            observed: hd,
        });
    }
    check_len("rope.x", x.len(), &[l, heads, hd])?;
    let half = hd / 2;
    let inv: Vec<f32> = (0..half)
        .map(|p| (1.0 / theta.powf((2 * p) as f64 / hd as f64)) as f32)
        .collect();
    x.par_chunks_mut(heads * hd)
        .enumerate()
        .take(l)
        .for_each(|(pos, row)| {
            for h in 0..heads {
                let v = &mut row[h * hd..(h + 1) * hd];
                for (p, &f) in inv.iter().enumerate() {
                    let ang = pos as f32 * f;
                    let (s, c) = ang.sin_cos();
                    let (a, b) = (v[p], v[p + half]);
                    v[p] = a * c - b * s;
                    v[p + half] = b * c + a * s;
                }
            }
        });
    Ok(())
}

/// Key range `[lo, hi)` row `i` attends to: all `l` keys, or `|i - j| <= window`.
fn key_range(i: usize, l: usize, window: Option<usize>) -> (usize, usize) {
    match window {
        Some(w) => (
            i.saturating_sub(w),
            i.saturating_add(w).saturating_add(1).min(l),
        ),
        None => (0, l),
    }
}

/// Bidirectional multi-head scaled-dot-product attention over one row.
///
/// `q`, `k`, `v` are `[l, heads * hd]`; `window = Some(w)` keeps `|i - j| <= w`
/// (inclusive), `None` is global. Returns `[l, heads * hd]`.
///
/// # Errors
///
/// [`ModernBertError::InputShape`] when `heads` or `hd` is 0 or any operand is not
/// `[l, heads * hd]`.
pub fn attention(
    q: &[f32],
    k: &[f32],
    v: &[f32],
    l: usize,
    heads: usize,
    hd: usize,
    window: Option<usize>,
) -> Result<Vec<f32>, ModernBertError> {
    if heads == 0 || hd == 0 {
        return Err(ModernBertError::InputShape {
            what: "attention.heads_x_hd",
            expected: None,
            observed: heads * hd,
        });
    }
    check_len("attention.q", q.len(), &[l, heads, hd])?;
    check_len("attention.k", k.len(), &[l, heads, hd])?;
    check_len("attention.v", v.len(), &[l, heads, hd])?;
    let d = heads * hd;
    let scale = 1.0 / (hd as f32).sqrt();
    let mut out = vec![0.0f32; l * d];
    out.par_chunks_mut(d).enumerate().for_each(|(i, o)| {
        let (lo, hi) = key_range(i, l, window);
        let mut s = vec![0.0f32; hi - lo];
        for h in 0..heads {
            let qi = &q[i * d + h * hd..i * d + (h + 1) * hd];
            let mut mx = f32::NEG_INFINITY;
            for (jj, sv) in s.iter_mut().enumerate() {
                let j = lo + jj;
                let kj = &k[j * d + h * hd..j * d + (h + 1) * hd];
                *sv = qi.iter().zip(kj).map(|(a, b)| a * b).sum::<f32>() * scale;
                mx = mx.max(*sv);
            }
            let mut z = 0.0f32;
            for sv in &mut s {
                *sv = (*sv - mx).exp();
                z += *sv;
            }
            let oh = &mut o[h * hd..(h + 1) * hd];
            for (jj, &w) in s.iter().enumerate() {
                let j = lo + jj;
                let vj = &v[j * d + h * hd..j * d + (h + 1) * hd];
                let w = w / z;
                oh.iter_mut().zip(vj).for_each(|(a, b)| *a += w * b);
            }
        }
    });
    Ok(out)
}

/// One ModernBERT encoder layer: pre-norm attention (layer 0 has no `attn_norm`) and
/// a GeGLU MLP, both residual. Constructed only by the loader, so its weights agree
/// with the config.
#[derive(Debug, Clone)]
pub struct ModernBertLayer {
    attn_norm: Option<Vec<f32>>,
    wqkv: Linear,
    wo: Linear,
    mlp_norm: Vec<f32>,
    wi: Linear,
    wo_mlp: Linear,
    global: bool,
}

impl ModernBertLayer {
    pub(crate) fn from_parts(
        attn_norm: Option<Vec<f32>>,
        wqkv: Linear,
        wo: Linear,
        mlp_norm: Vec<f32>,
        wi: Linear,
        wo_mlp: Linear,
        global: bool,
    ) -> Self {
        Self {
            attn_norm,
            wqkv,
            wo,
            mlp_norm,
            wi,
            wo_mlp,
            global,
        }
    }

    /// Whether this layer attends globally (else within the local window).
    pub fn is_global(&self) -> bool {
        self.global
    }

    /// Apply the layer in place to `x` (`[l, d]`); `window` is the local half-window
    /// (ignored on a global layer).
    ///
    /// # Errors
    ///
    /// Any [`ModernBertError`] from the primitives (shape or GEMM).
    pub fn forward(
        &self,
        x: &mut [f32],
        l: usize,
        config: &ModernBertConfig,
        window: usize,
    ) -> Result<(), ModernBertError> {
        let d = config.hidden_size();
        let heads = config.num_attention_heads();
        let hd = config.head_dim();
        let eps = config.norm_eps();
        check_len("layer.x", x.len(), &[l, d])?;
        let xn = match &self.attn_norm {
            Some(w) => layer_norm(x, d, w, None, eps)?,
            None => x.to_vec(),
        };
        let qkv = self.wqkv.forward(&xn, l)?;
        check_len("layer.qkv", qkv.len(), &[l, 3, d])?;
        let (mut q, mut k, mut v) = (
            vec![0.0f32; l * d],
            vec![0.0f32; l * d],
            vec![0.0f32; l * d],
        );
        for i in 0..l {
            let r = &qkv[i * 3 * d..(i + 1) * 3 * d];
            q[i * d..(i + 1) * d].copy_from_slice(&r[..d]);
            k[i * d..(i + 1) * d].copy_from_slice(&r[d..2 * d]);
            v[i * d..(i + 1) * d].copy_from_slice(&r[2 * d..]);
        }
        let theta = if self.global {
            config.rope_theta_global()
        } else {
            config.rope_theta_local()
        };
        rope_rotate_half(&mut q, l, heads, hd, theta)?;
        rope_rotate_half(&mut k, l, heads, hd, theta)?;
        let a = attention(
            &q,
            &k,
            &v,
            l,
            heads,
            hd,
            if self.global { None } else { Some(window) },
        )?;
        let a = self.wo.forward(&a, l)?;
        check_len("layer.attn_out", a.len(), &[l, d])?;
        x.par_iter_mut()
            .zip(a.par_iter())
            .for_each(|(h, o)| *h += o);
        let xn = layer_norm(x, d, &self.mlp_norm, None, eps)?;
        let h = self.wi.forward(&xn, l)?;
        let inter = self.wi.out / 2;
        check_len("layer.wi_out", h.len(), &[l, 2, inter])?;
        let mut g = vec![0.0f32; l * inter];
        if inter > 0 {
            g.par_chunks_mut(inter)
                .zip(h.par_chunks(2 * inter))
                .for_each(|(o, r)| {
                    for (j, oj) in o.iter_mut().enumerate() {
                        *oj = gelu_exact(r[j]) * r[inter + j];
                    }
                });
        }
        let m = self.wo_mlp.forward(&g, l)?;
        check_len("layer.mlp_out", m.len(), &[l, d])?;
        x.par_iter_mut()
            .zip(m.par_iter())
            .for_each(|(h, o)| *h += o);
        Ok(())
    }
}
