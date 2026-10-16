#!/usr/bin/env python3
"""OpenAI chat-completions conformance probe against an `apr serve` endpoint.

Stdlib only (run it as `python3 -I`; it imports nothing from PyPI). Replays the request shapes
pmcp.run's two clients send (pmcp-agent `OpenAiCompatSource` and durable-agent-lambda `openai_v1`),
records every request/response pair with timings to a JSON file, and prints a Markdown table.

This is the harness for contracts/llm-endpoint-openai-conformance-v1.yaml. The record it writes is
read by the Rust test api::llm_endpoint_conformance_tests::conformance_record_matches_contract; the
record format (the "probe-record contract") is stated in README.md and is NOT restated here.

    # open server (the pinned release as measured): baseline profile, unchanged spike verdicts
    python3 -I probe_openai.py --base http://127.0.0.1:8029/v1 --model Qwen3.5-0.8B-Q4_K_M \\
        --profile baseline --label release-local --pin v0.70.3 \\
        --server-version "apr 0.70.3 (95f64ba14)" --out results/conformance-v0.70.3-release-local.json

    # gated server (the D-04 auth gate present): fixed profile, key from APR_PROBE_KEY
    APR_PROBE_KEY=... python3 -I probe_openai.py --base http://127.0.0.1:8029/v1 \\
        --model Qwen3.5-0.8B-Q4_K_M --profile fixed --label release-local --pin v0.71.4 \\
        --server-version "apr 0.71.4 (abcdef123)" --out results/conformance-v0.71.4-release-local.json

The key is read from `--key` or, when that is absent, the APR_PROBE_KEY environment variable. It is
sent as a Bearer header and is NEVER written to the record: requests are recorded as bodies only,
and the record is refused (nothing written) if the key's value appears anywhere in it.
"""
import argparse
import datetime
import json
import os
import sys
import time
import urllib.error
import urllib.request

OK = "✓"  # check mark
CROSS = "✗"  # ballot x
INFO = "ℹ"  # information source
SKIP = "○"  # white circle
KNOWN_LIMITATION = "KNOWN LIMITATION"

# The probe names the reader asserts on (contract: probe-name rules).
P_TOOL_RESULT = "tool result -> final answer"
P_REQUIRED = "tool_choice=required"
P_NONE = "tool_choice=none"
P_MAX_COMPLETION = "max_completion_tokens honoured"
P_ENVELOPE = "error envelope {error:{message,type}}"
P_FOREIGN_MODEL = "foreign model id"
P_NO_AUTH = "no Authorization header"
P_CPU_PART = "cpu_part in /health"

# The five D-05 gaps that stay open: limitations, never fixes (README, "Known limitations").
DEFAULT_LIMITATIONS = [P_REQUIRED, P_ENVELOPE, P_FOREIGN_MODEL, P_NONE, P_CPU_PART]

# The two fixes that may be absent from the pinned release (D-03 content, D-05 alias). They may be
# recorded as a limitation only through an explicit --unreleased flag.
UNRELEASED_ELIGIBLE = [P_TOOL_RESULT, P_MAX_COMPLETION]

LABELS = ["baseline", "release-local", "interim", "deployed", "deployed-https"]
PROFILES = ["baseline", "fixed"]

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


def auth_headers(key):
    """Content-Type plus a Bearer header when a key is set. The value is never recorded."""
    headers = {"Content-Type": "application/json"}
    if key:
        headers["Authorization"] = "Bearer " + key
    return headers


