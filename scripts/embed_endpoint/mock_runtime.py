#!/usr/bin/env python3
"""Local AWS Lambda Runtime API driver (test harness, standard library only).

Drives a REAL binary exactly as Lambda would: it serves the Lambda Runtime
API on 127.0.0.1 and spawns the binary with AWS_LAMBDA_RUNTIME_API pointing
at it. The binary then performs

  GET  /2018-06-01/runtime/invocation/next                  (the event)
  POST /2018-06-01/runtime/invocation/<id>/response         (the answer)
  POST /2018-06-01/runtime/invocation/<id>/error            (Lambda-Runtime-Function-Error-Type)
  POST /2018-06-01/runtime/init/error                       (an init failure)

Two shapes are offered:

  invoke_many(...)  a finite batch: every event is queued at once, the
                    child is killed after the batch.
  Session(...)      a persistent session: ONE child process serves many
                    invokes, one event at a time (Session.invoke).

Payloads are delivered byte for byte: a `bytes` value is sent verbatim
(invalid UTF-8, a 100,000-deep array, a number outside f64 range, an empty
body, a 6 MiB body); every other Python value is JSON-serialized (compact,
UTF-8). The runtime deserializes the event before the handler runs, so a
driver that could only send well-formed JSON objects could never observe
those hostile shapes.

Usage:
  python3 -I scripts/embed_endpoint/mock_runtime.py --selftest
  python3 -I scripts/embed_endpoint/mock_runtime.py --bin <binary> \
      --event <file.json> [--event ...] [--raw-event <file>] [--env K=V ...] \
      [--out <dir>] [--timeout S] [--deadline-ms MS]

CLI output: one JSON line per payload (events first, then raw events).
Exit status is 0 only when every record is a `response`.
"""

import argparse
import base64
import collections
import hashlib
import http.server
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import threading
import time
import uuid

API_PATH = "/2018-06-01/runtime"
NEXT_PATH = API_PATH + "/invocation/next"
INIT_ERROR_PATH = API_PATH + "/init/error"
INVOCATION_RE = re.compile(r"/2018-06-01/runtime/invocation/([^/]+)/(response|error)")

# The server sends these; the stand-in client below spells them by parts so a
# change to one of these literals in this file cannot silently reach the client.
REQ_ID_HEADER = "Lambda-Runtime-Aws-Request-Id"
DEADLINE_HEADER = "Lambda-Runtime-Deadline-Ms"
ARN_HEADER = "Lambda-Runtime-Invoked-Function-Arn"
ERROR_TYPE_HEADER = "Lambda-Runtime-Function-Error-Type"
FAKE_FUNCTION_ARN = "local-embed-function"

FUNCTION_ENV = {
    "AWS_LAMBDA_FUNCTION_NAME": "local-embed-function",
    "AWS_LAMBDA_FUNCTION_VERSION": "1",
    "AWS_LAMBDA_FUNCTION_MEMORY_SIZE": "3008",
}

NO_ANSWER = "no_answer"


def encode_payload(value):
    """Bytes are sent verbatim; any other value is compact UTF-8 JSON."""
    if isinstance(value, (bytes, bytearray, memoryview)):
        return bytes(value)
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def _decode_body(raw):
    """Parsed JSON when the bytes parse; otherwise the text or base64 of them."""
    try:
        return json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, ValueError, RecursionError):
        pass
    try:
        return {"raw_utf8": raw.decode("utf-8")}
    except UnicodeDecodeError:
        return {"raw_b64": base64.b64encode(raw).decode("ascii")}


def _no_answer(request_id):
    return {"kind": NO_ANSWER, "body": None, "error_type": None, "request_id": request_id}


