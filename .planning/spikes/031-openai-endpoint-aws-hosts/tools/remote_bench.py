#!/usr/bin/env python3
"""Agent-shaped turns against a deployed `apr serve`, on Lambda (invoke with an API-GW v2 event through
the Lambda Web Adapter; buffered, so prefill is read from a max_tokens=1 request and decode from the
difference to a max_tokens=64 request) or over HTTP (Fargate; streamed, TTFT direct).

    uv run --with boto3 python3 tools/remote_bench.py lambda llm-spike-031-lambda-0.8b --model 0.8B --label lambda-0.8b --out results/bench-lambda-0.8b.json
    uv run --with boto3 python3 tools/remote_bench.py http http://<ip>:8080/v1 --model 4B --label fargate-4b --out results/bench-fargate-4b.json
    ... [--qualifier <version>] [--cold] [--conc 2] [--long]

--cold bumps a COLD_NONCE env var first (new environments) and times the first invoke separately.
Every record carries the server's REPORT line fields (Lambda) and the client wall time.
"""
import argparse
import base64
import concurrent.futures as cf
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from shapes import shape_short, shape_tools2, shape_tools10, shape_turn2, stream_chat  # noqa: E402

PRICE_LAMBDA_GB_S = 0.0000133334  # arm64, us-east-1 list


def lambda_event(body):
    return {"version": "2.0", "routeKey": "$default", "rawPath": "/v1/chat/completions", "rawQueryString": "",
            "headers": {"content-type": "application/json", "authorization": "Bearer spike-031", "accept": "application/json"},
            "requestContext": {"http": {"method": "POST", "path": "/v1/chat/completions", "protocol": "HTTP/1.1",
                                        "sourceIp": "127.0.0.1", "userAgent": "spike-031"}, "requestId": "spike", "stage": "$default"},
            "body": json.dumps(body), "isBase64Encoded": False}


def parse_report(tail_b64):
    if not tail_b64:
        return {}
    text = base64.b64decode(tail_b64).decode(errors="replace")
    rep = {}
    for line in text.splitlines():
        if line.startswith("REPORT"):
            for part in line.split("\t"):
                part = part.strip()
                if part.startswith("Duration:"):
                    rep["duration_ms"] = float(part.split()[1])
                elif part.startswith("Init Duration:"):
                    rep["init_ms"] = float(part.split()[2])
                elif part.startswith("Max Memory Used:"):
                    rep["max_mem_mb"] = int(part.split()[3])
                elif part.startswith("Billed Duration:"):
                    rep["billed_ms"] = float(part.split()[2])
    return rep


class LambdaTarget:
    def __init__(self, fn, qualifier, profile, region, mem_mb, tail=True):
        import boto3
        from botocore.config import Config
        self.lam = boto3.Session(profile_name=profile, region_name=region).client(
            "lambda", config=Config(read_timeout=910, connect_timeout=30, retries={"max_attempts": 0}))
        self.fn, self.q, self.mem_mb, self.tail = fn, qualifier, mem_mb, tail

    def bump(self):
        cfg = self.lam.get_function_configuration(FunctionName=self.fn)
        env = cfg.get("Environment", {}).get("Variables", {})
        env["COLD_NONCE"] = str(time.time())
        self.lam.update_function_configuration(FunctionName=self.fn, Environment={"Variables": env})
        self.lam.get_waiter("function_updated_v2").wait(FunctionName=self.fn)

    def chat(self, body):
        kw = {"FunctionName": self.fn, "Payload": json.dumps(lambda_event(body)).encode()}
        if self.tail:
            kw["LogType"] = "Tail"  # rejected on Lambda Managed Instances (023): "Tail logs are not supported"
        if self.q:
            kw["Qualifier"] = self.q
        t0 = time.perf_counter()
        try:
            r = self.lam.invoke(**kw)
        except Exception as e:  # noqa: BLE001
            return {"status": 0, "total_s": round(time.perf_counter() - t0, 3), "error": str(e)[:300]}
        wall = time.perf_counter() - t0
        rep = parse_report(r.get("LogResult"))
        payload = json.loads(r["Payload"].read() or b"{}")
        out = {"status": payload.get("statusCode", 0) if isinstance(payload, dict) else 0, "total_s": round(wall, 3), **rep}
        if r.get("FunctionError"):
            out["error"] = f"{r['FunctionError']}: {json.dumps(payload)[:300]}"
            return out
        body_s = payload.get("body", "") if isinstance(payload, dict) else ""
        if payload.get("isBase64Encoded"):
            body_s = base64.b64decode(body_s).decode(errors="replace")
        try:
            js = json.loads(body_s) if body_s else {}
        except json.JSONDecodeError:
            js = {"_raw": body_s[:300]}
        ch = (js.get("choices") or [{}])[0] if isinstance(js, dict) else {}
        m = ch.get("message", {})
        u = js.get("usage") or {} if isinstance(js, dict) else {}
        out.update({"prompt_tokens": u.get("prompt_tokens"), "completion_tokens": u.get("completion_tokens"),
                    "finish": ch.get("finish_reason"), "text": (m.get("content") or "")[:160],
                    "tool_calls": [(t.get("function") or {}).get("name") for t in (m.get("tool_calls") or [])]})
        if out["status"] != 200:
            out["error"] = json.dumps(js)[:300]
        if rep.get("billed_ms"):
            out["cost_usd"] = round(self.mem_mb / 1024 * rep["billed_ms"] / 1000 * PRICE_LAMBDA_GB_S, 6)
        return out


