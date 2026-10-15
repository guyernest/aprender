#!/usr/bin/env python3
"""Agent-shaped cost of `apr serve` on CPU: prefill tok/s, decode tok/s, TTFT, RSS, turn-2 reuse,
concurrency. Stdlib only. One invocation = one running server (model x thread count).

    python3 -I tools/bench.py --base http://127.0.0.1:8040/v1 --model Qwen3.5-0.8B-Q4_K_M \
        --label 0.8B-t6 --threads 6 --pid <server pid> --out results/bench-0.8B-t6.json \
        --md results/bench-0.8B-t6.md --reps 2 --budget-s 300

Every request carries a nonce so the server's session KV state (which reuses a prompt that
EXTENDS the previous one) never helps by accident; the `turn2_*` shapes are the controlled
experiment on that state. Streaming is used everywhere so TTFT (prefill) and decode are read
from chunk timestamps, not inferred.
"""
import argparse
import concurrent.futures as cf
import http.client
import json
import os
import statistics
import subprocess
import sys
import time
from urllib.parse import urlparse

SYSTEM = ("You are a helpful assistant for an operations team. Use the tools when a question "
          "needs live data; otherwise answer directly. Be concise.")

TOOLS2 = [
    {"type": "function", "function": {"name": "get_weather", "description": "Get the current weather for a city.",
     "parameters": {"type": "object", "properties": {"city": {"type": "string", "description": "City name, e.g. 'Paris'"},
                                                     "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}},
                    "required": ["city"]}}},
    {"type": "function", "function": {"name": "add", "description": "Add two numbers and return the sum.",
     "parameters": {"type": "object", "properties": {"a": {"type": "number"}, "b": {"type": "number"}}, "required": ["a", "b"]}}},
]
TOOLS10 = TOOLS2 + [
    {"type": "function", "function": {"name": n, "description": d,
     "parameters": {"type": "object", "properties": {k: {"type": t} for k, t in props}, "required": [k for k, _ in props]}}}
    for n, d, props in [
        ("list_orders", "List orders for a customer id.", [("customer_id", "string")]),
        ("get_order", "Fetch one order by id.", [("order_id", "string")]),
        ("create_ticket", "Open a support ticket.", [("subject", "string"), ("body", "string")]),
        ("search_docs", "Full-text search in the documentation.", [("query", "string")]),
        ("get_stock_price", "Current price of a stock ticker.", [("ticker", "string")]),
        ("convert_currency", "Convert an amount between currencies.", [("amount", "number"), ("from", "string"), ("to", "string")]),
        ("send_email", "Send an email.", [("to", "string"), ("subject", "string"), ("body", "string")]),
        ("get_time", "Current time in an IANA timezone.", [("timezone", "string")]),
    ]
]


def handbook(n_chars):
    """Deterministic English filler for a long system prompt, ~4.2 chars/token."""
    out, i = [], 0
    while sum(len(s) for s in out) < n_chars:
        i += 1
        out.append(f"Policy {i}: Tickets of priority {i % 5} are acknowledged within {(i * 7) % 60 + 1} minutes and "
                   f"escalated to the on-call engineer for region {['eu-west', 'us-east', 'ap-south'][i % 3]} when the "
                   f"customer tier is {['bronze', 'silver', 'gold'][i % 3]}; refunds above {i * 10} euros need a second approver. ")
    return "".join(out)


HANDBOOK_2K = handbook(7800)
HANDBOOK_4K = handbook(15800)


def rss_mb(pid):
    if not pid:
        return None
    try:
        out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True, check=False).stdout.strip()
        return round(int(out) / 1024, 1) if out else None
    except (ValueError, OSError):
        return None


