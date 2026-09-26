"""The fail-closed quality gate and the bounded temperature fit (D-06, D-07).

No torch import (module level or anywhere): `python gate.py --selftest` runs with numpy + pyyaml only.

    fit_temperature(z, y, t_min, t_max)
        laya-finetune-gate-v1 `calibration_fit_bounded`: T_fitted = argmin over T in [t_min, t_max] of
        NLL(softmax(z / T), y). NLL is convex in beta = 1 / T (a log-sum-exp minus a linear term), so
        dNLL/dbeta = mean(E_p[z] - z_y) is monotone and its root is found by bisection in float64. When
        the derivative does not change sign over the interval the optimum lies outside it: T_fitted is
        the bound and clamp_hit is true. T_applied = clamp(T_fitted) (identical inside the interval),
        which is what Laya's loader would serve anyway (Pitfall 6).

    EarlyStopper(decl, max_epochs), calibration_monitor(z, y, t_min, t_max)
        the `early_stopping` rule (laya-finetune-gate-v1 1.1.0) on calibration-slice logits only.

    evaluate_gate(zs_probs, ft_probs, ft_probs_pre, y, calibration, f_avg_labels)
        macro-F1 / F_avg / house top-label ECE / NLL through metrics.py, thresholds READ from the
        contract (contract.thresholds()), pass = margin >= min_macro_f1_margin AND ece_post <= max_ece.
        Returns the metric blocks of the gate report. A non-finite metric can never pass.
"""
import math
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import contract  # noqa: E402
import metrics  # noqa: E402


def softmax(z, t):
    z = np.asarray(z, dtype=np.float64) / float(t)
    z = z - z.max(1, keepdims=True)
    e = np.exp(z)
    return e / e.sum(1, keepdims=True)


def _dnll_dbeta(z, y, beta):
    p = softmax(z, 1.0 / beta)
    return float(((p * z).sum(1) - z[np.arange(len(y)), y]).mean())


def fit_temperature(z, y, t_min, t_max, iters=200):
    """(t_fitted, t_applied, clamp_hit) for logits z [N, K] and labels y [N]."""
    z = np.asarray(z, dtype=np.float64)
    y = np.asarray(y, dtype=np.int64)
    if z.ndim != 2 or len(y) != z.shape[0] or len(y) == 0:
        raise ValueError("fit_temperature needs z [N, K] and y [N], N >= 1")
    if not np.isfinite(z).all():
        raise ValueError("fit_temperature: non-finite logits")
    t_min, t_max = float(t_min), float(t_max)
    b_lo, b_hi = 1.0 / t_max, 1.0 / t_min           # beta range
    d_lo, d_hi = _dnll_dbeta(z, y, b_lo), _dnll_dbeta(z, y, b_hi)
    if d_lo >= 0.0:                                  # NLL rising already at T = t_max: optimum T >= t_max
        t_fit = t_max
    elif d_hi <= 0.0:                                # still falling at T = t_min: optimum T <= t_min
        t_fit = t_min
    else:
        lo, hi = b_lo, b_hi
        for _ in range(iters):
            mid = 0.5 * (lo + hi)
            if _dnll_dbeta(z, y, mid) < 0.0:
                lo = mid
            else:
                hi = mid
        t_fit = 1.0 / (0.5 * (lo + hi))
    t_applied = min(t_max, max(t_min, t_fit))
    clamp_hit = bool(t_fit <= t_min or t_fit >= t_max)
    return float(t_fit), float(t_applied), clamp_hit


