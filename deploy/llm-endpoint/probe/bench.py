#!/usr/bin/env python3
"""Agent-shaped turns against the DEPLOYED llm-endpoint Lambda, measured through aws-cli (D-14).

Stdlib only: run it as `python3 -I`. Every AWS call is the `aws` CLI driven through subprocess; there is
no boto3/requests/huggingface_hub. It replays the spike-031 shapes (shapes.py, every prompt carries a
nonce) with `aws lambda invoke --log-type Tail`, and records on EVERY record:

  client_wall_s   what the caller waited (the spike saw it disagree with the REPORT by up to 46 s)
  duration_ms / billed_ms / init_ms / max_mem_mb   from the REPORT line
  cost_usd        memory/1024 * billed_ms/1000 * 0.0000133334 (billed duration, never the wall)
  cpu_part        from the function's `[entry] host:` log line (Graviton2 vs 3 is a 2-3x difference)

    APR_PROBE_KEY=... python3 -I bench.py --function aprender-llm-endpoint-dev --profile ze-kasher-dev \
        --model Qwen3.5-0.8B-Q4_K_M --label deployed --out deploy/llm-endpoint/.ids/bench-deployed.json \
        [--cold] [--long] [--conc 2] [--budget-usd 0.15]

Budget guard: with --budget-usd, BEFORE each invocation the running MEASURED spend plus a conservative
per-shape ceiling (EST_USD) is compared with the budget; if it would pass, no further sample is run, the
partial record is written with a `budget_stop` object and the exit status is 4.

Conformance mode (plan 09-07): `--conformance` replays probe_openai.py's request bodies and verdict rules through
lambda:Invoke (no HTTPS surface exists) and writes a probe record labelled `deployed` with `transport: lambda-invoke`:

    APR_PROBE_KEY=... python3 -I bench.py --conformance --function aprender-llm-endpoint-dev --profile ze-kasher-dev \
        --model Qwen3.5-0.8B-Q4_K_M --pin v0.71.4 --pin-profile baseline --server-version "apr 0.71.4 (5027da624)" \
        --out deploy/llm-endpoint/probe/results/conformance-v0.71.4-deployed-invoke.json [--budget-usd 0.2] [--unreleased P=REF]

Offline proofs (no aws, no spend):
    python3 -I bench.py --selftest
    python3 -I bench.py --selftest-budget-only --budget-usd 0.0001     # exits 4: proves the guard can fire

The bearer key is read from APR_PROBE_KEY only. It is written into a mode-0600 temp event that is
deleted in a `finally`, never put on a command line, and redacted from every output file.
"""
import argparse
import base64
import concurrent.futures as cf
import contextlib
import io
import json
import os
import re
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from shapes import shape_short, shape_tools2, shape_tools10, shape_turn2  # noqa: E402
import probe_openai  # noqa: E402  (its request bodies and verdict rules are reused by --conformance)

PRICE_LAMBDA_GB_S = 0.0000133334  # arm64, us-east-1 list: 0.0000133334

# GUARD ESTIMATES ONLY, never a measurement. A conservative per-shape ceiling in USD, derived from the
# spike-031 prices (two-tool turn ~ $0.0095, ten-tool prefill ~146 s billed ~ $0.0199, a cold start that
# is answered 401 ~ $0.005) and rounded UP. The budget guard compares (measured spend so far + this) with
# the budget BEFORE each sample; nothing in a record is ever taken from this table.
EST_USD = {
    "cold_short_1tok": 0.0070,
    "short_1tok": 0.0010,
    "short_64tok": 0.0040,
    "tools2_1tok": 0.0090,
    "tools2_turn1": 0.0120,
    "turn2_extend_1tok": 0.0020,
    "turn2_extend_64tok": 0.0045,
    "tools10_1tok": 0.0250,
    "conc2": 0.0200,  # for the pair; --conc N scales it by N/2
    "no_auth_401": 0.0070,
}

# GUARD ESTIMATE ONLY (never recorded as a measurement): the ceiling for ONE conformance invocation, the ten-tool
# prefill (about $0.02 in spike 031) rounded up. The conformance guard checks spent + this against the budget.
EST_CONFORMANCE_CALL_USD = 0.025

EXIT_BUDGET = 4
HOST_RE = re.compile(r"cpu_part=(\S+)")
APR_RE = re.compile(r"start pid=\d+ apr=(.*)$")


def price_usd(mem_mb, billed_ms):
    return round(mem_mb / 1024 * billed_ms / 1000 * PRICE_LAMBDA_GB_S, 6)


def lambda_event(body, key):
    """The API-GW v2 event the Lambda Web Adapter expects; the authorization header only when key is set."""
    headers = {"content-type": "application/json", "accept": "application/json"}
    if key:
        headers["authorization"] = "Bearer " + key
    return {"version": "2.0", "routeKey": "$default", "rawPath": "/v1/chat/completions", "rawQueryString": "",
            "headers": headers,
            "requestContext": {"http": {"method": "POST", "path": "/v1/chat/completions", "protocol": "HTTP/1.1",
                                        "sourceIp": "127.0.0.1", "userAgent": "llm-endpoint-bench"},
                               "requestId": "bench", "stage": "$default"},
            "body": json.dumps(body), "isBase64Encoded": False}


def lambda_event_raw(method, path, body, key):
    """An API-GW v2 event for any method and path (the conformance replay also GETs /v1/models and /health)."""
    headers = {"accept": "application/json"}
    if body is not None:
        headers["content-type"] = "application/json"
    if key:
        headers["authorization"] = "Bearer " + key
    return {"version": "2.0", "routeKey": "$default", "rawPath": path, "rawQueryString": "", "headers": headers,
            "requestContext": {"http": {"method": method, "path": path, "protocol": "HTTP/1.1", "sourceIp": "127.0.0.1",
                                        "userAgent": "llm-endpoint-bench-conformance"},
                               "requestId": "bench", "stage": "$default"},
            "body": json.dumps(body) if body is not None else "", "isBase64Encoded": False}


def parse_report_text(text):
    rep = {}
    for line in text.splitlines():
        if not line.startswith("REPORT"):
            continue
        for part in line.split("\t"):
            part = part.strip()
            if part.startswith("Duration:"):
                rep["duration_ms"] = float(part.split()[1])
            elif part.startswith("Init Duration:"):
                rep["init_ms"] = float(part.split()[2])
            elif part.startswith("Max Memory Used:"):
                rep["max_mem_mb"] = int(part.split()[3])
            elif part.startswith("Memory Size:"):
                rep["memory_size_mb"] = int(part.split()[2])
            elif part.startswith("Billed Duration:"):
                rep["billed_ms"] = float(part.split()[2])
    return rep