class _Handler(http.server.BaseHTTPRequestHandler):
    """HTTP/1.0 (one request per connection, as the runtime client accepts)."""

    def log_message(self, fmt, *args):  # keep the driver's output clean
        pass

    def _reply(self, code, body=b""):
        self.send_response(code)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if body:
            self.wfile.write(body)

    def _read_body(self):
        if "chunked" in self.headers.get("Transfer-Encoding", "").lower():
            chunks = []
            while True:
                size = int(self.rfile.readline().split(b";")[0].strip(), 16)
                if size == 0:
                    while self.rfile.readline() not in (b"\r\n", b"\n", b""):
                        pass
                    break
                chunks.append(self.rfile.read(size))
                self.rfile.readline()
            return b"".join(chunks)
        length = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(length)

    def do_GET(self):
        if self.path != NEXT_PATH:
            self._reply(404)
            return
        item = self.server.runtime._next_payload()
        if item is None:
            self._reply(503)
            return
        request_id, data = item
        deadline = int(time.time() * 1000) + self.server.runtime.deadline_ms
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.send_header(REQ_ID_HEADER, request_id)
        self.send_header(DEADLINE_HEADER, str(deadline))
        self.send_header(ARN_HEADER, FAKE_FUNCTION_ARN)
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        data = self._read_body()
        runtime = self.server.runtime
        if self.path == INIT_ERROR_PATH:
            runtime._record("init_error", None, data, self.headers.get(ERROR_TYPE_HEADER))
            self._reply(202, b'{"status":"OK"}')
            return
        match = INVOCATION_RE.fullmatch(self.path)
        if not match:
            self._reply(404)
            return
        kind = "response" if match.group(2) == "response" else "error"
        error_type = self.headers.get(ERROR_TYPE_HEADER) if kind == "error" else None
        accepted = runtime._record(kind, match.group(1), data, error_type)
        self._reply(202 if accepted else 404, b'{"status":"OK"}' if accepted else b"")


class _Server(http.server.ThreadingHTTPServer):
    daemon_threads = True


class MockRuntime:
    """The Runtime API on an ephemeral 127.0.0.1 port, with an event queue."""

    def __init__(self, deadline_ms=60000):
        self.deadline_ms = deadline_ms
        self._cond = threading.Condition()
        self._queue = collections.deque()
        self._in_flight = set()
        self._stopping = False
        self.records = []
        self._server = _Server(("127.0.0.1", 0), _Handler)
        self._server.runtime = self
        self._thread = threading.Thread(
            target=self._server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True
        )

    @property
    def address(self):
        return "127.0.0.1:%d" % self._server.server_address[1]

    def start(self):
        self._thread.start()
        return self

    def stop(self):
        with self._cond:
            self._stopping = True
            self._cond.notify_all()
        self._server.shutdown()
        self._server.server_close()

    def enqueue(self, value):
        request_id = str(uuid.uuid4())
        data = encode_payload(value)
        with self._cond:
            self._queue.append((request_id, data))
            self._cond.notify_all()
        return request_id

    def _next_payload(self):
        with self._cond:
            while not self._queue and not self._stopping:
                self._cond.wait()
            if self._stopping:
                return None
            request_id, data = self._queue.popleft()
            self._in_flight.add(request_id)
            return request_id, data

    def _record(self, kind, request_id, raw, error_type):
        with self._cond:
            if kind != "init_error":
                if request_id not in self._in_flight:
                    return False
                self._in_flight.discard(request_id)
            self.records.append(
                {
                    "kind": kind,
                    "body": _decode_body(raw),
                    "error_type": error_type,
                    "request_id": request_id,
                }
            )
            self._cond.notify_all()
        return True


CLIENT_SRC = r'''
import hashlib, json, os, sys, time, urllib.request

API = os.environ["AWS_LAMBDA_RUNTIME_API"]
BASE = "http://" + API + "/2018-06-01/runtime"
MODE = os.environ.get("MOCK_BEHAVIOUR", "echo")
RID_HDR = "Lambda-Runtime-" + "Aws-Request-Id"
DL_HDR = "Lambda-Runtime-" + "Deadline-Ms"
ERR_HDR = "Lambda-Runtime-" + "Function-Error-Type"
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def post(path, data, headers):
    req = urllib.request.Request(BASE + path, data=data, method="POST", headers=headers)
    with OPENER.open(req, timeout=60) as resp:
        resp.read()


if MODE == "exit_silent":
    sys.exit(0)
if MODE == "init_error":
    post("/init/error", json.dumps({"errorMessage": "mock init", "errorType": "Sandbox.MockInit"}).encode(), {ERR_HDR: "Sandbox.MockInit"})
    sys.exit(1)

seq = 0
while True:
    with OPENER.open(BASE + "/invocation/next", timeout=3600) as resp:
        body = resp.read()
        rid = resp.headers.get(RID_HDR)
        dl = resp.headers.get(DL_HDR)
    if not rid:
        sys.exit(3)
    if MODE == "hang":
        time.sleep(3600)
    seq += 1
    if MODE == "error":
        post("/invocation/%s/error" % rid, json.dumps({"errorMessage": "mock", "errorType": "Sandbox.MockError"}).encode(), {ERR_HDR: "Sandbox.MockError"})
        continue
    info = {
        "pid": os.getpid(),
        "seq": seq,
        "len": len(body),
        "sha256": hashlib.sha256(body).hexdigest(),
        "deadline": int(dl) if dl else None,
        "parent_secret_visible": "MOCK_PARENT_SECRET" in os.environ,
        "path_set": "PATH" in os.environ,
        "home_set": "HOME" in os.environ,
    }
    post("/invocation/%s/response" % rid, json.dumps(info).encode(), {"Content-Type": "application/json"})
'''