class EarlyStopper:
    """laya-finetune-gate-v1 `early_stopping` / equation early_stopping_train_side, torch-free.

    Fed one calibration-slice monitor value per evaluated epoch (never an eval number). Epoch e
    improves iff m_e is finite and m_e < m_best - min_delta (strict, so ties keep the EARLIEST
    epoch); epochs before first_candidate_epoch (epoch 0 = the untrained base) are never candidates.
    Stops when e - best_epoch >= patience_epochs, or at max_epochs."""

    def __init__(self, decl, max_epochs):
        if (decl["monitor"], decl["mode"], decl["restore"], decl["tie_break"]) != (
                "calibration_nll_at_fitted_t", "min", "best", "earliest"):
            raise ValueError("EarlyStopper implements monitor calibration_nll_at_fitted_t, mode min, restore best, "
                             "tie_break earliest; the recipe declares %s" % (decl,))
        self.every = int(decl["eval_every_epochs"])
        self.first = int(decl["first_candidate_epoch"])
        self.patience = int(decl["patience_epochs"])
        self.min_delta = float(decl["min_delta"])
        self.max_epochs = int(max_epochs)
        if self.every < 1 or self.first < 1 or self.patience < 1 or self.min_delta < 0.0:
            raise ValueError("early_stopping needs eval_every_epochs, first_candidate_epoch, patience_epochs >= 1 "
                             "and min_delta >= 0")
        self.best_epoch, self.best = None, math.inf
        self.trace = []
        self.reason = None

    def evaluates(self, epoch):
        return epoch >= self.first and (epoch % self.every == 0 or epoch == self.max_epochs)

    def update(self, epoch, monitor, t_star=None):
        """Record epoch `epoch`; returns (improved, stop)."""
        m = float(monitor)
        improved = math.isfinite(m) and m < self.best - self.min_delta
        if improved:
            self.best, self.best_epoch = m, int(epoch)
        self.trace.append({"epoch": int(epoch), "monitor": m if math.isfinite(m) else None, "t_star": t_star})
        anchor = self.best_epoch if self.best_epoch is not None else self.first - 1
        if epoch >= self.max_epochs:
            self.reason = "max_epochs"
        elif epoch - anchor >= self.patience:
            self.reason = "patience"
        return improved, self.reason is not None

    def record(self, epochs_run):
        if self.best_epoch is None:
            raise ValueError("early stopping saw no finite calibration monitor value; nothing to restore")
        return {"rule": "early_stopping", "monitor": "calibration_nll_at_fitted_t", "per_epoch": list(self.trace),
                "best_epoch": self.best_epoch, "best_monitor": self.best, "epochs_run": int(epochs_run),
                "reason": self.reason or "max_epochs"}


def calibration_monitor(z, y, t_min, t_max):
    """(m, T*) = (NLL at the bounded fitted temperature, that temperature) -- `early_stopping.monitor`."""
    z = np.asarray(z, dtype=np.float64)
    if not np.isfinite(z).all():
        return float("nan"), None
    _, t_star, _ = fit_temperature(z, y, t_min, t_max)
    return metrics.nll(softmax(z, t_star), y), t_star


def _finite(x):
    return x is not None and isinstance(x, float) and math.isfinite(x)


def evaluate_gate(zs_probs, ft_probs, ft_probs_pre, y, f_avg_labels=None):
    """The metric blocks, margin and pass of a gate report (thresholds from the contract)."""
    th = contract.thresholds()
    bins = int(th["ece_bins"])
    y = np.asarray(y, dtype=np.int64)
    zs_P, ft_P, pre_P = (np.asarray(p, dtype=np.float64) for p in (zs_probs, ft_probs, ft_probs_pre))

    def fav(P):
        return None if f_avg_labels is None else metrics.f_avg(P, y, list(f_avg_labels))

    zs = {"macro_f1": metrics.macro_f1(zs_P, y), "f_avg": fav(zs_P),
          "ece": metrics.ece_top_label(zs_P, y, bins), "n": int(len(y))}
    ft = {"macro_f1": metrics.macro_f1(ft_P, y), "f_avg": fav(ft_P),
          "ece_pre": metrics.ece_top_label(pre_P, y, bins), "ece_post": metrics.ece_top_label(ft_P, y, bins),
          "nll": metrics.nll(ft_P, y), "n": int(len(y))}
    margin = ft["macro_f1"] - zs["macro_f1"]
    passed = bool(_finite(margin) and _finite(ft["ece_post"])
                  and margin >= float(th["min_macro_f1_margin"]) and ft["ece_post"] <= float(th["max_ece"]))
    return {"pass": passed, "thresholds": th, "zero_shot": zs, "fine_tuned": ft, "margin": margin}


if __name__ == "__main__":
    print("usage: python gate.py --selftest", file=sys.stderr)
    sys.exit(2)
