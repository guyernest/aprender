//! Prophet, second cut: linear / logistic / flat growth, additive AND multiplicative
//! terms, holiday indicator columns with windows, analytic gradients for all of it,
//! component decomposition, and the vectorised uncertainty simulation of
//! `forecaster.py` (`_make_trend_shift_matrix` → `_sample_uncertainty` →
//! `sample_model_vectorized` → percentiles).
//!
//! Ported VERBATIM from `sources/004-forecast-mcp-thin-server/src/prophet.rs` (D-08).
//! The only change is that the spike's private civil-date copies (its lines 8-36) are
//! replaced by the re-export below, so `dates.rs` is the single implementation (D-17)
//! and downstream code reaching `prophet::days_from_civil` still compiles.

pub use crate::dates::{civil_from_days, days_from_civil, format_ymd, parse_ymd};

// ----------------------------------------------------------------- spec ----
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Growth {
    Linear,
    Logistic,
    Flat,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Additive,
    Multiplicative,
}

#[derive(Clone, Debug)]
pub struct Seasonality {
    pub name: String,
    pub period: f64,
    pub order: usize,
    pub prior_scale: f64,
    pub mode: Mode,
}
#[derive(Clone, Debug)]
pub struct Holiday {
    pub name: String,
    pub days: Vec<i64>,
    pub lower_window: i64,
    pub upper_window: i64,
    pub prior_scale: f64,
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub growth: Growth,
    /// Constant capacity (logistic only), original scale.
    pub cap: Option<f64>,
    pub seasonalities: Vec<Seasonality>,
    pub holidays: Vec<Holiday>,
    pub holidays_mode: Mode,
    pub n_changepoints: usize,
    pub changepoint_range: f64,
    pub changepoint_prior_scale: f64,
    pub interval_width: f64,
    pub uncertainty_samples: usize,
}

impl Spec {
    pub fn default_linear(seasonalities: Vec<Seasonality>) -> Self {
        Spec {
            growth: Growth::Linear,
            cap: None,
            seasonalities,
            holidays: vec![],
            holidays_mode: Mode::Additive,
            n_changepoints: 25,
            changepoint_range: 0.8,
            changepoint_prior_scale: 0.05,
            interval_width: 0.8,
            uncertainty_samples: 1000,
        }
    }
}

pub fn auto_seasonalities(ds_days: &[i64], prior_scale: f64, mode: Mode) -> Vec<Seasonality> {
    let span = (ds_days[ds_days.len() - 1] - ds_days[0]) as f64;
    let min_dt = ds_days
        .windows(2)
        .map(|w| (w[1] - w[0]) as f64)
        .filter(|d| *d > 0.0)
        .fold(f64::INFINITY, f64::min);
    let mut out = Vec::new();
    if span >= 730.0 {
        out.push(Seasonality {
            name: "yearly".into(),
            period: 365.25,
            order: 10,
            prior_scale,
            mode,
        });
    }
    if span >= 14.0 && min_dt < 7.0 {
        out.push(Seasonality {
            name: "weekly".into(),
            period: 7.0,
            order: 3,
            prior_scale,
            mode,
        });
    }
    if span >= 2.0 && min_dt < 1.0 {
        out.push(Seasonality {
            name: "daily".into(),
            period: 1.0,
            order: 4,
            prior_scale,
            mode,
        });
    }
    out
}

// -------------------------------------------------------------- columns ----
/// One regressor column of X: which component it belongs to and its mode.
#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    pub component: String,
    pub mode: Mode,
    pub prior_scale: f64,
    pub holiday: Option<(usize, i64)>,
}

/// Prophet's column order: seasonalities in insertion order (sin, cos interleaved),
/// then holiday columns sorted by their `{holiday}_delim_{±offset}` name.
pub fn columns(spec: &Spec) -> Vec<Column> {
    let mut cols = Vec::new();
    for s in &spec.seasonalities {
        for i in 0..s.order {
            cols.push(Column {
                name: format!("{}_delim_{}", s.name, 2 * i + 1),
                component: s.name.clone(),
                mode: s.mode,
                prior_scale: s.prior_scale,
                holiday: None,
            });
            cols.push(Column {
                name: format!("{}_delim_{}", s.name, 2 * i + 2),
                component: s.name.clone(),
                mode: s.mode,
                prior_scale: s.prior_scale,
                holiday: None,
            });
        }
    }
    let mut hcols = Vec::new();
    for (hi, h) in spec.holidays.iter().enumerate() {
        for off in h.lower_window..=h.upper_window {
            hcols.push(Column {
                name: format!(
                    "{}_delim_{}{}",
                    h.name,
                    if off >= 0 { '+' } else { '-' },
                    off.abs()
                ),
                component: h.name.clone(),
                mode: spec.holidays_mode,
                prior_scale: h.prior_scale,
                holiday: Some((hi, off)),
            });
        }
    }
    hcols.sort_by(|a, b| a.name.cmp(&b.name));
    cols.extend(hcols);
    cols
}

/// One membership set per holiday, in `spec.holidays` order, built ONCE per design or
/// prediction rather than rescanned per row per column.
///
/// `feature_row` used to answer "is `day` in this holiday's window offset `off`?" with a
/// linear `.any()` over `days`, i.e. `rows x holiday_columns x dates` comparisons per
/// design build. The set answers the IDENTICAL predicate in O(1): `d + off == day` iff
/// `d == day - off`.
#[must_use]
pub fn holiday_day_sets(spec: &Spec) -> Vec<std::collections::HashSet<i64>> {
    spec.holidays
        .iter()
        .map(|h| h.days.iter().copied().collect())
        .collect()
}

/// `hol_sets` must be the [`holiday_day_sets`] `HashSet` slice for the same `spec` — one
/// set per holiday, in order. Passing it in rather than rebuilding it is the whole point:
/// the caller hoists the construction out of the row loop.
pub fn feature_row(
    day: i64,
    spec: &Spec,
    cols: &[Column],
    hol_sets: &[std::collections::HashSet<i64>],
    out: &mut Vec<f64>,
) {
    let x_t = std::f64::consts::PI * 2.0 * day as f64;
    for s in &spec.seasonalities {
        for i in 0..s.order {
            let c = (i + 1) as f64 / s.period * x_t;
            out.push(c.sin());
            out.push(c.cos());
        }
    }
    for c in cols.iter().filter(|c| c.holiday.is_some()) {
        let (hi, off) = c.holiday.expect("holiday col");
        // Same predicate as the former `days.iter().any(|&d| d + off == day)`, rearranged.
        let hit = hol_sets[hi].contains(&(day - off));
        out.push(if hit { 1.0 } else { 0.0 });
    }
}

// --------------------------------------------------------------- design ----
#[derive(Clone, Debug)]
pub struct Design {
    pub spec: Spec,
    pub cols: Vec<Column>,
    pub t: Vec<f64>,
    pub y_scaled: Vec<f64>,
    pub cap_scaled: Option<Vec<f64>>,
    pub y_scale: f64,
    pub start_days: i64,
    pub t_scale_days: f64,
    pub changepoints_t: Vec<f64>,
    /// T×K row-major.
    pub x: Vec<f64>,
    pub k: usize,
    pub s_a: Vec<f64>,
    pub s_m: Vec<f64>,
    pub prior_scales: Vec<f64>,
}

fn linspace_round(stop: f64, num: usize) -> Vec<usize> {
    let step = stop / (num as f64 - 1.0);
    (0..num)
        .map(|i| {
            let v = if i + 1 == num { stop } else { step * i as f64 };
            v.round_ties_even() as usize
        })
        .collect()
}