class Session:
    """One live child process serving many invokes (plan 11-11 uses this)."""

    def __init__(self, binary, env=None, deadline_ms=60000, timeout_s=300):
        self._argv = [binary] if isinstance(binary, str) else list(binary)
        self._env = dict(env or {})
        self._timeout_s = timeout_s
        self._closed = False
        self._log = tempfile.TemporaryFile()
        self._rt = MockRuntime(deadline_ms).start()
        self._proc = None
        child_env = {k: os.environ[k] for k in ("PATH", "HOME") if k in os.environ}
        child_env.update(FUNCTION_ENV)
        child_env.update(self._env)
        child_env["AWS_LAMBDA_RUNTIME_API"] = self._rt.address
        try:
            self._proc = subprocess.Popen(
                self._argv,
                env=child_env,
                stdin=subprocess.DEVNULL,
                stdout=self._log,
                stderr=subprocess.STDOUT,
            )
        except OSError:
            self.close()
            raise

    @property
    def pid(self):
        return self._proc.pid

    def alive(self):
        return self._proc is not None and self._proc.poll() is None

    def wait_for_exit(self, timeout):
        try:
            self._proc.wait(timeout=timeout)
            return True
        except subprocess.TimeoutExpired:
            return False

    def log_tail(self, limit=2000):
        self._log.seek(0)
        return self._log.read()[-limit:].decode("utf-8", errors="replace")

    def _await(self, done, timeout):
        """Return 'ok', 'exited' or 'timeout'. Caller holds no lock."""
        end = time.monotonic() + timeout
        cond = self._rt._cond
        with cond:
            while True:
                if done():
                    return "ok"
                if self._proc.poll() is not None:
                    grace_end = time.monotonic() + 0.2
                    while not done() and time.monotonic() < grace_end:
                        cond.wait(0.02)
                    return "ok" if done() else "exited"
                remaining = end - time.monotonic()
                if remaining <= 0:
                    return "timeout"
                cond.wait(min(0.05, remaining))

    def _kill(self):
        if self._proc is not None:
            if self._proc.poll() is None:
                self._proc.kill()
            self._proc.wait()

    def invoke(self, event, timeout_s=None):
        if not self.alive():
            return _no_answer(None)
        timeout = self._timeout_s if timeout_s is None else timeout_s
        with self._rt._cond:
            before = len(self._rt.records)
        request_id = self._rt.enqueue(event)
        outcome = self._await(lambda: len(self._rt.records) > before, timeout)
        if outcome == "ok":
            return self._rt.records[before]
        if outcome == "timeout":
            self._kill()
        with self._rt._cond:
            self._rt._queue.clear()
        return _no_answer(request_id)

    def close(self):
        if self._closed:
            return
        self._closed = True
        self._kill()
        self._rt.stop()

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        self.close()
        return False


def invoke_many(binary, events, env=None, deadline_ms=60000, timeout_s=300):
    """Finite batch: queue every event, return one record per event."""
    session = Session(binary, env, deadline_ms, timeout_s)
    want = len(events)
    try:
        with session._rt._cond:
            for event in events:
                session._rt.enqueue(event)
        session._await(
            lambda: len(session._rt.records) >= want
            or any(r["kind"] == "init_error" for r in session._rt.records),
            timeout_s,
        )
        with session._rt._cond:
            records = list(session._rt.records)
    finally:
        session.close()
    out = records[:want]
    while len(out) < want:
        out.append(_no_answer(None))
    return out


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _env(mode):
    return {"MOCK_BEHAVIOUR": mode}