def parse_log_result(tail_b64):
    """LogResult (base64 of the last 4 KB of the log) -> REPORT fields plus cpu_part / apr line when present."""
    if not tail_b64 or not tail_b64.strip():
        return {}
    try:
        text = base64.b64decode(tail_b64.strip()).decode(errors="replace")
    except ValueError:
        return {}
    out = parse_report_text(text)
    for line in text.splitlines():
        if "[entry]" not in line:
            continue
        m = HOST_RE.search(line)
        if m and "host:" in line and "cpu_part" not in out:
            out["cpu_part"] = m.group(1)
        a = APR_RE.search(line.strip())
        if a and "apr_version_line" not in out:
            out["apr_version_line"] = a.group(1).strip()
    return out


def body_fields(payload):
    """statusCode and the chat fields out of the invoke payload (base64 body when flagged)."""
    if not isinstance(payload, dict):
        return {"status": 0}
    out = {"status": payload.get("statusCode", 0)}
    body_s = payload.get("body", "")
    if payload.get("isBase64Encoded") and body_s:
        body_s = base64.b64decode(body_s).decode(errors="replace")
    try:
        js = json.loads(body_s) if body_s else {}
    except json.JSONDecodeError:
        js = {"_raw": body_s[:300]}
    if not isinstance(js, dict):
        js = {"_raw": str(js)[:300]}
    ch = (js.get("choices") or [{}])[0]
    m = ch.get("message", {})
    u = js.get("usage") or {}
    out.update({"prompt_tokens": u.get("prompt_tokens"), "completion_tokens": u.get("completion_tokens"),
                "finish": ch.get("finish_reason"), "text": (m.get("content") or "")[:160],
                "tool_calls": [(t.get("function") or {}).get("name") for t in (m.get("tool_calls") or [])]})
    if out["status"] != 200:
        out["error"] = json.dumps(js)[:300]
    return out


class LambdaTarget:
    """One deployed function, driven entirely through the aws CLI."""

    def __init__(self, fn, profile, region, mem_mb, log_group=None):
        self.fn, self.mem_mb = fn, mem_mb
        self.log_group = log_group or f"/aws/lambda/{fn}"
        self.ap = ["--profile", profile, "--region", region]
        # never retry an invoke (a retry after a read timeout would run, and bill, the request twice); the
        # probe key is not the CLI's business
        self.env = {k: v for k, v in os.environ.items() if k != "APR_PROBE_KEY"}
        self.env["AWS_MAX_ATTEMPTS"] = "1"

    def _aws(self, args, timeout=300):
        return subprocess.run(["aws", *args], capture_output=True, text=True, timeout=timeout, env=self.env)

    def bump(self):
        """Force a new execution environment: COLD_NONCE read back, merged, written, then wait."""
        p = self._aws(["lambda", "get-function-configuration", *self.ap, "--function-name", self.fn,
                       "--query", "Environment.Variables", "--output", "json"])
        if p.returncode != 0:
            raise RuntimeError("get-function-configuration failed: " + p.stderr[-200:])
        env = json.loads(p.stdout or "null") or {}
        env["COLD_NONCE"] = str(time.time())
        fd, path = tempfile.mkstemp(prefix="bench-env-", suffix=".json")
        try:
            with os.fdopen(fd, "w") as f:
                json.dump({"Variables": env}, f)
            p = self._aws(["lambda", "update-function-configuration", *self.ap, "--function-name", self.fn,
                           "--environment", "file://" + path, "--output", "text", "--query", "LastUpdateStatus"])
            if p.returncode != 0:
                raise RuntimeError("update-function-configuration failed: " + p.stderr[-200:])
        finally:
            os.unlink(path)
        p = self._aws(["lambda", "wait", "function-updated-v2", *self.ap, "--function-name", self.fn], timeout=600)
        if p.returncode != 0:
            raise RuntimeError("wait function-updated-v2 failed: " + p.stderr[-200:])

    def chat(self, body, auth=True, shape=""):
        key = os.environ.get("APR_PROBE_KEY", "") if auth else ""
        evfd, evpath = tempfile.mkstemp(prefix="bench-event-", suffix=".json")  # mode 0600
        ofd, opath = tempfile.mkstemp(prefix="bench-out-", suffix=".json")
        os.close(ofd)
        t0 = time.perf_counter()
        try:
            with os.fdopen(evfd, "w") as f:
                json.dump(lambda_event(body, key), f)
            cmd = ["lambda", "invoke", *self.ap, "--function-name", self.fn,
                   "--cli-binary-format", "raw-in-base64-out", "--cli-read-timeout", "910",
                   "--payload", "file://" + evpath, "--log-type", "Tail", "--query", "LogResult",
                   "--output", "text", opath]
            try:
                p = self._aws(cmd, timeout=910)
            except subprocess.TimeoutExpired:
                return {"status": 0, "client_wall_s": round(time.perf_counter() - t0, 3), "error": "aws invoke timed out"}
            wall = round(time.perf_counter() - t0, 3)
            if p.returncode != 0:
                return {"status": 0, "client_wall_s": wall, "error": f"aws rc={p.returncode}: {p.stderr[-300:]}"}
            rep = parse_log_result(p.stdout)
            try:
                with open(opath) as f:
                    payload = json.load(f)
            except (OSError, json.JSONDecodeError) as e:
                return {"status": 0, "client_wall_s": wall, "error": f"unreadable payload: {e}", **rep}
        finally:
            os.unlink(evpath)
            os.unlink(opath)
        out = {"client_wall_s": wall, **body_fields(payload), **rep}
        if isinstance(payload, dict) and payload.get("errorMessage"):
            out["error"] = ("FunctionError: " + str(payload.get("errorMessage")))[:300]
        mem = rep.get("memory_size_mb") or self.mem_mb
        if rep.get("billed_ms"):
            out["cost_usd"] = price_usd(mem, rep["billed_ms"])
        return out

    def invoke_raw(self, method, path, body, key):
        """One invoke of any route: (status, headers, raw body bytes, client wall s, REPORT fields). A failed or
        unreadable invoke is status 0 with the reason as the body, so a probe verdict reads it as a cross."""
        evfd, evpath = tempfile.mkstemp(prefix="bench-event-", suffix=".json")  # mode 0600
        ofd, opath = tempfile.mkstemp(prefix="bench-out-", suffix=".json")
        os.close(ofd)
        t0 = time.perf_counter()
        try:
            with os.fdopen(evfd, "w") as f:
                json.dump(lambda_event_raw(method, path, body, key), f)
            cmd = ["lambda", "invoke", *self.ap, "--function-name", self.fn,
                   "--cli-binary-format", "raw-in-base64-out", "--cli-read-timeout", "910",
                   "--payload", "file://" + evpath, "--log-type", "Tail", "--query", "LogResult",
                   "--output", "text", opath]
            try:
                p = self._aws(cmd, timeout=910)
            except subprocess.TimeoutExpired:
                return 0, {}, b"aws invoke timed out", round(time.perf_counter() - t0, 3), {}
            wall = round(time.perf_counter() - t0, 3)
            if p.returncode != 0:
                return 0, {}, f"aws rc={p.returncode}: {p.stderr[-300:]}".encode(), wall, {}
            rep = parse_log_result(p.stdout)
            try:
                with open(opath) as f:
                    payload = json.load(f)
            except (OSError, json.JSONDecodeError) as e:
                return 0, {}, f"unreadable payload: {e}".encode(), wall, rep
        finally:
            os.unlink(evpath)
            os.unlink(opath)
        if not isinstance(payload, dict):
            return 0, {}, b"payload is not an object", wall, rep
        if payload.get("errorMessage"):
            return 0, {}, ("FunctionError: " + str(payload.get("errorMessage")))[:300].encode(), wall, rep
        body_s = payload.get("body", "") or ""
        raw = base64.b64decode(body_s) if payload.get("isBase64Encoded") and body_s else body_s.encode()
        return int(payload.get("statusCode", 0)), dict(payload.get("headers") or {}), raw, wall, rep

    def host_info(self):
        """Newest `[entry] host:` (and apr version) line of the log group, for when the tail lacks them."""
        info = {}
        start = str(int((time.time() - 7 * 86400) * 1000))
        for key, pattern in (("host", '"[entry] host:"'), ("start", '"[entry]" "start pid="')):
            p = self._aws(["logs", "filter-log-events", *self.ap, "--log-group-name", self.log_group,
                           "--start-time", start, "--filter-pattern", pattern,
                           "--query", "events[-1].message", "--output", "text"])
            msg = p.stdout.strip() if p.returncode == 0 else ""
            if key == "host":
                m = HOST_RE.search(msg)
                if m:
                    info["cpu_part"] = m.group(1)
            else:
                a = APR_RE.search(msg[msg.find("[entry]"):] if "[entry]" in msg else msg)
                if a:
                    info["apr_version_line"] = a.group(1).strip()
        return info


