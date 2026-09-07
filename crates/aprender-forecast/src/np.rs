//! NeuralProphet-lite on aprender's f32 autograd: trend (11 segments, NP's
//! parametrisation), Fourier seasonality (days since 1900-01-01, sin block then
//! cos block per NP), optional AR-Net on stationarised lags, weighted Huber
//! loss, AdamW + three-phase one-cycle cosine schedule.
//!
//! Ported verbatim from `sources/004-forecast-mcp-thin-server/src/np.rs` (D-08); the only
//! edits are the ones clippy/rustfmt forced and the `days_from_civil` import, which lives in
//! [`crate::dates`] here rather than in the spike's `prophet` module. Every training rule in
//! this file is load-bearing (D-10) and is pinned by an invariant test in `mod parity`:
//!
//! * [`weighted_huber`] is built from graph-connected ops with a constant 0/1 mask. Core's
//!   own Huber loss in `aprender::nn::loss` (the smooth-L1 type; deliberately NOT named here,
//!   so the D-10 ban stays a file-wide grep) extracts raw data and builds a fresh `Tensor`, so
//!   the parameter gradient after `backward()` is `None` — it cannot train anything. Never
//!   import it into this module; 06-09 files the core ticket to fix it.
//! * [`clear_graph`] runs after EVERY optimiser step; the tape is thread-local and unbounded.
//! * Mini-batches, never full batch: full-batch training collapses to test MAE 2.6.
//! * The learning rate is selected by TRAIN loss, never by test error.
//! * Fourier features are on days since 1900-01-01, which is NeuralProphet's own epoch.

use crate::dates::days_from_civil;
use aprender::autograd::{clear_graph, graph_tape_len, no_grad, Tensor};
use aprender::nn::optim::{AdamW, Optimizer};
use aprender::nn::{Linear, Module};
use std::time::Instant;