def post(base, path, body, key, timeout=600):
    req = urllib.request.Request(
        base + path,
        data=json.dumps(body).encode(),
        headers=auth_headers(key),
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
    headers = {"Authorization": "Bearer " + key} if key else {}
    req = urllib.request.Request(base + path, headers=headers)
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.read(), time.perf_counter() - t0
    except urllib.error.HTTPError as e:
        return e.code, e.read(), time.perf_counter() - t0


def server_root(base):
    """`http://h:p/v1` -> `http://h:p` (the /health route lives outside /v1)."""
    trimmed = base.rstrip("/")
    return trimmed[: -len("/v1")] if trimmed.endswith("/v1") else trimmed


class HttpTransport:
    """The probe's default transport: plain HTTP to --base. The private-bench transport (lambda:Invoke,
    deploy/llm-endpoint/probe/bench.py --conformance) implements the same four methods, so the request
    bodies and the verdict rules below run unchanged over either. `stream_note` is appended to the two
    streaming probes' notes: empty over HTTP, `buffered-through-adapter` over lambda:Invoke."""

    stream_note = ""

    def __init__(self, base):
        self.base = base

    def get(self, path, key):
        return get(self.base, path, key)

    def get_root(self, path, key):
        return get(server_root(self.base), path, key)

    def post(self, path, body, key):
        return post(self.base, path, body, key)

    def post_noauth(self, path, body):
        req = urllib.request.Request(self.base + path, data=json.dumps(body).encode(),
                                     headers={"Content-Type": "application/json"}, method="POST")
        t0 = time.perf_counter()
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                st = r.status
        except urllib.error.HTTPError as e:
            st = e.code
        return st, time.perf_counter() - t0


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


def parse_args(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--base", required=True, help="e.g. http://127.0.0.1:8029/v1")
    ap.add_argument("--model", required=True)
    ap.add_argument("--key", default=None,
                    help="bearer key; when absent, the APR_PROBE_KEY environment variable is used; "
                         "when that is unset too, no Authorization header is sent")
    ap.add_argument("--out", required=True)
    ap.add_argument("--max-tokens", type=int, default=256)
    ap.add_argument("--profile", required=True, choices=PROFILES,
                    help="baseline: spike-029 verdicts unchanged; fixed: the D-04 auth gate is "
                         "present, no cross remains, open gaps read KNOWN LIMITATION")
    ap.add_argument("--label", required=True, choices=LABELS,
                    help="what this record is: the probe-record contract's label set")
    ap.add_argument("--pin", required=True, help="release tag (vX.Y.Z), or upstream-main+PRs for interim")
    ap.add_argument("--server-version", required=True, dest="server_version",
                    help="the `apr --version` line of the server under test")
    ap.add_argument("--known-limitations", action="append", default=None, dest="known_limitations",
                    metavar="PROBE",
                    help="repeatable (a probe name contains commas, so there is no list syntax): a probe "
                         "name whose computed cross may be recorded as KNOWN LIMITATION (fixed profile "
                         "only). When absent the default is the five D-05 gaps: %s"
                         % "; ".join(DEFAULT_LIMITATIONS))
    ap.add_argument("--unreleased", action="append", default=[], metavar="PROBE=REFERENCE",
                    help="repeatable; fixed profile only; PROBE is one of %r. A computed cross on that "
                         "probe is recorded as KNOWN LIMITATION with an unreleased_at_pin entry "
                         "(reason: 'unreleased at pin <pin>: <reference>')" % (UNRELEASED_ELIGIBLE,))
    args = ap.parse_args(argv)

    args.unreleased_map = {}
    for item in args.unreleased:
        if "=" not in item:
            ap.error("--unreleased expects PROBE=REFERENCE, got %r" % item)
        name, ref = item.split("=", 1)
        name, ref = name.strip(), ref.strip()
        if args.profile != "fixed":
            ap.error("--unreleased is accepted only with --profile fixed")
        if name not in UNRELEASED_ELIGIBLE:
            ap.error("--unreleased is accepted only for %r, not %r (the D-04 auth gate is never "
                     "a limitation)" % (UNRELEASED_ELIGIBLE, name))
        if not ref:
            ap.error("--unreleased %r needs a non-empty reference (a PR, branch or issue)" % name)
        args.unreleased_map[name] = ref

    args.limitations = [n.strip() for n in (args.known_limitations or DEFAULT_LIMITATIONS) if n.strip()]
    for n in args.limitations:
        if n == P_NO_AUTH:
            ap.error("%r is never a limitation: an open server must not be recorded as fixed" % P_NO_AUTH)
        if n in UNRELEASED_ELIGIBLE:
            ap.error("%r may be a limitation only through --unreleased (it needs a reference)" % n)
    if args.key is None:
        args.key = os.environ.get("APR_PROBE_KEY", "")
    return args


def main(argv=None, tx=None, extra_header=None):
    """tx: a transport (default HttpTransport(--base)); extra_header: keys merged into the record header
    after `probes` is built (the private bench adds `transport: lambda-invoke`; the reader ignores it)."""
    args = parse_args(sys.argv[1:] if argv is None else argv)
    tx = tx or HttpTransport(args.base)
    key = args.key
    fixed = args.profile == "fixed"

    rows = []
    record = []
    limitations = []  # names actually recorded as KNOWN LIMITATION
    unreleased_at_pin = []

    def add(name, verdict, status, dt, note, req=None, resp=None):
        # A computed cross on a probe the profile allows as a limitation is recorded as KNOWN
        # LIMITATION; everything else keeps its computed verdict. Requests are recorded as bodies
        # only, so no Authorization value can reach the record.
        if fixed and verdict == CROSS:
            if name in args.unreleased_map:
                verdict = KNOWN_LIMITATION
                note = "%s [unreleased at pin %s: %s]" % (note, args.pin, args.unreleased_map[name])
                limitations.append(name)
                unreleased_at_pin.append({
                    "probe": name,
                    "reason": "unreleased at pin %s: %s" % (args.pin, args.unreleased_map[name]),
                })
            elif name in args.limitations:
                verdict = KNOWN_LIMITATION
                note = "%s [limitation recorded, not a fix]" % note
                limitations.append(name)
        if any(r["probe"] == name for r in record):
            raise SystemExit("probe %r emitted twice; probe names are unique per record" % name)
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
        st, raw, dt = tx.get(path, key)
        js = body_json(raw)
        ok = st == 200
        if path == "/models" and ok:
            ids = [m.get("id") for m in js.get("data", [])] if isinstance(js, dict) else []
            note = f"ids={ids}"
        else:
            note = (json.dumps(js)[:160] if not ok else "ok")
        add(f"GET {path}", OK if ok else CROSS, st, dt, note, resp=js)

    # --- P1: plain chat, the pmcp-agent shape (model, messages, max_tokens, temperature) ---
    req = {"model": args.model, "messages": [{"role": "system", "content": SYSTEM},
                                              {"role": "user", "content": "In one sentence, what does an MCP server do?"}],
           "max_tokens": args.max_tokens, "temperature": 0.2}
    st, hdr, raw, dt = tx.post("/chat/completions", req, key)
    js = body_json(raw)
    msg = (js.get("choices") or [{}])[0].get("message", {}) if isinstance(js, dict) else {}
    fields = sorted(js.keys()) if isinstance(js, dict) else []
    usage = js.get("usage") if isinstance(js, dict) else None
    ok = st == 200 and bool(msg.get("content"))
    add("chat plain", OK if ok else CROSS, st, dt,
        f"fields={fields} usage={usage} finish={(js.get('choices') or [{}])[0].get('finish_reason') if isinstance(js, dict) else None}",
        req, js)

    # --- P2: tools, tool_choice auto -> expect a tool call -------------------------------
    req = {"model": args.model, "messages": [{"role": "system", "content": SYSTEM},
                                              {"role": "user", "content": "What's the weather in Paris right now?"}],
           "tools": TOOLS, "tool_choice": "auto", "max_tokens": args.max_tokens, "temperature": 0.0}
    st, hdr, raw, dt = tx.post("/chat/completions", req, key)
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
    add("tools auto -> tool_call", OK if call_ok else CROSS, st, dt, call_note, req, js)

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
        st, hdr, raw, dt = tx.post("/chat/completions", req3, key)
        js3 = body_json(raw)
        ch3 = (js3.get("choices") or [{}])[0] if isinstance(js3, dict) else {}
        m3 = ch3.get("message", {})
        content = m3.get("content") or ""
        ok3 = st == 200 and ("17" in content or "rain" in content.lower()) and not m3.get("tool_calls")
        add(P_TOOL_RESULT, OK if ok3 else CROSS, st, dt,
            f"finish={ch3.get('finish_reason')} tool_calls={bool(m3.get('tool_calls'))} content={json.dumps(content)[:160]}",
            req3, js3)
    elif fixed:
        # A skip must not pass a fixed record: the required probe is a cross when it cannot run.
        add(P_TOOL_RESULT, CROSS, 0, 0.0, "not exercised: no tool call came back from the tools-auto probe")
    else:
        add(P_TOOL_RESULT, SKIP, 0, 0.0, "skipped: no tool call in P2")

    # --- P4: tool_choice required / none ---------------------------------------------------
    for mode, q, want_call in (("required", "Say hello.", True), ("none", "What's the weather in Paris?", False)):
        req = {"model": args.model, "messages": [{"role": "user", "content": q}], "tools": TOOLS,
               "tool_choice": mode, "max_tokens": args.max_tokens, "temperature": 0.0}
        st, hdr, raw, dt = tx.post("/chat/completions", req, key)
        js = body_json(raw)
        ch = (js.get("choices") or [{}])[0] if isinstance(js, dict) else {}
        got_call = bool(ch.get("message", {}).get("tool_calls"))
        ok = st == 200 and got_call == want_call
        add(f"tool_choice={mode}", OK if ok else CROSS, st, dt,
            f"finish={ch.get('finish_reason')} got_call={got_call} want_call={want_call} "
            f"err={json.dumps(js.get('error'))[:120] if isinstance(js, dict) and js.get('error') else ''}", req, js)

    # --- P5: ten tools, pick the right one -------------------------------------------------
    req = {"model": args.model, "messages": [{"role": "system", "content": SYSTEM},
                                              {"role": "user", "content": "Convert 250 euros to US dollars."}],
           "tools": MANY_TOOLS, "tool_choice": "auto", "max_tokens": args.max_tokens, "temperature": 0.0}
    st, hdr, raw, dt = tx.post("/chat/completions", req, key)
    js = body_json(raw)
    ch = (js.get("choices") or [{}])[0] if isinstance(js, dict) else {}
    tcs5 = ch.get("message", {}).get("tool_calls") or []
    name5 = tcs5[0]["function"]["name"] if tcs5 else None
    add("10 tools -> picks convert_currency", OK if name5 == "convert_currency" else CROSS, st, dt,
        f"finish={ch.get('finish_reason')} name={name5} args={tcs5[0]['function'].get('arguments') if tcs5 else None} "
        f"prompt_tokens={(js.get('usage') or {}).get('prompt_tokens') if isinstance(js, dict) else None}", req, js)

    # --- P6: streaming with tools (the 0.70.3 change) ---------------------------------------
    req = {"model": args.model, "messages": [{"role": "user", "content": "What is 17 plus 25? Use the tool."}],
           "tools": TOOLS, "tool_choice": "auto", "stream": True, "max_tokens": args.max_tokens, "temperature": 0.0}
    st, hdr, raw, dt = tx.post("/chat/completions", req, key)
    events, done = parse_sse(raw)
    deltas_tc = [e for e in events if (e.get("choices") or [{}])[0].get("delta", {}).get("tool_calls")]
    finishes = [(e.get("choices") or [{}])[0].get("finish_reason") for e in events]
    finishes = [f for f in finishes if f]
    ok = st == 200 and bool(deltas_tc) and "tool_calls" in finishes
    add("stream + tools", OK if ok else CROSS, st, dt,
        f"ctype={hdr.get('Content-Type') or hdr.get('content-type')} events={len(events)} tool_call_deltas={len(deltas_tc)} "
        f"finish={finishes} done={done}" + tx.stream_note, req, {"events": events[:6] + (["..."] if len(events) > 6 else []), "done": done})

    # --- P7: streaming plain (TTFT shape) --------------------------------------------------
    req = {"model": args.model, "messages": [{"role": "user", "content": "Count from one to ten in words."}],
           "stream": True, "max_tokens": 64, "temperature": 0.0}
    st, hdr, raw, dt = tx.post("/chat/completions", req, key)
    events, done = parse_sse(raw)
    text = "".join(((e.get("choices") or [{}])[0].get("delta", {}).get("content") or "") for e in events)
    has_usage = any("usage" in e and e["usage"] for e in events)
    add("stream plain", OK if st == 200 and events and done else CROSS, st, dt,
        f"events={len(events)} done={done} usage_in_stream={has_usage} text={json.dumps(text)[:80]}" + tx.stream_note, req,
        {"n_events": len(events), "first": events[:2], "last": events[-2:]})

    # --- P8: fields other clients send (should be tolerated, not 400) -----------------------
    req = {"model": args.model, "messages": [{"role": "user", "content": "Hi"}], "max_tokens": 16,
           "stream_options": {"include_usage": True}, "parallel_tool_calls": False, "n": 1, "user": "spike",
           "response_format": {"type": "text"}, "seed": 7, "top_p": 0.9, "presence_penalty": 0.0,
           "frequency_penalty": 0.0, "logit_bias": {}, "metadata": {"x": "y"}, "store": False}
    st, hdr, raw, dt = tx.post("/chat/completions", req, key)
    js = body_json(raw)
    add("extra OpenAI fields tolerated", OK if st == 200 else CROSS, st, dt,
        f"err={json.dumps(js.get('error'))[:200] if isinstance(js, dict) and js.get('error') else 'ok'}", req, js)

    # --- P9: max_completion_tokens (the newer OpenAI name) ----------------------------------
    req = {"model": args.model, "messages": [{"role": "user", "content": "Write 200 words about rivers."}],
           "max_completion_tokens": 12}
    st, hdr, raw, dt = tx.post("/chat/completions", req, key)
    js = body_json(raw)
    ct = ((js.get("usage") or {}).get("completion_tokens") if isinstance(js, dict) else None)
    add(P_MAX_COMPLETION, OK if st == 200 and ct is not None and ct <= 16 else CROSS, st, dt,
        f"completion_tokens={ct} finish={(js.get('choices') or [{}])[0].get('finish_reason') if isinstance(js, dict) else None}", req, js)

    # --- P10: error envelope ----------------------------------------------------------------
    req = {"model": args.model, "messages": "not-a-list"}
    st, hdr, raw, dt = tx.post("/chat/completions", req, key)
    js = body_json(raw)
    openai_shape = isinstance(js, dict) and isinstance(js.get("error"), dict) and "message" in js["error"]
    add(P_ENVELOPE, OK if 400 <= st < 500 and openai_shape else CROSS, st, dt,
        json.dumps(js)[:200], req, js)

    # --- P11: wrong model id -> what happens? -------------------------------------------------
    req = {"model": "gpt-4o-mini", "messages": [{"role": "user", "content": "Hi"}], "max_tokens": 8}
    st, hdr, raw, dt = tx.post("/chat/completions", req, key)
    js = body_json(raw)
    served = (js.get("model") if isinstance(js, dict) else None)
    if fixed:
        # The gap: `model` is echoed, not checked. A strict server refuses the foreign id (4xx).
        add(P_FOREIGN_MODEL, OK if 400 <= st < 500 else CROSS, st, dt,
            f"served_as={served} status={st}", req, js)
    else:
        add(P_FOREIGN_MODEL, INFO, st, dt, f"served_as={served} status={st}", req, js)

    # --- P12: no Authorization header -> auth enforced? ------------------------------------
    st, dt = tx.post_noauth("/chat/completions", {"model": args.model, "messages": [{"role": "user", "content": "Hi"}], "max_tokens": 4})
    if fixed:
        # D-04: the proof is a 401. An open server (200) turns the fixed table red; never a limitation.
        add(P_NO_AUTH, OK if st == 401 else CROSS, st, dt,
            "rejected 401 (the D-04 gate)" if st == 401 else "NOT rejected: the server is open, it must not be recorded as fixed")
    else:
        add(P_NO_AUTH, INFO, st, dt,
            "accepted (no auth at the server; the front door must enforce it)" if st == 200 else "rejected")

    # --- P13: cpu_part in /health ------------------------------------------------------------
    # The bearer is sent when the key is set: a fixed server gates /health (FALSIFY-AUTH-001).
    st, raw, dt = tx.get_root("/health", key)
    js = body_json(raw)
    has_cpu_part = st == 200 and isinstance(js, dict) and "cpu_part" in js
    note = "cpu_part=%s" % (js.get("cpu_part"),) if has_cpu_part else \
        "no cpu_part key in /health (the entrypoint [entry] log line carries it)"
    if has_cpu_part:
        verdict = OK
    elif fixed:
        verdict = CROSS  # becomes KNOWN LIMITATION through the limitation list
    else:
        verdict = INFO  # informational in the baseline profile, so the baseline cross set stays four
    add(P_CPU_PART, verdict, st, dt, note, resp=js if isinstance(js, dict) else None)

    out = {
        "label": args.label,
        "profile": args.profile,
        "pin": args.pin,
        "server_version": args.server_version,
        "generated_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "base": args.base,
        "model": args.model,
        "known_limitations": limitations,
        "unreleased_at_pin": unreleased_at_pin,
        "probes": record,
    }
    if extra_header:
        out.update(extra_header)
    dumped = json.dumps(out, indent=1, ensure_ascii=False)
    if key and key in dumped:
        # Fail closed: no key value, plaintext or otherwise, may reach a committed record.
        raise SystemExit("refusing to write %s: the bearer key's value appears in the record" % args.out)
    with open(args.out, "w", encoding="utf-8") as f:
        f.write(dumped + "\n")

    print(f"\n### Conformance probe ({args.profile}/{args.label}): `{args.model}` at `{args.base}`\n")
    print("| Probe | Verdict | HTTP | s | Notes |")
    print("|---|---|---|---|---|")
    for r in rows:
        print("| " + " | ".join(str(x).replace("|", "\\|") for x in r) + " |")
    print(f"\nLegend: {KNOWN_LIMITATION} = a gap recorded as a limitation, not a fix (contract rows 008-012); "
          f"{INFO} = informational in the baseline profile.")
    print(f"\nFull record: `{args.out}`")

    crosses = [r["probe"] for r in record if r["verdict"] == CROSS]
    if fixed and crosses:
        print(f"\nFAIL: fixed profile has {len(crosses)} cross(es): {crosses}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