class StubTarget:
    """Offline stand-in used by the budget proofs: canned answers, measured costs, no aws."""

    mem_mb = 10240

    def __init__(self, cost_by_shape=None):
        self.cost_by_shape = cost_by_shape or {}
        self.calls = []

    def bump(self):
        pass

    def chat(self, body, auth=True, shape=""):
        self.calls.append(shape)
        r = {"status": 200 if auth else 401, "client_wall_s": 1.0, "billed_ms": 100.0, "duration_ms": 99.0,
             "max_mem_mb": 2000, "prompt_tokens": 10, "completion_tokens": 1, "finish": "length",
             "text": "stub", "tool_calls": ["get_weather"] if shape == "tools2_turn1" else [],
             "cost_usd": self.cost_by_shape.get(shape, 0.0010)}
        return r

    def host_info(self):
        return {"cpu_part": "0xd40", "apr_version_line": "apr 0.0.0 (stub)"}


class BudgetStop(Exception):
    def __init__(self, info):
        super().__init__(info["before"])
        self.info = info


def admit(budget_usd, spent_usd, shape, est_usd):
    """The guard, on its own so a test can drive it: None when the sample may run, else the budget_stop object."""
    if budget_usd is not None and spent_usd + est_usd > budget_usd:
        return {"before": shape, "spent_usd": round(spent_usd, 6), "est_usd": est_usd, "budget_usd": budget_usd}
    return None


class LambdaInvokeTransport:
    """probe_openai's transport over lambda:Invoke (plan 09-07): the same request bodies and verdict rules, no URL.
    Every invocation is budget-guarded BEFORE it runs (admit) and priced from its billed duration afterwards."""

    stream_note = " [buffered-through-adapter]"

    def __init__(self, tgt, budget_usd=None, log=None):
        self.tgt, self.budget_usd, self.log = tgt, budget_usd, log
        self.spent_usd, self.calls, self.budget_stop = 0.0, [], None

    def _call(self, method, path, body, key):
        label = f"{method} {path}"
        stop = admit(self.budget_usd, self.spent_usd, label, EST_CONFORMANCE_CALL_USD)
        if stop:
            self.budget_stop = stop
            raise BudgetStop(stop)
        st, hdr, raw, wall, rep = self.tgt.invoke_raw(method, path, body, key)
        cost = price_usd(rep.get("memory_size_mb") or self.tgt.mem_mb, rep["billed_ms"]) if rep.get("billed_ms") else 0.0
        self.spent_usd += cost
        rec = {"call": label, "status": st, "client_wall_s": wall, "billed_ms": rep.get("billed_ms"),
               "init_ms": rep.get("init_ms"), "max_mem_mb": rep.get("max_mem_mb"), "cost_usd": cost}
        self.calls.append(rec)
        if self.log:
            self.log(rec)
        return st, hdr, raw, wall

    def get(self, path, key):
        st, _, raw, wall = self._call("GET", "/v1" + path, None, key)
        return st, raw, wall

    def get_root(self, path, key):
        st, _, raw, wall = self._call("GET", path, None, key)
        return st, raw, wall

    def post(self, path, body, key):
        return self._call("POST", "/v1" + path, body, key)

    def post_noauth(self, path, body):
        st, _, _, wall = self._call("POST", "/v1" + path, body, "")
        return st, wall


