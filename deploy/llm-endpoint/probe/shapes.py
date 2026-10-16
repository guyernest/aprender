"""Request shapes and the streaming client, copied verbatim in spirit from spike 030's bench.py
(CONVENTIONS: copy helpers into the spike, don't import across spikes). Every prompt carries a
nonce so the server's session state never flatters a number by accident."""
import http.client
import json
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


def nonce():
    return f"[ref {time.time_ns() % 10_000_000}]"


def shape_short(model, max_tokens=64):
    return {"model": model, "max_tokens": max_tokens, "temperature": 0.0,
            "messages": [{"role": "system", "content": SYSTEM},
                         {"role": "user", "content": f"In three sentences, explain what an MCP server is and why an agent would use one. {nonce()}"}]}


def shape_tools2(model, max_tokens=64):
    return {"model": model, "max_tokens": max_tokens, "temperature": 0.0, "tools": TOOLS2, "tool_choice": "auto",
            "messages": [{"role": "system", "content": SYSTEM},
                         {"role": "user", "content": f"What's the weather in Paris right now? {nonce()}"}]}


def shape_tools10(model, max_tokens=64):
    return {"model": model, "max_tokens": max_tokens, "temperature": 0.0, "tools": TOOLS10, "tool_choice": "auto",
            "messages": [{"role": "system", "content": SYSTEM},
                         {"role": "user", "content": f"Convert 250 euros to US dollars. {nonce()}"}]}


def shape_turn2(turn1_body, tool_name="get_weather", max_tokens=64):
    """The agent's second turn: turn 1's messages + the assistant tool call (content "") + the tool result."""
    b = dict(turn1_body)
    b["max_tokens"] = max_tokens
    b["messages"] = turn1_body["messages"] + [
        {"role": "assistant", "content": "", "tool_calls": [{"id": "call_0", "type": "function",
                                                              "function": {"name": tool_name, "arguments": json.dumps({"city": "Paris", "unit": "celsius"})}}]},
        {"role": "tool", "tool_call_id": "call_0", "content": json.dumps({"city": "Paris", "temperature_c": 17, "condition": "light rain"})},
    ]
    return b


def stream_chat(base, body, timeout=900):
    """POST with stream=true over plain HTTP; TTFT from the first content/tool-call delta."""
    u = urlparse(base)
    conn = http.client.HTTPConnection(u.hostname, u.port or 80, timeout=timeout)
    body = dict(body, stream=True)
    t0 = time.perf_counter()
    conn.request("POST", u.path.rstrip("/") + "/chat/completions", body=json.dumps(body).encode(),
                 headers={"Content-Type": "application/json", "Authorization": "Bearer spike-031"})
    resp = conn.getresponse()
    status = resp.status
    first = last = None
    text, tool_calls, finish, usage, err = [], [], None, None, b""
    if status != 200:
        err = resp.read()
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
                first = first or now
                last = now
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
    ttft = (first - t0) if first else None
    dec = (last - first) if (first and last and last > first) else None
    return {"status": status, "total_s": round(t_end - t0, 3), "ttft_s": round(ttft, 3) if ttft else None,
            "prompt_tokens": pt, "completion_tokens": ct, "finish": finish,
            "prefill_tok_s": round(pt / ttft, 1) if (pt and ttft) else None,
            "decode_tok_s": round((ct - 1) / dec, 1) if (ct and ct > 1 and dec) else None,
            "text": "".join(text)[:160], "tool_calls": [(t.get("function") or {}).get("name") for t in tool_calls],
            "error": err.decode(errors="replace")[:300] if err else None}