pub fn make_design(ds_days: &[i64], y: &[f64], spec: &Spec) -> Design {
    let n = y.len();
    assert!(n >= 2 && ds_days.windows(2).all(|w| w[0] < w[1]));
    let start_days = ds_days[0];
    let t_scale_days = (ds_days[n - 1] - start_days) as f64;
    let t: Vec<f64> = ds_days
        .iter()
        .map(|&d| (d - start_days) as f64 / t_scale_days)
        .collect();
    let mut y_scale = y.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
    if y_scale == 0.0 {
        y_scale = 1.0;
    }
    let y_scaled: Vec<f64> = y.iter().map(|v| v / y_scale).collect();
    let cap_scaled = match (spec.growth, spec.cap) {
        (Growth::Logistic, Some(c)) => Some(vec![c / y_scale; n]),
        (Growth::Logistic, None) => panic!("logistic growth needs cap"),
        _ => None,
    };
    let hist_size = (n as f64 * spec.changepoint_range).floor() as usize;
    let mut n_cp = spec.n_changepoints;
    if n_cp + 1 > hist_size {
        n_cp = hist_size.saturating_sub(1);
    }
    let changepoints_t: Vec<f64> = if n_cp > 0 {
        let idx = linspace_round((hist_size - 1) as f64, n_cp + 1);
        idx[1..].iter().map(|&i| t[i]).collect()
    } else {
        vec![0.0]
    };
    let cols = columns(spec);
    let k = cols.len();
    let mut x = Vec::with_capacity(n * k);
    let hol_sets = holiday_day_sets(spec);
    for &d in ds_days {
        feature_row(d, spec, &cols, &hol_sets, &mut x);
    }
    let s_a: Vec<f64> = cols
        .iter()
        .map(|c| if c.mode == Mode::Additive { 1.0 } else { 0.0 })
        .collect();
    let s_m: Vec<f64> = cols
        .iter()
        .map(|c| {
            if c.mode == Mode::Multiplicative {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    let prior_scales = cols.iter().map(|c| c.prior_scale).collect();
    Design {
        spec: spec.clone(),
        cols,
        t,
        y_scaled,
        cap_scaled,
        y_scale,
        start_days,
        t_scale_days,
        changepoints_t,
        x,
        k,
        s_a,
        s_m,
        prior_scales,
    }
}

// ---------------------------------------------------------------- model ----
#[derive(Clone, Debug)]
pub struct Params {
    pub k: f64,
    pub m: f64,
    pub delta: Vec<f64>,
    pub beta: Vec<f64>,
    pub sigma_obs: f64,
}

pub struct Model<'a> {
    pub d: &'a Design,
    pub scale: f64,
    pub guard: bool,
}

/// Piecewise-linear trend for sorted `t`.
pub fn piecewise_linear(t: &[f64], cps: &[f64], delta: &[f64], k: f64, m: f64) -> Vec<f64> {
    let (mut out, mut j, mut k_t, mut m_t) = (Vec::with_capacity(t.len()), 0, k, m);
    for &ti in t {
        while j < cps.len() && cps[j] <= ti {
            k_t += delta[j];
            m_t -= cps[j] * delta[j];
            j += 1;
        }
        out.push(k_t * ti + m_t);
    }
    out
}

/// Stan's `logistic_gamma`: continuity offsets for the logistic trend.
pub fn logistic_gammas(k: f64, m: f64, delta: &[f64], cps: &[f64]) -> Vec<f64> {
    let s = cps.len();
    let mut k_s = Vec::with_capacity(s + 1);
    k_s.push(k);
    for j in 0..s {
        k_s.push(k_s[j] + delta[j]);
    }
    let mut gamma = vec![0.0; s];
    let mut m_pr = m;
    for i in 0..s {
        gamma[i] = (cps[i] - m_pr) * (1.0 - k_s[i] / k_s[i + 1]);
        m_pr += gamma[i];
    }
    gamma
}

pub fn piecewise_logistic(
    t: &[f64],
    cap: &[f64],
    cps: &[f64],
    delta: &[f64],
    k: f64,
    m: f64,
) -> Vec<f64> {
    let gamma = logistic_gammas(k, m, delta, cps);
    let (mut out, mut j, mut k_t, mut m_t) = (Vec::with_capacity(t.len()), 0, k, m);
    for (i, &ti) in t.iter().enumerate() {
        while j < cps.len() && cps[j] <= ti {
            k_t += delta[j];
            m_t += gamma[j];
            j += 1;
        }
        out.push(cap[i] / (1.0 + (-k_t * (ti - m_t)).exp()));
    }
    out
}

impl<'a> Model<'a> {
    pub fn new(d: &'a Design) -> Self {
        Model {
            d,
            scale: 1.0 / d.t.len() as f64,
            guard: true,
        }
    }
    pub fn n_params(&self) -> usize {
        2 + self.d.changepoints_t.len() + self.d.k + 1
    }
    pub fn pack(&self, p: &Params) -> Vec<f64> {
        let mut v = vec![p.k, p.m];
        v.extend_from_slice(&p.delta);
        v.extend_from_slice(&p.beta);
        v.push(p.sigma_obs.ln());
        v
    }
    pub fn unpack(&self, th: &[f64]) -> Params {
        let (s, k) = (self.d.changepoints_t.len(), self.d.k);
        Params {
            k: th[0],
            m: th[1],
            delta: th[2..2 + s].to_vec(),
            beta: th[2 + s..2 + s + k].to_vec(),
            sigma_obs: th[2 + s + k].exp(),
        }
    }

    /// Prophet's `*_growth_init`.
    pub fn init(&self) -> Params {
        let d = self.d;
        let n = d.t.len();
        let (k, m) = match d.spec.growth {
            Growth::Linear => {
                let tt = d.t[n - 1] - d.t[0];
                let k = (d.y_scaled[n - 1] - d.y_scaled[0]) / tt;
                (k, d.y_scaled[0] - k * d.t[0])
            }
            Growth::Flat => (0.0, d.y_scaled.iter().sum::<f64>() / n as f64),
            Growth::Logistic => {
                let cap = d.cap_scaled.as_ref().expect("cap");
                let tt = d.t[n - 1] - d.t[0];
                let (c0, c1) = (cap[0], cap[n - 1]);
                let y0 = (0.01 * c0).max((0.99 * c0).min(d.y_scaled[0]));
                let y1 = (0.01 * c1).max((0.99 * c1).min(d.y_scaled[n - 1]));
                let (mut r0, r1) = (c0 / y0, c1 / y1);
                if (r0 - r1).abs() <= 0.01 {
                    r0 *= 1.05;
                }
                let (l0, l1) = ((r0 - 1.0).ln(), (r1 - 1.0).ln());
                ((l0 - l1) / tt, l0 * tt / (l0 - l1))
            }
        };
        Params {
            k,
            m,
            delta: vec![0.0; d.changepoints_t.len()],
            beta: vec![0.0; d.k],
            sigma_obs: 1.0,
        }
    }

    pub fn trend(&self, p: &Params, t: &[f64], cap: Option<&[f64]>) -> Vec<f64> {
        let d = self.d;
        match d.spec.growth {
            Growth::Linear => piecewise_linear(t, &d.changepoints_t, &p.delta, p.k, p.m),
            Growth::Flat => vec![p.m; t.len()],
            Growth::Logistic => {
                piecewise_logistic(t, cap.expect("cap"), &d.changepoints_t, &p.delta, p.k, p.m)
            }
        }
    }

    /// μ = trend·(1 + X·(β∘s_m)) + X·(β∘s_a); returns (trend, xm = X·(β∘s_m), residuals).
    fn parts(&self, p: &Params) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        let d = self.d;
        let n = d.t.len();
        let trend = self.trend(p, &d.t, d.cap_scaled.as_deref());
        let (mut xm, mut r) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for i in 0..n {
            let row = &d.x[i * d.k..(i + 1) * d.k];
            let (mut a, mut m) = (0.0, 0.0);
            for c in 0..d.k {
                let v = row[c] * p.beta[c];
                a += v * d.s_a[c];
                m += v * d.s_m[c];
            }
            xm.push(m);
            r.push(d.y_scaled[i] - (trend[i] * (1.0 + m) + a));
        }
        (trend, xm, r)
    }

    pub fn objective(&self, th: &[f64]) -> f64 {
        let p = self.unpack(th);
        let (_, _, r) = self.parts(&p);
        self.objective_from(&p, &r)
    }

    fn objective_from(&self, p: &Params, r: &[f64]) -> f64 {
        let d = self.d;
        let (n, s) = (d.t.len() as f64, p.sigma_obs);
        let mut f = p.k * p.k / 50.0 + p.m * p.m / 50.0;
        f += p.delta.iter().map(|v| v.abs()).sum::<f64>() / d.spec.changepoint_prior_scale;
        f += 2.0 * s * s;
        f += p
            .beta
            .iter()
            .zip(&d.prior_scales)
            .map(|(b, sc)| b * b / (2.0 * sc * sc))
            .sum::<f64>();
        f += n * s.ln() + r.iter().map(|v| v * v).sum::<f64>() / (2.0 * s * s);
        let f = f * self.scale;
        if self.guard && !f.is_finite() {
            return 1e300;
        }
        f
    }

    pub fn gradient(&self, th: &[f64]) -> Vec<f64> {
        let p = self.unpack(th);
        let (trend, xm, r) = self.parts(&p);
        self.gradient_from(th.len(), &p, &trend, &xm, &r)
    }

    /// Objective and gradient from ONE pass through the model.
    pub fn value_and_grad(&self, th: &[f64]) -> (f64, Vec<f64>) {
        let p = self.unpack(th);
        let (trend, xm, r) = self.parts(&p);
        (
            self.objective_from(&p, &r),
            self.gradient_from(th.len(), &p, &trend, &xm, &r),
        )
    }

    fn gradient_from(
        &self,
        n_theta: usize,
        p: &Params,
        trend: &[f64],
        xm: &[f64],
        r: &[f64],
    ) -> Vec<f64> {
        let d = self.d;
        let n = d.t.len();
        let s = p.sigma_obs;
        let inv_s2 = 1.0 / (s * s);
        let ncp = d.changepoints_t.len();
        let mut g = vec![0.0; n_theta];
        // dL/dtrend_i = -r_i (1 + xm_i) / σ²   (L = negative log posterior)
        let tbar: Vec<f64> = (0..n).map(|i| -r[i] * (1.0 + xm[i]) * inv_s2).collect();
        // trend parameters
        match d.spec.growth {
            Growth::Linear => {
                g[0] += tbar.iter().zip(&d.t).map(|(a, b)| a * b).sum::<f64>();
                g[1] += tbar.iter().sum::<f64>();
                let (mut st, mut s0, mut i) = (0.0, 0.0, n);
                for j in (0..ncp).rev() {
                    while i > 0 && d.t[i - 1] >= d.changepoints_t[j] {
                        i -= 1;
                        st += tbar[i] * d.t[i];
                        s0 += tbar[i];
                    }
                    g[2 + j] += st - d.changepoints_t[j] * s0;
                }
            }
            Growth::Flat => {
                g[1] += tbar.iter().sum::<f64>();
            }
            Growth::Logistic => {
                let cap = d.cap_scaled.as_ref().expect("cap");
                let cps = &d.changepoints_t;
                let gamma = logistic_gammas(p.k, p.m, &p.delta, cps);
                let mut k_s = vec![p.k];
                for j in 0..ncp {
                    k_s.push(k_s[j] + p.delta[j]);
                }
                // forward per-point k_t, m_t and adjoints w.r.t. k_t (kt_bar) and m_t (mt_bar)
                let (mut j, mut k_t, mut m_t) = (0usize, p.k, p.m);
                let (mut kbar_seg, mut mbar_seg) = (vec![0.0; ncp + 1], vec![0.0; ncp + 1]); // accumulated per segment index
                for i in 0..n {
                    while j < ncp && cps[j] <= d.t[i] {
                        k_t += p.delta[j];
                        m_t += gamma[j];
                        j += 1;
                    }
                    let z = k_t * (d.t[i] - m_t);
                    let sig = 1.0 / (1.0 + (-z).exp());
                    let dsig = cap[i] * sig * (1.0 - sig);
                    kbar_seg[j] += tbar[i] * dsig * (d.t[i] - m_t);
                    mbar_seg[j] += tbar[i] * dsig * (-k_t);
                }
                // k_t in segment j = k + Σ_{l<j} δ_l ; m_t in segment j = m + Σ_{l<j} γ_l
                let mut suffix_k = 0.0;
                let mut suffix_m = 0.0;
                let mut gbar = vec![0.0; ncp]; // adjoint of γ_l from m_t
                let mut dbar = vec![0.0; ncp];
                for seg in (0..=ncp).rev() {
                    suffix_k += kbar_seg[seg];
                    suffix_m += mbar_seg[seg];
                    if seg > 0 {
                        dbar[seg - 1] += suffix_k;
                        gbar[seg - 1] += suffix_m;
                    }
                }
                g[0] += suffix_k; // direct k
                g[1] += suffix_m; // direct m
                                  // reverse through γ_j = a_j b_j, a_j = cp_j − m − Σ_{i<j} γ_i, b_j = 1 − k_s[j]/k_s[j+1]
                let mut ksbar = vec![0.0; ncp + 1];
                let mut acc = 0.0; // Σ_{l>j} γ̄_l b_l
                let mut gsum = 0.0; // Σ_{i<j} γ_i, computed forward; store prefix sums
                let mut prefix = vec![0.0; ncp];
                for j in 0..ncp {
                    prefix[j] = gsum;
                    gsum += gamma[j];
                }
                for j in (0..ncp).rev() {
                    let b = 1.0 - k_s[j] / k_s[j + 1];
                    let a = cps[j] - p.m - prefix[j];
                    let gb = gbar[j] - acc; // total adjoint of γ_j (later a_l depend on γ_j with −1)
                    g[1] += -gb * b; // ∂a_j/∂m = −1
                    ksbar[j] += gb * a * (-1.0 / k_s[j + 1]);
                    ksbar[j + 1] += gb * a * (k_s[j] / (k_s[j + 1] * k_s[j + 1]));
                    acc += gb * b;
                }
                // k_s[j] = k + Σ_{l<j} δ_l
                let mut suf = 0.0;
                for j in (0..=ncp).rev() {
                    suf += ksbar[j];
                    if j > 0 {
                        dbar[j - 1] += suf;
                    }
                }
                g[0] += suf;
                for j in 0..ncp {
                    g[2 + j] += dbar[j];
                }
            }
        }
        // priors on k, m, δ
        g[0] += p.k / 25.0;
        g[1] += p.m / 25.0;
        for j in 0..ncp {
            g[2 + j] += p.delta[j].signum() * if p.delta[j] == 0.0 { 0.0 } else { 1.0 }
                / d.spec.changepoint_prior_scale;
        }
        // β: dμ_i/dβ_c = X_ic (trend_i s_m,c + s_a,c)
        let off = 2 + ncp;
        // one row-major pass over X: acc_c = Σ_i −r_i X_ic (trend_i s_m,c + s_a,c)
        let mut acc = vec![0.0; d.k];
        for i in 0..n {
            let row = &d.x[i * d.k..(i + 1) * d.k];
            let (ri, ti) = (-r[i], trend[i]);
            for c in 0..d.k {
                acc[c] += ri * row[c] * (ti * d.s_m[c] + d.s_a[c]);
            }
        }
        for c in 0..d.k {
            g[off + c] = p.beta[c] / (d.prior_scales[c] * d.prior_scales[c]) + acc[c] * inv_s2;
        }
        let ss: f64 = r.iter().map(|v| v * v).sum();
        g[off + d.k] = 4.0 * s * s + n as f64 - ss * inv_s2;
        for v in g.iter_mut() {
            *v *= self.scale;
        }
        if self.guard && g.iter().any(|v| !v.is_finite()) {
            return vec![0.0; n_theta];
        }
        g
    }
}

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
        let u1 = self.uniform().max(1e-300);
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
    pub fn laplace(&mut self, b: f64) -> f64 {
        let u = self.uniform() - 0.5;
        -b * u.signum() * (1.0 - 2.0 * u.abs()).ln()
    }
}