class Runner:
    def __init__(self, tgt, budget_usd=None, log=None):
        self.tgt, self.budget_usd, self.log = tgt, budget_usd, log
        self.records, self.spent_usd = [], 0.0
        self.budget_stop = None

    def step(self, shape, est_usd, fn):
        """Run one step (fn returns a list of records) unless the guard refuses it BEFORE it runs."""
        stop = admit(self.budget_usd, self.spent_usd, shape, est_usd)
        if stop:
            self.budget_stop = stop
            raise BudgetStop(stop)
        recs = fn()
        for r in recs:
            self.records.append(r)
            self.spent_usd += r.get("cost_usd") or 0.0  # MEASURED cost only, never the estimate
            if self.log:
                self.log(r)
        return recs

    def chat1(self, shape, body, auth=True):
        def one():
            r = self.tgt.chat(body, auth=auth, shape=shape)
            r["shape"] = shape
            return [r]
        return self.step(shape, EST_USD[shape], one)[0]


def run_sequence(rn, model, cold=True, long=False, conc=0):
    """The spike-031 order, then the no-auth 401 case. Raises BudgetStop at the first refused sample."""
    if cold:
        rn.tgt.bump()
        rn.chat1("cold_short_1tok", shape_short(model, 1))
    rn.chat1("short_1tok", shape_short(model, 1))
    rn.chat1("short_64tok", shape_short(model, 64))
    rn.chat1("tools2_1tok", shape_tools2(model, 1))
    t1b = shape_tools2(model, 64)
    r1 = rn.chat1("tools2_turn1", t1b)
    tool = (r1.get("tool_calls") or ["get_weather"])[0] or "get_weather"
    rn.chat1("turn2_extend_1tok", shape_turn2(t1b, tool, 1))
    rn.chat1("turn2_extend_64tok", shape_turn2(t1b, tool, 64))
    if long:
        rn.chat1("tools10_1tok", shape_tools10(model, 1))
    if conc:
        name = f"conc{conc}"

        def pair():
            bodies = [shape_short(model, 32) for _ in range(conc)]
            t0 = time.perf_counter()
            with cf.ThreadPoolExecutor(max_workers=conc) as ex:
                rs = list(ex.map(lambda b: rn.tgt.chat(b, auth=True, shape=name), bodies))
            wall = round(time.perf_counter() - t0, 3)
            for i, r in enumerate(rs):
                r["shape"] = f"{name}_{i}"
            rs.append({"shape": f"{name}_wall", "client_wall_s": wall})
            return rs

        rn.step(name, EST_USD["conc2"] * conc / 2, pair)
    # the 401 case: no authorization header; the expected statusCode is recorded here, asserted by the proof recipe
    rn.chat1("no_auth_401", shape_short(model, 1), auth=False)


def derive(records):
    by = {r["shape"]: r for r in records}
    d = {}
    s1, s64 = by.get("short_1tok", {}), by.get("short_64tok", {})
    if s1.get("prompt_tokens") and s1.get("client_wall_s"):
        d["prefill_tok_s_short"] = round(s1["prompt_tokens"] / s1["client_wall_s"], 2)
    if s64.get("completion_tokens") and s1.get("client_wall_s") and s64.get("client_wall_s", 0) > s1["client_wall_s"]:
        d["decode_tok_s"] = round((s64["completion_tokens"] - 1) / (s64["client_wall_s"] - s1["client_wall_s"]), 2)
    t2 = by.get("tools2_1tok", {})
    if t2.get("prompt_tokens") and t2.get("client_wall_s"):
        d["prefill_tok_s_tools2"] = round(t2["prompt_tokens"] / t2["client_wall_s"], 2)
    t10 = by.get("tools10_1tok", {})
    if t10.get("prompt_tokens") and t10.get("client_wall_s"):
        d["prefill_tok_s_tools10"] = round(t10["prompt_tokens"] / t10["client_wall_s"], 2)
    e1 = by.get("turn2_extend_1tok", {})
    if e1.get("client_wall_s") and t2.get("client_wall_s"):
        d["turn2_extend_vs_turn1_1tok"] = f"{e1['client_wall_s']}s vs {t2['client_wall_s']}s"
    t1 = by.get("tools2_turn1", {})
    if t1:
        d["two_tool_turn_s"] = t1.get("client_wall_s")
        d["two_tool_turn_usd"] = t1.get("cost_usd")
    return d


def attach_host(rn):
    """cpu_part on EVERY record: the record's own tail when it has one, else the function-level value
    (the cold record's tail, or the newest `[entry] host:` line of the log group)."""
    cold = next((r for r in rn.records if r.get("shape") == "cold_short_1tok"), None)
    info, source = {}, "absent"
    if cold and cold.get("cpu_part"):
        info, source = {"cpu_part": cold["cpu_part"], "apr_version_line": cold.get("apr_version_line")}, "cold-record-tail"
    if not info.get("cpu_part") or not info.get("apr_version_line"):
        looked = rn.tgt.host_info()
        if not info.get("cpu_part") and looked.get("cpu_part"):
            source = "log-lookup"
        info = {"cpu_part": info.get("cpu_part") or looked.get("cpu_part"),
                "apr_version_line": info.get("apr_version_line") or looked.get("apr_version_line")}
    for r in rn.records:
        if r.get("cpu_part"):
            r["cpu_part_source"] = "tail"
        else:
            r["cpu_part"] = info.get("cpu_part")
            r["cpu_part_source"] = source if info.get("cpu_part") else "absent"
            if r.get("init_ms") and r.get("shape") != "cold_short_1tok":
                r["cpu_part_source"] += " (this record initialised a new environment: may differ)"
        r.pop("apr_version_line", None)
    return info


def build_result(rn, ident, model, label, info):
    return {"label": label, "function": ident.get("function"), "image_digest": ident.get("image_digest"),
            "apr_tag": ident.get("apr_tag"), "gguf_sha256": ident.get("gguf_sha256"),
            "apr_version_line": info.get("apr_version_line"), "model": model, "memory_mb": ident.get("memory_mb"),
            "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "cpu_part": info.get("cpu_part"),
            "price_usd_per_gb_s": PRICE_LAMBDA_GB_S, "budget_usd": rn.budget_usd, "spent_usd": round(rn.spent_usd, 6),
            "budget_stop": rn.budget_stop, "records": rn.records, "derived": derive(rn.records)}


def redact(text):
    key = os.environ.get("APR_PROBE_KEY", "")
    return text.replace(key, "[redacted]") if len(key) >= 4 else text


def write_json(path, obj):
    with open(path, "w") as f:
        f.write(redact(json.dumps(obj, indent=1)) + "\n")