// ------------------------------------------------------------------ rng ----
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn normal(&mut self) -> f64 {
        let u1 = self.uniform().max(1e-12);
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
    pub fn shuffle(&mut self, v: &mut [usize]) {
        for i in (1..v.len()).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

// ------------------------------------------------------------- features ----
#[derive(Clone, Debug)]
pub struct NpSeason {
    pub name: String,
    pub period: f64,
    pub order: usize,
}

/// NeuralProphet's auto seasonality: same disable rules as Prophet, resolutions 6 / 3 / 6.
pub fn np_auto_seasonalities(ds_days: &[i64]) -> Vec<NpSeason> {
    let span = (ds_days[ds_days.len() - 1] - ds_days[0]) as f64;
    let min_dt = ds_days
        .windows(2)
        .map(|w| (w[1] - w[0]) as f64)
        .filter(|d| *d > 0.0)
        .fold(f64::INFINITY, f64::min);
    let mut v = Vec::new();
    if span >= 730.0 {
        v.push(NpSeason {
            name: "yearly".into(),
            period: 365.25,
            order: 6,
        });
    }
    if span >= 14.0 && min_dt < 7.0 {
        v.push(NpSeason {
            name: "weekly".into(),
            period: 7.0,
            order: 3,
        });
    }
    if span >= 2.0 && min_dt < 1.0 {
        v.push(NpSeason {
            name: "daily".into(),
            period: 1.0,
            order: 6,
        });
    }
    v
}

pub fn season_dim(s: &[NpSeason]) -> usize {
    s.iter().map(|x| 2 * x.order).sum()
}

/// NP: t in days since 1900-01-01; per seasonality all sin(k*2*pi*t/P) then all cos.
pub fn fourier_feats(day: i64, seasons: &[NpSeason], out: &mut Vec<f32>) {
    let t = (day - days_from_civil(1900, 1, 1)) as f64;
    for s in seasons {
        let f = 2.0 * std::f64::consts::PI / s.period;
        for k in 1..=s.order {
            out.push((f * k as f64 * t).sin() as f32);
        }
        for k in 1..=s.order {
            out.push((f * k as f64 * t).cos() as f32);
        }
    }
}

/// NP trend as a linear function of `[k0, delta_0..delta_S]`: slope in segment i is
/// `k0 + delta_i`, continuity offsets `-cp_j*(delta_j - delta_{j-1})` for every passed
/// changepoint `j >= 1`.
pub fn trend_feats(t: f64, cps: &[f64], out: &mut Vec<f32>) {
    let s = cps.len();
    let seg = cps[1..].iter().filter(|&&c| t >= c).count();
    let mut phi = vec![0.0f64; s];
    phi[seg] += t;
    for j in 1..s {
        if t >= cps[j] {
            phi[j] -= cps[j];
            phi[j - 1] += cps[j];
        }
    }
    out.push(t as f32);
    out.extend(phi.iter().map(|v| *v as f32));
}

/// NeuralProphet's own `end_w`, and the default [`TrainConfig::newer_w`] carries.
///
/// Used by the PREDICTION paths, whose `Rows::w` no loss ever reads; training reads the
/// caller's `cfg.newer_w` instead of this.
pub const DEFAULT_END_W: f64 = 2.0;

/// Newer-samples weight (NP `_get_time_based_sample_weight`, `end_w = 2`, `start_t = 0`).
pub fn sample_weight(t: f64, end_w: f64) -> f32 {
    let time = t.clamp(0.0, 1.0);
    let c = 0.5 * (std::f64::consts::PI * (time - 1.0)).cos() + 0.5;
    ((1.0 + c * (end_w - 1.0)) / end_w) as f32
}

// ----------------------------------------------------------------- data ----
pub struct NpData {
    /// Daily grid covering `[first, last]` of the FULL series (train + test); y linearly imputed.
    pub grid_days: Vec<i64>,
    pub grid_y: Vec<f64>,
    pub grid_observed: Vec<bool>,
    pub n_train_grid: usize,
    pub shift: f64,
    pub scale: f64,
    pub t0: i64,
    pub t_span: f64,
    pub cps: Vec<f64>,
    pub seasons: Vec<NpSeason>,
}

impl NpData {
    pub fn new(
        ds_days: &[i64],
        y: &[f64],
        n_train_rows: usize,
        n_changepoints: usize,
        changepoints_range: f64,
    ) -> Self {
        let first = ds_days[0];
        let last = ds_days[ds_days.len() - 1];
        let n = (last - first + 1) as usize;
        let mut grid_y = vec![f64::NAN; n];
        let mut grid_observed = vec![false; n];
        for (d, v) in ds_days.iter().zip(y) {
            let i = (d - first) as usize;
            grid_y[i] = *v;
            grid_observed[i] = true;
        }
        // linear imputation
        let mut i = 0;
        while i < n {
            if grid_y[i].is_nan() {
                let a = i - 1;
                let mut b = i;
                while grid_y[b].is_nan() {
                    b += 1;
                }
                for k in i..b {
                    let f = (k - a) as f64 / (b - a) as f64;
                    grid_y[k] = grid_y[a] + f * (grid_y[b] - grid_y[a]);
                }
                i = b;
            }
            i += 1;
        }
        let train_last = ds_days[n_train_rows - 1];
        let n_train_grid = (train_last - first + 1) as usize;
        // soft normalisation from OBSERVED train values: shift = min, scale = q95 - min
        let mut obs: Vec<f64> = y[..n_train_rows].to_vec();
        obs.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        let lowest = obs[0];
        let q = 0.95 * (obs.len() - 1) as f64;
        let lo = q.floor() as usize;
        let frac = q - lo as f64;
        let q95 = obs[lo] + frac * (obs[(lo + 1).min(obs.len() - 1)] - obs[lo]);
        let width = if (q95 - lowest).abs() < 1e-12 {
            obs[obs.len() - 1] - lowest
        } else {
            q95 - lowest
        };
        let cps: Vec<f64> = (0..=n_changepoints)
            .map(|i| changepoints_range * i as f64 / (n_changepoints + 1) as f64)
            .collect();
        // The auto rules can select NOTHING — 20 points seven days apart give span 133
        // (no yearly) and min_dt exactly 7.0 (no weekly, the test is `< 7.0`) — and
        // `season_dim() == 0` then builds a `Linear::without_bias(0, 1)` whose transpose
        // trips trueno's `Contract transpose: input is empty`. Through the server that is
        // an `Internal` for a caller-fixable input; with debug assertions off it is worse,
        // a silently zero seasonality term. Prophet's arm already fills the same hole with
        // a harmless weekly column (`forecast.rs`); do the same here, on the daily grid
        // this struct imputes, so the design always has K >= 1.
        let mut seasons = np_auto_seasonalities(&ds_days[..n_train_rows]);
        if seasons.is_empty() {
            seasons.push(NpSeason {
                name: "weekly".into(),
                period: 7.0,
                order: 3,
            });
        }
        NpData {
            grid_days: (first..=last).collect(),
            grid_y,
            grid_observed,
            n_train_grid,
            shift: lowest,
            scale: width,
            t0: first,
            t_span: (train_last - first) as f64,
            cps,
            seasons,
        }
    }
    pub fn t_of(&self, day: i64) -> f64 {
        (day - self.t0) as f64 / self.t_span
    }
    pub fn norm(&self, v: f64) -> f32 {
        ((v - self.shift) / self.scale) as f32
    }
    pub fn denorm(&self, v: f32) -> f64 {
        v as f64 * self.scale + self.shift
    }
    pub fn trend_dim(&self) -> usize {
        self.cps.len() + 1
    }
    pub fn season_dim(&self) -> usize {
        season_dim(&self.seasons)
    }
    pub fn row_feats(&self, day: i64, tr: &mut Vec<f32>, se: &mut Vec<f32>) {
        trend_feats(self.t_of(day), &self.cps, tr);
        fourier_feats(day, &self.seasons, se);
    }
}

// ---------------------------------------------------------------- model ----
pub struct NpModel {
    pub trend: Linear,
    pub season: Linear,
    pub bias: Tensor,
    pub ar: Vec<Linear>,
    pub n_lags: usize,
}

fn normal_tensor(rng: &mut Rng, shape: &[usize], std: f64) -> Tensor {
    let n: usize = shape.iter().product();
    Tensor::from_vec((0..n).map(|_| (rng.normal() * std) as f32).collect(), shape).requires_grad()
}

impl NpModel {
    /// NP init: `k0 ~ xavier` on `[1,1]` (std 1), `delta` std `sqrt(2/(2S))`, season std
    /// `sqrt(1/(2R))`, bias std 1, AR kaiming `fan_in`.
    pub fn new(d: &NpData, n_lags: usize, ar_layers: &[usize], rng: &mut Rng) -> Self {
        let td = d.trend_dim();
        let mut trend = Linear::without_bias(td, 1);
        let mut w: Vec<f32> = vec![rng.normal() as f32];
        let s = d.cps.len();
        w.extend((0..s).map(|_| (rng.normal() * (2.0 / (2.0 * s as f64)).sqrt()) as f32));
        trend.set_weight(Tensor::from_vec(w, &[1, td]).requires_grad());
        let sd = d.season_dim();
        let mut season = Linear::without_bias(sd, 1);
        let mut w = Vec::with_capacity(sd);
        for se in &d.seasons {
            let std = (1.0 / (2.0 * se.order as f64)).sqrt();
            w.extend((0..2 * se.order).map(|_| (rng.normal() * std) as f32));
        }
        season.set_weight(Tensor::from_vec(w, &[1, sd]).requires_grad());
        let bias = normal_tensor(rng, &[1], 1.0);
        let mut ar = Vec::new();
        if n_lags > 0 {
            let mut d_in = n_lags;
            for &h in ar_layers {
                let mut l = Linear::new(d_in, h);
                l.set_weight(normal_tensor(rng, &[h, d_in], (2.0 / d_in as f64).sqrt()));
                l.set_bias(Tensor::zeros(&[h]).requires_grad());
                ar.push(l);
                d_in = h;
            }
            let mut l = Linear::without_bias(d_in, 1);
            l.set_weight(normal_tensor(rng, &[1, d_in], (2.0 / d_in as f64).sqrt()));
            ar.push(l);
        }
        NpModel {
            trend,
            season,
            bias,
            ar,
            n_lags,
        }
    }

    pub fn parameters_mut(&mut self) -> Vec<&mut Tensor> {
        let mut v = self.trend.parameters_mut();
        v.extend(self.season.parameters_mut());
        v.push(&mut self.bias);
        for l in &mut self.ar {
            v.extend(l.parameters_mut());
        }
        v
    }

    pub fn n_params(&self) -> usize {
        self.trend.num_parameters()
            + self.season.num_parameters()
            + 1
            + self.ar.iter().map(Module::num_parameters).sum::<usize>()
    }

    /// `lags`: (raw normalised lags `[b,L]`, season feats at lag times `[b*L,F]`, trend at lag
    /// times `[b,L]` (detached)).
    pub fn forward(
        &self,
        x_trend: &Tensor,
        x_season: &Tensor,
        lags: Option<(&Tensor, &Tensor, &Tensor)>,
    ) -> Tensor {
        let b = x_trend.shape()[0];
        let out = self
            .trend
            .forward(x_trend)
            .add(&self.season.forward(x_season))
            .broadcast_add(&self.bias);
        match lags {
            None => out,
            Some((raw, se_lag, tr_lag)) => {
                let s_lag = self.season.forward(se_lag).view(&[b, self.n_lags]);
                let mut h = raw.sub(tr_lag).sub(&s_lag);
                let n = self.ar.len();
                for (i, l) in self.ar.iter().enumerate() {
                    h = l.forward(&h);
                    if i + 1 < n {
                        h = h.relu();
                    }
                }
                out.add(&h)
            }
        }
    }
}

/// Exact SmoothL1 (Huber, beta) x sample weight, mean — built from graph-connected ops with a
/// constant 0/1 mask (the mask is piecewise constant, so this IS the exact gradient).
///
/// This function exists because core's own smooth-L1 (Huber) loss in `aprender::nn::loss` is
/// DETACHED from the graph (D-10): its `forward` maps over `diff.data()` and wraps the result
/// in a fresh `Tensor`, so every parameter's gradient after `backward()` is `None`. The type is
/// deliberately not named here — the D-10 ban is enforced as a file-wide grep. Pinned by
/// `parity::weighted_huber_is_graph_connected`.
pub fn weighted_huber(pred: &Tensor, target: &Tensor, w: &Tensor, beta: f32) -> Tensor {
    let d = pred.sub(target);
    let a = d.abs();
    let mask: Vec<f32> = a
        .data()
        .iter()
        .map(|v| if *v < beta { 1.0 } else { 0.0 })
        .collect();
    let inv: Vec<f32> = mask.iter().map(|m| 1.0 - m).collect();
    let m = Tensor::from_vec(mask, a.shape());
    let im = Tensor::from_vec(inv, a.shape());
    let half_beta = Tensor::from_vec(vec![0.5 * beta; a.numel()], a.shape());
    let quad = d.pow(2.0).mul_scalar(0.5 / beta).mul(&m);
    let lin = a.sub(&half_beta).mul(&im);
    quad.add(&lin).mul(w).mean()
}

/// NP's `OneCycleLR` (`three_phase`, cos, div 10, `final_div` 10) over progress `p` in `[0,1]`.
pub fn one_cycle_lr(p: f64, max_lr: f64) -> f64 {
    let (init, fin) = (max_lr / 10.0, max_lr / 100.0);
    let cosine =
        |s: f64, e: f64, f: f64| e + (s - e) / 2.0 * (1.0 + (std::f64::consts::PI * f).cos());
    if p < 0.3 {
        cosine(init, max_lr, p / 0.3)
    } else if p < 0.6 {
        cosine(max_lr, init, (p - 0.3) / 0.3)
    } else {
        cosine(init, fin, (p - 0.6) / 0.4)
    }
}

/// NeuralProphet's auto batch size. ALWAYS a mini-batch for `n >= 64` (D-10): full-batch
/// training collapses the fit (measured test MAE 2.6 against 0.45).
pub fn auto_batch(n: usize) -> usize {
    (2usize.pow(1 + (1.5 * (n as f64).log10()) as u32))
        .clamp(8, 2048)
        .min(n)
}

/// NeuralProphet's auto epoch count. The formula ASSUMES mini-batches — see [`auto_batch`].
pub fn auto_epochs(n: usize) -> usize {
    (10.0 * (100.0 / n as f64 * 2f64.powf(2.25 * (10.0 + n as f64).log10())).ceil())
        .clamp(20.0, 500.0) as usize
}

/// The number of grid rows [`train`] will actually step over for this `n_lags`.
///
/// The DOOR needs this BEFORE it decides whether to train at all (cost axis C-08), and
/// [`train`] derives its own `n` from the same rule — a `debug_assert` inside `train` pins
/// the two together. A second inline copy of this rule in `forecast.rs` is exactly the drift
/// hazard `prophet::changepoint_count` was extracted to avoid in plan 06-14: a bound the
/// door computes differently from the work that is spent is a bound that can be evaded
/// wherever the two disagree.
#[must_use]
pub fn n_training_samples(d: &NpData, n_lags: usize) -> usize {
    if n_lags == 0 {
        // NP trains the lag-free model on the OBSERVED rows only.
        d.grid_observed[..d.n_train_grid]
            .iter()
            .filter(|o| **o)
            .count()
    } else {
        // With lags it trains on `(n_lags..n_train_grid)` of the imputed daily grid.
        d.n_train_grid.saturating_sub(n_lags)
    }
}

/// A door-computable PROXY for the multiply-accumulates ONE [`train`] call spends:
/// `epochs * n_samples * (n_lags + 1)`.
///
/// **It is a proxy, not a wall-clock prediction.** It counts the per-sample feature width
/// the optimiser sweeps (`n_lags` AR inputs plus the trend/seasonality block, collapsed to
/// `+ 1`) times the number of samples times the number of epochs — deliberately ignoring
/// the AR head width, the batch size and the autograd tape, none of which the door can
/// price and none of which change the SHAPE of the growth. Its only job is to be the SAME
/// number the door checks and the trainer spends, so a request cannot buy work the door
/// did not price.
///
/// The door multiplies this by the learning-rate SWEEP width (2 with lags, 3 without),
/// because the sweep is run by `forecast::forecast` and not by `train`.
///
/// Saturating throughout: at the structural maximum the product is ~3.6e8 per training,
/// nine orders of magnitude below `u64::MAX`, so saturation is unreachable — it is here so
/// that a future bound change cannot turn an overflow into a silently SMALL cost that
/// passes the door.
#[must_use]
pub fn train_cost(n_samples: usize, epochs: usize, n_lags: usize) -> u64 {
    (epochs as u64)
        .saturating_mul(n_samples as u64)
        .saturating_mul(n_lags as u64 + 1)
}

/// The learning-rate sweep the DOOR runs for this `n_lags`.
///
/// spike-002's stand-in for NeuralProphet's range test (D-10: selected by TRAIN loss, never
/// by test error). It lives here, not inlined in `forecast.rs`, so the door's COST estimate
/// multiplies by the same width the door actually runs — a second inline copy is precisely
/// the drift hazard `prophet::changepoint_count` was extracted to avoid in plan 06-14.
#[must_use]
pub fn door_lr_sweep(n_lags: usize) -> &'static [f64] {
    if n_lags > 0 {
        &[0.03, 0.1]
    } else {
        &[0.01, 0.03, 0.1]
    }
}

/// The epoch count the DOOR configures for one [`train`] call.
///
/// With lags on, spike-002 gives the linear AR case 4x the auto epochs of the POINT count,
/// capped at 320; without lags the door passes `epochs: None` and `train` falls back to
/// `auto_epochs(n_samples)` — which is exactly what this returns, so the door can price the
/// lag-free arm without changing its behaviour.
#[must_use]
pub fn door_epochs(n_points: usize, n_samples: usize, n_lags: usize) -> usize {
    if n_lags > 0 {
        auto_epochs(n_points).min(320)
    } else {
        auto_epochs(n_samples)
    }
}

/// The TOTAL priced work of ONE `forecast` request on the neuralprophet arm: [`train_cost`]
/// for a single training, times the width of the learning-rate sweep the door runs.
///
/// This is the number cost axis **C-08** is bounded on. It is computable at the door — every
/// factor is known once [`NpData::new`] has run and the `n_lags` range checks have fired —
/// and it is built out of the same three functions the door then uses to CONFIGURE the
/// sweep, so a request cannot buy work the door did not price.
#[must_use]
pub fn request_train_cost(d: &NpData, n_points: usize, n_lags: usize) -> u64 {
    let n_samples = n_training_samples(d, n_lags);
    train_cost(n_samples, door_epochs(n_points, n_samples, n_lags), n_lags)
        .saturating_mul(door_lr_sweep(n_lags).len() as u64)
}

// -------------------------------------------------------------- training ----
pub struct TrainConfig {
    pub n_lags: usize,
    pub ar_layers: Vec<usize>,
    pub max_lr: f64,
    pub epochs: Option<usize>,
    pub batch: Option<usize>,
    pub weight_decay: f32,
    pub huber_beta: f32,
    pub newer_w: f64,
    pub seed: u64,
}

pub struct TrainLog {
    pub epochs: usize,
    pub batch: usize,
    pub n_samples: usize,
    pub n_params: usize,
    pub epoch_loss: Vec<f64>,
    pub seconds: f64,
    pub tape_len_per_step: usize,
    pub steps: usize,
    /// [`train_cost`] evaluated on the `n_samples` / `epochs` / `n_lags` this call actually
    /// used — the same number the door priced the request at before calling in.
    pub train_cost: u64,
}

/// Precomputed per-grid-row features.
pub struct Rows {
    pub tr: Vec<f32>,
    pub se: Vec<f32>,
    pub y: Vec<f32>,
    pub w: Vec<f32>,
    pub td: usize,
    pub sd: usize,
}

/// `end_w` is [`TrainConfig::newer_w`]: the sample weight at the END of the series, with
/// 1.0 at the start. It is a PARAMETER rather than the literal `2.0` it used to be because
/// `newer_w` is a public knob on a public config — hardcoding it here meant a caller could
/// set `newer_w: 4.0`, get a fit trained at 2.0, and be told nothing. Prediction paths pass
/// the same default; `Rows::w` is only read by the training loss.
pub fn rows_for(d: &NpData, days: &[i64], y_norm: &[f32], end_w: f64) -> Rows {
    let (td, sd) = (d.trend_dim(), d.season_dim());
    let (mut tr, mut se, mut w) = (
        Vec::with_capacity(days.len() * td),
        Vec::with_capacity(days.len() * sd),
        Vec::with_capacity(days.len()),
    );
    for &day in days {
        d.row_feats(day, &mut tr, &mut se);
        w.push(sample_weight(d.t_of(day), end_w));
    }
    Rows {
        tr,
        se,
        y: y_norm.to_vec(),
        w,
        td,
        sd,
    }
}

fn gather(rows: &[f32], width: usize, idx: &[usize]) -> Vec<f32> {
    let mut v = Vec::with_capacity(idx.len() * width);
    for &i in idx {
        v.extend_from_slice(&rows[i * width..(i + 1) * width]);
    }
    v
}

/// Train the NeuralProphet-lite model. `clear_graph()` runs after EVERY optimiser step (D-10):
/// the autograd tape is thread-local and unbounded, so a fit that does not clear it leaks the
/// whole training history and leaves the tape dirty for the next caller on that thread.
pub fn train(d: &NpData, cfg: &TrainConfig, verbose: bool) -> (NpModel, TrainLog) {
    let mut rng = Rng::new(cfg.seed);
    let mut model = NpModel::new(d, cfg.n_lags, &cfg.ar_layers, &mut rng);
    let n_grid = d.n_train_grid;
    let y_norm: Vec<f32> = d.grid_y[..n_grid].iter().map(|v| d.norm(*v)).collect();
    let rows = rows_for(d, &d.grid_days[..n_grid], &y_norm, cfg.newer_w);
    let l = cfg.n_lags;
    // NP trains the lag-free model on the OBSERVED rows only (no imputation needed);
    // with lags it trains on the imputed daily grid so every window is complete.
    let samples: Vec<usize> = if l == 0 {
        (0..n_grid).filter(|&i| d.grid_observed[i]).collect()
    } else {
        (l..n_grid).collect()
    };
    let n = samples.len();
    let batch = cfg.batch.unwrap_or_else(|| auto_batch(n));
    let epochs = cfg.epochs.unwrap_or_else(|| auto_epochs(n));
    let n_batches = n.div_ceil(batch);
    let total_steps = epochs * n_batches;
    // The door priced this request with `train_cost(n_training_samples(d, l), epochs, l)`
    // BEFORE calling in (cost axis C-08). If the two ever derived a different `n` the bound
    // would be evadable wherever they disagreed, so the sample rule is pinned here rather
    // than argued. `debug_assert` because this is the hot path and the rule is one branch.
    debug_assert_eq!(
        n,
        n_training_samples(d, l),
        "np::train and np::n_training_samples must agree about how many rows a request buys"
    );
    let mut opt = {
        let params = model.parameters_mut();
        AdamW::new(params, cfg.max_lr as f32).weight_decay(cfg.weight_decay)
    };
    let mut log = TrainLog {
        epochs,
        batch,
        n_samples: n,
        n_params: model.n_params(),
        epoch_loss: Vec::new(),
        seconds: 0.0,
        tape_len_per_step: 0,
        steps: 0,
        train_cost: train_cost(n, epochs, l),
    };
    let t0 = Instant::now();
    let mut order = samples.clone();
    let mut step = 0usize;
    for epoch in 0..epochs {
        rng.shuffle(&mut order);
        let mut acc = 0.0;
        for chunk in order.chunks(batch) {
            let p = step as f64 / total_steps as f64;
            opt.set_lr(one_cycle_lr(p, cfg.max_lr) as f32);
            let b = chunk.len();
            let xt = Tensor::from_vec(gather(&rows.tr, rows.td, chunk), &[b, rows.td]);
            let xs = Tensor::from_vec(gather(&rows.se, rows.sd, chunk), &[b, rows.sd]);
            let yt = Tensor::from_vec(chunk.iter().map(|&i| rows.y[i]).collect(), &[b, 1]);
            let wt = Tensor::from_vec(chunk.iter().map(|&i| rows.w[i]).collect(), &[b, 1]);
            let pred = if l == 0 {
                model.forward(&xt, &xs, None)
            } else {
                let lag_idx: Vec<usize> = chunk.iter().flat_map(|&i| i - l..i).collect();
                let raw = Tensor::from_vec(lag_idx.iter().map(|&j| rows.y[j]).collect(), &[b, l]);
                let se_lag =
                    Tensor::from_vec(gather(&rows.se, rows.sd, &lag_idx), &[b * l, rows.sd]);
                let tr_lag_feats =
                    Tensor::from_vec(gather(&rows.tr, rows.td, &lag_idx), &[b * l, rows.td]);
                let tr_lag = no_grad(|| model.trend.forward(&tr_lag_feats).detach()).view(&[b, l]);
                let tr_lag = Tensor::from_vec(tr_lag.data().to_vec(), &[b, l]);
                model.forward(&xt, &xs, Some((&raw, &se_lag, &tr_lag)))
            };
            let loss = weighted_huber(&pred, &yt, &wt, cfg.huber_beta);
            acc += f64::from(loss.item()) * b as f64;
            loss.backward();
            if step == 0 {
                log.tape_len_per_step = graph_tape_len();
            }
            let mut params = model.parameters_mut();
            opt.step_with_params(&mut params);
            opt.zero_grad();
            clear_graph();
            step += 1;
        }
        log.epoch_loss.push(acc / n as f64);
        if verbose && (epoch % 10 == 0 || epoch + 1 == epochs) {
            eprintln!(
                "  epoch {epoch:3} loss {:.5} lr {:.2e}",
                acc / n as f64,
                one_cycle_lr(step as f64 / total_steps as f64, cfg.max_lr)
            );
        }
    }
    log.seconds = t0.elapsed().as_secs_f64();
    log.steps = step;
    (model, log)
}

/// Trend + seasonality prediction (original scale) for arbitrary days.
pub fn predict_ts(d: &NpData, m: &NpModel, days: &[i64]) -> Vec<f64> {
    let rows = rows_for(d, days, &vec![0.0; days.len()], DEFAULT_END_W);
    let xt = Tensor::from_vec(rows.tr.clone(), &[days.len(), rows.td]);
    let xs = Tensor::from_vec(rows.se.clone(), &[days.len(), rows.sd]);
    let out = no_grad(|| m.forward(&xt, &xs, None));
    clear_graph();
    out.data().iter().map(|v| d.denorm(*v)).collect()
}

/// One-step-ahead AR prediction (original scale) for grid indices `idx` using true lags.
pub fn predict_ar_1step(d: &NpData, m: &NpModel, idx: &[usize]) -> Vec<f64> {
    let l = m.n_lags;
    let y_norm: Vec<f32> = d.grid_y.iter().map(|v| d.norm(*v)).collect();
    let rows = rows_for(d, &d.grid_days, &y_norm, DEFAULT_END_W);
    let b = idx.len();
    let xt = Tensor::from_vec(gather(&rows.tr, rows.td, idx), &[b, rows.td]);
    let xs = Tensor::from_vec(gather(&rows.se, rows.sd, idx), &[b, rows.sd]);
    let lag_idx: Vec<usize> = idx.iter().flat_map(|&i| i - l..i).collect();
    let raw = Tensor::from_vec(lag_idx.iter().map(|&j| rows.y[j]).collect(), &[b, l]);
    let se_lag = Tensor::from_vec(gather(&rows.se, rows.sd, &lag_idx), &[b * l, rows.sd]);
    let tr_feats = Tensor::from_vec(gather(&rows.tr, rows.td, &lag_idx), &[b * l, rows.td]);
    let out = no_grad(|| {
        let tr_lag = Tensor::from_vec(m.trend.forward(&tr_feats).data().to_vec(), &[b, l]);
        m.forward(&xt, &xs, Some((&raw, &se_lag, &tr_lag)))
    });
    clear_graph();
    out.data().iter().map(|v| d.denorm(*v)).collect()
}

/// Trend component only (original scale) for arbitrary days.
pub fn predict_trend(d: &NpData, m: &NpModel, days: &[i64]) -> Vec<f64> {
    let rows = rows_for(d, days, &vec![0.0; days.len()], DEFAULT_END_W);
    let xt = Tensor::from_vec(rows.tr.clone(), &[days.len(), rows.td]);
    let out = no_grad(|| m.trend.forward(&xt).broadcast_add(&m.bias));
    clear_graph();
    out.data().iter().map(|v| d.denorm(*v)).collect()
}

/// Multi-step AR forecast by feeding predictions back as lags (days must continue the daily grid).
pub fn predict_ar_recursive(d: &NpData, m: &NpModel, days: &[i64]) -> Vec<f64> {
    let l = m.n_lags;
    let mut hist: Vec<f32> = d.grid_y.iter().map(|v| d.norm(*v)).collect();
    let mut hist_days: Vec<i64> = d.grid_days.clone();
    let mut out = Vec::with_capacity(days.len());
    for &day in days {
        // fill any gap between the last known day and this one recursively
        while hist_days[hist_days.len() - 1] < day {
            let next = hist_days[hist_days.len() - 1] + 1;
            let n = hist.len();
            let (mut tr, mut se) = (Vec::new(), Vec::new());
            d.row_feats(next, &mut tr, &mut se);
            let lag_days: Vec<i64> = (next - l as i64..next).collect();
            let (mut se_lag, mut tr_lag) = (Vec::new(), Vec::new());
            for &ld in &lag_days {
                d.row_feats(ld, &mut tr_lag, &mut se_lag);
            }
            let xt = Tensor::from_vec(tr, &[1, d.trend_dim()]);
            let xs = Tensor::from_vec(se, &[1, d.season_dim()]);
            let raw = Tensor::from_vec(hist[n - l..].to_vec(), &[1, l]);
            let se_l = Tensor::from_vec(se_lag, &[l, d.season_dim()]);
            let tr_feats = Tensor::from_vec(tr_lag, &[l, d.trend_dim()]);
            let v = no_grad(|| {
                let tl = Tensor::from_vec(m.trend.forward(&tr_feats).data().to_vec(), &[1, l]);
                m.forward(&xt, &xs, Some((&raw, &se_l, &tl))).data()[0]
            });
            clear_graph();
            hist.push(v);
            hist_days.push(next);
        }
        let idx = (day - hist_days[0]) as usize;
        out.push(d.denorm(hist[idx]));
    }
    out
}

// ============================================================== parity ====
// The SC3 correctness ladder and the D-10 training-rule invariants. Every numeric bar is
// READ from `contracts/neuralprophet-parity-v1.yaml` at test time (D-15) — a bar that also
// exists as a literal here could be loosened without the contract ever noticing.
#[cfg(test)]
mod parity {
    use super::{
        auto_batch, auto_epochs, fourier_feats, predict_ar_1step, predict_ts, train,
        weighted_huber, NpData, NpModel, NpSeason, Rng, TrainConfig,
    };
    use crate::dates::{days_from_civil, parse_ymd};
    use crate::test_support::{
        constant_u64, contract_path, equation_tolerance, load_json, read_csv,
    };
    use aprender::autograd::{clear_graph, get_grad, graph_tape_len, Tensor};

    /// The oracle holds out the last 365 daily rows (spike-002 `main.rs`: `n_train = n - H`).
    const HOLDOUT: usize = 365;

    /// Read a comma-separated `constants.<key>` learning-rate sweep from the contract.
    ///
    /// `test_support` carries `constant_u64` only, and a sweep is an ordered list rather than a
    /// scalar. Kept local to this module so the plan's `files_modified` set stays honest.
    fn constant_lr_sweep(key: &str) -> Vec<f64> {
        let path = contract_path("neuralprophet-parity-v1");
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "contract neuralprophet-parity-v1 must exist at {}: {e}",
                path.display()
            )
        });
        let doc: serde_yaml::Value = serde_yaml::from_str(&raw)
            .unwrap_or_else(|e| panic!("contract neuralprophet-parity-v1 is not valid YAML: {e}"));
        let s = doc
            .get("constants")
            .and_then(|c| c.get(key))
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or_else(|| {
                panic!("contract neuralprophet-parity-v1 must define constants.{key} as a string")
            });
        s.split(',')
            .map(|p| {
                p.trim()
                    .parse::<f64>()
                    .unwrap_or_else(|e| panic!("constants.{key}: {p:?} is not a float: {e}"))
            })
            .collect()
    }

    /// The oracle split: the whole Peyton Manning CSV, and the index where the 365-row holdout
    /// begins. Read from the same de-duplicated CSV the oracle was generated from.
    fn peyton_split() -> (Vec<i64>, Vec<f64>, usize) {
        let (ds_s, y) = read_csv("peyton_manning.csv");
        let ds: Vec<i64> = ds_s.iter().map(|s| parse_ymd(s)).collect();
        let n_train = y.len() - HOLDOUT;
        (ds, y, n_train)
    }

    fn mae(a: &[f64], b: &[f64]) -> f64 {
        a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f64>() / a.len() as f64
    }

    /// The oracle stores `data_params` as pandas reprs, e.g. `"5.26269018890489"` and
    /// `"2596 days 00:00:00"`. Take the leading numeric token.
    fn leading_f64(s: &str) -> f64 {
        let head: String = s
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+' || *c == 'e')
            .collect();
        head.parse().unwrap_or_else(|e| {
            panic!("oracle data_params {s:?} does not start with a number: {e}")
        })
    }

    fn oracle_str<'a>(v: &'a serde_json::Value, path: &[&str]) -> &'a str {
        let mut cur = v;
        for k in path {
            cur = &cur[*k];
        }
        cur.as_str()
            .unwrap_or_else(|| panic!("oracle key {} must be a string", path.join(".")))
    }

    fn oracle_f64(v: &serde_json::Value, path: &[&str]) -> f64 {
        let mut cur = v;
        for k in path {
            cur = &cur[*k];
        }
        cur.as_f64()
            .unwrap_or_else(|| panic!("oracle key {} must be a number", path.join(".")))
    }

    // ---------------------------------------------------------- rung 1 ----

    #[test]
    fn data_prep_matches_np_oracle() {
        let (ds, y, n_train) = peyton_split();
        let d = NpData::new(&ds, &y, n_train, 10, 0.8);
        let ora = load_json("np_oracle_peyton.json");
        let ts = &ora["trend_seasonality"];
        let tol = equation_tolerance("neuralprophet-parity-v1", "data_prep_vs_oracle_abs");

        let np_shift = leading_f64(oracle_str(ts, &["data_params", "y", "shift"]));
        let np_scale = leading_f64(oracle_str(ts, &["data_params", "y", "scale"]));
        assert!(
            (d.shift - np_shift).abs() <= tol,
            "soft-normalisation shift: rust {} vs NeuralProphet {np_shift} (bar {tol:e})",
            d.shift
        );
        assert!(
            (d.scale - np_scale).abs() <= tol,
            "soft-normalisation scale (q95 - min): rust {} vs NeuralProphet {np_scale} (bar {tol:e})",
            d.scale
        );

        let np_t0_str = oracle_str(ts, &["data_params", "ds", "shift"]);
        let np_t0 = parse_ymd(&np_t0_str[..10]);
        assert_eq!(
            d.t0, np_t0,
            "time origin: rust day {} vs NeuralProphet {np_t0_str}",
            d.t0
        );
        let np_span = leading_f64(oracle_str(ts, &["data_params", "ds", "scale"]));
        assert!(
            (d.t_span - np_span).abs() <= tol,
            "train span in days: rust {} vs NeuralProphet {np_span} (bar {tol:e})",
            d.t_span
        );

        let np_cps = ts["changepoints_t"]
            .as_array()
            .expect("oracle trend_seasonality.changepoints_t");
        assert_eq!(
            d.cps.len(),
            np_cps.len(),
            "changepoint count: rust {} vs NeuralProphet {}",
            d.cps.len(),
            np_cps.len()
        );
        let worst = d
            .cps
            .iter()
            .zip(np_cps)
            .map(|(a, b)| (a - b.as_f64().expect("changepoint")).abs())
            .fold(0.0_f64, f64::max);
        assert!(
            worst <= tol,
            "changepoints_t max abs diff {worst:e} exceeds the contract bar {tol:e}"
        );

        // The two auto formulas are asserted as EXACT integers against NeuralProphet's own
        // reported values — these are not tolerance comparisons.
        let np_batch = constant_u64("neuralprophet-parity-v1", "np_peyton_batch") as usize;
        let np_epochs = constant_u64("neuralprophet-parity-v1", "np_peyton_epochs") as usize;
        assert_eq!(
            auto_batch(n_train),
            np_batch,
            "auto_batch({n_train}) must reproduce NeuralProphet's own batch size"
        );
        assert_eq!(
            auto_epochs(n_train),
            np_epochs,
            "auto_epochs({n_train}) must reproduce NeuralProphet's own epoch count"
        );
        assert_eq!(
            auto_batch(n_train),
            ts["batch"].as_u64().expect("oracle batch") as usize,
            "the contract's np_peyton_batch and the oracle must agree"
        );
        assert_eq!(
            auto_epochs(n_train),
            ts["epochs"].as_u64().expect("oracle epochs") as usize,
            "the contract's np_peyton_epochs and the oracle must agree"
        );
    }

    // ---------------------------------------------------------- rung 2 ----

    #[test]
    fn lag_free_365_day_mae_within_contract() {
        let (ds, y, n_train) = peyton_split();
        let d = NpData::new(&ds, &y, n_train, 10, 0.8);
        let test_days = &ds[n_train..];
        let test_y = &y[n_train..];
        let sweep = constant_lr_sweep("lr_sweep_lag_free");

        // Selected by lowest final TRAIN loss, NEVER by test error (D-10).
        let mut best: Option<(f64, f64, Vec<f64>)> = None;
        for &lr in &sweep {
            let cfg = TrainConfig {
                n_lags: 0,
                ar_layers: vec![],
                max_lr: lr,
                epochs: None,
                batch: None,
                weight_decay: 1e-3,
                huber_beta: 0.3,
                newer_w: 2.0,
                seed: 42,
            };
            let (m, log) = train(&d, &cfg, false);
            let fl = *log.epoch_loss.last().unwrap_or(&f64::INFINITY);
            if !fl.is_finite() {
                continue;
            }
            if best.as_ref().is_none_or(|b| fl < b.0) {
                best = Some((fl, lr, predict_ts(&d, &m, test_days)));
            }
        }
        let (train_loss, selected_lr, yhat) =
            best.expect("at least one learning rate in the sweep must produce a finite loss");
        assert!(
            sweep.iter().any(|c| (c - selected_lr).abs() < 1e-12),
            "the selected lr {selected_lr} must come from the contract sweep {sweep:?}"
        );

        let e = mae(&yhat, test_y);
        let bar = equation_tolerance("neuralprophet-parity-v1", "lag_free_holdout_mae");
        let ora = load_json("np_oracle_peyton.json");
        let np_mae = oracle_f64(&ora, &["trend_seasonality", "mae_365ahead_on_test_rows"]);
        let np_rows = ora["trend_seasonality"]["n_test_rows"]
            .as_u64()
            .expect("oracle n_test_rows");
        assert!(
            e <= bar,
            "{HOLDOUT}-day-ahead holdout MAE {e:.4} exceeds the contract bar {bar} \
             (lr {selected_lr} selected by train loss {train_loss:.5}; \
             Python NeuralProphet 0.9.0 scored {np_mae:.4} over {np_rows} of these rows)"
        );
        eprintln!(
            "lag-free: lr {selected_lr} (train loss {train_loss:.5}) -> test MAE {e:.4} \
             over {HOLDOUT} rows; NeuralProphet {np_mae:.4} over {np_rows}; bar {bar}"
        );
    }

    // ---------------------------------------------------------- rung 3 ----

    #[test]
    fn ar_net_30_lags_beats_naive_one_step() {
        let (ds, y, n_train) = peyton_split();
        let d = NpData::new(&ds, &y, n_train, 10, 0.8);
        let test_days = &ds[n_train..];
        let test_y = &y[n_train..];
        let test_idx: Vec<usize> = test_days.iter().map(|&day| (day - d.t0) as usize).collect();
        let sweep = constant_lr_sweep("lr_sweep_with_lags");
        let cap = constant_u64("neuralprophet-parity-v1", "auto_epochs_cap_with_lags") as usize;

        let mut best: Option<(f64, f64, Vec<f64>)> = None;
        for &lr in &sweep {
            let cfg = TrainConfig {
                n_lags: 30,
                ar_layers: vec![32],
                max_lr: lr,
                epochs: Some(auto_epochs(n_train).min(cap)),
                batch: None,
                weight_decay: 1e-3,
                huber_beta: 0.3,
                newer_w: 2.0,
                seed: 42,
            };
            let (m, log) = train(&d, &cfg, false);
            let fl = *log.epoch_loss.last().unwrap_or(&f64::INFINITY);
            if !fl.is_finite() {
                continue;
            }
            if best.as_ref().is_none_or(|b| fl < b.0) {
                best = Some((fl, lr, predict_ar_1step(&d, &m, &test_idx)));
            }
        }
        let (train_loss, selected_lr, yhat) =
            best.expect("at least one learning rate in the sweep must produce a finite loss");

        let e = mae(&yhat, test_y);
        let margin = equation_tolerance("neuralprophet-parity-v1", "ar_net_vs_naive_margin");
        let ora = load_json("np_oracle_peyton.json");
        let naive = oracle_f64(&ora, &["naive_1step_mae_test"]);
        let np_ar = oracle_f64(&ora, &["ar30_hidden32", "mae_1step_test"]);
        assert!(
            e < naive - margin,
            "AR-Net(30 lags, hidden [32]) one-step MAE {e:.4} must beat the oracle's naive \
             baseline {naive:.4} by more than {margin} \
             (lr {selected_lr} selected by train loss {train_loss:.5}; \
             Python NeuralProphet's own AR-Net scored {np_ar:.4})"
        );
        eprintln!(
            "AR-Net: lr {selected_lr} (train loss {train_loss:.5}) -> 1-step test MAE {e:.4}; \
             naive {naive:.4}; Python NP AR-Net {np_ar:.4}"
        );
    }

    // ------------------------------------------------- D-10 invariants ----

    #[test]
    fn weighted_huber_is_graph_connected() {
        clear_graph();
        // A tiny NpData is enough: the property is about the LOSS reaching the parameters,
        // not about the fit. Core's own smooth-L1 loss fails this exact check.
        let t0 = days_from_civil(2020, 1, 1);
        let ds: Vec<i64> = (0..40).map(|i| t0 + i).collect();
        let y: Vec<f64> = (0..40).map(|i| 1.0 + f64::from(i) * 0.1).collect();
        let d = NpData::new(&ds, &y, ds.len(), 3, 0.8);
        let mut rng = Rng::new(42);
        let mut model = NpModel::new(&d, 0, &[], &mut rng);

        let b = 8usize;
        let (mut tr, mut se) = (Vec::new(), Vec::new());
        for &day in &ds[..b] {
            d.row_feats(day, &mut tr, &mut se);
        }
        let xt = Tensor::from_vec(tr, &[b, d.trend_dim()]);
        let xs = Tensor::from_vec(se, &[b, d.season_dim()]);
        let yt = Tensor::from_vec(y[..b].iter().map(|v| d.norm(*v)).collect(), &[b, 1]);
        let wt = Tensor::from_vec(vec![1.0; b], &[b, 1]);

        let pred = model.forward(&xt, &xs, None);
        let loss = weighted_huber(&pred, &yt, &wt, 0.3);
        assert!(loss.item().is_finite(), "the loss itself must be finite");
        loss.backward();

        let floor = equation_tolerance("neuralprophet-parity-v1", "huber_grad_min_abs");
        // Gradients live on the tape and are read through `get_grad(id)` — that is exactly
        // what `Optimizer::step_with_params` consults, so it is the ground truth for
        // "can this loss train anything".
        let ids: Vec<_> = model.parameters_mut().iter().map(|p| p.id()).collect();
        assert!(!ids.is_empty(), "the model must have parameters");
        for (i, id) in ids.iter().enumerate() {
            let g = get_grad(*id).unwrap_or_else(|| {
                panic!(
                    "parameter {i} has NO gradient after backward(): the loss is DETACHED from \
                     the graph (D-10). This is precisely how core's own smooth-L1 loss fails."
                )
            });
            assert!(
                g.data().iter().any(|v| v.abs() > floor as f32),
                "parameter {i}: every gradient entry is <= {floor}; the loss reaches the \
                 parameter but carries no signal"
            );
        }
        clear_graph();
    }

    #[test]
    fn auto_batch_is_mini_batch_over_grid() {
        let lo = constant_u64("neuralprophet-parity-v1", "min_rows_for_mini_batch") as usize;
        let bmin = constant_u64("neuralprophet-parity-v1", "auto_batch_min") as usize;
        let bmax = constant_u64("neuralprophet-parity-v1", "auto_batch_max") as usize;
        let mut n = lo;
        let mut checked = 0usize;
        while n <= crate::types::MAX_POINTS {
            let b = auto_batch(n);
            assert!(
                b < n,
                "auto_batch({n}) = {b} is a FULL batch; full-batch training collapses this \
                 model (measured test MAE 2.6 against 0.45)"
            );
            assert!(
                (bmin..=bmax).contains(&b),
                "auto_batch({n}) = {b} is outside the contract clamp [{bmin}, {bmax}]"
            );
            checked += 1;
            n += 97;
        }
        assert!(
            checked > 100,
            "the grid must actually cover the range; only {checked} points were checked"
        );
    }

    #[test]
    fn train_clears_the_tape() {
        clear_graph();
        let t0 = days_from_civil(2020, 1, 1);
        let ds: Vec<i64> = (0..120).map(|i| t0 + i).collect();
        let y: Vec<f64> = (0..120)
            .map(|i| 10.0 + f64::from(i) * 0.01 + (f64::from(i) / 7.0).sin())
            .collect();
        let d = NpData::new(&ds, &y, ds.len(), 10, 0.8);
        let cfg = TrainConfig {
            n_lags: 0,
            ar_layers: vec![],
            max_lr: 0.1,
            epochs: Some(3),
            batch: None,
            weight_decay: 1e-3,
            huber_beta: 0.3,
            newer_w: 2.0,
            seed: 42,
        };
        let (m, log) = train(&d, &cfg, false);
        assert!(log.steps > 0, "the fit must have taken at least one step");
        assert_eq!(
            graph_tape_len(),
            0,
            "train() must leave the thread-local tape empty (clear_graph() after EVERY step)"
        );
        // The prediction helpers must clear it too: they build a no_grad forward that still
        // allocates tape entries.
        let _ = predict_ts(&d, &m, &ds[..5]);
        assert_eq!(
            graph_tape_len(),
            0,
            "predict_ts() must leave the thread-local tape empty"
        );
    }

    #[test]
    fn full_batch_is_not_what_auto_batch_returns() {
        let (_, y, n_train) = peyton_split();
        assert!(n_train > 0 && !y.is_empty());
        let np_batch = constant_u64("neuralprophet-parity-v1", "np_peyton_batch") as usize;
        assert_eq!(
            auto_batch(n_train),
            np_batch,
            "auto_batch on the Peyton training split must be NeuralProphet's own value, not \
             the full {n_train}-row batch"
        );
        assert!(
            auto_batch(n_train) < n_train,
            "a full batch here means 80 optimiser steps instead of 3200"
        );
    }

    #[test]
    fn fourier_features_are_phased_on_1900() {
        let epoch_year = constant_u64("neuralprophet-parity-v1", "fourier_epoch_year") as i64;
        let seasons = vec![NpSeason {
            name: "weekly".into(),
            period: 7.0,
            order: 3,
        }];
        let day = parse_ymd("2020-01-01");
        let mut out = Vec::new();
        fourier_feats(day, &seasons, &mut out);
        assert_eq!(out.len(), 6, "order 3 gives 3 sin terms then 3 cos terms");

        let t = (day - days_from_civil(epoch_year, 1, 1)) as f64;
        let f = 2.0 * std::f64::consts::PI / 7.0;
        for k in 1..=3usize {
            let want = (f * k as f64 * t).sin() as f32;
            assert!(
                (out[k - 1] - want).abs() < 1e-5,
                "sin block position {}: got {} want {want} (t = day - {epoch_year}-01-01)",
                k - 1,
                out[k - 1]
            );
            let want_cos = (f * k as f64 * t).cos() as f32;
            assert!(
                (out[2 + k] - want_cos).abs() < 1e-5,
                "cos block position {}: got {} want {want_cos}",
                2 + k,
                out[2 + k]
            );
        }
        // The epoch is load-bearing: phasing on the Unix epoch instead would move the features.
        let t_unix = day as f64;
        let unix_first_sin = (f * t_unix).sin() as f32;
        assert!(
            (out[0] - unix_first_sin).abs() > 1e-3,
            "the {epoch_year} epoch must be distinguishable from the Unix epoch, otherwise \
             this test cannot detect the epoch moving"
        );
    }

    #[test]
    fn lr_selection_is_by_train_loss() {
        // The shipped door and an independent argmin over the contract's sweep must agree.
        let t0 = days_from_civil(2020, 1, 1);
        let mut rng = crate::prophet::Rng::new(7);
        let n = 120usize;
        let ds: Vec<i64> = (0..n as i64).map(|i| t0 + i).collect();
        let y: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64;
                10.0 + 0.01 * t + (2.0 * std::f64::consts::PI * t / 7.0).sin() + 0.05 * rng.normal()
            })
            .collect();

        let args = crate::types::ForecastArgs {
            ds: ds.iter().map(|d| crate::dates::format_ymd(*d)).collect(),
            y: y.clone(),
            horizon: 7,
            freq: None,
            model: Some("neuralprophet".into()),
            growth: None,
            cap: None,
            seasonality_mode: None,
            interval_width: None,
            holidays: None,
            n_lags: None,
            seed: None,
        };
        let r = crate::forecast::forecast(&args).expect("the neuralprophet arm must dispatch");
        let reported_lr = r.diagnostics["selected_lr"]
            .as_f64()
            .expect("diagnostics.selected_lr");
        let reported_loss = r.diagnostics["final_train_loss"]
            .as_f64()
            .expect("diagnostics.final_train_loss");

        let d = NpData::new(&ds, &y, n, 10, 0.8);
        let sweep = constant_lr_sweep("lr_sweep_lag_free");
        let mut argmin: Option<(f64, f64)> = None;
        for &lr in &sweep {
            let cfg = TrainConfig {
                n_lags: 0,
                ar_layers: vec![],
                max_lr: lr,
                epochs: None,
                batch: None,
                weight_decay: 1e-3,
                huber_beta: 0.3,
                newer_w: 2.0,
                seed: 42,
            };
            let (_, log) = train(&d, &cfg, false);
            let fl = *log.epoch_loss.last().unwrap_or(&f64::INFINITY);
            if fl.is_finite() && argmin.as_ref().is_none_or(|a| fl < a.0) {
                argmin = Some((fl, lr));
            }
        }
        let (best_loss, best_lr) = argmin.expect("the sweep must produce a finite loss");
        assert!(
            (reported_lr - best_lr).abs() < 1e-12,
            "the door reported lr {reported_lr} but the lowest FINAL TRAIN LOSS over the \
             contract sweep {sweep:?} is at lr {best_lr} (loss {best_loss:.6}). Selection must \
             be by train loss, never by test error (D-10)."
        );
        assert!(
            (reported_loss - best_loss).abs() < 1e-9,
            "the door reported final_train_loss {reported_loss:.6} but the independent argmin \
             is {best_loss:.6}"
        );
    }
}