/// Above this mean the draw switches from Knuth's product method to a normal approximation.
///
/// The value the `06-VERIFICATION.md` gap-3 report named, and it is far above the regime the
/// parity ladder reaches: `wp_log_R_logistic` is the ONLY logistic fixture and its lambda is
/// **3.1239** (25 changepoints x (t_max 1.124957 - 1), measured by
/// `sampler::wp_log_r_logistic_fixture_lambda_is_far_below_the_branch_threshold`), so every parity
/// rung takes the unchanged Knuth path and no rung can move because of this branch.
pub const POISSON_NORMAL_BRANCH_LAMBDA: f64 = 30.0;

/// Draw a Poisson count with mean `lambda`.
///
/// Extracted from `predict`'s `Growth::Logistic` uncertainty arm so its numerical domain can be
/// asserted directly — the only path to it before was a full logistic forecast.
///
/// # Why two branches
///
/// Knuth's product method compares a running product of uniforms against `l = (-lambda).exp()`,
/// and that is **exactly 0.0 for lambda > 745.13**. Past that point the loop can only end when the
/// product itself underflows through f64's subnormals, which takes ~745 steps on average
/// (E[-ln U] = 1, and the smallest positive subnormal is near e^-744) — so the count SATURATES
/// near 745 whatever lambda is. That regime is reachable through the door: 100 daily points with
/// `horizon: 3650, growth: "logistic"` gives lambda ~= 922, and the 10-point `MIN_POINTS` floor
/// with the same horizon gives ~2839. Measured pre-fix means: 745.13 at lambda 900 and 745.45 at
/// lambda 2839.
///
/// # Why a normal approximation and not a PTRS port
///
/// A transformed-rejection port of numpy's `np.random.poisson` would buy a draw-for-draw match
/// this sampler never had: the stream here is this file's own seeded xorshift `Rng`, not MT19937.
/// The bar this draw feeds is `band_width_rel` — a RELATIVE bar on mean band width, stated in
/// `contracts/prophet-parity-v1.yaml` as a Monte-Carlo estimate — and a band-width estimate
/// depends on the count's MEAN and VARIANCE, both of which are exactly lambda for
/// `lambda + sqrt(lambda) * N(0, 1)`. The approximation is standard above lambda ~= 30, where the
/// Poisson's skew (1/sqrt(lambda) <= 0.18) is already small.
pub fn poisson(rng: &mut Rng, lambda: f64) -> usize {
    if lambda > POISSON_NORMAL_BRANCH_LAMBDA {
        // Mean and variance are both exactly lambda; clamped at zero because a normal draw is
        // unbounded below while a count is not (at the threshold that is a 5.5-sigma event).
        return (lambda + lambda.sqrt() * rng.normal()).round().max(0.0) as usize;
    }
    // Poisson via Knuth
    let mut n_changes = 0usize;
    let mut pp = 1.0;
    let l = (-lambda).exp();
    loop {
        pp *= rng.uniform();
        if pp <= l {
            break;
        }
        n_changes += 1;
    }
    n_changes
}