def table(result):
    rows = ["| shape | status | wall s | billed ms | init ms | max MB | $ | cpu_part |", "|---|---|---|---|---|---|---|---|"]
    for r in result["records"]:
        rows.append("| {} | {} | {} | {} | {} | {} | {} | {} |".format(
            r.get("shape"), r.get("status", "-"), r.get("client_wall_s", "-"), r.get("billed_ms", "-"),
            r.get("init_ms", "-"), r.get("max_mem_mb", "-"), r.get("cost_usd", "-"), r.get("cpu_part", "-")))
    return "\n".join(rows)


def progress(r):
    print(f"  {r.get('shape')}: status={r.get('status')} wall={r.get('client_wall_s')}s billed={r.get('billed_ms')}ms "
          f"init={r.get('init_ms')} cost={r.get('cost_usd')} err={r.get('error')}", file=sys.stderr, flush=True)


def load_ids(ids_dir):
    ident, aws = {}, {}
    for name, dest in (("function.json", ident), ("aws.json", aws)):
        p = os.path.join(ids_dir, name)
        if os.path.isfile(p):
            with open(p) as f:
                dest.update(json.load(f))
    return ident, aws


# ---------------------------------------------------------------------------------------------------------
# offline proofs
# ---------------------------------------------------------------------------------------------------------

FIXTURE_TAIL = "\n".join([
    "START RequestId: 11111111-2222-3333-4444-555555555555 Version: $LATEST",
    "[entry] t=2ms start pid=1 apr=apr 0.71.4 (5027da624)",
    "[entry] t=5ms host: nproc=4 threads=unset cpu_part=0xd40 model_name=Neoverse-V1",
    "[entry] t=9ms apr serve start",
    "END RequestId: 11111111-2222-3333-4444-555555555555",
    "REPORT RequestId: 11111111-2222-3333-4444-555555555555\tDuration: 70999.55 ms\tBilled Duration: 71047 ms\t"
    "Memory Size: 10240 MB\tMax Memory Used: 2296 MB\tInit Duration: 9983.53 ms\t",
])

SHIM = r'''#!/usr/bin/env python3
# offline aws shim for the bench self-test: logs argv, answers the few calls the bench makes
import json, os, sys, base64
a = sys.argv[1:]
log = os.environ["BENCH_SHIM_LOG"]
with open(log, "a") as f:
    f.write(json.dumps({"argv": a, "max_attempts": os.environ.get("AWS_MAX_ATTEMPTS"),
                        "key_in_env": "APR_PROBE_KEY" in os.environ}) + "\n")
cmd = " ".join(a[:2])
def opt(name):
    return a[a.index(name) + 1] if name in a else None
if cmd == "lambda invoke":
    ev = json.load(open(opt("--payload")[len("file://"):]))
    auth = "authorization" in ev["headers"]
    with open(log, "a") as f:
        f.write(json.dumps({"event_authorization_header": auth, "event_path": opt("--payload")[len("file://"):],
                            "event_rawpath": ev["rawPath"], "event_method": ev["requestContext"]["http"]["method"],
                            "out_path": a[-1]}) + "\n")
    body = json.loads(ev["body"]) if ev["body"] else {}
    if not auth:
        payload = {"statusCode": 401, "body": json.dumps({"error": "unauthorized"}), "isBase64Encoded": False}
    else:
        tools = bool(body.get("tools"))
        msg = {"role": "assistant", "content": "ok"}
        if tools and body.get("max_tokens") == 64 and len(body["messages"]) == 2:
            msg = {"role": "assistant", "content": "", "tool_calls": [{"id": "c0", "type": "function", "function": {"name": "get_weather", "arguments": "{}"}}]}
        payload = {"statusCode": 200, "isBase64Encoded": False, "body": json.dumps({
            "choices": [{"message": msg, "finish_reason": "length"}], "usage": {"prompt_tokens": 73, "completion_tokens": body.get("max_tokens", 1)}})}
    json.dump(payload, open(a[-1], "w"))
    tail = os.environ["BENCH_SHIM_TAIL"]
    if os.path.exists(os.environ["BENCH_SHIM_COLD"]):
        os.unlink(os.environ["BENCH_SHIM_COLD"])
    else:
        # a warm invoke: no init logs, no Init Duration
        tail = "\n".join(l.replace("\tInit Duration: 9983.53 ms", "") for l in tail.splitlines() if "[entry]" not in l)
    print(base64.b64encode(tail.encode()).decode())
elif cmd == "lambda get-function-configuration":
    print(json.dumps({"APR_API_KEY_HASH": "h" * 64, "MODEL_SHA256": "m" * 64}))
elif cmd == "lambda update-function-configuration":
    env = json.load(open(opt("--environment")[len("file://"):]))
    with open(log, "a") as f:
        f.write(json.dumps({"bump_env": env}) + "\n")
    open(os.environ["BENCH_SHIM_COLD"], "w").write("x")
    print("InProgress")
elif cmd == "lambda wait":
    pass
elif cmd == "logs filter-log-events":
    print("[entry] t=5ms host: nproc=4 cpu_part=0xd4f model_name=x")
else:
    sys.stderr.write("shim: unexpected call " + cmd + "\n")
    sys.exit(97)
'''