def stream_chat(base, body, timeout):
    """POST with stream=true; return timing + usage parsed from the SSE chunks."""
    u = urlparse(base)
    conn = http.client.HTTPConnection(u.hostname, u.port, timeout=timeout)
    body = dict(body, stream=True)
    data = json.dumps(body).encode()
    t0 = time.perf_counter()
    conn.request("POST", u.path.rstrip("/") + "/chat/completions", body=data,
                 headers={"Content-Type": "application/json", "Authorization": "Bearer spike-030"})
    resp = conn.getresponse()
    status = resp.status
    first_delta_t = None
    last_delta_t = None
    n_content_chunks = 0
    text = []
    tool_calls = []
    finish = None
    usage = None
    raw_err = b""
    if status != 200:
        raw_err = resp.read()
    else:
        for line in resp:
            if not line.startswith(b"data:"):
                continue
            payload = line[5:].strip()
            if payload == b"[DONE]":
                break
            try:
                ev = json.loads(payload)
            except json.JSONDecodeError:
                continue
            now = time.perf_counter()
            ch = (ev.get("choices") or [{}])[0]
            d = ch.get("delta", {})
            if d.get("content") or d.get("tool_calls"):
                if first_delta_t is None:
                    first_delta_t = now
                last_delta_t = now
                n_content_chunks += 1
                if d.get("content"):
                    text.append(d["content"])
                if d.get("tool_calls"):
                    tool_calls.extend(d["tool_calls"])
            if ch.get("finish_reason"):
                finish = ch["finish_reason"]
            if ev.get("usage"):
                usage = ev["usage"]
    t_end = time.perf_counter()
    conn.close()
    pt = (usage or {}).get("prompt_tokens")
    ct = (usage or {}).get("completion_tokens")
    ttft = (first_delta_t - t0) if first_delta_t else None
    decode_s = (last_delta_t - first_delta_t) if (first_delta_t and last_delta_t and last_delta_t > first_delta_t) else None
    return {
        "status": status, "total_s": round(t_end - t0, 3), "ttft_s": round(ttft, 3) if ttft else None,
        "prompt_tokens": pt, "completion_tokens": ct, "finish": finish, "chunks": n_content_chunks,
        "prefill_tok_s": round(pt / ttft, 1) if (pt and ttft) else None,
        "decode_tok_s": round((ct - 1) / decode_s, 1) if (ct and ct > 1 and decode_s) else None,
        "text": "".join(text)[:160], "tool_calls": [(t.get("function") or {}).get("name") for t in tool_calls],
        "error": raw_err.decode(errors="replace")[:200] if raw_err else None,
    }


def nonce():
    return f"[ref {time.time_ns() % 10_000_000}]"


def shape_short(model):
    return {"model": model, "max_tokens": 128, "temperature": 0.0,
            "messages": [{"role": "system", "content": SYSTEM},
                         {"role": "user", "content": f"In three sentences, explain what an MCP server is and why an agent would use one. {nonce()}"}]}


def shape_tools2(model):
    return {"model": model, "max_tokens": 128, "temperature": 0.0, "tools": TOOLS2, "tool_choice": "auto",
            "messages": [{"role": "system", "content": SYSTEM},
                         {"role": "user", "content": f"What's the weather in Paris right now? {nonce()}"}]}


def shape_tools10(model):
    return {"model": model, "max_tokens": 128, "temperature": 0.0, "tools": TOOLS10, "tool_choice": "auto",
            "messages": [{"role": "system", "content": SYSTEM},
                         {"role": "user", "content": f"Convert 250 euros to US dollars. {nonce()}"}]}


def shape_long(model, text):
    return {"model": model, "max_tokens": 64, "temperature": 0.0,
            "messages": [{"role": "system", "content": SYSTEM + "\n\nOperations handbook:\n" + text},
                         {"role": "user", "content": f"Which region handles gold-tier priority 3 tickets, per the handbook? One sentence. {nonce()}"}]}


def shape_answer(model, max_tokens=256):
    return {"model": model, "max_tokens": max_tokens, "temperature": 0.0,
            "messages": [{"role": "system", "content": SYSTEM},
                         {"role": "user", "content": f"Write a 200-word briefing on why rivers matter to cities. {nonce()}"}]}


