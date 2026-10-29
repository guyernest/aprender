#!/usr/bin/env python3
"""The authenticated MCP client for the Laya training server's live checks (plan 14-10; D-10, T-14-41, T-14-72).

    python3 -I scripts/laya_deploy/train_call.py --url URL [--allow-loopback-http] COMMAND [ARGS...]
    python3 -I scripts/laya_deploy/train_call.py --selftest

COMMAND is one of

    list-tools                          tools/list
    validate URI                        tools/call validate_dataset {dataset_uri}
    train URI NAME [EPOCHS]             tools/call train {dataset_uri, name[, epochs]} as a task-augmented call ("task": {})
    status TASK_ID                      tools/call train_status {task_id}
    result TASK_ID                      tasks/result {taskId}
    cancel TASK_ID                      tasks/cancel {taskId}
    call-tool TOOL JSON_ARGS            tools/call TOOL with EXACTLY the given argument object, embedded byte for byte (the live
                                        negative tests' path: an unknown `seeds` key must reach the server, not be normalised away)

`just laya-trainer-call PROFILE URL COMMAND...` is the one way to run it: it mints the bearer with m2m_token.py from
LAYA_M2M_FILE and execs this file with the token in APRENDER_DECIDE_PROBE_TOKEN (and LAYA_M2M_MINTED=1). The token is read
ONLY from that environment variable; it is never on argv, never printed (every output line is scrubbed of it), never written.
No token, or a token that was not minted (LAYA_M2M_MINTED != 1): `REFUSED call: no token`, exit 2.

The client speaks streamable HTTP: `initialize`, then `notifications/initialized`, then the call, carrying the Mcp-Session-Id the
server assigned; it accepts a JSON or an SSE answer. It prints ONE JSON line per run,

    {"method": ..., "ok": true,  "result": ...}
    {"method": ..., "ok": false, "error": ...}

and exits 0 only when ok is true (a JSON-RPC error and a tool result with isError are not ok). An HTTP error answers
{"error": {"http_status": N}} with no body echoed (a 401 is exit 1), and a transport failure {"error": {"transport": ...}}.

Transport rules: https only (http://127.0.0.1:<port>/mcp only with --allow-loopback-http, for the offline selftests); no proxy; and
EVERY redirect is refused (`REFUSED call: redirect`, exit 2): urllib's default handler would re-send the Authorization header to
the Location, so a bearer could follow a 3xx to another host. When SSL_CERT_FILE is unset and the default context carries no CA
certificates (python.org builds), /etc/ssl/cert.pem is used (13-HANDOFF section 9, F-13-08-3).

stdlib only.
"""

import http.client
import http.server
import json
import os
import re
import ssl
import subprocess
import sys
import threading
import urllib.error
import urllib.parse
import urllib.request

TOKEN_ENV = "APRENDER_DECIDE_PROBE_TOKEN"
MINTED_ENV = "LAYA_M2M_MINTED"
TIMEOUT_S = 60
MAX_BODY = 4 * 1024 * 1024
PROTOCOL = "2025-11-25"
HEADER_SAFE_RE = re.compile(r"[\x21-\x7e]+")
TOOL_RE = re.compile(r"[A-Za-z0-9_.-]{1,64}")
OAUTH_CODE_RE = re.compile(r"[a-z_]{1,40}")
CA_FALLBACK = "/etc/ssl/cert.pem"
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMMANDS = ("list-tools", "validate", "train", "status", "result", "cancel", "call-tool")


class Refused(Exception):
    def __init__(self, rule, msg):
        super().__init__(rule)
        self.rule = rule
        self.msg = msg


def refuse(rule, msg):
    raise Refused(rule, msg)


# ---- transport ------------------------------------------------------------------------------------------------