def selftest():
    failures = []

    def check(cond, what):
        if not cond:
            failures.append(what)

    # 1. parse a fixture LogResult, price it
    tail = base64.b64encode(FIXTURE_TAIL.encode()).decode()
    got = parse_log_result(tail)
    check(got.get("billed_ms") == 71047.0, f"billed_ms {got.get('billed_ms')}")
    check(got.get("init_ms") == 9983.53, f"init_ms {got.get('init_ms')}")
    check(got.get("duration_ms") == 70999.55, f"duration_ms {got.get('duration_ms')}")
    check(got.get("max_mem_mb") == 2296, f"max_mem_mb {got.get('max_mem_mb')}")
    check(got.get("memory_size_mb") == 10240, f"memory_size_mb {got.get('memory_size_mb')}")
    check(got.get("cpu_part") == "0xd40", f"cpu_part {got.get('cpu_part')}")
    check(got.get("apr_version_line") == "apr 0.71.4 (5027da624)", f"apr line {got.get('apr_version_line')}")
    priced = price_usd(10240, 71047)
    check(abs(priced - 0.009473) < 1e-6, f"price {priced}")
    check(parse_log_result("") == {} and parse_log_result("None") == {}, "empty/None tail must parse to {}")

    # 2. the budget guard over a stub target: with 0.012 the first tools2_turn1 (measured 0.0095) is admitted and
    #    the second is refused; with no budget both run
    def two_turns(budget):
        tgt = StubTarget({"tools2_turn1": 0.0095})
        rn = Runner(tgt, budget)
        model = "m"
        try:
            for i in (1, 2):
                rn.chat1("tools2_turn1", shape_tools2(model, 64))
        except BudgetStop as e:
            return rn, e.info, i
        return rn, None, 0

    rn, stop, which = two_turns(0.012)
    check(stop is not None and stop["before"] == "tools2_turn1" and which == 2, f"budget guard did not refuse #2: {stop} {which}")
    check(len(rn.records) == 1 and abs(rn.spent_usd - 0.0095) < 1e-9, f"spent {rn.spent_usd} with {len(rn.records)} records")
    check(stop is not None and stop["spent_usd"] == 0.0095 and stop["est_usd"] == EST_USD["tools2_turn1"] and stop["budget_usd"] == 0.012,
          f"budget_stop shape {stop}")
    rn2, stop2, _ = two_turns(None)
    check(stop2 is None and len(rn2.records) == 2, "without a budget both turns must run")
    check(admit(0.012, 0.0, "tools2_turn1", EST_USD["tools2_turn1"]) is None, "an estimate equal to the budget is admitted")
    check(admit(0.012, 0.0095, "tools2_turn1", EST_USD["tools2_turn1"]) is not None, "an overspend must be refused")

    # 3. the real LambdaTarget code path against an aws shim: argv, key handling, temp-file removal, cold bump, 401 case
    saved_path, saved_key = os.environ.get("PATH", ""), os.environ.get("APR_PROBE_KEY")
    with tempfile.TemporaryDirectory(prefix="bench-selftest-") as td:
        shim = os.path.join(td, "aws")
        with open(shim, "w") as f:
            f.write(SHIM)
        os.chmod(shim, 0o755)
        log = os.path.join(td, "shim.log")
        open(log, "w").close()
        os.environ.update({"PATH": td + os.pathsep + saved_path, "BENCH_SHIM_LOG": log, "BENCH_SHIM_TAIL": FIXTURE_TAIL,
                           "BENCH_SHIM_COLD": os.path.join(td, "cold.flag"), "APR_PROBE_KEY": "selftest-key-0123456789"})
        try:
            tgt = LambdaTarget("fn-selftest", "p", "us-east-1", 10240)
            rn = Runner(tgt, None)
            run_sequence(rn, "Qwen3.5-0.8B-Q4_K_M", cold=True, long=True, conc=2)
            info = attach_host(rn)
            res = build_result(rn, {"function": "fn-selftest", "image_digest": "sha256:x", "apr_tag": "v0.0.0",
                                    "gguf_sha256": "g", "memory_mb": 10240}, "Qwen3.5-0.8B-Q4_K_M", "selftest", info)
        finally:
            os.environ["PATH"] = saved_path
            if saved_key is None:
                os.environ.pop("APR_PROBE_KEY", None)
            else:
                os.environ["APR_PROBE_KEY"] = saved_key
        with open(log) as f:
            calls = [json.loads(line) for line in f if line.strip()]
        invokes = [c for c in calls if "argv" in c and c["argv"][:2] == ["lambda", "invoke"]]
        events = [c for c in calls if "event_authorization_header" in c]
        check(len(invokes) == 11, f"expected 11 invokes (cold, 6 warm, tools10, 2 conc, 401), saw {len(invokes)}")
        check(all(c["max_attempts"] == "1" and not c["key_in_env"] for c in calls if "argv" in c), "AWS_MAX_ATTEMPTS=1 and no key in the child env")
        check("selftest-key-0123456789" not in json.dumps(calls), "key reached an argv")
        check(sum(1 for e in events if not e["event_authorization_header"]) == 1, "exactly one event (the 401 case) lacks the authorization header")
        check(not any(os.path.exists(e["event_path"]) or os.path.exists(e["out_path"]) for e in events), "a temp event or output file survived")
        bump = [c for c in calls if "bump_env" in c]
        check(len(bump) == 1 and "COLD_NONCE" in bump[0]["bump_env"]["Variables"] and "APR_API_KEY_HASH" in bump[0]["bump_env"]["Variables"],
              "the cold bump must merge COLD_NONCE into the existing environment")
        shapes = [r["shape"] for r in res["records"]]
        check(shapes[0] == "cold_short_1tok" and shapes[-1] == "no_auth_401" and "conc2_wall" in shapes and "tools10_1tok" in shapes,
              f"shape order {shapes}")
        by = {r["shape"]: r for r in res["records"]}
        check(by["no_auth_401"]["status"] == 401, "the 401 case must record statusCode 401")
        check(res["cpu_part"] == "0xd40" and by["cold_short_1tok"]["cpu_part_source"] == "tail", "cold cpu_part must come from the tail")
        check(by["short_1tok"]["cpu_part"] == "0xd40" and by["short_1tok"]["cpu_part_source"] == "cold-record-tail", "warm records carry the cold record's cpu_part")
        check(all(r.get("cpu_part") for r in res["records"]), "a record lacks cpu_part")
        check(by["cold_short_1tok"]["init_ms"] == 9983.53 and "billed_ms" in by["short_1tok"] and "client_wall_s" in by["short_1tok"],
              "billed_ms/client_wall_s/init_ms on the records")
        check(abs(by["cold_short_1tok"]["cost_usd"] - 0.009473) < 1e-6, f"cold cost {by['cold_short_1tok'].get('cost_usd')}")
        check(res["derived"].get("two_tool_turn_usd") == by["tools2_turn1"].get("cost_usd"), "two_tool_turn_usd")
        check(res["apr_version_line"] == "apr 0.71.4 (5027da624)", f"apr version line {res['apr_version_line']}")
        check("selftest-key-0123456789" not in json.dumps(res), "key in the result")

        # no cold run: the tail carries no host line, so cpu_part must come from the log-group lookup
        os.environ.update({"PATH": td + os.pathsep + saved_path, "APR_PROBE_KEY": "selftest-key-0123456789"})
        try:
            rn_w = Runner(tgt, None)
            rn_w.chat1("short_1tok", shape_short("m", 1))
            info_w = attach_host(rn_w)
        finally:
            os.environ["PATH"] = saved_path
            if saved_key is None:
                os.environ.pop("APR_PROBE_KEY", None)
            else:
                os.environ["APR_PROBE_KEY"] = saved_key
        check(info_w.get("cpu_part") == "0xd4f" and rn_w.records[0]["cpu_part_source"] == "log-lookup",
              f"log-lookup fallback {info_w} {rn_w.records[0].get('cpu_part_source')}")

    # 3b. --conformance over the same shimmed aws: routes, the single no-auth event, the record header, no key in the
    #     record or any argv, and the guard (a budget below one call's ceiling writes NOTHING and exits 4)
    with tempfile.TemporaryDirectory(prefix="bench-conf-selftest-") as td:
        shim = os.path.join(td, "aws")
        with open(shim, "w") as f:
            f.write(SHIM)
        os.chmod(shim, 0o755)
        log = os.path.join(td, "shim.log")
        open(log, "w").close()
        out_json = os.path.join(td, "conf.json")
        os.environ.update({"PATH": td + os.pathsep + saved_path, "BENCH_SHIM_LOG": log, "BENCH_SHIM_TAIL": FIXTURE_TAIL,
                           "BENCH_SHIM_COLD": os.path.join(td, "cold.flag"), "APR_PROBE_KEY": "selftest-key-0123456789"})
        try:
            def conf_args(budget):
                return argparse.Namespace(function="fn-selftest", model="m", out=out_json, pin="v0.71.4", pin_profile="baseline",
                                          server_version="apr 0.71.4 (5027da624)", unreleased=[], budget_usd=budget,
                                          profile="p", region="us-east-1")
            sink = io.StringIO()
            with contextlib.redirect_stdout(sink), contextlib.redirect_stderr(sink):
                rc_c = conformance(conf_args(None), {"function": "fn-selftest", "memory_mb": 10240}, {})
            check(rc_c == 0, f"conformance rc {rc_c}")
            with open(out_json) as f:
                rec_text = f.read()
            rec = json.loads(rec_text)
            check(rec.get("label") == "deployed" and rec.get("transport") == "lambda-invoke" and rec.get("pin") == "v0.71.4"
                  and rec.get("profile") == "baseline" and rec.get("streaming") == "buffered-through-adapter",
                  f"conformance header {[rec.get(k) for k in ('label', 'transport', 'pin', 'profile', 'streaming')]}")
            check("selftest-key-0123456789" not in rec_text and "Bearer " not in rec_text, "key or bearer in the conformance record")
            check(any("[buffered-through-adapter]" in p.get("note", "") for p in rec["probes"] if p["probe"].startswith("stream")),
                  "streaming probes must carry the buffered-through-adapter note")
            with open(log) as f:
                ccalls = [json.loads(line) for line in f if line.strip()]
            cev = [c for c in ccalls if "event_rawpath" in c]
            check(len(cev) >= 15, f"conformance replay made {len(cev)} invokes, expected 15 (16 when the tools probe returns a call and P3 runs)")
            check({"/v1/models", "/v1/effective-config", "/v1/chat/completions", "/health"} <= {c["event_rawpath"] for c in cev},
                  f"conformance routes {sorted({c['event_rawpath'] for c in cev})}")
            check({c["event_method"] for c in cev} == {"GET", "POST"}, "conformance must use GET and POST")
            check(sum(1 for c in cev if not c["event_authorization_header"]) == 1, "exactly one conformance event (P12) lacks the authorization header")
            check("selftest-key-0123456789" not in json.dumps(ccalls), "key reached an argv in conformance mode")
            check(not any(os.path.exists(c["event_path"]) or os.path.exists(c["out_path"]) for c in cev), "a conformance temp file survived")
            os.unlink(out_json)
            before = len(cev)
            open(log, "w").close()
            with contextlib.redirect_stdout(sink), contextlib.redirect_stderr(sink):
                rc_b = conformance(conf_args(0.01), {"function": "fn-selftest", "memory_mb": 10240}, {})
            with open(log) as f:
                after = sum(1 for line in f if "event_rawpath" in line)
            check(rc_b == EXIT_BUDGET and not os.path.exists(out_json) and after == 0,
                  f"conformance budget guard: rc {rc_b}, record written {os.path.exists(out_json)}, invokes {after} (was {before})")
        finally:
            os.environ["PATH"] = saved_path
            if saved_key is None:
                os.environ.pop("APR_PROBE_KEY", None)
            else:
                os.environ["APR_PROBE_KEY"] = saved_key

    # 4. a key that somehow reached a record (an echoing error body) is redacted from the written file
    os.environ["APR_PROBE_KEY"] = "selftest-key-0123456789"
    try:
        with tempfile.TemporaryDirectory(prefix="bench-redact-") as td:
            out = os.path.join(td, "r.json")
            write_json(out, {"records": [{"error": "echo selftest-key-0123456789 back"}]})
            with open(out) as f:
                written = f.read()
        check("selftest-key-0123456789" not in written and "[redacted]" in written, "write_json must redact the key")
    finally:
        if saved_key is None:
            os.environ.pop("APR_PROBE_KEY", None)
        else:
            os.environ["APR_PROBE_KEY"] = saved_key

    if failures:
        for f in failures:
            print("SELFTEST FAIL: " + f, file=sys.stderr)
        return 1
    print(f"BENCH SELFTEST OK price={priced:.6f} (71047 ms at 10240 MB) parsed cpu_part={got['cpu_part']} "
          f"budget guard refused tools2_turn1 #2 (11 shimmed invokes, key in no argv, temp events removed; --conformance routes, header, no-auth event and guard checked)")
    return 0