// ------------------------------------------- the C-08 wall harness (ignored) ----

/// Wall-clock harness for cost axis **C-08**, the NeuralProphet training path — the one
/// axis in this door with no budget of ANY kind on it. `fit::FIT_BUDGET_SECS` is read at
/// exactly one place, inside `fit::fit_prophet`, and the `"neuralprophet"` arm never enters
/// it; the door then drives 2 or 3 full [`train`] calls per request through its
/// learning-rate sweep.
///
/// `#[ignore]`d, because it is a MEASUREMENT and not an assertion about correctness. Run it
/// deliberately, on a RELEASE build, one mode per invocation:
///
/// ```text
/// NP_WALL_MODE=structural_max cargo test --release -p aprender-forecast --lib \
///     np_train_wall -- --ignored --nocapture
/// ```
///
/// It drives the DOOR (`crate::forecast::forecast`), not [`train`] directly, so its
/// `outcome=` field can report `refused` once a bound exists. That field is load-bearing:
/// after a bound is added the worst legal request is REFUSED rather than slow, and the
/// post-condition gate has to be able to tell those two apart from the same line.
#[cfg(test)]
mod wall {
    use crate::types::{ForecastArgs, MAX_HORIZON, MAX_POINTS};

    /// One machine-parsable line per run. `profile=` is derived from `cfg!(debug_assertions)`
    /// rather than from intent (CLAUDE.md rule 2): a debug wall labelled `release` is exactly
    /// the class of error that turns a measurement into a confident wrong answer.
    fn emit(mode: &str, args: &ForecastArgs, span_days: i64, cost: u64, outcome: &str, secs: f64) {
        println!(
            "NP TRAIN WALL: mode={mode} points={} span_days={span_days} n_lags={} horizon={} \
             train_cost={cost} outcome={outcome} total_s={secs:.3} profile={}",
            args.ds.len(),
            args.n_lags.unwrap_or(0),
            args.horizon,
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
        );
    }

