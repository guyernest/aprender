#!/usr/bin/env python3
"""Aggregate results/bench-*.json into one cross-config table plus the agent-turn model.

    python3 -I tools/summarize.py results/bench-*.json > results/SUMMARY.md

Agent-turn model (per config): T(turn) = prompt_tokens / prefill_tok_s + output_tokens / decode_tok_s,
evaluated for pmcp.run's measured shapes (029 wire logs): a 2-tool turn (390 prompt, 38 out), a
10-tool turn (926 prompt, 53 out) and a 2k-token handbook turn (64 out). Graviton2 column = M4 x 3.2
(the ratio measured twice at 6 threads in spikes 021 and 026) — a projection, marked est.
"""
import json
import statistics
import sys

G2_SLOWDOWN = 3.2


def med(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else None


def fmt(x, nd=1):
    return "" if x is None else (f"{x:.{nd}f}" if isinstance(x, float) else str(x))


def main(paths):
    rows = []
    for p in sorted(paths):
        d = json.load(open(p))
        recs = [r for r in d["records"] if r.get("status") == 200]
        by = {}
        for r in recs:
            by.setdefault(r["shape"], []).append(r)
        pre = med([r["prefill_tok_s"] for s in ("short", "answer256", "tools2", "tools10", "long2k", "long4k") for r in by.get(s, [])])
        dec = med([r["decode_tok_s"] for s in ("short", "answer256", "tools2", "tools10") for r in by.get(s, [])])
        # prefill rate by prompt length (does it change with length? token-at-a-time would be flat)
        pre_by_len = {s: (med([r["prompt_tokens"] for r in by.get(s, [])]), med([r["prefill_tok_s"] for r in by.get(s, [])]))
                      for s in ("short", "tools2", "tools10", "long2k", "long4k") if by.get(s)}
        rss = max((r.get("rss_mb_after") or 0) for r in d["records"]) or None
        t2e = by.get("turn2_extend", [{}])[0]
        t2f = by.get("turn2_after_foreign", [{}])[0]
        t1 = by.get("turn1", [{}])[0]
        conc = d.get("concurrency", {})
        short_total = med([r["total_s"] for r in by.get("short", [])])
        rows.append({
            "label": d["label"], "threads": d["threads"], "prefill": pre, "decode": dec, "rss": rss,
            "pre_by_len": pre_by_len,
            "tools2_total": med([r["total_s"] for r in by.get("tools2", [])]),
            "tools10_total": med([r["total_s"] for r in by.get("tools10", [])]),
            "long2k_total": med([r["total_s"] for r in by.get("long2k", [])]),
            "long4k_total": med([r["total_s"] for r in by.get("long4k", [])]),
            "t1": t1, "t2e": t2e, "t2f": t2f,
            "conc2": conc.get("2") or conc.get(2), "conc4": conc.get("4") or conc.get(4), "short_total": short_total,
            "tools_ok": {s: [r["tool_calls"] for r in by.get(s, [])] for s in ("tools2", "tools10")},
        })

    out = []
    out.append("## Cross-config summary (medians; M4 Pro, alone on the host)\n")
    out.append("| config | prefill tok/s | decode tok/s | RSS MB | tools2 turn s (390 tok) | tools10 turn s (926 tok) | long2k s | long4k s |")
    out.append("|---|---|---|---|---|---|---|---|")
    for r in rows:
        out.append(f"| {r['label']} | {fmt(r['prefill'])} | {fmt(r['decode'])} | {fmt(r['rss'],0)} | {fmt(r['tools2_total'])} | {fmt(r['tools10_total'])} | {fmt(r['long2k_total'])} | {fmt(r['long4k_total'])} |")

    out.append("\n## Prefill rate by prompt length (flat = token-at-a-time)\n")
    out.append("| config | " + " | ".join(f"{s}" for s in ("short", "tools2", "tools10", "long2k", "long4k")) + " |")
    out.append("|---|" + "---|" * 5)
    for r in rows:
        cells = []
        for s in ("short", "tools2", "tools10", "long2k", "long4k"):
            v = r["pre_by_len"].get(s)
            cells.append(f"{fmt(v[1])} tok/s @ {fmt(v[0],0)}" if v else "")
        out.append(f"| {r['label']} | " + " | ".join(cells) + " |")

    out.append("\n## Turn-2 session reuse (TTFT s)\n")
    out.append("| config | turn1 TTFT (prompt tok) | turn2 extend TTFT (prompt tok) | turn2 after a foreign request TTFT | reuse factor |")
    out.append("|---|---|---|---|---|")
    for r in rows:
        t1, e, f = r["t1"], r["t2e"], r["t2f"]
        factor = (f.get("ttft_s") / e.get("ttft_s")) if (e.get("ttft_s") and f.get("ttft_s")) else None
        out.append(f"| {r['label']} | {fmt(t1.get('ttft_s'))} ({fmt(t1.get('prompt_tokens'),0)}) | {fmt(e.get('ttft_s'))} ({fmt(e.get('prompt_tokens'),0)}) | {fmt(f.get('ttft_s'))} | {fmt(factor)}x |")

    out.append("\n## Concurrency (short requests, 64 output tokens)\n")
    out.append("| config | 1 request total s | 2 at once: wall s / per-request s | 4 at once: wall s / per-request s | verdict |")
    out.append("|---|---|---|---|---|")
    for r in rows:
        c2, c4 = r["conc2"] or {}, r["conc4"] or {}
        def v(c, n):
            if not c:
                return ""
            return f"{fmt(c.get('wall_s'))} / {[round(x,1) for x in c.get('per_request_total_s', [])]}"
        verdict = ""
        if r["short_total"] and c2.get("wall_s"):
            ratio = c2["wall_s"] / r["short_total"]
            verdict = "serialises" if ratio > 1.6 else ("parallel" if ratio < 1.3 else "partial")
        out.append(f"| {r['label']} | {fmt(r['short_total'])} | {v(c2,2)} | {v(c4,4)} | {verdict} |")

    out.append("\n## Agent-turn model and Graviton2 projection (est., x3.2)\n")
    out.append("| config | 2-tool turn (390+38) M4 s | est. Graviton2 s | 10-tool turn (926+53) M4 s | est. Graviton2 s | 2k handbook (2000+64) M4 s | est. Graviton2 s |")
    out.append("|---|---|---|---|---|---|---|")
    for r in rows:
        if not (r["prefill"] and r["decode"]):
            continue
        def turn(p, o):
            return p / r["prefill"] + o / r["decode"]
        a, b, c = turn(390, 38), turn(926, 53), turn(2000, 64)
        out.append(f"| {r['label']} | {a:.0f} | {a*G2_SLOWDOWN:.0f} | {b:.0f} | {b*G2_SLOWDOWN:.0f} | {c:.0f} | {c*G2_SLOWDOWN:.0f} |")

    out.append("\n## Tool choice correctness\n")
    for r in rows:
        out.append(f"- {r['label']}: tools2 → {r['tools_ok']['tools2']}, tools10 → {r['tools_ok']['tools10']}")
    print("\n".join(out))


if __name__ == "__main__":
    main(sys.argv[1:])