/// numpy's default (linear) percentile.
pub fn percentile(sorted: &[f64], q: f64) -> f64 {
    let n = sorted.len();
    let pos = q / 100.0 * (n - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    sorted[lo] + (pos - lo as f64) * (sorted[hi] - sorted[lo])
}

// ----------------------------------------------------------- prediction ----
pub struct Forecast {
    pub ds_days: Vec<i64>,
    pub trend: Vec<f64>,
    pub yhat: Vec<f64>,
    pub yhat_lower: Vec<f64>,
    pub yhat_upper: Vec<f64>,
    pub trend_lower: Vec<f64>,
    pub trend_upper: Vec<f64>,
    /// Named components on the original scale (additive) or as fractions (multiplicative),
    /// including `additive_terms`, `multiplicative_terms`, `holidays`.
    pub components: Vec<(String, Vec<f64>)>,
}

pub fn predict(d: &Design, p: &Params, ds_days: &[i64], seed: u64) -> Forecast {
    let spec = &d.spec;
    let n = ds_days.len();
    let t: Vec<f64> = ds_days
        .iter()
        .map(|&x| (x - d.start_days) as f64 / d.t_scale_days)
        .collect();
    let cap: Option<Vec<f64>> = d.cap_scaled.as_ref().map(|c| vec![c[0]; n]);
    let model = Model::new(d);
    let trend_s = model.trend(p, &t, cap.as_deref());
    let mut x = Vec::with_capacity(n * d.k);
    let hol_sets = holiday_day_sets(spec);
    for &day in ds_days {
        feature_row(day, spec, &d.cols, &hol_sets, &mut x);
    }
    // components
    let mut names: Vec<String> = Vec::new();
    for c in &d.cols {
        if !names.contains(&c.component) {
            names.push(c.component.clone());
        }
    }
    let comp_of = |sel: &dyn Fn(&Column) -> bool, additive: bool| -> Vec<f64> {
        (0..n)
            .map(|i| {
                let mut v = 0.0;
                for c in 0..d.k {
                    if sel(&d.cols[c]) {
                        v += x[i * d.k + c] * p.beta[c];
                    }
                }
                if additive {
                    v * d.y_scale
                } else {
                    v
                }
            })
            .collect()
    };
    let mut components: Vec<(String, Vec<f64>)> = Vec::new();
    for nm in &names {
        let mode = d
            .cols
            .iter()
            .find(|c| &c.component == nm)
            .expect("col")
            .mode;
        let v = comp_of(&|c: &Column| &c.component == nm, mode == Mode::Additive);
        components.push((nm.clone(), v));
    }
    let add_terms = comp_of(&|c: &Column| c.mode == Mode::Additive, true);
    let mul_terms = comp_of(&|c: &Column| c.mode == Mode::Multiplicative, false);
    if !spec.holidays.is_empty() {
        let hol = comp_of(
            &|c: &Column| c.holiday.is_some(),
            spec.holidays_mode == Mode::Additive,
        );
        components.push(("holidays".into(), hol));
    }
    components.push(("additive_terms".into(), add_terms.clone()));
    components.push(("multiplicative_terms".into(), mul_terms.clone()));
    let trend: Vec<f64> = trend_s.iter().map(|v| v * d.y_scale).collect();
    let yhat: Vec<f64> = (0..n)
        .map(|i| trend[i] * (1.0 + mul_terms[i]) + add_terms[i])
        .collect();

    // ---- uncertainty: Prophet 1.4 vectorised path ----
    let ns = spec.uncertainty_samples;
    let mut rng = Rng::new(seed);
    let n_future = t.iter().filter(|&&v| v > 1.0).count();
    let n_past = n - n_future;
    // uncertainties[s][i] on the scaled trend
    let mut unc = vec![vec![0.0; n]; ns];
    if n_future > 0 {
        let future_t: Vec<f64> = t.iter().cloned().filter(|&v| v > 1.0).collect();
        let single_diff = if n_future > 1 {
            (future_t[n_future - 1] - future_t[0]) / (n_future - 1) as f64
        } else {
            (d.t[d.t.len() - 1] - d.t[0]) / (d.t.len() - 1) as f64
        };
        let likelihood = d.changepoints_t.len() as f64 * single_diff;
        let mean_delta = p.delta.iter().map(|v| v.abs()).sum::<f64>() / p.delta.len() as f64 + 1e-8;
        match spec.growth {
            Growth::Linear => {
                for row in unc.iter_mut() {
                    // _make_trend_shift_matrix: laplace shifts where U < likelihood, averaged with the previous column
                    let mut mat: Vec<f64> = (0..n_future)
                        .map(|_| {
                            let hit = rng.uniform() < likelihood;
                            let v = rng.laplace(mean_delta);
                            if hit {
                                v
                            } else {
                                0.0
                            }
                        })
                        .collect();
                    for i in (0..n_future).rev() {
                        let prev = if i == 0 { 0.0 } else { mat[i - 1] };
                        mat[i] = (prev + mat[i]) / 2.0;
                    }
                    let mut c1 = 0.0;
                    let mut c2 = 0.0;
                    for i in 0..n_future {
                        c1 += mat[i];
                        c2 += c1;
                        row[n_past + i] = c2 * single_diff;
                    }
                }
            }
            Growth::Flat => {}
            Growth::Logistic => {
                // Prophet's pre-vectorised algorithm (sample_predictive_trend): Poisson-many new
                // changepoints on (1, T], Laplace deltas, full piecewise_logistic re-evaluation.
                let cap_s = cap.as_ref().expect("cap");
                let t_max = t[n - 1];
                let s_cnt = d.changepoints_t.len() as f64;
                let mean_trend = &trend_s;
                for row in unc.iter_mut() {
                    let lambda = s_cnt * (t_max - 1.0);
                    let n_changes = poisson(&mut rng, lambda);
                    let mut cps: Vec<f64> = d.changepoints_t.clone();
                    let mut deltas: Vec<f64> = p.delta.clone();
                    let mut new: Vec<(f64, f64)> = (0..n_changes)
                        .map(|_| (1.0 + rng.uniform() * (t_max - 1.0), rng.laplace(mean_delta)))
                        .collect();
                    new.sort_by(|a, b| a.0.partial_cmp(&b.0).expect("f"));
                    for (c, dl) in new {
                        cps.push(c);
                        deltas.push(dl);
                    }
                    let tr = piecewise_logistic(&t, cap_s, &cps, &deltas, p.k, p.m);
                    for i in n_past..n {
                        row[i] = tr[i] - mean_trend[i];
                    }
                }
            }
        }
    }
    // sample_model_vectorized: yhat = (trend + unc)·y_scale·(1 + Xb_m) + Xb_a + N(0, σ)·y_scale
    let (lo_p, hi_p) = (
        100.0 * (1.0 - spec.interval_width) / 2.0,
        100.0 * (1.0 + spec.interval_width) / 2.0,
    );
    let (mut yl, mut yu, mut tl, mut tu) = (
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    );
    let mut col_y = vec![0.0; ns];
    let mut col_t = vec![0.0; ns];
    for i in 0..n {
        for s in 0..ns {
            let tr = (trend_s[i] + unc[s][i]) * d.y_scale;
            col_t[s] = tr;
            col_y[s] =
                tr * (1.0 + mul_terms[i]) + add_terms[i] + rng.normal() * p.sigma_obs * d.y_scale;
        }
        col_y.sort_by(|a, b| a.partial_cmp(b).expect("f"));
        col_t.sort_by(|a, b| a.partial_cmp(b).expect("f"));
        yl.push(percentile(&col_y, lo_p));
        yu.push(percentile(&col_y, hi_p));
        tl.push(percentile(&col_t, lo_p));
        tu.push(percentile(&col_t, hi_p));
    }
    Forecast {
        ds_days: ds_days.to_vec(),
        trend,
        yhat,
        yhat_lower: yl,
        yhat_upper: yu,
        trend_lower: tl,
        trend_upper: tu,
        components,
    }
}

#[cfg(test)]
mod sampler {
    //! FALSIFICATION of the changepoint-count sampler's numerical DOMAIN (VERIFICATION gap 3).
    //!
    //! Deliberately NOT in [`super::parity`]: `prophet::parity` means "the Prophet 1.4.0 ladder"
    //! and its count (32) is asserted by this plan's own verification, so a sampler test living
    //! there would change what that number means.
    //!
    //! What is barred here is the sampler's MEAN against its own lambda, at six lambdas that
    //! straddle both the branch threshold and Knuth's 745.13 underflow point. One failing input
    //! is an anecdote (CLAUDE.md Verification Discipline rule 6); six points make the shape of
    //! the failure visible — the pre-fix implementation tracks lambda up to ~745 and saturates
    //! there for anything larger, which is a different claim from "it is wrong at 900".
    use super::{poisson, Rng, POISSON_NORMAL_BRANCH_LAMBDA};
    use crate::dates::parse_ymd;
    use crate::test_support::load_json;

    /// Draws per lambda.
    ///
    /// Chosen so sampling error cannot be mistaken for the defect: at lambda = 900 the standard
    /// error of the mean is sqrt(900 / 20 000) ~= 0.212, i.e. ~0.024 % of lambda — three orders
    /// of magnitude below the ~17 % shortfall the saturation produces.
    const N: usize = 20_000;
    const N_F: f64 = 20_000.0;

    /// Fixed so the sweep is reproducible. The bar is on the DISTRIBUTION's mean, never on a
    /// draw-for-draw match with numpy — this file's `Rng` is a xorshift, not MT19937.
    const SEED: u64 = 20_260_907;

    /// Six lambdas: two below the branch threshold (Knuth must stay untouched), two above it but
    /// below Knuth's underflow point, and the two the verifier measured as reachable IN BOUNDS
    /// (100 daily points + horizon 3650 gives ~922; the 10-point floor gives ~2839).
    const LAMBDAS: [f64; 6] = [5.0, 29.0, 31.0, 100.0, 900.0, 2839.0];

    /// TRANSIENT — read by Tasks 1 and 2 of plan 06-13 ONLY, because
    /// `equations.poisson_sampler_domain.float_tolerance` does not exist in
    /// `contracts/prophet-parity-v1.yaml` until Task 3. Task 3 DELETES this const in the same
    /// edit that adds the key, and proves the swap by mutating the contract value (D-15).
    const REL_TOLERANCE: f64 = 0.02;

    #[test]
    fn poisson_mean_tracks_lambda_across_its_whole_domain() {
        let bar = REL_TOLERANCE;
        // Measure and PRINT all six first, then assert: a print-and-assert loop would abort at
        // the first out-of-domain lambda and hide the shape of the failure at the larger ones.
        let mut observed: Vec<(f64, f64, f64)> = Vec::with_capacity(LAMBDAS.len());
        for (i, &lambda) in LAMBDAS.iter().enumerate() {
            let mut rng = Rng::new(SEED + u64::try_from(i).expect("index fits u64"));
            let mut sum = 0.0_f64;
            for _ in 0..N {
                let k = poisson(&mut rng, lambda);
                sum += f64::from(u32::try_from(k).expect("a Poisson count fits u32"));
            }
            let mean = sum / N_F;
            let rel = (mean - lambda).abs() / lambda;
            println!("POISSON MEAN: lambda={lambda:.1} n={N} mean={mean:.4} rel={rel:.6}");
            observed.push((lambda, mean, rel));
        }
        for (lambda, mean, rel) in observed {
            assert!(
                rel <= bar,
                "poisson sampler outside its domain at lambda={lambda:.1}: \
                 mean={mean:.4} rel={rel:.6} > bar={bar:e}"
            );
        }
    }

    /// The parity argument, as a MEASUREMENT rather than a claim.
    ///
    /// `wp_log_R_logistic` is the only logistic fixture, so it is the only parity rung that can
    /// reach this sampler at all. Its lambda is `changepoints_t.len() * (t_max - 1)`, and both
    /// factors are published by the fixture — the `..._data_prep_exact` rung already asserts that
    /// Rust's `make_design` reproduces `changepoints_t` exactly, so reading them here measures the
    /// same quantity the ladder runs on.
    #[test]
    fn wp_log_r_logistic_fixture_lambda_is_far_below_the_branch_threshold() {
        let fx = load_json("wp_log_R_logistic_prophet140.json");
        let n_cp = fx["changepoints_t"]
            .as_array()
            .expect("fixture publishes changepoints_t")
            .len();
        let start = parse_ymd(fx["start"].as_str().expect("fixture publishes start"));
        let t_scale = fx["t_scale_days"]
            .as_f64()
            .expect("fixture publishes t_scale_days");
        let last_ds = fx["forecast"]["ds"]
            .as_array()
            .expect("fixture publishes forecast.ds")
            .last()
            .and_then(serde_json::Value::as_str)
            .expect("forecast.ds is non-empty");
        let t_max = f64::from(
            i32::try_from(parse_ymd(last_ds) - start).expect("a forecast span fits i32 days"),
        ) / t_scale;
        let lambda =
            f64::from(u32::try_from(n_cp).expect("a changepoint count fits u32")) * (t_max - 1.0);
        println!(
            "FIXTURE LAMBDA: fixture=wp_log_R_logistic n_changepoints={n_cp} \
             t_max={t_max:.6} lambda={lambda:.4}"
        );
        // If this ever goes red the parity ladder has started exercising the
        // normal-approximation branch, and the claim that no rung moved because of it would no
        // longer be a measurement.
        assert!(
            lambda < POISSON_NORMAL_BRANCH_LAMBDA,
            "the only logistic parity fixture now reaches lambda={lambda:.4}, \
             at or above the sampler's branch threshold {POISSON_NORMAL_BRANCH_LAMBDA}"
        );
    }

    /// The worst IN-BOUNDS logistic request, walled on a release build.
    ///
    /// This is the configuration the verifier named as reachable: 100 consecutive daily points
    /// with `horizon: 3650, growth: "logistic"`, every factor inside every door bound. Correcting
    /// the sampler makes `n_changes` grow from the ~745 the underflow imposed to ~lambda, and the
    /// per-row work (`logistic_gammas` + a sort + `piecewise_logistic`) is LINEAR in that count —
    /// so the cost increase is a bounded multiple, and this harness is what turns that argument
    /// into two numbers against SC1's 2 s bar.
    ///
    /// `#[ignore]` for the same reason `design_cost::holiday_design_wall` is: a wall-clock number
    /// from a debug build measures the profile, not the code.
    #[test]
    #[ignore = "release-profile wall-clock measurement; run with --release --ignored"]
    fn logistic_band_wall() {
        use crate::dates::{days_from_civil, format_ymd};
        use crate::types::ForecastArgs;

        // Selectable so the OTHER reachable extreme can be walled with the same harness: the
        // 10-point `MIN_POINTS` floor at the same horizon is the lambda ~= 2839 case, where the
        // pre-fix truncation was ~3.8x rather than ~1.24x. `.unwrap()` is banned by
        // `.clippy.toml`; an unparseable selector falls back to the default rather than aborting
        // the run with a panic that would look like a measurement.
        let env_usize = |key: &str, default: usize| -> usize {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default)
        };
        let points = env_usize("LOGISTIC_BENCH_POINTS", 100);
        let horizon = env_usize("LOGISTIC_BENCH_HORIZON", 3650);
        let t0 = days_from_civil(2015, 1, 1);
        let ds: Vec<String> = (0..points).map(|i| format_ymd(t0 + i as i64)).collect();
        let y: Vec<f64> = (0..points)
            .map(|i| {
                let t = i as f64;
                10.0 + 0.01 * t + (2.0 * std::f64::consts::PI * t / 7.0).sin()
            })
            .collect();
        let args = ForecastArgs {
            ds,
            y,
            horizon,
            growth: Some("logistic".into()),
            cap: Some(50.0),
            ..ForecastArgs::default()
        };

        let t = std::time::Instant::now();
        let r = crate::forecast::forecast(&args).expect("the bench configuration must be accepted");
        let total = t.elapsed().as_secs_f64();
        assert_eq!(r.yhat.len(), horizon, "one row per horizon step");

        // The mean band width is the OUTPUT this plan is about: a sampler stuck at ~745 draws
        // fewer changepoints than the law asks for, so the simulated trends spread less and the
        // interval comes back narrower than the `interval_width` it advertises.
        let width: f64 = r
            .yhat_upper
            .iter()
            .zip(r.yhat_lower.iter())
            .map(|(u, l)| u - l)
            .sum::<f64>()
            / horizon as f64;
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        println!(
            "LOGISTIC BAND WALL: points={points} horizon={horizon} growth=logistic \
             total_s={total:.3} fit_s={:.3} predict_s={:.3} mean_band_width={width:.4} \
             profile={profile}",
            r.fit_seconds, r.predict_seconds
        );
    }
}

