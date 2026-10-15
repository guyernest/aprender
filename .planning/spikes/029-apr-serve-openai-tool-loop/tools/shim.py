#!/usr/bin/env python3
"""Wire-logging reverse proxy between an OpenAI client and `apr serve`.

Forensic layer for the agent-loop runs: every request and response body is appended as one
JSON line (ISO timestamp, category, latency, status) to the log file, so the loop's exact wire
shape is a replayable artifact.

Optional `--fix-null-content` rewrites an assistant message whose `content` is missing or
null to `content: ""` before forwarding — the one-line client-side fix for the 422
`apr serve` 0.70.3 returns on pmcp-agent's tool-use turn. Nothing else is touched.

    python3 -I tools/shim.py --listen 8129 --upstream http://127.0.0.1:8029 \
        --log results/wire-0.8b.jsonl [--fix-null-content]
"""
import argparse
import json
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ARGS = None
STATS = {"requests": 0, "fixed": 0, "errors": 0, "t0": time.perf_counter()}


def now():
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def log(entry):
    with open(ARGS.log, "a") as f:
        f.write(json.dumps(entry) + "\n")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):  # quiet
        pass

    def _forward(self, method):
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b""
        body = None
        fixed = []
        if raw:
            try:
                body = json.loads(raw)
            except json.JSONDecodeError:
                body = None
        if ARGS.fix_null_content and isinstance(body, dict) and isinstance(body.get("messages"), list):
            for i, m in enumerate(body["messages"]):
                if isinstance(m, dict) and m.get("role") == "assistant" and m.get("content") is None:
                    m["content"] = ""
                    fixed.append(i)
            if fixed:
                raw = json.dumps(body).encode()
                STATS["fixed"] += 1
        headers = {k: v for k, v in self.headers.items()
                   if k.lower() not in ("host", "content-length", "transfer-encoding", "connection")}
        if raw:
            headers["Content-Length"] = str(len(raw))
        req = urllib.request.Request(ARGS.upstream + self.path, data=raw or None, headers=headers, method=method)
        t0 = time.perf_counter()
        try:
            with urllib.request.urlopen(req, timeout=ARGS.timeout) as r:
                status, resp_raw, resp_headers = r.status, r.read(), dict(r.headers)
        except urllib.error.HTTPError as e:
            status, resp_raw, resp_headers = e.code, e.read(), dict(e.headers)
            STATS["errors"] += 1
        dt = time.perf_counter() - t0
        STATS["requests"] += 1
        try:
            resp_body = json.loads(resp_raw)
        except json.JSONDecodeError:
            resp_body = {"_raw": resp_raw.decode(errors="replace")[:4000]}
        log({"ts": now(), "cat": "wire", "n": STATS["requests"], "method": method, "path": self.path,
             "status": status, "seconds": round(dt, 3), "fixed_assistant_indices": fixed,
             "request": body if body is not None else raw.decode(errors="replace")[:4000],
             "response": resp_body})
        print(f"[shim] #{STATS['requests']} {method} {self.path} -> {status} in {dt:.2f}s"
              + (f" (fixed content on assistant msgs {fixed})" if fixed else ""), file=sys.stderr, flush=True)
        self.send_response(status)
        for k, v in resp_headers.items():
            if k.lower() in ("content-length", "transfer-encoding", "connection"):
                continue
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(resp_raw)))
        self.end_headers()
        self.wfile.write(resp_raw)

    def do_POST(self):
        self._forward("POST")

    def do_GET(self):
        if self.path == "/__shim/summary":
            out = json.dumps({**STATS, "uptime_s": round(time.perf_counter() - STATS["t0"], 1)}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(out)))
            self.end_headers()
            self.wfile.write(out)
            return
        self._forward("GET")


def main():
    global ARGS
    ap = argparse.ArgumentParser()
    ap.add_argument("--listen", type=int, required=True)
    ap.add_argument("--upstream", required=True)
    ap.add_argument("--log", required=True)
    ap.add_argument("--fix-null-content", action="store_true")
    ap.add_argument("--timeout", type=float, default=900)
    ARGS = ap.parse_args()
    log({"ts": now(), "cat": "start", "listen": ARGS.listen, "upstream": ARGS.upstream,
         "fix_null_content": ARGS.fix_null_content})
    srv = ThreadingHTTPServer(("127.0.0.1", ARGS.listen), Handler)
    print(f"[shim] listening on 127.0.0.1:{ARGS.listen} -> {ARGS.upstream} fix_null_content={ARGS.fix_null_content}",
          file=sys.stderr, flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
