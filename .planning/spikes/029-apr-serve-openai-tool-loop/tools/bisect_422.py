#!/usr/bin/env python3
"""Bisect the 422 on the tool-result turn: which part of the OpenAI history shape does
`apr serve` reject? Each variant is one hypothesis; the table says which are accepted.

    python3 -I tools/bisect_422.py --base http://127.0.0.1:8029/v1 --model Qwen3.5-0.8B-Q4_K_M
"""
import argparse
import json
import sys
import time
import urllib.error
import urllib.request

TOOLS = [{"type": "function", "function": {"name": "get_weather", "description": "Weather for a city.",
          "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}}]
Q = {"role": "user", "content": "What's the weather in Paris right now?"}
RESULT = json.dumps({"city": "Paris", "temperature_c": 17, "condition": "light rain"})
TC = {"id": "call_0", "type": "function", "function": {"name": "get_weather", "arguments": json.dumps({"city": "Paris"})}}


def post(base, body, key):
    req = urllib.request.Request(base + "/chat/completions", data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json", "Authorization": f"Bearer {key}"}, method="POST")
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=600) as r:
            return r.status, r.read(), time.perf_counter() - t0
    except urllib.error.HTTPError as e:
        return e.code, e.read(), time.perf_counter() - t0


VARIANTS = {
    # pmcp-agent shape: assistant turn has tool_calls and NO content key
    "A assistant{tool_calls, no content} + tool{tool_call_id}": [Q, {"role": "assistant", "tool_calls": [TC]},
                                                                 {"role": "tool", "tool_call_id": "call_0", "content": RESULT}],
    # OpenAI SDK shape: assistant content is null
    "B assistant{content:null, tool_calls} + tool": [Q, {"role": "assistant", "content": None, "tool_calls": [TC]},
                                                    {"role": "tool", "tool_call_id": "call_0", "content": RESULT}],
    # assistant content is an empty string
    "C assistant{content:'', tool_calls} + tool": [Q, {"role": "assistant", "content": "", "tool_calls": [TC]},
                                                  {"role": "tool", "tool_call_id": "call_0", "content": RESULT}],
    # only the tool message is new; assistant turn is plain text
    "D assistant{content:text} + tool{tool_call_id}": [Q, {"role": "assistant", "content": "Calling get_weather."},
                                                       {"role": "tool", "tool_call_id": "call_0", "content": RESULT}],
    # tool role without tool_call_id
    "E assistant{content:text} + tool{no tool_call_id}": [Q, {"role": "assistant", "content": "Calling get_weather."},
                                                          {"role": "tool", "content": RESULT}],
    # assistant with tool_calls, no tool message at all (is tool_calls itself the problem?)
    "F assistant{content:'', tool_calls} only": [Q, {"role": "assistant", "content": "", "tool_calls": [TC]}],
    # the degraded fallback: result passed back as a user message
    "G assistant{content:text} + user{result}": [Q, {"role": "assistant", "content": "Calling get_weather(city=Paris)."},
                                                 {"role": "user", "content": "Tool result: " + RESULT}],
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--key", default="spike-029-not-a-secret")
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    rows, rec = [], []
    for name, msgs in VARIANTS.items():
        body = {"model": args.model, "messages": msgs, "tools": TOOLS, "max_tokens": 96, "temperature": 0.0}
        st, raw, dt = post(args.base, body, args.key)
        try:
            js = json.loads(raw)
        except json.JSONDecodeError:
            js = {"_raw": raw.decode(errors="replace")[:300]}
        note = ""
        if st == 200:
            m = js["choices"][0]["message"]
            note = f"finish={js['choices'][0].get('finish_reason')} tool_calls={bool(m.get('tool_calls'))} content={json.dumps(m.get('content'))[:110]}"
        else:
            note = json.dumps(js)[:160]
        rows.append((name, st, f"{dt:.1f}", note))
        rec.append({"variant": name, "status": st, "seconds": dt, "request": body, "response": js})
        print(f"  {st} {name}", file=sys.stderr)
    print(f"\n### 422 bisect: `{args.model}` at `{args.base}`\n")
    print("| Variant | HTTP | s | Notes |\n|---|---|---|---|")
    for r in rows:
        print("| " + " | ".join(str(x).replace("|", "\\|") for x in r) + " |")
    if args.out:
        json.dump(rec, open(args.out, "w"), indent=1)


if __name__ == "__main__":
    main()