def _selftest_cases(argv):
    """Each case returns (ok, detail). A case that raises is a FAIL, not a crash."""

    def c01():
        ev = {"hello": "world"}
        (r,) = invoke_many(argv, [ev], _env("echo"), timeout_s=20)
        ok = r["kind"] == "response" and r["body"]["sha256"] == _sha(encode_payload(ev))
        return ok, r["kind"]

    def c02():
        e1, e2 = {"n": 1}, {"n": 2}
        r1, r2 = invoke_many(argv, [e1, e2], _env("echo"), timeout_s=20)
        ok = (
            r1["body"]["seq"] == 1 and r1["body"]["sha256"] == _sha(encode_payload(e1))
            and r2["body"]["seq"] == 2 and r2["body"]["sha256"] == _sha(encode_payload(e2))
        )
        return ok, "seq %s,%s" % (r1["body"].get("seq"), r2["body"].get("seq"))

    def c03():
        (r,) = invoke_many(argv, [{}], _env("error"), timeout_s=20)
        ok = r["kind"] == "error" and r["error_type"] == "Sandbox.MockError"
        return ok, "%s %s" % (r["kind"], r["error_type"])

    def c04():
        (r,) = invoke_many(argv, [{}], _env("init_error"), timeout_s=20)
        ok = r["kind"] == "init_error" and r["error_type"] == "Sandbox.MockInit"
        return ok, "%s %s" % (r["kind"], r["error_type"])

    def c05():
        started = time.monotonic()
        (r,) = invoke_many(argv, [{}], _env("exit_silent"), timeout_s=30)
        elapsed = time.monotonic() - started
        return r["kind"] == NO_ANSWER and elapsed < 10, "%s in %.2fs" % (r["kind"], elapsed)

    def c06():
        started = time.monotonic()
        with Session(argv, _env("hang"), timeout_s=2) as s:
            r = s.invoke({"x": 1})
            elapsed = time.monotonic() - started
            dead = not s.alive()
        ok = r["kind"] == NO_ANSWER and 1.5 <= elapsed < 15 and dead
        return ok, "%s in %.2fs, dead=%s" % (r["kind"], elapsed, dead)

    def c07():
        raw_bad, raw_empty = b"\xff\xfe\x00", b""
        rb, re_ = invoke_many(argv, [raw_bad, raw_empty], _env("echo"), timeout_s=20)
        ok = (
            rb["body"]["sha256"] == _sha(raw_bad) and rb["body"]["len"] == 3
            and re_["body"]["sha256"] == _sha(raw_empty) and re_["body"]["len"] == 0
        )
        return ok, "invalid utf-8 len %s, empty len %s" % (rb["body"].get("len"), re_["body"].get("len"))

    def c08():
        deep = b"[" * 100000 + b"]" * 100000
        huge_num = b'{"a":1e999}'
        rd, rn = invoke_many(argv, [deep, huge_num], _env("echo"), timeout_s=20)
        ok = (
            rd["body"]["sha256"] == _sha(deep) and rd["body"]["len"] == len(deep)
            and rn["body"]["sha256"] == _sha(huge_num)
        )
        return ok, "deep len %s" % rd["body"].get("len")

    def c09():
        six_mib = bytes(range(256)) * ((6 * 2 ** 20) // 256)
        (r,) = invoke_many(argv, [six_mib], _env("echo"), timeout_s=60)
        ok = (
            r["kind"] == "response" and r["body"]["sha256"] == _sha(six_mib)
            and r["body"]["len"] == len(six_mib)
        )
        return ok, "len %s" % r["body"].get("len")

    def c10():
        before_ms = int(time.time() * 1000)
        (r,) = invoke_many(argv, [{}], _env("echo"), deadline_ms=60000, timeout_s=20)
        deadline = r["body"].get("deadline")
        ok = deadline is not None and before_ms < deadline <= before_ms + 60000 + 5000
        return ok, "deadline - now = %s ms" % (deadline - before_ms if deadline else None)

    def c11():
        with Session(argv, _env("echo"), timeout_s=20) as s:
            outs = [s.invoke({"i": i}) for i in range(3)]
            session_pid = s.pid
        pids = {o["body"]["pid"] for o in outs if o["body"]}
        seqs = [o["body"].get("seq") for o in outs]
        ok = pids == {session_pid} and seqs == [1, 2, 3]
        return ok, "pids %s seqs %s" % (sorted(pids), seqs)

    def c12():
        with Session(argv, _env("echo"), timeout_s=20) as s:
            first = s.invoke({"x": 1})
            os.kill(s.pid, signal.SIGKILL)
            s.wait_for_exit(10)
            started = time.monotonic()
            after = s.invoke({"x": 2})
            elapsed = time.monotonic() - started
        ok = first["kind"] == "response" and after["kind"] == NO_ANSWER and elapsed < 2
        return ok, "%s in %.3fs" % (after["kind"], elapsed)

    def c13():
        s = Session(argv, _env("echo"), timeout_s=20)
        s.invoke({"x": 1})
        child_pid = s.pid
        s.close()
        try:
            os.kill(child_pid, 0)
            gone = False
        except ProcessLookupError:
            gone = True
        return gone, "pid %s gone=%s" % (child_pid, gone)

    def c14():
        os.environ["MOCK_PARENT_SECRET"] = "present-in-parent"
        try:
            (r,) = invoke_many(argv, [{}], _env("echo"), timeout_s=20)
        finally:
            del os.environ["MOCK_PARENT_SECRET"]
        body = r["body"] or {}
        ok = body.get("parent_secret_visible") is False and body.get("path_set") is True and body.get("home_set") is True
        return ok, "secret visible=%s" % body.get("parent_secret_visible")

    return [
        ("1 echo one event", c01),
        ("2 two events in order", c02),
        ("3 error post recorded with type", c03),
        ("4 init error recorded", c04),
        ("5 exits without answering is no_answer at once", c05),
        ("6 hung client killed at the timeout", c06),
        ("7 invalid UTF-8 and empty body verbatim", c07),
        ("8 deep nesting and 1e999 verbatim", c08),
        ("9 6 MiB body verbatim", c09),
        ("10 deadline header is future epoch ms", c10),
        ("11 persistent session served by one process", c11),
        ("12 invoke after child died is no_answer at once", c12),
        ("13 close kills the child", c13),
        ("14 parent environment not inherited", c14),
    ]


def _selftest():
    tmp = tempfile.mkdtemp(prefix="mockrt-selftest-")
    client = os.path.join(tmp, "stand_in_client.py")
    with open(client, "w", encoding="utf-8") as handle:
        handle.write(CLIENT_SRC)
    argv = [sys.executable, "-I", client]

    print("MOCK RUNTIME SELFTEST")
    passed = 0
    cases = _selftest_cases(argv)
    for name, fn in cases:
        try:
            ok, detail = fn()
        except Exception as exc:  # a broken driver must FAIL its own table
            ok, detail = False, "raised %s: %s" % (type(exc).__name__, exc)
        passed += 1 if ok else 0
        print("  %s %s: %s" % ("PASS" if ok else "FAIL", name, detail))

    if passed == len(cases):
        print("MOCK RUNTIME SELFTEST OK %d cases" % passed)
        return 0
    print("MOCK RUNTIME SELFTEST FAILED %d of %d cases passed" % (passed, len(cases)))
    return 1


def _parse_env(pairs, parser):
    env = {}
    for pair in pairs:
        key, sep, value = pair.partition("=")
        if not sep or not key:
            parser.error("--env expects K=V, got %r" % pair)
        env[key] = value
    return env


def main(argv=None):
    parser = argparse.ArgumentParser(description="Local Lambda Runtime API driver")
    parser.add_argument("--selftest", action="store_true", help="run the case table")
    parser.add_argument("--bin", help="binary to drive (the real runtime child)")
    parser.add_argument("--event", action="append", default=[], help="JSON file, re-serialized")
    parser.add_argument("--raw-event", action="append", default=[], dest="raw_event",
                        help="file sent as its exact bytes")
    parser.add_argument("--env", action="append", default=[], help="K=V for the child")
    parser.add_argument("--out", help="directory for records.jsonl")
    parser.add_argument("--timeout", type=float, default=300.0)
    parser.add_argument("--deadline-ms", type=int, default=60000, dest="deadline_ms")
    args = parser.parse_args(argv)

    if args.selftest:
        return _selftest()
    if not args.bin or not (args.event or args.raw_event):
        parser.error("--bin and at least one --event or --raw-event are required")

    payloads = []
    for path in args.event:
        with open(path, "rb") as handle:
            payloads.append(json.loads(handle.read().decode("utf-8")))
    for path in args.raw_event:
        with open(path, "rb") as handle:
            payloads.append(handle.read())

    records = invoke_many(
        args.bin, payloads, _parse_env(args.env, parser), args.deadline_ms, args.timeout
    )
    lines = [json.dumps(record, ensure_ascii=False) for record in records]
    for line in lines:
        print(line)
    if args.out:
        os.makedirs(args.out, exist_ok=True)
        with open(os.path.join(args.out, "records.jsonl"), "w", encoding="utf-8") as handle:
            handle.write("\n".join(lines) + "\n")
    ok = len(records) == len(payloads) and all(r["kind"] == "response" for r in records)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