def selftest_budget_only(budget_usd):
    """Run only the guard against the stub target: exit 4 with a budget_stop object when the budget is too small."""
    rn = Runner(StubTarget(), budget_usd)
    try:
        run_sequence(rn, "m", cold=True, long=True, conc=2)
    except BudgetStop as e:
        print(json.dumps({"budget_stop": e.info, "samples_run": len(rn.records), "aws_calls": 0}))
        print(f"budget guard refused {e.info['before']} before any sample would overspend", file=sys.stderr)
        return EXIT_BUDGET
    print(json.dumps({"budget_stop": None, "samples_run": len(rn.records), "aws_calls": 0}))
    return 0


def conformance(a, ident, aws, tgt=None):
    """--conformance: probe_openai.main over lambda:Invoke. Writes a `deployed` record with `transport: lambda-invoke`
    (the 09-01 probe-record contract's named exception: the HTTP wrapper never writes label `deployed`).
    Exit 0 written, 4 budget stop (NOTHING written: a table needs every probe), 2 refused or fixed-profile crosses."""
    missing = [n for n, v in (("--function", ident.get("function")), ("--model", a.model), ("--out", a.out), ("--pin", a.pin),
                              ("--pin-profile", a.pin_profile), ("--server-version", a.server_version)) if not v]
    if missing:
        print("REFUSED args: --conformance needs " + ", ".join(missing), file=sys.stderr)
        return 2
    fn = ident["function"]
    tgt = tgt or LambdaTarget(fn, a.profile, a.region or aws.get("region", "us-east-1"), int(ident.get("memory_mb") or 10240),
                              aws.get("log_group"))
    tx = LambdaInvokeTransport(tgt, a.budget_usd, log=lambda r: print(
        f"  {r['call']}: status={r['status']} wall={r['client_wall_s']}s billed={r['billed_ms']}ms cost={r['cost_usd']}",
        file=sys.stderr, flush=True))
    argv = ["--base", f"lambda-invoke://{fn}/v1", "--model", a.model, "--profile", a.pin_profile, "--label", "deployed",
            "--pin", a.pin, "--server-version", a.server_version, "--out", a.out]
    for u in a.unreleased:
        argv += ["--unreleased", u]
    extra = {"transport": "lambda-invoke", "streaming": "buffered-through-adapter", "function": fn,
             "image_digest": ident.get("image_digest")}
    try:
        rc = probe_openai.main(argv, tx=tx, extra_header=extra)
    except BudgetStop as e:
        print(f"budget_stop: refused {e.info['before']} (spent {e.info['spent_usd']} + est {e.info['est_usd']} > {e.info['budget_usd']}); "
              f"no record written", file=sys.stderr, flush=True)
        print(json.dumps({"budget_stop": e.info, "calls": len(tx.calls), "spent_usd": round(tx.spent_usd, 6)}))
        return EXIT_BUDGET
    print(f"conformance invokes={len(tx.calls)} spent_usd={round(tx.spent_usd, 6)} budget_usd={a.budget_usd}")
    print(json.dumps({"conformance_calls": tx.calls, "spent_usd": round(tx.spent_usd, 6)}))
    return rc


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--selftest", action="store_true", help="offline: parse, price, guard, shimmed aws path")
    ap.add_argument("--selftest-budget-only", action="store_true", help="offline: run only the guard on a stub; exits 4 when budget is too small")
    ap.add_argument("--function")
    ap.add_argument("--ids-dir", default=os.environ.get("LLM_ENDPOINT_IDS_DIR", "deploy/llm-endpoint/.ids"))
    ap.add_argument("--profile", default="ze-kasher-dev")
    ap.add_argument("--region")
    ap.add_argument("--model", help="the model id sent, e.g. Qwen3.5-0.8B-Q4_K_M")
    ap.add_argument("--label", default="deployed")
    ap.add_argument("--out")
    ap.add_argument("--cold", action="store_true", help="bump COLD_NONCE and time the first invoke separately")
    ap.add_argument("--long", action="store_true", help="include the 10-tool prefill")
    ap.add_argument("--conc", type=int, default=0, help="also fire N simultaneous short requests")
    ap.add_argument("--budget-usd", type=float, default=None)
    ap.add_argument("--conformance", action="store_true", help="replay the probe's P1-P13 through lambda:Invoke and write a `deployed` record")
    ap.add_argument("--pin", help="--conformance: the release tag the record is about (APR_TAG)")
    ap.add_argument("--pin-profile", dest="pin_profile", choices=probe_openai.PROFILES, help="--conformance: baseline or fixed (pin.env APR_PIN_PROFILE)")
    ap.add_argument("--server-version", dest="server_version", help="--conformance: the apr --version line from the function's [entry] log")
    ap.add_argument("--unreleased", action="append", default=[], metavar="PROBE=REFERENCE", help="--conformance, fixed profile only: see probe_openai.py")
    a = ap.parse_args()

    if a.selftest:
        return selftest()
    if a.selftest_budget_only:
        if a.budget_usd is None:
            print("--selftest-budget-only needs --budget-usd", file=sys.stderr)
            return 2
        return selftest_budget_only(a.budget_usd)

    if not os.environ.get("APR_PROBE_KEY"):
        print("REFUSED key: APR_PROBE_KEY is not set (the bearer is read from the environment only)", file=sys.stderr)
        return 2
    ident, aws = load_ids(a.ids_dir)
    fn = a.function or ident.get("function")
    if not fn or not a.model or not a.out:
        print("REFUSED args: need --function (or function.json in --ids-dir), --model and --out", file=sys.stderr)
        return 2
    if a.budget_usd is not None and a.budget_usd <= 0:
        print("REFUSED budget: --budget-usd must be positive", file=sys.stderr)
        return 2
    ident.setdefault("function", fn)
    if a.conformance:
        return conformance(a, ident, aws)
    mem_mb = int(ident.get("memory_mb") or 10240)
    tgt = LambdaTarget(fn, a.profile, a.region or aws.get("region", "us-east-1"), mem_mb, aws.get("log_group"))
    rn = Runner(tgt, a.budget_usd, log=progress)
    print(f"[bench] {a.label} -> {fn} model={a.model} budget={a.budget_usd}", file=sys.stderr, flush=True)
    stopped = False
    try:
        run_sequence(rn, a.model, cold=a.cold, long=a.long, conc=a.conc)
    except BudgetStop as e:
        stopped = True
        print(f"budget_stop: refused {e.info['before']} (spent {e.info['spent_usd']} + est {e.info['est_usd']} > {e.info['budget_usd']})",
              file=sys.stderr, flush=True)
    except (RuntimeError, subprocess.SubprocessError, OSError) as e:
        print(f"BENCH ABORTED: {e}", file=sys.stderr)
        write_json(a.out, build_result(rn, ident, a.model, a.label, attach_host(rn) if rn.records else {}))
        return 3
    info = attach_host(rn) if rn.records else {}
    result = build_result(rn, ident, a.model, a.label, info)
    write_json(a.out, result)
    print(table(result))
    print(json.dumps(result["derived"], indent=1))
    print(f"spent_usd={result['spent_usd']} budget_usd={result['budget_usd']} cpu_part={result['cpu_part']}")
    return EXIT_BUDGET if stopped else 0


if __name__ == "__main__":
    sys.exit(main())