    /// A daily series with a trend and a weekly term, one point per day from 2020-01-01, so
    /// `span_days == points` and `n_train_grid` is at its ceiling when `points == MAX_POINTS`.
    fn contiguous_daily(n: usize) -> (Vec<String>, Vec<f64>) {
        let t0 = crate::dates::days_from_civil(2020, 1, 1);
        let mut ds = Vec::with_capacity(n);
        let mut y = Vec::with_capacity(n);
        for i in 0..n {
            ds.push(crate::dates::format_ymd(t0 + i as i64));
            let t = i as f64;
            y.push(10.0 + 0.001 * t + (2.0 * std::f64::consts::PI * t / 7.0).sin());
        }
        (ds, y)
    }

    #[test]
    #[ignore = "wall-clock measurement; run with NP_WALL_MODE=... --release -- --ignored"]
    fn np_train_wall() {
        let mode = std::env::var("NP_WALL_MODE").unwrap_or_else(|_| "mid_range".into());
        // freq "D" is the ONLY frequency this arm accepts, so every mode uses it.
        let (points, n_lags, horizon) = match mode.as_str() {
            // The worst LEGAL request: MAX_POINTS contiguous daily points (so span_days is
            // also at MAX_SPAN_DAYS and n_train_grid is at its ceiling), n_lags at its own
            // ceiling of 365, horizon at MAX_HORIZON.
            "structural_max" => (MAX_POINTS, 365usize, MAX_HORIZON),
            // A point in the middle of the legal range, so a single number is not the whole
            // basis (CLAUDE.md rule 6 — one input is an anecdote).
            "mid_range" => (2_000usize, 30usize, 365usize),
            // The OTHER arm: n_lags == 0 takes a THREE-point learning-rate sweep instead of
            // two, trains on the observed rows only, and lets `train` pick the epochs.
            "lag_free" => (MAX_POINTS, 0usize, MAX_HORIZON),
            // THREE compositions sitting just under `MAX_NP_TRAIN_COST`, used to DERIVE that
            // value rather than to assert it. They differ in every factor the proxy
            // multiplies — history length, n_lags and horizon — so a value that clears the
            // 2 s bar on all three is not clearing it on one shape (CLAUDE.md rule 6).
            // A first pass at a candidate of 20 000 000 measured 2.089 s on the long-history
            // composition — OVER the bar — which is what moved the value down to 15 000 000.
            // The REJECTED candidate, kept as a named mode so its rejection is reproducible
            // rather than only quoted: at a bound of 20 000 000 this composition prices at
            // 19 991 000 and measured 2.089 s — OVER SC1's 2 s bar. At the SHIPPED bound of
            // 15 000 000 it is refused, so reproducing the 2.089 s means raising
            // `MAX_NP_TRAIN_COST` and `constants.fit_max_np_train_cost` to 20 000 000 first.
            "rejected_candidate_20m" => (MAX_POINTS, 9usize, MAX_HORIZON),
            "at_bound_long_history" => (MAX_POINTS, 6usize, MAX_HORIZON),
            "at_bound_mid_history" => (10_000usize, 11usize, MAX_HORIZON),
            "at_bound_short_history" => (2_000usize, 41usize, 365usize),
            other => panic!(
                "NP_WALL_MODE={other:?}: want structural_max, mid_range, lag_free, \
                 at_bound_long_history, at_bound_mid_history, at_bound_short_history or \
                 rejected_candidate_20m"
            ),
        };
        let (ds, y) = contiguous_daily(points);
        let span_days = points as i64;
        let mut args = ForecastArgs {
            ds,
            y,
            horizon,
            ..ForecastArgs::default()
        };
        args.model = Some("neuralprophet".into());
        args.freq = Some("D".into());
        if n_lags > 0 {
            args.n_lags = Some(n_lags);
        }

        // The door's own pricing of this request, computed the way the door computes it, so
        // the line reports the number the bound is (or would be) compared against.
        let d = super::NpData::new(
            &args
                .ds
                .iter()
                .map(|s| crate::dates::parse_date(s).expect("harness dates are well formed"))
                .collect::<Vec<_>>(),
            &args.y,
            points,
            10,
            0.8,
        );
        // The door's OWN pricing function, so this line reports the number the bound is
        // compared against rather than a second arithmetic that could drift from it.
        let cost = super::request_train_cost(&d, points, n_lags);
        drop(d);

        let t0 = std::time::Instant::now();
        let result = crate::forecast::forecast(&args);
        let secs = t0.elapsed().as_secs_f64();
        let outcome = match &result {
            Ok(_) => "accepted",
            Err(crate::types::ForecastError::Validation(_)) => "refused",
            Err(e) => panic!("the harness must produce an accept or a refusal, got {e:?}"),
        };
        emit(&mode, &args, span_days, cost, outcome, secs);
        if let Err(crate::types::ForecastError::Validation(m)) = &result {
            println!("NP TRAIN WALL REFUSAL: {m}");
        }
    }
}