#[cfg(test)]
mod parity {
    //! The Prophet 1.4.0 parity ladder (SC2, D-04).
    //!
    //! ONE TEST PER RUNG PER FIXTURE, so a regression names the rung and the fixture rather
    //! than "Prophet broke". The rungs, in the order they must be believed:
    //!
    //! 1. `<f>_data_prep_exact` — the Stan data block, rebuilt from raw `(ds, y)`.
    //! 2. `<f>_objective_at_python_map` — the objective at PYTHON's MAP vs Python's own `-lp`.
    //! 3. `<f>_predict_path_via_python_params` — Python's parameters through Rust `predict`,
    //!    with the named components and the `trend*(1+mul)+add == yhat` identity.
    //! 4. `<f>_fit_objective_and_forecast` — what the Rust MAP fit itself reaches, plus the
    //!    D-09 diagnostics.
    //! 5. `<f>_band_widths_within_contract` — the 80 % interval widths.
    //!
    //! NO TOLERANCE LITERAL LIVES HERE (D-15). Every bar is read at test time from
    //! `contracts/prophet-parity-v1.yaml` through [`equation_tolerance`], and `max_rounds`
    //! through [`constant_u64`]. A bar that lives in a test can be loosened without the
    //! contract noticing; a bar that lives in the contract moves only as a `pv diff`-visible
    //! edit.
    //!
    //! WHAT IS DELIBERATELY NOT BARRED. `air_passengers` and `retail_sales` have no committed
    //! future-`yhat` control band: the yearly Fourier block is near-unidentified on monthly
    //! data, so Prophet's own two optimisers disagree there by far more than any epsilon worth
    //! writing down. Those numbers are RECORDED (printed, and carried in the assertion message)
    //! and asserted against nothing — the contract says why.

    use super::{
        make_design, predict, Design, Forecast, Growth, Holiday, Mode, Model, Params, Seasonality,
        Spec,
    };
    use crate::dates::{civil_from_days, days_from_civil, future_days, parse_ymd};
    use crate::fit::{fit_prophet, FitInfo};
    use crate::test_support::{constant_u64, equation_tolerance, load_json};
    use serde_json::Value;
    use std::sync::{Arc, OnceLock};

    /// The uncertainty seed every band rung draws with. The fixtures' Python bands come from
    /// Prophet's own RNG, so this only has to be FIXED, never matched — the bar is relative
    /// and wide enough to cover the ~1.2 % seed-to-seed variance the spike measured.
    const SEED: u64 = 42;

    /// The seven committed Prophet 1.4.0 oracles: `(test-name stem, fixture file)`.
    ///
    /// The first three are spike 001 (they publish `log_posterior_at_map_unnormalized` and
    /// carry no `uncertainty` block); the last four are spike 003 (they publish `columns`,
    /// `s_a`, `s_m` and a precomputed `uncertainty` block, and no `-lp`).
    const FIXTURES: [(&str, &str); 7] = [
        ("peyton_manning", "peyton_manning_prophet140.json"),
        ("air_passengers", "air_passengers_prophet140.json"),
        ("retail_sales", "retail_sales_prophet140.json"),
        ("peyton_default", "peyton_default_prophet140.json"),
        ("peyton_holidays", "peyton_holidays_prophet140.json"),
        ("wp_log_r_logistic", "wp_log_R_logistic_prophet140.json"),
        ("air_multiplicative", "air_multiplicative_prophet140.json"),
    ];

    /// The two fixtures that carry a committed Newton-vs-L-BFGS control band on future `yhat`.
    const PEYTON_BANDED: [&str; 2] = ["peyton_manning", "peyton_default"];

    fn fixture_index(stem: &str) -> usize {
        FIXTURES
            .iter()
            .position(|(s, _)| *s == stem)
            .unwrap_or_else(|| panic!("{stem} is not one of the seven committed fixtures"))
    }

    fn f64s(v: &Value, what: &str) -> Vec<f64> {
        v.as_array()
            .unwrap_or_else(|| panic!("{what} must be a JSON array"))
            .iter()
            .map(|x| {
                x.as_f64()
                    .unwrap_or_else(|| panic!("{what} must hold only numbers"))
            })
            .collect()
    }

    fn strings(v: &Value, what: &str) -> Vec<String> {
        v.as_array()
            .unwrap_or_else(|| panic!("{what} must be a JSON array"))
            .iter()
            .map(|x| {
                x.as_str()
                    .unwrap_or_else(|| panic!("{what} must hold only strings"))
                    .to_string()
            })
            .collect()
    }

    fn days(v: &Value, what: &str) -> Vec<i64> {
        strings(v, what).iter().map(|s| parse_ymd(s)).collect()
    }