class _RedirectRefused(urllib.request.HTTPRedirectHandler):
    """Never follow: nothing is re-sent anywhere, and the bearer stays on the one host it was minted for."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise Refused("redirect", "the server answered HTTP %d with a Location; redirects are never followed (the bearer would follow it)" % code)


def tls_context():
    ctx = ssl.create_default_context()
    if not os.environ.get("SSL_CERT_FILE") and ctx.cert_store_stats().get("x509_ca", 0) == 0 and os.path.isfile(CA_FALLBACK):
        ctx = ssl.create_default_context(cafile=CA_FALLBACK)
    return ctx


def build_opener(url):
    handlers = [urllib.request.ProxyHandler({}), _RedirectRefused()]
    if url.startswith("https://"):
        handlers.append(urllib.request.HTTPSHandler(context=tls_context()))
    return urllib.request.build_opener(*handlers)


def check_url(url, allow_loopback_http):
    try:
        u = urllib.parse.urlsplit(url)
        port = u.port
    except ValueError:
        refuse("url", "the URL does not parse")
    host = (u.hostname or "").lower()
    if u.username is not None or u.password is not None or u.query or u.fragment:
        refuse("url", "the URL carries userinfo, a query or a fragment")
    if u.scheme == "https":
        if not re.fullmatch(r"[a-z0-9-]+(\.[a-z0-9-]+)+", host):
            refuse("url", "the URL host is not a DNS name")
    elif u.scheme == "http" and allow_loopback_http and host == "127.0.0.1" and port:
        pass
    elif u.scheme == "http":
        refuse("url", "https only: the bearer is never sent over http (loopback http only with --allow-loopback-http)")
    else:
        refuse("url", "the URL is not https")
    if u.path != "/mcp":
        refuse("url", "the URL path is not /mcp")


class Wire:
    """One streamable-HTTP session: every request carries the bearer; the session id and protocol version follow initialize."""

    def __init__(self, url, token):
        self.url = url
        self.token = token
        self.opener = build_opener(url)
        self.session = None
        self.protocol = None

    def post(self, body):
        """POST one JSON-RPC body (str). Returns (status, content_type, text, headers); an HTTP error status is a return, not a raise."""
        req = urllib.request.Request(self.url, data=body.encode("utf-8"), method="POST")
        req.add_header("Content-Type", "application/json")
        req.add_header("Accept", "application/json, text/event-stream")
        req.add_unredirected_header("Authorization", "Bearer " + self.token)
        if self.session:
            req.add_header("Mcp-Session-Id", self.session)
        if self.protocol:
            req.add_header("MCP-Protocol-Version", self.protocol)
        try:
            with self.opener.open(req, timeout=TIMEOUT_S) as r:
                text = r.read(MAX_BODY + 1)
                status, ctype, headers = r.status, r.headers.get("Content-Type", ""), r.headers
        except Refused:
            raise
        except urllib.error.HTTPError as e:
            if 300 <= e.code < 400:
                refuse("redirect", "the server answered HTTP %d; redirects are never followed" % e.code)
            try:
                text = e.read(65537) if e.fp is not None else b""
            except (OSError, http.client.HTTPException):
                text = b""
            status, ctype, headers = e.code, e.headers.get("Content-Type", "") if e.headers else "", e.headers
        except (urllib.error.URLError, OSError, http.client.HTTPException, ValueError) as e:
            raise Transport(type(e).__name__)
        if len(text) > MAX_BODY:
            raise Transport("ResponseTooLarge")
        return status, ctype, text.decode("utf-8", "replace"), headers


class Transport(Exception):
    pass


def parse_answer(ctype, text, want_id):
    """The JSON-RPC message answering want_id, from a JSON body or an SSE stream; None when there is none."""
    msgs = []
    if "text/event-stream" in ctype:
        for block in re.split(r"\r?\n\r?\n", text):
            data = "\n".join(ln[5:].lstrip() for ln in block.splitlines() if ln.startswith("data:"))
            if data:
                try:
                    msgs.append(json.loads(data))
                except ValueError:
                    pass
    else:
        try:
            msgs.append(json.loads(text))
        except ValueError:
            pass
    flat = []
    for m in msgs:
        flat.extend(m if isinstance(m, list) else [m])
    for m in flat:
        if isinstance(m, dict) and m.get("id") == want_id and ("result" in m or "error" in m):
            return m
    return None


def http_error(status, text):
    err = {"http_status": status}
    try:
        obj = json.loads(text)
        code = obj.get("error") if isinstance(obj, dict) else None
        if isinstance(code, str) and OAUTH_CODE_RE.fullmatch(code):
            err["oauth_error"] = code
    except ValueError:
        pass
    return err


# ---- the request bodies -----------------------------------------------------------------------------------------

def rpc(method, params, rid):
    return json.dumps({"jsonrpc": "2.0", "id": rid, "method": method, "params": params}, separators=(",", ":"))


def build_call(cmd, args):
    """(label, tool, body) for the one call after initialize (id 2)."""
    def need(n_min, n_max, usage):
        if not (n_min <= len(args) <= n_max):
            refuse("usage", "usage: %s" % usage)

    if cmd == "list-tools":
        need(0, 0, "list-tools")
        return "tools/list", None, rpc("tools/list", {}, 2)
    if cmd == "validate":
        need(1, 1, "validate URI")
        return "tools/call", "validate_dataset", rpc("tools/call", {"name": "validate_dataset", "arguments": {"dataset_uri": args[0]}}, 2)
    if cmd == "train":
        need(2, 3, "train URI NAME [EPOCHS]")
        a = {"dataset_uri": args[0], "name": args[1]}
        if len(args) == 3:
            if not re.fullmatch(r"[0-9]{1,6}", args[2]):
                refuse("usage", "EPOCHS is not a whole number")
            a["epochs"] = int(args[2])
        return "tools/call", "train", rpc("tools/call", {"name": "train", "arguments": a, "task": {}}, 2)
    if cmd == "status":
        need(1, 1, "status TASK_ID")
        return "tools/call", "train_status", rpc("tools/call", {"name": "train_status", "arguments": {"task_id": args[0]}}, 2)
    if cmd in ("result", "cancel"):
        need(1, 1, "%s TASK_ID" % cmd)
        return "tasks/" + cmd, None, rpc("tasks/" + cmd, {"taskId": args[0]}, 2)
    if cmd == "call-tool":
        need(2, 2, "call-tool TOOL JSON_ARGS")
        tool, raw = args
        if not TOOL_RE.fullmatch(tool):
            refuse("usage", "TOOL is not a tool name")
        try:
            parsed = json.loads(raw)
        except ValueError:
            refuse("usage", "JSON_ARGS is not JSON")
        if not isinstance(parsed, dict):
            refuse("usage", "JSON_ARGS is not a JSON object")
        # Embedded as given: re-serialising would normalise away exactly what a negative test sends.
        body = '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":%s,"arguments":%s}}' % (json.dumps(tool), raw.strip())
        return "tools/call", tool, body
    refuse("usage", "unknown command %r (%s)" % (cmd[:40], ", ".join(COMMANDS)))


# ---- run ---------------------------------------------------------------------------------------------------------

def parse_argv(argv):
    url, loop, rest, i = None, False, [], 0
    while i < len(argv):
        a = argv[i]
        if a == "--url":
            if url is not None or i + 1 >= len(argv):
                refuse("usage", "--url needs one value")
            url = argv[i + 1]
            i += 2
        elif a == "--allow-loopback-http":
            loop = True
            i += 1
        elif not rest and a.startswith("--"):
            refuse("usage", "unknown option %s" % a[:40])
        else:
            rest.append(a)
            i += 1
    if url is None or not rest:
        refuse("usage", "usage: train_call.py --url URL COMMAND [ARGS...] (COMMAND: %s)" % ", ".join(COMMANDS))
    return url, loop, rest[0], rest[1:]


def run(argv, env, out, err):
    token = env.get(TOKEN_ENV, "")
    scrub = lambda s: s.replace(token, "<token>") if token else s

    def emit(obj):
        print(scrub(json.dumps(obj, separators=(",", ":"))), file=out, flush=True)

    try:
        url, loop, cmd, args = parse_argv(argv)
        if not token or env.get(MINTED_ENV) != "1":
            refuse("no token", "the bearer comes only from `just laya-trainer-call` (m2m_token.py sets %s and %s=1); nothing was sent" % (TOKEN_ENV, MINTED_ENV))
        if not HEADER_SAFE_RE.fullmatch(token):
            refuse("no token", "the token in %s is not a header-safe string; nothing was sent" % TOKEN_ENV)
        if any(token in a for a in argv):
            refuse("usage", "a command-line argument carries the token; the token is never on argv")
        check_url(url, loop)
        label, tool, call_body = build_call(cmd, args)
        wire = Wire(url, token)
        line = {"method": label}
        if tool:
            line["tool"] = tool
        status, ctype, text, headers = wire.post(rpc("initialize", {"protocolVersion": PROTOCOL, "capabilities": {},
                                                                    "clientInfo": {"name": "laya-train-call", "version": "1"}}, 1))
        if status != 200:
            line.update(ok=False, error=http_error(status, text), stage="initialize")
            emit(line)
            return 1
        init = parse_answer(ctype, text, 1)
        if init is None or "result" not in init:
            line.update(ok=False, error={"initialize": "no result"}, stage="initialize")
            emit(line)
            return 1
        wire.session = headers.get("Mcp-Session-Id") or headers.get("mcp-session-id")
        wire.protocol = (init["result"] or {}).get("protocolVersion") if isinstance(init.get("result"), dict) else None
        wire.post('{"jsonrpc":"2.0","method":"notifications/initialized"}')
        status, ctype, text, _ = wire.post(call_body)
        if status != 200:
            line.update(ok=False, error=http_error(status, text))
            emit(line)
            return 1
        ans = parse_answer(ctype, text, 2)
        if ans is None:
            line.update(ok=False, error={"response": "no JSON-RPC answer to the call"})
            emit(line)
            return 1
        if "error" in ans:
            line.update(ok=False, error=ans["error"])
            emit(line)
            return 1
        res = ans["result"]
        is_err = isinstance(res, dict) and res.get("isError") is True
        line.update(ok=not is_err, result=res)
        emit(line)
        return 1 if is_err else 0
    except Refused as e:
        print(scrub("REFUSED call: %s: %s" % (e.rule, e.msg)), file=err, flush=True)
        return 2
    except Transport as e:
        emit({"method": "transport", "ok": False, "error": {"transport": str(e)}})
        return 1


# ---- the selftest: a local stdlib HTTP fake -----------------------------------------------------------------------

class Fake:
    """One loopback MCP server. mode: ok | sse | 401 | redirect | echo | rpc-error | isError. Records every POST."""

    def __init__(self, token, mode="ok", other=None):
        self.token = token
        self.mode = mode
        self.other = other
        self.log = []
        outer = self

        class H(http.server.BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def _send(self, code, body=b"", headers=(), ctype="application/json"):
                self.send_response(code)
                for k, v in headers:
                    self.send_header(k, v)
                if body:
                    self.send_header("Content-Type", ctype)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_POST(self):
                n = int(self.headers.get("Content-Length") or 0)
                raw = self.rfile.read(n).decode("utf-8") if n else ""
                try:
                    msg = json.loads(raw)
                except ValueError:
                    msg = {}
                outer.log.append({"path": self.path, "auth": self.headers.get("Authorization"), "session": self.headers.get("Mcp-Session-Id"),
                                  "raw": raw, "method": msg.get("method"), "params": msg.get("params")})
                if outer.mode == "401":
                    return self._send(401, b'{"error":"invalid_token","error_description":"expired"}')
                if outer.mode == "redirect":
                    return self._send(302, b"", [("Location", "http://127.0.0.1:%d/mcp" % outer.other.port)])
                m = msg.get("method")
                if outer.mode == "401-call" and m not in ("initialize", "notifications/initialized"):
                    return self._send(401, b'{"error":"invalid_token"}')
                if m == "initialize":
                    body = json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": {"protocolVersion": PROTOCOL, "capabilities": {"tools": {}},
                                                                                    "serverInfo": {"name": "fake", "version": "0"}}}).encode()
                    return self._send(200, body, [("Mcp-Session-Id", "sess-1")])
                if m == "notifications/initialized":
                    return self._send(202)
                if outer.mode == "rpc-error":
                    return self._send(200, json.dumps({"jsonrpc": "2.0", "id": msg["id"], "error": {"code": -32602, "message": "unknown field `seeds`"}}).encode())
                result = {"method": m, "params": msg.get("params")}
                if outer.mode == "echo":
                    result["leak"] = self.headers.get("Authorization")
                if outer.mode == "isError":
                    result["isError"] = True
                body = json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": result}).encode()
                if outer.mode == "sse":
                    return self._send(200, b"event: message\ndata: " + body + b"\n\n", ctype="text/event-stream")
                return self._send(200, body)

        self.srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.srv.daemon_threads = True
        self.port = self.srv.server_address[1]
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()

    @property
    def url(self):
        return "http://127.0.0.1:%d/mcp" % self.port

    def close(self):
        self.srv.shutdown()
        self.srv.server_close()


def selftest():
    import io
    failures = []

    def case(name, ok, detail=""):
        print("  %-4s %-16s %s" % ("ok" if ok else "FAIL", name, detail), flush=True)
        if not ok:
            failures.append(name)

    token = "selftest-token-AbC123_xyz-0123456789"
    good_env = {TOKEN_ENV: token, MINTED_ENV: "1"}

    def call(fake, cmd, env=None, extra=()):
        out, err = io.StringIO(), io.StringIO()
        rc = run(["--url", fake.url, "--allow-loopback-http", *extra, *cmd], good_env if env is None else env, out, err)
        return rc, out.getvalue(), err.getvalue()

    def sent(fake):
        return [e for e in fake.log if e["method"] not in ("initialize", "notifications/initialized")]

    def one_line(text):
        lines = text.splitlines()
        if len(lines) != 1:
            return None
        try:
            return json.loads(lines[0])
        except ValueError:
            return None

    fake = Fake(token)
    try:
        # list-tools, and the session / bearer carried on every request
        rc, o, e = call(fake, ["list-tools"])
        j = one_line(o)
        reqs = fake.log
        case("list-tools", rc == 0 and j and j["ok"] is True and j["method"] == "tools/list" and [r["method"] for r in reqs] == ["initialize", "notifications/initialized", "tools/list"]
             and all(r["auth"] == "Bearer " + token for r in reqs) and reqs[2]["session"] == "sess-1" and reqs[0]["session"] is None,
             "initialize, notifications/initialized, tools/list; the bearer on all three, the server's session id on the last two; one JSON line")
        # validate
        fake.log.clear()
        rc, o, e = call(fake, ["validate", "s3://b/datasets/p1/"])
        s = sent(fake)[0]
        case("validate", rc == 0 and s["method"] == "tools/call" and s["params"] == {"name": "validate_dataset", "arguments": {"dataset_uri": "s3://b/datasets/p1/"}}
             and one_line(o)["tool"] == "validate_dataset", "tools/call validate_dataset with exactly dataset_uri")
        # train, with and without epochs
        fake.log.clear()
        rc1, o1, _ = call(fake, ["train", "s3://b/datasets/p1/", "run-a", "12"])
        rc2, o2, _ = call(fake, ["train", "s3://b/datasets/p1/", "run-b"])
        t1, t2 = [x for x in sent(fake)]
        case("train", rc1 == 0 and rc2 == 0 and t1["params"] == {"name": "train", "arguments": {"dataset_uri": "s3://b/datasets/p1/", "name": "run-a", "epochs": 12}, "task": {}}
             and t2["params"] == {"name": "train", "arguments": {"dataset_uri": "s3://b/datasets/p1/", "name": "run-b"}, "task": {}},
             "task-augmented tools/call train: epochs 12 as a number, and no epochs key when none is given")
        rc3, o3, e3 = call(fake, ["train", "s3://b/datasets/p1/", "run-c", "twelve"])
        case("train-usage", rc3 == 2 and "REFUSED call: usage: EPOCHS is not a whole number" in e3 and o3 == "", "a non-numeric EPOCHS is refused before anything is sent")
        # status / result / cancel
        fake.log.clear()
        rs, os_, _ = call(fake, ["status", "task-1"])
        rr, or_, _ = call(fake, ["result", "task-1"])
        rcn, ocn, _ = call(fake, ["cancel", "task-1"])
        a, b, c = sent(fake)
        case("status", rs == 0 and a["params"] == {"name": "train_status", "arguments": {"task_id": "task-1"}}, "tools/call train_status {task_id}")
        case("result", rr == 0 and b["method"] == "tasks/result" and b["params"] == {"taskId": "task-1"} and one_line(or_)["method"] == "tasks/result", "tasks/result {taskId}")
        case("cancel", rcn == 0 and c["method"] == "tasks/cancel" and c["params"] == {"taskId": "task-1"} and one_line(ocn)["method"] == "tasks/cancel", "tasks/cancel {taskId}")
        # call-tool: the exact argument text, an unknown key included
        fake.log.clear()
        raw_args = '{"dataset_uri":"s3://b/datasets/p1/",  "name":"n", "seeds":[13,17]}'
        rt, ot, _ = call(fake, ["call-tool", "train", raw_args])
        st = sent(fake)[0]
        rb, ob, eb = call(fake, ["call-tool", "train", "[1,2]"])
        rb2, ob2, eb2 = call(fake, ["call-tool", "bad name", "{}"])
        case("call-tool", rt == 0 and ('"arguments":' + raw_args + "}") in st["raw"] and st["params"]["arguments"]["seeds"] == [13, 17]
             and rb == 2 and "REFUSED call: usage: JSON_ARGS is not a JSON object" in eb and rb2 == 2 and "REFUSED call: usage: TOOL is not a tool name" in eb2,
             "the argument object (an unknown `seeds` key, odd spacing) reaches the server byte for byte; a non-object and a bad tool name are refused")
    finally:
        fake.close()

    # SSE and JSON-RPC errors
    sse = Fake(token, "sse")
    try:
        rc, o, e = call(sse, ["status", "t"])
        case("call-sse", rc == 0 and one_line(o)["ok"] is True and one_line(o)["result"]["method"] == "tools/call", "a text/event-stream answer is parsed")
    finally:
        sse.close()
    err_fake = Fake(token, "rpc-error")
    try:
        rc, o, e = call(err_fake, ["call-tool", "train", '{"seeds":[1]}'])
        j = one_line(o)
        case("call-rpc-error", rc == 1 and j["ok"] is False and j["error"]["code"] == -32602 and "seeds" in j["error"]["message"], "a JSON-RPC error is one ok:false line, exit 1")
    finally:
        err_fake.close()
    ie = Fake(token, "isError")
    try:
        rc, o, e = call(ie, ["status", "t"])
        case("call-iserror", rc == 1 and one_line(o)["ok"] is False and one_line(o)["result"]["isError"] is True, "a tool result with isError is not ok (exit 1)")
    finally:
        ie.close()

    # the token never appears in output, even when the server echoes the Authorization header
    echo = Fake(token, "echo")
    try:
        rc, o, e = call(echo, ["status", "t"])
        case("call-scrub", rc == 0 and token not in o + e and "<token>" in o, "a server that echoes the bearer cannot make the client print it")
    finally:
        echo.close()

    # 401
    f401 = Fake(token, "401")
    try:
        rc, o, e = call(f401, ["list-tools"])
        j = one_line(o)
        case("call-401", rc == 1 and j["ok"] is False and j["error"] == {"http_status": 401, "oauth_error": "invalid_token"} and j.get("stage") == "initialize"
             and token not in o + e and "expired" not in o,
             "a 401 prints one error line (status and OAuth code only, no body), exits 1, and never echoes the token")
    finally:
        f401.close()

    f401c = Fake(token, "401-call")
    try:
        rc, o, e = call(f401c, ["status", "t"])
        j = one_line(o)
        case("call-401-call", rc == 1 and j["ok"] is False and j["error"] == {"http_status": 401, "oauth_error": "invalid_token"} and "stage" not in j and token not in o + e,
             "a 401 on the call itself (the session was accepted, the token then refused) is also one error line, exit 1")
    finally:
        f401c.close()

    # redirect: refused, and the target receives nothing
    target = Fake(token)
    redir = Fake(token, "redirect", other=target)
    try:
        rc, o, e = call(redir, ["list-tools"])
        case("call-redirect", rc == 2 and "REFUSED call: redirect" in e and o == "" and target.log == [] and len(redir.log) == 1,
             "a 302 is `REFUSED call: redirect` (exit 2) and the redirect target receives no request, so the bearer is never forwarded")
    finally:
        redir.close()
        target.close()

    # no token / not minted / http / url
    fake = Fake(token)
    try:
        r1 = call(fake, ["list-tools"], env={})
        r2 = call(fake, ["list-tools"], env={TOKEN_ENV: token})
        r3 = call(fake, ["list-tools"], env={TOKEN_ENV: "bad token", MINTED_ENV: "1"})
        case("call-no-token", r1[0] == 2 and "REFUSED call: no token" in r1[2] and r1[1] == "" and not fake.log, "a missing token env is `REFUSED call: no token` (exit 2), nothing sent")
        case("call-token-env", r1[0] == 2 and r2[0] == 2 and r3[0] == 2 and not fake.log and r1[1] == "" and all("REFUSED call: no token" in x[2] for x in (r1, r2, r3)),
             "no token, an unminted token and a malformed token each refuse (exit 2) before anything is sent")
        out, err = io.StringIO(), io.StringIO()
        rc_h = run(["--url", fake.url, "list-tools"], good_env, out, err)
        rc_u = run(["--url", "https://aprender-mcp-laya-train.us-east.true-mcp.com/other", "list-tools"], good_env, io.StringIO(), io.StringIO())
        rc_a = run(["--url", fake.url, "--allow-loopback-http", "--bearer", "x", "list-tools"], good_env, io.StringIO(), io.StringIO())
        case("call-url", rc_h == 2 and "REFUSED call: url: https only" in err.getvalue() and rc_u == 2 and rc_a == 2 and not fake.log,
             "http without --allow-loopback-http, a path other than /mcp and an unknown option refuse before anything is sent")
    finally:
        fake.close()

    # the recipe: no M2M file, no run
    env = {k: v for k, v in os.environ.items() if k not in ("LAYA_M2M_FILE", TOKEN_ENV)}
    p = subprocess.run(["just", "laya-trainer-call", "ze-kasher-dev", "https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp", "list-tools"],
                       cwd=REPO_ROOT, env=env, capture_output=True, text=True, stdin=subprocess.DEVNULL)
    case("call-recipe", p.returncode == 2 and "REFUSED call: LAYA_M2M_FILE is not set" in p.stderr, "`just laya-trainer-call` without LAYA_M2M_FILE is `REFUSED call: LAYA_M2M_FILE is not set` (exit 2)")

    print("%d FAIL" % len(failures))
    if failures:
        print("TRAIN CALL SELFTEST FAILED: " + ", ".join(failures))
        return 1
    print("TRAIN CALL SELFTEST OK")
    return 0


def main(argv):
    if argv == ["--selftest"]:
        return selftest()
    return run(argv, os.environ, sys.stdout, sys.stderr)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
