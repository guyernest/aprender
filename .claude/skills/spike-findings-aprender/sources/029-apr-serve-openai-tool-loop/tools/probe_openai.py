#!/usr/bin/env python3
"""OpenAI chat-completions contract probe against an `apr serve` endpoint.

Stdlib only. Replays the request shapes pmcp.run's two clients send
(pmcp-agent `OpenAiCompatSource` and durable-agent-lambda `openai_v1`) and records
every request/response pair with timings to a JSON file, printing a Markdown table.

    python3 -I tools/probe_openai.py --base http://127.0.0.1:8029/v1 --model Qwen3.5-0.8B-Q4_K_M \
        --out results/probe-0.8b.json
"""
import argparse
import json
import sys
import time
import urllib.error
import urllib.request

TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "get_weather",
            "description": "Get the current weather for a city.",
            "parameters": {
                "type": "object",
                "properties": {
                    "city": {"type": "string", "description": "City name, e.g. 'Paris'"},
                    "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]},
                },
                "required": ["city"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "add",
            "description": "Add two numbers and return the sum.",
            "parameters": {
                "type": "object",
                "properties": {"a": {"type": "number"}, "b": {"type": "number"}},
                "required": ["a", "b"],
            },
        },
    },
]

# Ten MCP-shaped tools, the size of a real server's tools/list (names + schemas only).
MANY_TOOLS = TOOLS + [
    {
        "type": "function",
        "function": {
            "name": name,
            "description": desc,
            "parameters": {
                "type": "object",
                "properties": {k: {"type": t} for k, t in props},
                "required": [k for k, _ in props],
            },
        },
    }
    for name, desc, props in [
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

SYSTEM = (
    "You are a helpful assistant for an operations team. Use the tools when a question "
    "needs live data; otherwise answer directly. Be concise."
)


def post(base, path, body, key, timeout=600):
    req = urllib.request.Request(
        base + path,
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "Authorization": f"Bearer {key}"},
        method="POST",
    )
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            raw = r.read()
            dt = time.perf_counter() - t0
            return r.status, dict(r.headers), raw, dt
    except urllib.error.HTTPError as e:
        raw = e.read()
        return e.code, dict(e.headers), raw, time.perf_counter() - t0


def get(base, path, key, timeout=30):
    req = urllib.request.Request(base + path, headers={"Authorization": f"Bearer {key}"})
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.read(), time.perf_counter() - t0
    except urllib.error.HTTPError as e:
        return e.code, e.read(), time.perf_counter() - t0


def parse_sse(raw):
    """Return the list of JSON data events and whether [DONE] was seen."""
    events, done = [], False
    for line in raw.decode(errors="replace").splitlines():
        if not line.startswith("data:"):
            continue
        payload = line[5:].strip()
        if payload == "[DONE]":
            done = True
            continue
        try:
            events.append(json.loads(payload))
        except json.JSONDecodeError:
            events.append({"_unparsed": payload})
    return events, done


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True, help="e.g. http://127.0.0.1:8029/v1")
    ap.add_argument("--model", required=True)
    ap.add_argument("--key", default="spike-029-not-a-secret")
    ap.add_argument("--out", required=True)
    ap.add_argument("--max-tokens", type=int, default=256)
    args = ap.parse_args()

    rows = []
    record = []

    def add(name, verdict, status, dt, note, req=None, resp=None):
        rows.append((name, verdict, status, f"{dt:.2f}", note))
        record.append({"probe": name, "verdict": verdict, "status": status, "seconds": dt,
                       "note": note, "request": req, "response": resp})
        print(f"  {verdict} {name}: {status} {dt:.2f}s {note}", file=sys.stderr)

    def body_json(raw):
        try:
            return json.loads(raw)
        except json.JSONDecodeError:
            return {"_raw": raw.decode(errors="replace")[:2000]}

    # --- P0: discovery routes -------------------------------------------------------------
    for path in ("/models", "/effective-config"):
        st, raw, dt = get(args.base, path, args.key)
        js = body_json(raw)
        ok = st == 200
        if path == "/models" and ok:
            ids = [m.get("id") for m in js.get("data", [])] if isinstance(js, dict) else []
            note = f"ids={ids}"
        else:
            note = (json.dumps(js)[:160] if not ok else "ok")
        add(f"GET {path}", "✓" if ok else "✗", st, dt, note, resp=js)

    # --- P1: plain chat, the pmcp-agent shape (model, messages, max_tokens, temperature) ---
    req = {"model": args.model, "messages": [{"role": "system", "content": SYSTEM},
                                              {"role": "user", "content": "In one sentence, what does an MCP server do?"}],
           "max_tokens": args.max_tokens, "temperature": 0.2}
    st, hdr, raw, dt = post(args.base, "/chat/completions", req, args.key)
    js = body_json(raw)
    msg = (js.get("choices") or [{}])[0].get("message", {}) if isinstance(js, dict) else {}
    fields = sorted(js.keys()) if isinstance(js, dict) else []
    usage = js.get("usage") if isinstance(js, dict) else None
    ok = st == 200 and bool(msg.get("content"))
    add("chat plain", "✓" if ok else "✗", st, dt,
        f"fields={fields} usage={usage} finish={(js.get('choices') or [{}])[0].get('finish_reason') if isinstance(js, dict) else None}",
        req, js)

    # --- P2: tools, tool_choice auto -> expect a tool call -------------------------------
    req = {"model": args.model, "messages": [{"role": "system", "content": SYSTEM},
                                              {"role": "user", "content": "What's the weather in Paris right now?"}],
           "tools": TOOLS, "tool_choice": "auto", "max_tokens": args.max_tokens, "temperature": 0.0}
    st, hdr, raw, dt = post(args.base, "/chat/completions", req, args.key)
    js = body_json(raw)
    ch = (js.get("choices") or [{}])[0] if isinstance(js, dict) else {}
    msg = ch.get("message", {})
    tcs = msg.get("tool_calls") or []
    call_ok = False
    call_note = f"finish={ch.get('finish_reason')} content={json.dumps(msg.get('content'))[:120]}"
    if tcs:
        fn = tcs[0].get("function", {})
        try:
            argsd = json.loads(fn.get("arguments", "")) if isinstance(fn.get("arguments"), str) else fn.get("arguments")
        except json.JSONDecodeError:
            argsd = {"_unparsed": fn.get("arguments")}
        call_ok = fn.get("name") == "get_weather" and isinstance(argsd, dict) and "city" in argsd
        call_note = (f"finish={ch.get('finish_reason')} id={tcs[0].get('id')} type={tcs[0].get('type')} "
                     f"name={fn.get('name')} args={argsd} args_is_str={isinstance(fn.get('arguments'), str)}")
    add("tools auto -> tool_call", "✓" if call_ok else "✗", st, dt, call_note, req, js)

    # --- P3: the tool-result turn (assistant tool_calls echoed + role:tool) -> final answer -
    if tcs:
        assistant_turn = {"role": "assistant", "tool_calls": [{
            "id": tcs[0].get("id") or "call_0", "type": "function",
            "function": {"name": "get_weather", "arguments": json.dumps({"city": "Paris"})}}]}
        req3 = {"model": args.model, "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": "What's the weather in Paris right now?"},
            assistant_turn,
            {"role": "tool", "tool_call_id": tcs[0].get("id") or "call_0",
             "content": json.dumps({"city": "Paris", "temperature_c": 17, "condition": "light rain"})},
        ], "tools": TOOLS, "tool_choice": "auto", "max_tokens": args.max_tokens, "temperature": 0.0}
        st, hdr, raw, dt = post(args.base, "/chat/completions", req3, args.key)
        js3 = body_json(raw)
        ch3 = (js3.get("choices") or [{}])[0] if isinstance(js3, dict) else {}
        m3 = ch3.get("message", {})
        content = m3.get("content") or ""
        ok3 = st == 200 and ("17" in content or "rain" in content.lower()) and not m3.get("tool_calls")
        add("tool result -> final answer", "✓" if ok3 else "✗", st, dt,
            f"finish={ch3.get('finish_reason')} tool_calls={bool(m3.get('tool_calls'))} content={json.dumps(content)[:160]}",
            req3, js3)
    else:
        add("tool result -> final answer", "○", 0, 0.0, "skipped: no tool call in P2")

    # --- P4: tool_choice required / none ---------------------------------------------------
    for mode, q, want_call in (("required", "Say hello.", True), ("none", "What's the weather in Paris?", False)):
        req = {"model": args.model, "messages": [{"role": "user", "content": q}], "tools": TOOLS,
               "tool_choice": mode, "max_tokens": args.max_tokens, "temperature": 0.0}
        st, hdr, raw, dt = post(args.base, "/chat/completions", req, args.key)
        js = body_json(raw)
        ch = (js.get("choices") or [{}])[0] if isinstance(js, dict) else {}
        got_call = bool(ch.get("message", {}).get("tool_calls"))
        ok = st == 200 and got_call == want_call
        add(f"tool_choice={mode}", "✓" if ok else "✗", st, dt,
            f"finish={ch.get('finish_reason')} got_call={got_call} want_call={want_call} "
            f"err={json.dumps(js.get('error'))[:120] if isinstance(js, dict) and js.get('error') else ''}", req, js)

    # --- P5: ten tools, pick the right one -------------------------------------------------
    req = {"model": args.model, "messages": [{"role": "system", "content": SYSTEM},
                                              {"role": "user", "content": "Convert 250 euros to US dollars."}],
           "tools": MANY_TOOLS, "tool_choice": "auto", "max_tokens": args.max_tokens, "temperature": 0.0}
    st, hdr, raw, dt = post(args.base, "/chat/completions", req, args.key)
    js = body_json(raw)
    ch = (js.get("choices") or [{}])[0] if isinstance(js, dict) else {}
    tcs5 = ch.get("message", {}).get("tool_calls") or []
    name5 = tcs5[0]["function"]["name"] if tcs5 else None
    add("10 tools -> picks convert_currency", "✓" if name5 == "convert_currency" else "✗", st, dt,
        f"finish={ch.get('finish_reason')} name={name5} args={tcs5[0]['function'].get('arguments') if tcs5 else None} "
        f"prompt_tokens={(js.get('usage') or {}).get('prompt_tokens') if isinstance(js, dict) else None}", req, js)

    # --- P6: streaming with tools (the 0.70.3 change) ---------------------------------------
    req = {"model": args.model, "messages": [{"role": "user", "content": "What is 17 plus 25? Use the tool."}],
           "tools": TOOLS, "tool_choice": "auto", "stream": True, "max_tokens": args.max_tokens, "temperature": 0.0}
    st, hdr, raw, dt = post(args.base, "/chat/completions", req, args.key)
    events, done = parse_sse(raw)
    deltas_tc = [e for e in events if (e.get("choices") or [{}])[0].get("delta", {}).get("tool_calls")]
    finishes = [(e.get("choices") or [{}])[0].get("finish_reason") for e in events]
    finishes = [f for f in finishes if f]
    ok = st == 200 and bool(deltas_tc) and "tool_calls" in finishes
    add("stream + tools", "✓" if ok else "✗", st, dt,
        f"ctype={hdr.get('Content-Type') or hdr.get('content-type')} events={len(events)} tool_call_deltas={len(deltas_tc)} "
        f"finish={finishes} done={done}", req, {"events": events[:6] + (["..."] if len(events) > 6 else []), "done": done})

    # --- P7: streaming plain (TTFT shape) --------------------------------------------------
    req = {"model": args.model, "messages": [{"role": "user", "content": "Count from one to ten in words."}],
           "stream": True, "max_tokens": 64, "temperature": 0.0}
    st, hdr, raw, dt = post(args.base, "/chat/completions", req, args.key)
    events, done = parse_sse(raw)
    text = "".join(((e.get("choices") or [{}])[0].get("delta", {}).get("content") or "") for e in events)
    has_usage = any("usage" in e and e["usage"] for e in events)
    add("stream plain", "✓" if st == 200 and events and done else "✗", st, dt,
        f"events={len(events)} done={done} usage_in_stream={has_usage} text={json.dumps(text)[:80]}", req,
        {"n_events": len(events), "first": events[:2], "last": events[-2:]})

    # --- P8: fields other clients send (should be tolerated, not 400) -----------------------
    req = {"model": args.model, "messages": [{"role": "user", "content": "Hi"}], "max_tokens": 16,
           "stream_options": {"include_usage": True}, "parallel_tool_calls": False, "n": 1, "user": "spike",
           "response_format": {"type": "text"}, "seed": 7, "top_p": 0.9, "presence_penalty": 0.0,
           "frequency_penalty": 0.0, "logit_bias": {}, "metadata": {"x": "y"}, "store": False}
    st, hdr, raw, dt = post(args.base, "/chat/completions", req, args.key)
    js = body_json(raw)
    add("extra OpenAI fields tolerated", "✓" if st == 200 else "✗", st, dt,
        f"err={json.dumps(js.get('error'))[:200] if isinstance(js, dict) and js.get('error') else 'ok'}", req, js)

    # --- P9: max_completion_tokens (the newer OpenAI name) ----------------------------------
    req = {"model": args.model, "messages": [{"role": "user", "content": "Write 200 words about rivers."}],
           "max_completion_tokens": 12}
    st, hdr, raw, dt = post(args.base, "/chat/completions", req, args.key)
    js = body_json(raw)
    ct = ((js.get("usage") or {}).get("completion_tokens") if isinstance(js, dict) else None)
    add("max_completion_tokens honoured", "✓" if st == 200 and ct is not None and ct <= 16 else "✗", st, dt,
        f"completion_tokens={ct} finish={(js.get('choices') or [{}])[0].get('finish_reason') if isinstance(js, dict) else None}", req, js)

    # --- P10: error envelope ----------------------------------------------------------------
    req = {"model": args.model, "messages": "not-a-list"}
    st, hdr, raw, dt = post(args.base, "/chat/completions", req, args.key)
    js = body_json(raw)
    openai_shape = isinstance(js, dict) and isinstance(js.get("error"), dict) and "message" in js["error"]
    add("error envelope {error:{message,type}}", "✓" if 400 <= st < 500 and openai_shape else "✗", st, dt,
        json.dumps(js)[:200], req, js)

    # --- P11: wrong model id -> what happens? -------------------------------------------------
    req = {"model": "gpt-4o-mini", "messages": [{"role": "user", "content": "Hi"}], "max_tokens": 8}
    st, hdr, raw, dt = post(args.base, "/chat/completions", req, args.key)
    js = body_json(raw)
    served = (js.get("model") if isinstance(js, dict) else None)
    add("foreign model id", "ℹ", st, dt, f"served_as={served} status={st}", req, js)

    # --- P12: no Authorization header -> auth enforced? ------------------------------------
    req2 = urllib.request.Request(args.base + "/chat/completions",
                                  data=json.dumps({"model": args.model, "messages": [{"role": "user", "content": "Hi"}], "max_tokens": 4}).encode(),
                                  headers={"Content-Type": "application/json"}, method="POST")
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(req2, timeout=120) as r:
            st = r.status
    except urllib.error.HTTPError as e:
        st = e.code
    add("no Authorization header", "ℹ", st, time.perf_counter() - t0,
        "accepted (no auth at the server; the front door must enforce it)" if st == 200 else "rejected")

    with open(args.out, "w") as f:
        json.dump({"base": args.base, "model": args.model, "probes": record}, f, indent=1)

    print(f"\n### Contract probe: `{args.model}` at `{args.base}`\n")
    print("| Probe | Verdict | HTTP | s | Notes |")
    print("|---|---|---|---|---|")
    for r in rows:
        print("| " + " | ".join(str(x).replace("|", "\\|") for x in r) + " |")
    print(f"\nFull record: `{args.out}`")


if __name__ == "__main__":
    main()