    fn max_abs_diff(a: &[f64], b: &[f64], what: &str) -> f64 {
        assert_eq!(a.len(), b.len(), "{what}: length mismatch");
        assert!(!a.is_empty(), "{what}: refusing to compare empty vectors");
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0_f64, f64::max)
    }

    fn mean(v: &[f64]) -> f64 {
        assert!(!v.is_empty(), "mean of an empty slice");
        v.iter().sum::<f64>() / v.len() as f64
    }

    /// Mean width of an interval, the quantity Prophet's fixtures publish.
    fn mean_width(lo: &[f64], hi: &[f64]) -> f64 {
        assert_eq!(lo.len(), hi.len(), "band bounds must be parallel");
        mean(&lo.iter().zip(hi).map(|(a, b)| b - a).collect::<Vec<_>>())
    }

    /// Assert a RELATIVE agreement and report the measured deviation either way.
    fn within_rel(rust: f64, python: f64, rel_bar: f64, what: &str) {
        assert!(
            python.abs() > 0.0,
            "{what}: the Python reference is zero, so a relative bar is meaningless"
        );
        let dev = (rust - python).abs() / python.abs();
        println!(
            "    {what}: rust {rust:.6} vs python {python:.6} -> {:.4} % ",
            dev * 100.0
        );
        assert!(
            dev <= rel_bar,
            "{what}: rust {rust} vs python {python} is {dev:e} relative, over the contract bar {rel_bar:e}"
        );
    }

    /// One fixture, loaded once and rebuilt into the design the port would build itself.
    struct Ladder {
        fx: Value,
        design: Design,
        ds_days: Vec<i64>,
        forecast_days: Vec<i64>,
        n_hist: usize,
    }

    impl Ladder {
        /// Python Prophet 1.4.0's MAP, verbatim from the fixture.
        fn python_params(&self) -> Params {
            let arr = |key: &str| f64s(&self.fx["params"][key], &format!("params.{key}"));
            Params {
                k: self.fx["params"]["k"].as_f64().expect("params.k"),
                m: self.fx["params"]["m"].as_f64().expect("params.m"),
                delta: arr("delta"),
                beta: arr("beta"),
                sigma_obs: self.fx["params"]["sigma_obs"]
                    .as_f64()
                    .expect("params.sigma_obs"),
            }
        }

        /// Spike-003 fixtures carry the precomputed `uncertainty` block; spike-001 ones do not.
        fn is_spike_003(&self) -> bool {
            self.fx.get("uncertainty").is_some_and(|u| u.is_object())
        }

        /// The Python objective at Python's MAP, UNSCALED, sign convention `f = -log posterior`.
        ///
        /// Where the fixture publishes `log_posterior_at_map_unnormalized` this is Python's own
        /// number. Where it does not (the four spike-003 files), it is the Rust objective at
        /// Python's MAP — a legitimate stand-in precisely because rung 2 proves, on every
        /// fixture that DOES publish `-lp`, that this quantity is Python's objective.
        fn f_python(&self) -> f64 {
            if let Some(lp) = self
                .fx
                .get("log_posterior_at_map_unnormalized")
                .and_then(Value::as_f64)
            {
                -lp
            } else {
                let model = Model::new(&self.design);
                model.objective(&model.pack(&self.python_params())) / model.scale
            }
        }
    }

    /// Rebuild the fixture's `Spec` from the fixture alone — never from a hand-copied default.
    fn spec_of(fx: &Value) -> Spec {
        let mode_of = |v: &Value| {
            if v.as_str() == Some("multiplicative") {
                Mode::Multiplicative
            } else {
                Mode::Additive
            }
        };
        let seasonalities: Vec<Seasonality> = fx["seasonalities"]
            .as_array()
            .expect("seasonalities")
            .iter()
            .map(|s| Seasonality {
                name: s["name"].as_str().expect("seasonality.name").to_string(),
                period: s["period"].as_f64().expect("seasonality.period"),
                order: usize::try_from(s["fourier_order"].as_u64().expect("fourier_order"))
                    .expect("fourier order fits usize"),
                prior_scale: s["prior_scale"].as_f64().expect("seasonality.prior_scale"),
                mode: mode_of(&s["mode"]),
            })
            .collect();

        // Holidays arrive as one row per (name, date); group them, preserving first-seen order.
        let mut holidays: Vec<Holiday> = Vec::new();
        let holiday_prior = fx["holidays_prior_scale"].as_f64().unwrap_or(10.0);
        let no_holidays: Vec<Value> = Vec::new();
        for h in fx["holidays"].as_array().unwrap_or(&no_holidays) {
            let name = h["holiday"].as_str().expect("holiday.holiday").to_string();
            let day = parse_ymd(h["ds"].as_str().expect("holiday.ds"));
            if let Some(existing) = holidays.iter_mut().find(|x| x.name == name) {
                existing.days.push(day);
            } else {
                holidays.push(Holiday {
                    name,
                    days: vec![day],
                    lower_window: h["lower_window"].as_i64().expect("lower_window"),
                    upper_window: h["upper_window"].as_i64().expect("upper_window"),
                    prior_scale: holiday_prior,
                });
            }
        }

        let mut spec = Spec::default_linear(seasonalities);
        spec.growth = match fx["growth"].as_str() {
            Some("logistic") => Growth::Logistic,
            Some("flat") => Growth::Flat,
            _ => Growth::Linear,
        };
        spec.cap = fx["cap"].as_f64();
        spec.holidays = holidays;
        spec.holidays_mode = mode_of(&fx["seasonality_mode"]);
        spec.changepoint_prior_scale = fx["changepoint_prior_scale"]
            .as_f64()
            .expect("changepoint_prior_scale");
        if let Some(u) = fx.get("uncertainty") {
            spec.interval_width = u["interval_width"].as_f64().expect("interval_width");
            spec.uncertainty_samples = usize::try_from(
                u["uncertainty_samples"]
                    .as_u64()
                    .expect("uncertainty_samples"),
            )
            .expect("uncertainty_samples fits usize");
        }
        spec
    }

    fn ladder(stem: &'static str) -> Ladder {
        let file = FIXTURES[fixture_index(stem)].1;
        let fx = load_json(file);
        assert_eq!(
            fx["prophet_version"], "1.4.0",
            "{stem}: the bar is Python Prophet 1.4.0, not whatever regenerated the fixture"
        );
        let ds_days = days(&fx["history"]["ds"], "history.ds");
        let y = f64s(&fx["history"]["y"], "history.y");
        let forecast_days = days(&fx["forecast"]["ds"], "forecast.ds");
        let design = make_design(&ds_days, &y, &spec_of(&fx));
        let n_hist = ds_days.len();
        Ladder {
            fx,
            design,
            ds_days,
            forecast_days,
            n_hist,
        }
    }

    /// The D-09 MAP fit for one fixture, computed at most ONCE per test binary run.
    ///
    /// The fit rung and the spike-003 band rung both need it, and a Peyton-sized fit is the
    /// most expensive thing in this module (RESEARCH Pitfall 9). `OnceLock::get_or_init`
    /// blocks the second caller rather than duplicating the work.
    fn fitted(stem: &'static str) -> Arc<(Params, FitInfo)> {
        static CELLS: [OnceLock<Arc<(Params, FitInfo)>>; FIXTURES.len()] =
            [const { OnceLock::new() }; FIXTURES.len()];
        CELLS[fixture_index(stem)]
            .get_or_init(|| {
                let l = ladder(stem);
                let rounds = usize::try_from(constant_u64("prophet-parity-v1", "max_rounds"))
                    .expect("max_rounds fits usize");
                Arc::new(fit_prophet(&l.design, rounds))
            })
            .clone()
    }

    /// PYTHON's parameters through Rust `predict`, over the fixture's own forecast grid.
    ///
    /// Fit-independent by construction, which is exactly why the band rung uses it on the
    /// three spike-001 fixtures: their optimiser disagreement cannot leak into a band verdict.
    fn python_forecast(stem: &'static str) -> Arc<Forecast> {
        static CELLS: [OnceLock<Arc<Forecast>>; FIXTURES.len()] =
            [const { OnceLock::new() }; FIXTURES.len()];
        CELLS[fixture_index(stem)]
            .get_or_init(|| {
                let l = ladder(stem);
                Arc::new(predict(
                    &l.design,
                    &l.python_params(),
                    &l.forecast_days,
                    SEED,
                ))
            })
            .clone()
    }

    fn component<'a>(f: &'a Forecast, name: &str) -> &'a [f64] {
        &f.components
            .iter()
            .find(|(n, _)| n.as_str() == name)
            .unwrap_or_else(|| panic!("predict must return a `{name}` component"))
            .1
    }

    // ---------------------------------------------------------------- rung 1 ----

    /// Rung 1: the Stan data block, rebuilt from the raw `(ds, y)` alone.
    ///
    /// The bar is EXACT (`data_prep_exact` is 0.0 in the contract): the spike measured
    /// `0.00e0` on all seven fixtures, so any non-zero difference is a defect, not noise.
    fn data_prep_exact(stem: &'static str) {
        let l = ladder(stem);
        let t = equation_tolerance("prophet-parity-v1", "data_prep_exact");
        let (d, fx) = (&l.design, &l.fx);

        let ours: Vec<(String, f64, usize)> = d
            .spec
            .seasonalities
            .iter()
            .map(|s| (s.name.clone(), s.period, s.order))
            .collect();
        let theirs: Vec<(String, f64, usize)> = fx["seasonalities"]
            .as_array()
            .expect("seasonalities")
            .iter()
            .map(|s| {
                (
                    s["name"].as_str().expect("name").to_string(),
                    s["period"].as_f64().expect("period"),
                    usize::try_from(s["fourier_order"].as_u64().expect("fourier_order"))
                        .expect("fits usize"),
                )
            })
            .collect();
        assert_eq!(
            ours, theirs,
            "{stem}: seasonality (name, period, fourier_order) list must match Prophet's"
        );

        assert!(
            (d.y_scale - fx["y_scale"].as_f64().expect("y_scale")).abs() <= t,
            "{stem}: y_scale {} vs {}",
            d.y_scale,
            fx["y_scale"]
        );
        assert!(
            (d.t_scale_days - fx["t_scale_days"].as_f64().expect("t_scale_days")).abs() <= t,
            "{stem}: t_scale_days {} vs {}",
            d.t_scale_days,
            fx["t_scale_days"]
        );

        let d_prior = max_abs_diff(
            &d.prior_scales,
            &f64s(&fx["prior_scales"], "prior_scales"),
            "prior_scales",
        );
        let d_t = max_abs_diff(&d.t, &f64s(&fx["history"]["t"], "history.t"), "t");
        let d_y = max_abs_diff(
            &d.y_scaled,
            &f64s(&fx["history"]["y_scaled"], "history.y_scaled"),
            "y_scaled",
        );
        let d_cp = max_abs_diff(
            &d.changepoints_t,
            &f64s(&fx["changepoints_t"], "changepoints_t"),
            "changepoints_t",
        );

        let flat = |v: &Value, what: &str| -> Vec<f64> {
            v.as_array()
                .unwrap_or_else(|| panic!("{what} must be an array of rows"))
                .iter()
                .flat_map(|r| f64s(r, what))
                .collect()
        };
        let d_x = max_abs_diff(
            &d.x[..3 * d.k],
            &flat(&fx["X_first3"], "X_first3"),
            "X first 3 rows",
        )
        .max(max_abs_diff(
            &d.x[(l.n_hist - 3) * d.k..],
            &flat(&fx["X_last3"], "X_last3"),
            "X last 3 rows",
        ));

        println!(
            "  {stem} rung 1: t {d_t:.2e}, y_scaled {d_y:.2e}, changepoints_t {d_cp:.2e} ({} cps), X {d_x:.2e} (K={})",
            d.changepoints_t.len(),
            d.k
        );
        assert!(
            d_t <= t && d_y <= t && d_cp <= t && d_x <= t && d_prior <= t,
            "{stem}: data-prep parity is EXACT in the contract; measured t {d_t:e}, y_scaled {d_y:e}, changepoints_t {d_cp:e}, X {d_x:e}, prior_scales {d_prior:e} against bar {t:e}"
        );

        if l.is_spike_003() {
            let ours_cols: Vec<String> = d.cols.iter().map(|c| c.name.clone()).collect();
            assert_eq!(
                ours_cols,
                strings(&fx["columns"], "columns"),
                "{stem}: design column names AND order must match Prophet's"
            );
            let d_sa = max_abs_diff(&d.s_a, &f64s(&fx["s_a"], "s_a"), "s_a");
            let d_sm = max_abs_diff(&d.s_m, &f64s(&fx["s_m"], "s_m"), "s_m");
            assert!(
                d_sa <= t && d_sm <= t,
                "{stem}: additive/multiplicative selectors differ: s_a {d_sa:e}, s_m {d_sm:e}"
            );
            if let Some(cap_scaled) = d.cap_scaled.as_ref() {
                let d_cap = max_abs_diff(
                    cap_scaled,
                    &f64s(&fx["history"]["cap_scaled"], "history.cap_scaled"),
                    "cap_scaled",
                );
                assert!(
                    d_cap <= t,
                    "{stem}: logistic cap_scaled differs by {d_cap:e}"
                );
            }
        } else {
            assert_eq!(
                d.k,
                fx["seasonality_columns"]
                    .as_array()
                    .expect("seasonality_columns")
                    .len(),
                "{stem}: design column count"
            );
        }
    }

    #[test]
    fn peyton_manning_data_prep_exact() {
        data_prep_exact("peyton_manning");
    }
    #[test]
    fn air_passengers_data_prep_exact() {
        data_prep_exact("air_passengers");
    }
    #[test]
    fn retail_sales_data_prep_exact() {
        data_prep_exact("retail_sales");
    }
    #[test]
    fn peyton_default_data_prep_exact() {
        data_prep_exact("peyton_default");
    }
    #[test]
    fn peyton_holidays_data_prep_exact() {
        data_prep_exact("peyton_holidays");
    }
    #[test]
    fn wp_log_r_logistic_data_prep_exact() {
        data_prep_exact("wp_log_r_logistic");
    }
    #[test]
    fn air_multiplicative_data_prep_exact() {
        data_prep_exact("air_multiplicative");
    }

    // ---------------------------------------------------------------- rung 2 ----

    /// Rung 2 (D-04): the Rust objective at PYTHON's MAP against Python's own `-lp`.
    ///
    /// Everything about the port — the priors, the exact L1 on `delta`, the `2σ²` term, the
    /// `n·ln σ` term, the Fourier column order and the changepoint grid — has to be right
    /// simultaneously for this number to land.
    ///
    /// ONLY THE THREE SPIKE-001 FIXTURES CAN CARRY THIS RUNG. The four spike-003 files publish
    /// no `log_posterior_at_map_unnormalized`, so there is no oracle to compare against there;
    /// writing a seventh "objective" test that compared Rust to Rust would be theatre. The
    /// contract states the asymmetry, and the spike-003 fixtures are held instead by
    /// `fitted_objective_slack` plus the whole Python-parameters-through-Rust chain.
    fn objective_at_python_map(stem: &'static str) {
        let l = ladder(stem);
        let t = equation_tolerance("prophet-parity-v1", "objective_at_python_map_abs");
        let lp = l.fx["log_posterior_at_map_unnormalized"]
            .as_f64()
            .expect("this rung binds only fixtures that publish Python's -lp");

        let py = l.python_params();
        assert_eq!(py.beta.len(), l.design.k, "{stem}: beta length vs design K");
        assert_eq!(
            py.delta.len(),
            l.design.changepoints_t.len(),
            "{stem}: delta length vs changepoints"
        );

        // `Model::new` scales by 1/T (the D-09 recipe); the fixture number is unscaled, so the
        // scale is undone before comparing. The fixture stores a POSITIVE log posterior while
        // the model computes a NEGATIVE one, so the residual is the SUM.
        let model = Model::new(&l.design);
        let f_rust = model.objective(&model.pack(&py)) / model.scale;
        let residual = (f_rust + lp).abs();
        println!(
            "  {stem} rung 2: f_rust {f_rust:.12}, python -lp {:.12}, residual {residual:e}",
            -lp
        );
        assert!(
            residual <= t,
            "{stem} rung 2: Rust f(theta_py) = {f_rust:.12}, Python -lp = {:.12}, abs diff {residual:e} over the contract bar {t:e}",
            -lp
        );
    }

    #[test]
    fn peyton_manning_objective_at_python_map() {
        objective_at_python_map("peyton_manning");
    }
    #[test]
    fn air_passengers_objective_at_python_map() {
        objective_at_python_map("air_passengers");
    }
    #[test]
    fn retail_sales_objective_at_python_map() {
        objective_at_python_map("retail_sales");
    }

    // ---------------------------------------------------------------- rung 3 ----

    /// Rung 3: PYTHON's parameters through Rust `predict`, plus components and the
    /// reconstruction identity. Fit-independent, so no optimiser difference can excuse it.
    fn predict_path_via_python_params(stem: &'static str) {
        let l = ladder(stem);
        let f = python_forecast(stem);

        // `make_future_dataframe(periods=365)` parity: the fixture's forecast grid is the
        // history followed by 365 DAILY rows, which is exactly what `dates::future_days` builds.
        assert_eq!(
            &l.forecast_days[..l.n_hist],
            &l.ds_days[..],
            "{stem}: the forecast grid must start with the history"
        );
        let last = *l.ds_days.last().expect("non-empty history");
        assert_eq!(
            &l.forecast_days[l.n_hist..],
            future_days(last, l.forecast_days.len() - l.n_hist, "D")
                .expect("D is supported")
                .as_slice(),
            "{stem}: the future grid must be daily from the last history day"
        );

        let py_yhat = f64s(&l.fx["forecast"]["yhat"], "forecast.yhat");
        let py_trend = f64s(&l.fx["forecast"]["trend"], "forecast.trend");
        let d_yhat = max_abs_diff(&f.yhat, &py_yhat, "yhat");
        let d_trend = max_abs_diff(&f.trend, &py_trend, "trend");

        let rel_bar =
            equation_tolerance("prophet-parity-v1", "predict_path_rel_yscale") * l.design.y_scale;
        println!(
            "  {stem} rung 3: yhat {d_yhat:.2e}, trend {d_trend:.2e} (y_scale {}, relative bar {rel_bar:.2e})",
            l.design.y_scale
        );
        assert!(
            d_yhat <= rel_bar && d_trend <= rel_bar,
            "{stem}: Python params through Rust predict differ by yhat {d_yhat:e} / trend {d_trend:e}, over the y_scale-relative bar {rel_bar:e}"
        );

        if PEYTON_BANDED.contains(&stem) {
            let abs_bar = equation_tolerance("prophet-parity-v1", "predict_path_abs_peyton");
            assert!(
                d_yhat <= abs_bar && d_trend <= abs_bar,
                "{stem}: D-04's ABSOLUTE predict-path bar {abs_bar:e} missed — yhat {d_yhat:e}, trend {d_trend:e}"
            );
        }

        // Named components: compare every component the fixture publishes AND predict returns.
        // The fixture also carries `<name>_lower` / `<name>_upper` (sampled) and, for logistic
        // growth, `cap` — none of which has a point-estimate twin, so they never match by name.
        let comp_bar = equation_tolerance("prophet-parity-v1", "components_via_python_params_abs");
        let py_comps = l.fx["forecast"]["components"]
            .as_object()
            .expect("forecast.components");
        let mut checked: Vec<String> = Vec::new();
        for (name, values) in &f.components {
            if let Some(py) = py_comps.get(name) {
                let d = max_abs_diff(values, &f64s(py, name), name);
                assert!(
                    d <= comp_bar,
                    "{stem}: component `{name}` differs by {d:e}, over the contract bar {comp_bar:e}"
                );
                checked.push(format!("{name} {d:.1e}"));
            }
        }
        assert!(
            !checked.is_empty(),
            "{stem}: no named component was compared — the fixture's component names drifted"
        );

        // The decomposition must reconstruct the number the tool returns.
        let add = component(&f, "additive_terms");
        let mul = component(&f, "multiplicative_terms");
        let recon: Vec<f64> = (0..f.yhat.len())
            .map(|i| f.trend[i] * (1.0 + mul[i]) + add[i])
            .collect();
        let d_recon = max_abs_diff(&recon, &f.yhat, "reconstruction");
        println!(
            "  {stem} rung 3 components: {} | rebuild {d_recon:.1e}",
            checked.join(", ")
        );
        assert!(
            d_recon <= equation_tolerance("prophet-parity-v1", "components_rebuild_yhat_abs"),
            "{stem}: trend*(1+multiplicative_terms)+additive_terms misses yhat by {d_recon:e}"
        );
    }

    #[test]
    fn peyton_manning_predict_path_via_python_params() {
        predict_path_via_python_params("peyton_manning");
    }
    #[test]
    fn air_passengers_predict_path_via_python_params() {
        predict_path_via_python_params("air_passengers");
    }
    #[test]
    fn retail_sales_predict_path_via_python_params() {
        predict_path_via_python_params("retail_sales");
    }
    #[test]
    fn peyton_default_predict_path_via_python_params() {
        predict_path_via_python_params("peyton_default");
    }
    #[test]
    fn peyton_holidays_predict_path_via_python_params() {
        predict_path_via_python_params("peyton_holidays");
    }
    #[test]
    fn wp_log_r_logistic_predict_path_via_python_params() {
        predict_path_via_python_params("wp_log_r_logistic");
    }
    #[test]
    fn air_multiplicative_predict_path_via_python_params() {
        predict_path_via_python_params("air_multiplicative");
    }

    // ---------------------------------------------------------------- rung 4 ----

    /// Rung 4: what the Rust MAP fit itself reaches, and the D-09 diagnostics it must report.
    ///
    /// The bar is ONE-SIDED objective slack, never parameter equality and never
    /// daily-resolution `yhat` on monthly data (`prophet-fit-and-predict.md`): the yearly
    /// Fourier block is near-unidentified on air/retail, so many parameter vectors sit within a
    /// fraction of an objective unit of Python's MAP and draw visibly different curves.
    fn fit_objective_and_forecast(stem: &'static str) {
        let l = ladder(stem);
        let max_rounds = usize::try_from(constant_u64("prophet-parity-v1", "max_rounds"))
            .expect("max_rounds fits usize");
        let handle = fitted(stem);
        let (p, info) = (&handle.0, &handle.1);

        let f_python = l.f_python();
        let slack = equation_tolerance("prophet-parity-v1", "fitted_objective_slack");
        println!(
            "  {stem} rung 4: f_rust {:.4} vs f_python {f_python:.4} (delta {:+.4}); {} rounds, {} iters, {} evals, status {}",
            info.objective,
            info.objective - f_python,
            info.rounds,
            info.iterations,
            info.evals,
            info.status
        );
        assert!(
            info.objective <= f_python + slack,
            "{stem}: the Rust fit landed at {} against Python's {f_python}, i.e. {:+} — over the contract slack {slack}",
            info.objective,
            info.objective - f_python
        );

        // D-09 diagnostics, observable on every fixture.
        assert!(
            info.rounds <= max_rounds,
            "{stem}: {} restart rounds exceeds the contract's max_rounds {max_rounds}",
            info.rounds
        );
        assert!(
            !info.budget_hit,
            "{stem}: the cooperative fit budget was crossed at a round boundary — a fixture-sized fit must never reach it"
        );
        assert!(
            info.status == "Stalled" || info.status == "Converged",
            "{stem}: L-BFGS terminal status {:?} is not one the D-09 recipe accepts",
            info.status
        );

        let f = predict(&l.design, p, &l.forecast_days, SEED);
        let py_yhat = f64s(&l.fx["forecast"]["yhat"], "forecast.yhat");
        let future = max_abs_diff(&f.yhat[l.n_hist..], &py_yhat[l.n_hist..], "future yhat");
        let history = max_abs_diff(&f.yhat[..l.n_hist], &py_yhat[..l.n_hist], "history yhat");
        println!("  {stem} rung 4 forecast: history max|d yhat| {history:.4}, future {future:.4}");

        if PEYTON_BANDED.contains(&stem) {
            let band = equation_tolerance("prophet-parity-v1", "future_yhat_band_peyton_abs");
            assert!(
                future <= band,
                "{stem}: future max|d yhat| {future} is outside Prophet's OWN Newton-vs-L-BFGS band {band}"
            );
        } else {
            // RECORDED, NOT BARRED. The contract says why: no committed control band exists for
            // these fixtures, so an epsilon here would bar optimiser luck rather than parity.
            assert!(
                future.is_finite(),
                "{stem}: future max|d yhat| must at least be finite (measured {future})"
            );
        }
    }

    #[test]
    fn peyton_manning_fit_objective_and_forecast() {
        fit_objective_and_forecast("peyton_manning");
    }
    #[test]
    fn air_passengers_fit_objective_and_forecast() {
        fit_objective_and_forecast("air_passengers");
    }
    #[test]
    fn retail_sales_fit_objective_and_forecast() {
        fit_objective_and_forecast("retail_sales");
    }
    #[test]
    fn peyton_default_fit_objective_and_forecast() {
        fit_objective_and_forecast("peyton_default");
    }
    #[test]
    fn peyton_holidays_fit_objective_and_forecast() {
        fit_objective_and_forecast("peyton_holidays");
    }
    #[test]
    fn wp_log_r_logistic_fit_objective_and_forecast() {
        fit_objective_and_forecast("wp_log_r_logistic");
    }
    #[test]
    fn air_multiplicative_fit_objective_and_forecast() {
        fit_objective_and_forecast("air_multiplicative");
    }

    // ---------------------------------------------------------------- rung 5 ----

    /// Rung 5: the 80 % interval widths, relative to Python's.
    ///
    /// TWO REFERENCE SHAPES, because the two fixture families publish different things.
    /// The spike-003 files carry precomputed `uncertainty.*_mean_width` values, and the Rust
    /// side there is the Rust FIT (the spike-003 probe). The spike-001 files carry no
    /// `uncertainty` block at all, so their reference is DERIVED from the fixture's own
    /// `forecast.yhat_upper - forecast.yhat_lower` — and the Rust side is PYTHON's parameters
    /// through Rust `predict`, the same fit-independent call rung 3 makes, so air/retail's
    /// unbarred optimiser disagreement cannot leak into a band verdict.
    fn band_widths_within_contract(stem: &'static str) {
        let l = ladder(stem);
        let rel = equation_tolerance("prophet-parity-v1", "band_width_rel");
        let n = l.n_hist;

        if l.is_spike_003() {
            let handle = fitted(stem);
            let f = predict(&l.design, &handle.0, &l.forecast_days, SEED);
            let u = &l.fx["uncertainty"];
            let py = |key: &str| {
                u[key]
                    .as_f64()
                    .unwrap_or_else(|| panic!("uncertainty.{key}"))
            };
            println!("  {stem} rung 5 (spike-003 reference, Rust fit params):");
            within_rel(
                mean_width(&f.yhat_lower[..n], &f.yhat_upper[..n]),
                py("hist_band_mean_width"),
                rel,
                &format!("{stem} history band width"),
            );
            within_rel(
                mean_width(&f.yhat_lower[n..], &f.yhat_upper[n..]),
                py("future_band_mean_width"),
                rel,
                &format!("{stem} future band width"),
            );
            let last30 = f.yhat.len() - 30;
            within_rel(
                mean_width(&f.yhat_lower[last30..], &f.yhat_upper[last30..]),
                py("future_band_last30_mean_width"),
                equation_tolerance("prophet-parity-v1", "band_width_last30_rel"),
                &format!("{stem} last-30 band width"),
            );
            within_rel(
                mean_width(&f.trend_lower[n..], &f.trend_upper[n..]),
                py("future_trend_band_mean_width"),
                equation_tolerance("prophet-parity-v1", "trend_band_width_rel"),
                &format!("{stem} future trend band width"),
            );
        } else {
            let f = python_forecast(stem);
            let py_lo = f64s(&l.fx["forecast"]["yhat_lower"], "forecast.yhat_lower");
            let py_hi = f64s(&l.fx["forecast"]["yhat_upper"], "forecast.yhat_upper");
            println!("  {stem} rung 5 (reference DERIVED from the fixture's own band, Python params through Rust predict):");
            within_rel(
                mean_width(&f.yhat_lower[..n], &f.yhat_upper[..n]),
                mean_width(&py_lo[..n], &py_hi[..n]),
                rel,
                &format!("{stem} history band width"),
            );
            within_rel(
                mean_width(&f.yhat_lower[n..], &f.yhat_upper[n..]),
                mean_width(&py_lo[n..], &py_hi[n..]),
                rel,
                &format!("{stem} future band width"),
            );
        }
    }

    #[test]
    fn peyton_manning_band_widths_within_contract() {
        band_widths_within_contract("peyton_manning");
    }
    #[test]
    fn air_passengers_band_widths_within_contract() {
        band_widths_within_contract("air_passengers");
    }
    #[test]
    fn retail_sales_band_widths_within_contract() {
        band_widths_within_contract("retail_sales");
    }
    #[test]
    fn peyton_default_band_widths_within_contract() {
        band_widths_within_contract("peyton_default");
    }
    #[test]
    fn peyton_holidays_band_widths_within_contract() {
        band_widths_within_contract("peyton_holidays");
    }
    #[test]
    fn wp_log_r_logistic_band_widths_within_contract() {
        band_widths_within_contract("wp_log_r_logistic");
    }
    #[test]
    fn air_multiplicative_band_widths_within_contract() {
        band_widths_within_contract("air_multiplicative");
    }

    // ------------------------------------------------- the bounded date grid ----

    /// The runnable evidence behind KANI-PROPHET-001 (declared, not executed).
    ///
    /// A grid over the three supported frequencies, 20 start days chosen to sit on leap days,
    /// month ends and year ends, and horizons up to the contract's bound: the future grid is
    /// strictly increasing and starts strictly after the last history day, and every `MS` date
    /// is the first of a month.
    #[test]
    fn future_days_strictly_increasing_bounded() {
        let starts = [
            (1968, 2, 28),
            (1968, 2, 29),
            (1968, 3, 1),
            (1970, 1, 1),
            (1999, 12, 31),
            (2000, 1, 1),
            (2000, 2, 29),
            (2001, 2, 28),
            (2004, 2, 29),
            (2008, 1, 31),
            (2012, 12, 1),
            (2015, 1, 1),
            (2016, 2, 29),
            (2019, 4, 30),
            (2020, 2, 29),
            (2021, 3, 31),
            (2023, 5, 31),
            (2024, 2, 29),
            (2024, 12, 31),
            (2100, 2, 28),
        ];
        assert_eq!(starts.len(), 20, "the grid is 20 start days wide");
        let mut cases = 0usize;
        for (y, m, d) in starts {
            let last = days_from_civil(y, m, d);
            for freq in ["D", "W", "MS"] {
                for horizon in [1_usize, 28, 365, 3650] {
                    let grid = future_days(last, horizon, freq)
                        .unwrap_or_else(|e| panic!("{freq} is supported: {e:?}"));
                    assert_eq!(grid.len(), horizon, "{freq}/{horizon}: length");
                    assert!(
                        grid[0] > last,
                        "{freq}/{horizon} from {y}-{m}-{d}: the grid must start after the history"
                    );
                    assert!(
                        grid.windows(2).all(|w| w[0] < w[1]),
                        "{freq}/{horizon} from {y}-{m}-{d}: the grid must be strictly increasing"
                    );
                    if freq == "MS" {
                        for &day in &grid {
                            let (_, _, dom) = civil_from_days(day);
                            assert_eq!(
                                dom, 1,
                                "MS/{horizon} from {y}-{m}-{d}: every date is the 1st of a month"
                            );
                        }
                    }
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 20 * 3 * 4, "the whole bounded grid was enumerated");
    }
}

// ------------------------------------------------------- design-cost bench ----
/// The release-profile wall-clock harness behind `just forecast-holiday-bench`.
///
/// It ATTRIBUTES a holiday-carrying request's wall rather than asserting a bar: the body
/// emits exactly one machine-parsable measurement line (the token is written in exactly
/// one place below, so the recipe's parse is unambiguous) and asserts only that the call
/// succeeded and returned one row per horizon step. REVIEW-06-04 removed a
/// wall-clock ratio assertion from `pool_equality` for the reason that binds here too — a
/// wall inside libtest moves with CPU throttling independently of what is being measured,
/// so the BAR lives in the host-gated `just` recipe and the TEST only measures.
#[cfg(test)]
mod design_cost {
    use crate::dates::{days_from_civil, format_ymd};
    use crate::types::{ForecastArgs, HolidayArg, MAX_HOLIDAY_WINDOW};

    /// `.unwrap()` is banned by `.clippy.toml`; an unparseable selector falls back to the
    /// default rather than aborting the run with a panic that looks like a measurement.
    fn env_usize(key: &str, default: usize) -> usize {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(default)
    }

    /// Build holidays whose windows sum to exactly `columns` design columns.
    ///
    /// One holiday cannot exceed `2 * MAX_HOLIDAY_WINDOW + 1` = 731 columns, so a wider
    /// request is split across as few holidays as that ceiling allows. Each carries
    /// `dates_per_holiday` occurrences spread across the series span.
    fn holidays_for(
        columns: usize,
        dates_per_holiday: usize,
        t0: i64,
        points: usize,
    ) -> Vec<HolidayArg> {
        let max_width = (2 * MAX_HOLIDAY_WINDOW + 1) as usize;
        let step = (points / dates_per_holiday.max(1)).max(1) as i64;
        let mut remaining = columns;
        let mut out: Vec<HolidayArg> = Vec::new();
        while remaining > 0 {
            let width = remaining.min(max_width);
            remaining -= width;
            let lower = -(((width - 1) / 2) as i64);
            let upper = (width - 1) as i64 + lower;
            out.push(HolidayArg {
                name: format!("h{}", out.len()),
                dates: (0..dates_per_holiday)
                    .map(|k| format_ymd(t0 + k as i64 * step))
                    .collect(),
                lower_window: lower,
                upper_window: upper,
            });
        }
        out
    }

    #[test]
    #[ignore = "release-profile wall-clock measurement; run via just forecast-holiday-bench"]
    fn holiday_design_wall() {
        let points = env_usize("HOLIDAY_BENCH_POINTS", 3000);
        let columns = env_usize("HOLIDAY_BENCH_COLUMNS", 181);
        let dates = env_usize("HOLIDAY_BENCH_DATES", 84);
        let horizon = env_usize("HOLIDAY_BENCH_HORIZON", 365);

        let t0 = days_from_civil(2015, 1, 1);
        let ds: Vec<String> = (0..points).map(|i| format_ymd(t0 + i as i64)).collect();
        let y: Vec<f64> = (0..points)
            .map(|i| {
                let t = i as f64;
                10.0 + 0.01 * t + (2.0 * std::f64::consts::PI * t / 7.0).sin()
            })
            .collect();
        let holidays = holidays_for(columns, dates, t0, points);
        let n_holidays = holidays.len();
        let dates_total: usize = holidays.iter().map(|h| h.dates.len()).sum();
        let args = ForecastArgs {
            ds,
            y,
            horizon,
            holidays: Some(holidays),
            ..ForecastArgs::default()
        };

        let t = std::time::Instant::now();
        let r = crate::forecast::forecast(&args).expect("the bench configuration must be accepted");
        let total = t.elapsed().as_secs_f64();
        assert_eq!(r.yhat.len(), horizon, "one row per horizon step");

        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        println!(
            "HOLIDAY DESIGN WALL: points={points} columns={columns} dates={dates_total} \
             holidays={n_holidays} horizon={horizon} cells={} triple={} total_s={total:.3} \
             fit_s={:.3} predict_s={:.3} other_s={:.3} arch={} profile={profile}",
            (points + horizon) * columns,
            points * columns * dates_total,
            r.fit_seconds,
            r.predict_seconds,
            total - r.fit_seconds - r.predict_seconds,
            std::env::consts::ARCH
        );
    }
}