def median(xs):
    xs = [x for x in xs if x is not None]
    return round(statistics.median(xs), 2) if xs else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--label", required=True)
    ap.add_argument("--threads", type=int, required=True)
    ap.add_argument("--pid", type=int, default=0)
    ap.add_argument("--out", required=True)
    ap.add_argument("--md", required=True)
    ap.add_argument("--reps", type=int, default=2)
    ap.add_argument("--budget-s", type=float, default=300.0)
    ap.add_argument("--timeout", type=float, default=1800.0)
    ap.add_argument("--shapes", default="short,answer256,tools2,tools10,long2k,long4k",
                    help="comma-separated subset of the request shapes")
    ap.add_argument("--no-turn", action="store_true", help="skip the turn-2 reuse experiment")
    ap.add_argument("--no-conc", action="store_true", help="skip the concurrency experiment")
    args = ap.parse_args()
    shapes = set(args.shapes.split(","))

    records = []
    rows = []
    skip_longer = False

    def run_shape(name, make, reps):
        nonlocal skip_longer
        if skip_longer and name in ("long2k", "long4k", "answer256"):
            rows.append((name, "skipped (budget)", None, None, None, None, None, None, None))
            return []
        rs = []
        for i in range(reps):
            before = rss_mb(args.pid)
            r = stream_chat(args.base, make(args.model), args.timeout)
            r["rss_mb_after"] = rss_mb(args.pid)
            r["rss_mb_before"] = before
            r["shape"] = name
            r["rep"] = i
            rs.append(r)
            records.append(r)
            print(f"  {name} rep{i}: {r['status']} total={r['total_s']}s ttft={r['ttft_s']} pt={r['prompt_tokens']} "
                  f"ct={r['completion_tokens']} prefill={r['prefill_tok_s']} decode={r['decode_tok_s']} "
                  f"finish={r['finish']} tools={r['tool_calls']} rss={r['rss_mb_after']}MB", file=sys.stderr, flush=True)
            if r["total_s"] > args.budget_s:
                skip_longer = True
                break
        ok = [r for r in rs if r["status"] == 200]
        rows.append((name, f"{len(ok)}/{len(rs)}", median([r["prompt_tokens"] for r in ok]), median([r["completion_tokens"] for r in ok]),
                     median([r["ttft_s"] for r in ok]), median([r["prefill_tok_s"] for r in ok]), median([r["decode_tok_s"] for r in ok]),
                     median([r["total_s"] for r in ok]), max((r["rss_mb_after"] or 0) for r in rs) or None))
        return rs

    print(f"[bench] {args.label} threads={args.threads} base={args.base}", file=sys.stderr, flush=True)
    # warm-up (first request in a process pays one-off costs)
    w = stream_chat(args.base, shape_short(args.model), args.timeout)
    w["shape"] = "warmup"
    records.append(w)
    print(f"  warmup: {w['status']} total={w['total_s']}s ttft={w['ttft_s']} pt={w['prompt_tokens']} ct={w['completion_tokens']}", file=sys.stderr, flush=True)

    if "short" in shapes:
        run_shape("short", shape_short, args.reps)
    if "answer256" in shapes:
        run_shape("answer256", lambda m: shape_answer(m, 256), args.reps)
    if "tools2" in shapes:
        run_shape("tools2", shape_tools2, args.reps)
    if "tools10" in shapes:
        run_shape("tools10", shape_tools10, args.reps)
    if "long2k" in shapes:
        run_shape("long2k", lambda m: shape_long(m, HANDBOOK_2K), args.reps)
    if "long4k" in shapes:
        run_shape("long4k", lambda m: shape_long(m, HANDBOOK_4K), 1)

    # --- turn-2 reuse experiment ------------------------------------------------------------
    if not args.no_turn:
        t1_body = shape_tools2(args.model)
        t1 = stream_chat(args.base, t1_body, args.timeout)
        t1["shape"] = "turn1"
        records.append(t1)
        tc_name = t1["tool_calls"][0] if t1["tool_calls"] else "get_weather"
        t2_body = dict(t1_body)
        t2_body["messages"] = t1_body["messages"] + [
            {"role": "assistant", "content": "", "tool_calls": [{"id": "call_0", "type": "function",
                                                                  "function": {"name": tc_name, "arguments": json.dumps({"city": "Paris", "unit": "celsius"})}}]},
            {"role": "tool", "tool_call_id": "call_0", "content": json.dumps({"city": "Paris", "temperature_c": 17, "condition": "light rain"})},
        ]
        t2a = stream_chat(args.base, t2_body, args.timeout)
        t2a["shape"] = "turn2_extend"
        records.append(t2a)
        foreign = stream_chat(args.base, shape_short(args.model), args.timeout)
        foreign["shape"] = "foreign_between"
        records.append(foreign)
        t2b = stream_chat(args.base, t2_body, args.timeout)
        t2b["shape"] = "turn2_after_foreign"
        records.append(t2b)
        for r in (t1, t2a, foreign, t2b):
            print(f"  {r['shape']}: total={r['total_s']}s ttft={r['ttft_s']} pt={r['prompt_tokens']} ct={r['completion_tokens']} "
                  f"prefill={r['prefill_tok_s']} finish={r['finish']} text={r['text'][:60]!r}", file=sys.stderr, flush=True)
            rows.append((r["shape"], "1/1" if r["status"] == 200 else "0/1", r["prompt_tokens"], r["completion_tokens"], r["ttft_s"],
                         r["prefill_tok_s"], r["decode_tok_s"], r["total_s"], rss_mb(args.pid)))

    # --- concurrency ---------------------------------------------------------------------------
    conc = {}
    if not args.no_conc:
        for n in (2, 4):
            bodies = [shape_short(args.model) for _ in range(n)]
            for b in bodies:
                b["max_tokens"] = 64
            t0 = time.perf_counter()
            with cf.ThreadPoolExecutor(max_workers=n) as ex:
                rs = list(ex.map(lambda b: stream_chat(args.base, b, args.timeout), bodies))
            wall = time.perf_counter() - t0
            ok = [r for r in rs if r["status"] == 200]
            tot_ct = sum((r["completion_tokens"] or 0) for r in ok)
            conc[n] = {"wall_s": round(wall, 2), "per_request_total_s": [r["total_s"] for r in rs], "ttft_s": [r["ttft_s"] for r in rs],
                       "statuses": [r["status"] for r in rs], "aggregate_decode_tok_s_over_wall": round(tot_ct / wall, 1) if wall else None,
                       "errors": [r["error"] for r in rs if r["error"]]}
            for r in rs:
                r["shape"] = f"conc{n}"
                records.append(r)
            print(f"  conc{n}: wall={wall:.2f}s per-request={[r['total_s'] for r in rs]} ttft={[r['ttft_s'] for r in rs]} statuses={[r['status'] for r in rs]}",
                  file=sys.stderr, flush=True)
            rows.append((f"conc{n} (wall)", f"{len(ok)}/{n}", None, tot_ct, median([r["ttft_s"] for r in ok]), None,
                         conc[n]["aggregate_decode_tok_s_over_wall"], round(wall, 2), rss_mb(args.pid)))

    with open(args.out, "w") as f:
        json.dump({"label": args.label, "model": args.model, "threads": args.threads, "base": args.base,
                   "records": records, "concurrency": conc}, f, indent=1)
    with open(args.md, "w") as f:
        f.write(f"### {args.label}: `{args.model}`, RAYON_NUM_THREADS={args.threads}\n\n")
        f.write("| shape | ok | prompt tok | compl tok | TTFT s | prefill tok/s | decode tok/s | total s | RSS MB |\n|---|---|---|---|---|---|---|---|---|\n")
        for r in rows:
            f.write("| " + " | ".join("" if x is None else str(x) for x in r) + " |\n")
    print(open(args.md).read())


if __name__ == "__main__":
    main()