class HttpTarget:
    def __init__(self, base):
        self.base = base

    def bump(self):
        pass

    def chat(self, body):
        return stream_chat(self.base, body)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("kind", choices=["lambda", "http"])
    ap.add_argument("target")
    ap.add_argument("--model", required=True, help="0.8B | 2B | 4B (the model id sent is Qwen3.5-<model>-Q4_K_M)")
    ap.add_argument("--label", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--qualifier", default=None)
    ap.add_argument("--profile", default="ze-kasher-dev")
    ap.add_argument("--region", default="us-east-1")
    ap.add_argument("--mem-mb", type=int, default=10240)
    ap.add_argument("--cold", action="store_true")
    ap.add_argument("--conc", type=int, default=0, help="also fire N simultaneous short requests")
    ap.add_argument("--long", action="store_true", help="include the 10-tool prefill (937 tokens)")
    ap.add_argument("--no-tail", action="store_true", help="LMI: LogType=Tail is rejected; take timings from the client wall only")
    a = ap.parse_args()
    model = f"Qwen3.5-{a.model}-Q4_K_M"
    tgt = LambdaTarget(a.target, a.qualifier, a.profile, a.region, a.mem_mb, tail=not a.no_tail) if a.kind == "lambda" else HttpTarget(a.target)
    recs = []

    def run(name, body):
        r = tgt.chat(body)
        r["shape"] = name
        recs.append(r)
        print(f"  {name}: status={r.get('status')} wall={r.get('total_s')}s pt={r.get('prompt_tokens')} ct={r.get('completion_tokens')} "
              f"ttft={r.get('ttft_s')} init={r.get('init_ms')} dur={r.get('duration_ms')} mem={r.get('max_mem_mb')} "
              f"finish={r.get('finish')} tools={r.get('tool_calls')} err={r.get('error')}", file=sys.stderr, flush=True)
        return r

    print(f"[remote_bench] {a.label} -> {a.kind} {a.target} model={model}", file=sys.stderr, flush=True)
    if a.cold:
        tgt.bump()
        run("cold_short_1tok", shape_short(model, 1))      # cold start + 73-token prefill + 1 token
    run("short_1tok", shape_short(model, 1))               # warm prefill of 73 tokens
    run("short_64tok", shape_short(model, 64))             # + 63 decode steps
    t1 = run("tools2_1tok", shape_tools2(model, 1))        # 421-token prefill
    t1b = shape_tools2(model, 64)
    r1 = run("tools2_turn1", t1b)                          # the real turn 1 (tool call)
    tool = (r1.get("tool_calls") or ["get_weather"])[0] or "get_weather"
    run("turn2_extend_1tok", shape_turn2(t1b, tool, 1))     # extends turn 1 -> suffix-only if the session held
    run("turn2_extend_64tok", shape_turn2(t1b, tool, 64))
    if a.long:
        run("tools10_1tok", shape_tools10(model, 1))
    if a.conc:
        bodies = [shape_short(model, 32) for _ in range(a.conc)]
        t0 = time.perf_counter()
        with cf.ThreadPoolExecutor(max_workers=a.conc) as ex:
            rs = list(ex.map(tgt.chat, bodies))
        wall = time.perf_counter() - t0
        for i, r in enumerate(rs):
            r["shape"] = f"conc{a.conc}_{i}"
            recs.append(r)
        print(f"  conc{a.conc}: wall={wall:.2f}s each={[r.get('total_s') for r in rs]} init={[r.get('init_ms') for r in rs]} status={[r.get('status') for r in rs]}",
              file=sys.stderr, flush=True)
        recs.append({"shape": f"conc{a.conc}_wall", "total_s": round(wall, 3)})

    # derived rates (Lambda: from the 1-token vs 64-token pairs; HTTP: direct from streaming)
    by = {r["shape"]: r for r in recs}
    derived = {}
    s1, s64 = by.get("short_1tok", {}), by.get("short_64tok", {})
    if s1.get("prompt_tokens") and s1.get("total_s"):
        derived["prefill_tok_s_short"] = round(s1["prompt_tokens"] / s1["total_s"], 2)
    if s64.get("completion_tokens") and s1.get("total_s") and s64.get("total_s", 0) > s1.get("total_s", 0):
        derived["decode_tok_s"] = round((s64["completion_tokens"] - 1) / (s64["total_s"] - s1["total_s"]), 2)
    t2 = by.get("tools2_1tok", {})
    if t2.get("prompt_tokens") and t2.get("total_s"):
        derived["prefill_tok_s_tools2"] = round(t2["prompt_tokens"] / t2["total_s"], 2)
    e1 = by.get("turn2_extend_1tok", {})
    if e1.get("total_s") and t2.get("total_s"):
        derived["turn2_extend_vs_turn1_1tok"] = f"{e1['total_s']}s vs {t2['total_s']}s"
    for r in recs:
        if r.get("ttft_s") and r.get("prompt_tokens"):
            r["prefill_tok_s"] = round(r["prompt_tokens"] / r["ttft_s"], 1)
    json.dump({"label": a.label, "kind": a.kind, "target": a.target, "model": model, "records": recs, "derived": derived},
              open(a.out, "w"), indent=1)
    print(json.dumps(derived, indent=1))


if __name__ == "__main__":
    main()
