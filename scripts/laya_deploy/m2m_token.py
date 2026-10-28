#!/usr/bin/env python3
"""Mint an OAuth client-credentials (M2M) token from a FILE and exec ONE command with it (plan 13-06, D-07).

    python3 -I scripts/laya_deploy/m2m_token.py --file PATH --expect-host HOST
        [--allow-loopback-http] [--allow-basic] [--min-expires SECONDS (default 900)] -- CMD [ARGS...]
    python3 -I scripts/laya_deploy/m2m_token.py --token-host --state-dir DIR --endpoint-host HOST
    python3 -I scripts/laya_deploy/m2m_token.py --selftest
    python3 -I scripts/laya_deploy/m2m_token.py --stub-server DIR [--stub-mismatch]      (test only)

The client file is the partner's Secrets Manager JSON shape (motivation-experiment docs/DEPLOY.md "M2M secret
contents"): an object with exactly client_id, client_secret and token_url (non-empty strings) plus an optional
string scope. It is opened with O_NOFOLLOW and every check reads the OPENED DESCRIPTOR (fstat), never a path
re-stat: a regular file, owned by the caller, mode exactly 0600 or 0400, at most 16384 bytes.

token_url must be https on --expect-host with the path /oauth2/token (http://127.0.0.1:<port>/oauth2/token only
with --allow-loopback-http and --expect-host 127.0.0.1, for the offline selftests). No redirect is ever followed:
the opener's redirect handler refuses, so any 3xx is `REFUSED m2m-redirect` (urllib's default handler would
re-send the request headers, Authorization included, to the Location). The credentials go as a FORM BODY, which
is what the partner's production client sends (motivation-experiment crates/core/src/auth.rs). On invalid_client
ONE HTTP Basic attempt is made, as a DIAGNOSTIC: its header is set with add_unredirected_header, and a Basic-only
success is `REFUSED m2m-auth-method` unless --allow-basic, which reports `(auth basic, NOT partner-ready)`.

On success the helper prints ONE line to stderr, `m2m-token ok: host <h> HTTP 200 expires_in <n> s (auth form)`,
and os.execvpe's CMD with the token in APRENDER_DECIDE_PROBE_TOKEN and LAYA_M2M_MINTED=1, the argv exactly
CMD [ARGS...]. The token and the secret are never on argv (a command line carrying either is refused), never
printed and never written anywhere. Refusals are `REFUSED m2m-<rule>: ...` with exit 2 and the command never
runs; the 14 rules are RULES below. An HTTP error names its status and the OAuth error code only when the code
matches ^[a-z_]{1,40}$; no exception text that could carry a response body is printed.

ACCEPTED RISK: while the command runs, the token is readable by same-user processes through the command's
environment (ps -E, /proc/<pid>/environ). This is a single-user workstation; the token is short-lived.

--token-host (one helper shared by laya-probe-auth, laya-deploy-verify and laya-deploy): prints the token host.
LAYA_M2M_TOKEN_HOST when set, else the content of <state dir>/m2m-token-host (a non-secret note plan 13-08
writes when the owner names a Cognito host), else --endpoint-host. Both present and different:
`REFUSED probe-auth: token-host conflict`. A host other than the endpoint's own must match
^[a-z0-9-]+(\\.[a-z0-9-]+)*\\.(pmcp\\.run|amazoncognito\\.com)$, which admits any pmcp.run host, the
shared-pool counter-coach host included: intended, the augury servers share one Cognito pool (RESEARCH A1).

stdlib only.
"""

import base64
import errno
import hashlib
import http.client
import http.server
import json
import os
import re
import secrets
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

MAX_FILE_BYTES = 16384
REQUIRED_KEYS = ("client_id", "client_secret", "token_url")
OPTIONAL_KEYS = ("scope",)
TOKEN_ENV = "APRENDER_DECIDE_PROBE_TOKEN"
MINTED_ENV = "LAYA_M2M_MINTED"
TOKEN_PATH = "/oauth2/token"
TIMEOUT_S = 30
ALLOWED_MODES = (0o600, 0o400)
RULES = ("file-missing", "file-type", "file-owner", "file-mode", "file-json", "file-keys", "token-url", "host",
         "http", "redirect", "auth-method", "response", "expires", "usage")
HOST_RE = re.compile(r"[a-z0-9-]+(\.[a-z0-9-]+)*")
TOKEN_HOST_RE = re.compile(r"[a-z0-9-]+(\.[a-z0-9-]+)*\.(pmcp\.run|amazoncognito\.com)")
OAUTH_CODE_RE = re.compile(r"[a-z_]{1,40}")
HEADER_SAFE_RE = re.compile(r"[\x21-\x7e]+")


class Refused(Exception):
    def __init__(self, rule, msg):
        super().__init__(rule)
        self.rule = rule
        self.msg = msg


def refuse(rule, msg):
    assert rule in RULES, rule
    raise Refused(rule, msg)


# ---- the client file ---------------------------------------------------------------------------------------

def read_client_file(path):
    flags = os.O_RDONLY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NONBLOCK", 0)
    try:
        fd = os.open(path, flags)
    except FileNotFoundError:
        refuse("file-missing", "the client file does not exist")
    except OSError as e:
        if e.errno == errno.ELOOP:
            refuse("file-type", "the client file is a symlink (opened with O_NOFOLLOW); name the file itself")
        refuse("file-missing", f"the client file cannot be opened ({errno.errorcode.get(e.errno, 'error')})")
    try:
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode):
            refuse("file-type", "the client file is not a regular file")
        if st.st_uid != os.getuid():
            refuse("file-owner", "the client file is owned by another user")
        mode = stat.S_IMODE(st.st_mode)
        if mode not in ALLOWED_MODES:
            refuse("file-mode", f"the client file is mode {mode:04o}; it must be exactly 0600 or 0400")
        if st.st_size > MAX_FILE_BYTES:
            refuse("file-type", f"the client file is {st.st_size} bytes, more than {MAX_FILE_BYTES}")
        chunks, total = [], 0
        while True:
            b = os.read(fd, MAX_FILE_BYTES + 1 - total)
            if not b:
                break
            chunks.append(b)
            total += len(b)
            if total > MAX_FILE_BYTES:
                refuse("file-type", f"the client file is more than {MAX_FILE_BYTES} bytes")
    finally:
        os.close(fd)
    raw = b"".join(chunks)
    try:
        obj = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        refuse("file-json", f"the client file ({len(raw)} bytes) is not UTF-8 JSON")
    if not isinstance(obj, dict):
        refuse("file-json", f"the client file holds a JSON {type(obj).__name__}, not an object")
    keys = set(obj)
    missing = [k for k in REQUIRED_KEYS if k not in keys]
    unknown = sorted(k for k in keys if k not in REQUIRED_KEYS + OPTIONAL_KEYS)
    if missing:
        refuse("file-keys", f"the client file lacks {', '.join(missing)} (it holds client_id, client_secret, token_url and an optional scope)")
    if unknown:
        named = [k for k in unknown if re.fullmatch(r"[A-Za-z0-9_]{1,40}", k)]
        refuse("file-keys", f"the client file has {len(unknown)} unknown key(s)" + (f": {', '.join(named)}" if named else ""))
    for k in REQUIRED_KEYS:
        if not isinstance(obj[k], str) or not obj[k]:
            refuse("file-keys", f"{k} is not a non-empty string")
    if "scope" in obj and not isinstance(obj["scope"], str):
        refuse("file-keys", "scope is not a string")
    return obj


def check_token_url(url, expect_host, allow_loopback_http):
    try:
        u = urllib.parse.urlsplit(url)
        port = u.port
    except ValueError:
        refuse("token-url", "token_url does not parse as a URL")
    host = (u.hostname or "").lower()
    if u.username is not None or u.password is not None or u.query or u.fragment:
        refuse("token-url", "token_url carries userinfo, a query or a fragment")
    if u.scheme == "https":
        pass
    elif u.scheme == "http" and allow_loopback_http and expect_host == "127.0.0.1" and host == "127.0.0.1" and port:
        pass
    elif u.scheme == "http":
        refuse("token-url", "token_url is http; only https is accepted (loopback http only with --allow-loopback-http and --expect-host 127.0.0.1)")
    else:
        refuse("token-url", "token_url is not an https URL")
    if host != expect_host:
        shown = host[:100] if HOST_RE.fullmatch(host or "-") or host == "127.0.0.1" else f"<{len(host)} characters>"
        refuse("host", f"the token_url host {shown} is not the expected host {expect_host}")
    if u.path != TOKEN_PATH:
        refuse("token-url", f"the token_url path is not {TOKEN_PATH}")
    return host


# ---- transport ----------------------------------------------------------------------------------------------

class _RedirectRefused(urllib.request.HTTPRedirectHandler):
    """Never follow: a 3xx surfaces as an HTTPError carrying its code, and nothing is re-sent anywhere."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise urllib.error.HTTPError(req.full_url, code, "redirect refused", headers, fp)


def build_opener():
    # ProxyHandler({}): an ambient http(s)_proxy never sees the credentials.
    return urllib.request.build_opener(urllib.request.ProxyHandler({}), _RedirectRefused())


def post_form(opener, url, form, basic=None):
    req = urllib.request.Request(url, data=urllib.parse.urlencode(form).encode("ascii"), method="POST")
    req.add_header("Content-Type", "application/x-www-form-urlencoded")
    req.add_header("Accept", "application/json")
    if basic is not None:
        # RFC 6749 2.3.1: each part form-encoded, then base64. add_unredirected_header: never on a re-sent request.
        pair = urllib.parse.quote_plus(basic[0]) + ":" + urllib.parse.quote_plus(basic[1])
        req.add_unredirected_header("Authorization", "Basic " + base64.b64encode(pair.encode("utf-8")).decode("ascii"))
    try:
        with opener.open(req, timeout=TIMEOUT_S) as r:
            return r.status, r.read(65537)
    except urllib.error.HTTPError as e:
        try:
            body = e.read(65537) if e.fp is not None else b""
        except (OSError, http.client.HTTPException):
            body = b""
        return e.code, body
    except (urllib.error.URLError, OSError, http.client.HTTPException, ValueError) as e:
        refuse("http", f"transport failure reaching the token endpoint ({type(e).__name__})")


def oauth_code(body):
    try:
        obj = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        return None
    code = obj.get("error") if isinstance(obj, dict) else None
    return code if isinstance(code, str) and OAUTH_CODE_RE.fullmatch(code) else None


def describe(status, body):
    code = oauth_code(body)
    return f"HTTP {status}" + (f" ({code})" if code else "")


def refuse_redirect(status):
    if 300 <= status < 400:
        refuse("redirect", f"the token endpoint answered HTTP {status}; redirects are never followed")


def mint(client, expect_host, allow_loopback_http, allow_basic, min_expires):
    host = check_token_url(client["token_url"], expect_host, allow_loopback_http)
    opener = build_opener()
    grant = {"grant_type": "client_credentials"}
    if "scope" in client:
        grant["scope"] = client["scope"]
    status, body = post_form(opener, client["token_url"],
                             {**grant, "client_id": client["client_id"], "client_secret": client["client_secret"]})
    refuse_redirect(status)
    method = "form"
    if status == 401 or (status == 400 and oauth_code(body) == "invalid_client"):
        form_status, form_body = status, body
        status, body = post_form(opener, client["token_url"], grant, basic=(client["client_id"], client["client_secret"]))
        refuse_redirect(status)
        if status != 200:
            refuse("http", f"{describe(form_status, form_body)} with form and {describe(status, body)} with basic")
        if not allow_basic:
            refuse("auth-method", "only client_secret_basic works at this token endpoint; the partner's client sends "
                   "form-body credentials (motivation-experiment crates/core/src/auth.rs), so this client is not partner-ready")
        method = "basic"
    if status != 200:
        refuse("http", describe(status, body))
    try:
        resp = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        refuse("response", f"the token endpoint answered HTTP 200 with a body that is not JSON ({len(body)} bytes)")
    if not isinstance(resp, dict):
        refuse("response", "the token response is not a JSON object")
    token = resp.get("access_token")
    if not isinstance(token, str) or not token:
        refuse("response", "the token response has no non-empty access_token")
    if not HEADER_SAFE_RE.fullmatch(token):
        refuse("response", "the access_token is not a header-safe string")
    ttype = resp.get("token_type")
    if not isinstance(ttype, str) or ttype.lower() != "bearer":
        refuse("response", "the token response's token_type is not Bearer")
    exp = resp.get("expires_in")
    if isinstance(exp, bool) or not isinstance(exp, int):
        refuse("response", "the token response has no integer expires_in")
    if exp < min_expires:
        refuse("expires", f"expires_in {exp} s is below the floor {min_expires} s (lower it with LAYA_M2M_MIN_EXPIRES)")
    return token, host, exp, method


# ---- the command line ---------------------------------------------------------------------------------------

def parse(argv):
    opts = {"file": None, "expect_host": None, "allow_loopback_http": False, "allow_basic": False, "min_expires": 900}
    if "--" not in argv:
        refuse("usage", "no command after -- (usage: --file PATH --expect-host HOST [...] -- CMD [ARGS...])")
    i = argv.index("--")
    head, cmd = argv[:i], argv[i + 1:]
    it = iter(head)
    for a in it:
        if a in ("--file", "--expect-host", "--min-expires"):
            v = next(it, None)
            if v is None:
                refuse("usage", f"{a} needs a value")
            opts[a[2:].replace("-", "_")] = v
        elif a == "--allow-loopback-http":
            opts["allow_loopback_http"] = True
        elif a == "--allow-basic":
            opts["allow_basic"] = True
        else:
            refuse("usage", f"unknown option ({len(a)} characters)")
    if not opts["file"]:
        refuse("usage", "--file is required")
    eh = opts["expect_host"]
    if not eh or not (HOST_RE.fullmatch(eh) or eh == "127.0.0.1") or len(eh) > 253:
        refuse("usage", "--expect-host is required and must be a lower-case host name")
    me = opts["min_expires"]
    if isinstance(me, str):
        if not re.fullmatch(r"[0-9]{1,6}", me) or int(me) > 86400:
            refuse("usage", "--min-expires must be a whole number of seconds, at most 86400")
        opts["min_expires"] = int(me)
    if not cmd:
        refuse("usage", "no command after --")
    return opts, cmd


def run_mint_and_exec(argv):
    opts, cmd = parse(argv)
    client = read_client_file(opts["file"])
    if any(client["client_secret"] in a for a in cmd):
        refuse("usage", "the command line carries the client secret")
    token, host, exp, method = mint(client, opts["expect_host"], opts["allow_loopback_http"], opts["allow_basic"],
                                    opts["min_expires"])
    if any(token in a for a in cmd):
        refuse("usage", "the command line carries the minted token")
    auth = "auth form" if method == "form" else "auth basic, NOT partner-ready"
    print(f"m2m-token ok: host {host} HTTP 200 expires_in {exp} s ({auth})", file=sys.stderr)
    env = dict(os.environ)
    env[TOKEN_ENV] = token
    env[MINTED_ENV] = "1"
    sys.stdout.flush()
    sys.stderr.flush()
    try:
        os.execvpe(cmd[0], cmd, env)
    except OSError as e:
        refuse("usage", f"cannot exec the command ({errno.errorcode.get(e.errno, 'error')})")


def run_token_host(argv):
    def no(msg):
        print(f"REFUSED probe-auth: {msg}", file=sys.stderr)
        return 2
    it = iter(argv)
    sd = eh = None
    for a in it:
        if a == "--state-dir":
            sd = next(it, None)
        elif a == "--endpoint-host":
            eh = next(it, None)
        else:
            return no("token-host usage: --token-host --state-dir DIR --endpoint-host HOST")
    if sd is None or eh is None:
        return no("token-host usage: --token-host --state-dir DIR --endpoint-host HOST")
    eh = eh.lower()
    var = os.environ.get("LAYA_M2M_TOKEN_HOST", "")
    note_path = os.path.join(sd, "m2m-token-host")
    note = ""
    if os.path.lexists(note_path):
        try:
            with open(note_path, "rb") as fh:
                raw = fh.read(301)
        except OSError:
            return no(f"the note {note_path} cannot be read")
        lines = raw.decode("utf-8", "replace").split()
        if len(raw) > 300 or len(lines) != 1:
            return no(f"the note {note_path} is not one host name")
        note = lines[0]
    if var and note and var != note:
        return no(f"token-host conflict: LAYA_M2M_TOKEN_HOST and {note_path} name different hosts; unset one")
    host = var or note or eh
    if not host:
        return no("no token host: LAYA_M2M_TOKEN_HOST is unset, there is no m2m-token-host note and no endpoint")
    if host != eh and not TOKEN_HOST_RE.fullmatch(host):
        shown = host[:100] if HOST_RE.fullmatch(host) else f"<{len(host)} characters>"
        return no(f"the token host {shown} is neither the endpoint host nor a *.pmcp.run / *.amazoncognito.com host")
    print(host)
    return 0


# ---- the loopback stub (test only) --------------------------------------------------------------------------

def _write_private(path, text):
    fd = os.open(path + ".tmp", os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as fh:
        fh.write(text)
    os.replace(path + ".tmp", path)


def run_stub(argv):
    if not argv:
        print("usage: --stub-server DIR [--stub-mismatch]", file=sys.stderr)
        return 2
    d, mismatch = argv[0], "--stub-mismatch" in argv[1:]
    with open(os.path.join(d, "client.json")) as fh:
        c = json.load(fh)
    cid, csec = c["client_id"], c["client_secret"]
    token = secrets.token_urlsafe(32)
    expect = secrets.token_urlsafe(32) if mismatch else token
    basic = "Basic " + base64.b64encode((urllib.parse.quote_plus(cid) + ":" + urllib.parse.quote_plus(csec)).encode()).decode()
    lock = threading.Lock()

    class H(http.server.BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def _send(self, code, obj):
            b = json.dumps(obj).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(b)))
            self.end_headers()
            self.wfile.write(b)

        def do_POST(self):
            n = int(self.headers.get("Content-Length") or 0)
            body = self.rfile.read(n) if n else b""
            if self.path == TOKEN_PATH:
                f = urllib.parse.parse_qs(body.decode("utf-8", "replace"))
                form_ok = f.get("client_id") == [cid] and f.get("client_secret") == [csec]
                ok = f.get("grant_type") == ["client_credentials"] and (form_ok or self.headers.get("Authorization") == basic)
                if ok:
                    self._send(200, {"access_token": token, "token_type": "Bearer", "expires_in": 3600})
                else:
                    self._send(400, {"error": "invalid_client"})
            elif self.path.startswith("/mcp"):
                m = self.headers.get("Authorization") == "Bearer " + expect
                with lock, open(os.path.join(d, "mcp.log"), "a") as fh:
                    fh.write(f"authorization_bearer_matches={'true' if m else 'false'}\n")
                self._send(401, {"error": "invalid_token"})
            else:
                self._send(404, {"error": "not_found"})

    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
    srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    _write_private(os.path.join(d, "token"), token)
    _write_private(os.path.join(d, "port"), str(srv.server_address[1]))
    deadline = time.monotonic() + 300
    while time.monotonic() < deadline and not os.path.exists(os.path.join(d, "stop")):
        time.sleep(0.1)
    srv.shutdown()
    return 0


# ---- the selftest -------------------------------------------------------------------------------------------

CHILD = ("import hashlib, os, sys\n"
         "t = os.environ.get('APRENDER_DECIDE_PROBE_TOKEN', '')\n"
         "print('child sha256=%s minted=%s argv_has_token=%s' % (hashlib.sha256(t.encode()).hexdigest(), "
         "os.environ.get('LAYA_M2M_MINTED', ''), bool(t) and any(t in a for a in sys.argv)))\n")


def selftest():
    failures = []

    def case(name, ok, detail=""):
        print("  %-4s %-84s %s" % ("ok" if ok else "FAIL", name, detail))
        if not ok:
            failures.append(name)

    cid = "selftest-client-" + secrets.token_hex(4)
    csec = "selftest-secret-" + secrets.token_urlsafe(18)
    token = "selftest-token-" + secrets.token_urlsafe(24)
    basic = "Basic " + base64.b64encode((urllib.parse.quote_plus(cid) + ":" + urllib.parse.quote_plus(csec)).encode()).decode()
    S = {"mode": "form", "expires": 3600, "location": "", "token_hits": 0, "other_hits": 0}

    class Tok(http.server.BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def _send(self, code, obj=None, raw=None, headers=()):
            b = raw if raw is not None else json.dumps(obj).encode()
            self.send_response(code)
            for k, v in headers:
                self.send_header(k, v)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(b)))
            self.end_headers()
            self.wfile.write(b)

        def do_POST(self):
            S["token_hits"] += 1
            n = int(self.headers.get("Content-Length") or 0)
            f = urllib.parse.parse_qs(self.rfile.read(n).decode("utf-8", "replace"))
            form_ok = f.get("client_id") == [cid] and f.get("client_secret") == [csec]
            basic_ok = self.headers.get("Authorization") == basic
            grant_ok = f.get("grant_type") == ["client_credentials"]
            mode = S["mode"]
            good = {"access_token": token, "token_type": "Bearer", "expires_in": S["expires"]}
            if mode.startswith("redirect:"):
                self._send(int(mode.split(":")[1]), {"moved": True}, headers=[("Location", S["location"])])
            elif mode == "status-invalid-scope":
                self._send(400, {"error": "invalid_scope", "error_description": "DESCRIPTION-" + csec})
            elif mode == "nonjson":
                self._send(200, raw=b"<html>" + token.encode() + b"</html>")
            elif mode == "token-type-mac":
                self._send(200, {**good, "token_type": "mac"})
            elif mode == "basic-fails-too":
                self._send(401, {"error": "invalid_client"})
            elif mode == "basic-only":
                self._send(200, good) if (basic_ok and grant_ok) else self._send(401, {"error": "invalid_client"})
            else:
                self._send(200, good) if (grant_ok and (form_ok or basic_ok)) else self._send(400, {"error": "invalid_client"})

    class Other(http.server.BaseHTTPRequestHandler):
        """The second listener: counts every request; answers as a token endpoint would, so a followed redirect 'works'."""

        def log_message(self, *a):
            pass

        def _any(self):
            S["other_hits"] += 1
            b = json.dumps({"access_token": token, "token_type": "Bearer", "expires_in": 3600}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(b)))
            self.end_headers()
            self.wfile.write(b)

        do_GET = do_POST = do_PUT = _any

    servers = []
    for handler in (Tok, Other):
        s = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        s.daemon_threads = True
        threading.Thread(target=s.serve_forever, daemon=True).start()
        servers.append(s)
    port, other = servers[0].server_address[1], servers[1].server_address[1]
    url = f"http://127.0.0.1:{port}/oauth2/token"
    tmp = tempfile.mkdtemp(prefix="m2m-selftest-")
    want_sha = hashlib.sha256(token.encode()).hexdigest()

    def client_file(name, obj=None, mode=0o600, raw=None):
        p = os.path.join(tmp, name)
        with open(p, "w") as fh:
            fh.write(raw if raw is not None else json.dumps(obj if obj is not None else
                                                             {"client_id": cid, "client_secret": csec, "token_url": url}))
        os.chmod(p, mode)
        return p

    def helper(path, *flags, expect_host="127.0.0.1", loopback=True, cmd=None):
        args = [sys.executable, "-I", os.path.abspath(__file__)]
        if path is not None:
            args += ["--file", path]
        args += ["--expect-host", expect_host] + (["--allow-loopback-http"] if loopback else []) + list(flags)
        args += ["--"] + (cmd if cmd is not None else [sys.executable, "-I", "-c", CHILD])
        env = {k: v for k, v in os.environ.items() if k not in (TOKEN_ENV, MINTED_ENV)}
        p = subprocess.run(args, capture_output=True, text=True, timeout=90, env=env)
        return p.returncode, p.stdout, p.stderr

    def clean(out, err):
        return token not in out + err and csec not in out + err

    def accepted(name, rc, out, err, auth="auth form"):
        okline = f"m2m-token ok: host 127.0.0.1 HTTP 200 expires_in {S['expires']} s ({auth})"
        ok = rc == 0 and f"child sha256={want_sha} minted=1 " in out and okline in err
        case(name, ok, f"rc={rc}" + ("" if ok else f" err={err.strip()[-160:]!r}"))
        return out, err

    def refused(name, rule, rc, out, err, extra=True):
        ok = rc == 2 and f"REFUSED m2m-{rule}:" in err and "child sha256=" not in out and clean(out, err) and extra
        case(name, ok, (err.strip().splitlines() or ["<no stderr>"])[-1][:150])

    try:
        # control: the absence check sees a planted token
        case("control: the token-absent check sees a planted token", not clean("x" + token, "") and not clean("", csec))
        good = client_file("good.json")
        rc, out, err = helper(good)
        accepted("form mint: the child sees the token (by sha256) and LAYA_M2M_MINTED=1; ok line (auth form)", rc, out, err)
        case("argv: the exec'd child's sys.argv does not carry the token", rc == 0 and "argv_has_token=False" in out)
        case("token-absent: the helper's stdout and stderr carry neither the token nor the secret", clean(out, err))
        case("prints: stderr is exactly the one ok line (host, HTTP status, expires_in, auth method)",
             err.strip() == f"m2m-token ok: host 127.0.0.1 HTTP 200 expires_in 3600 s (auth form)", repr(err.strip())[:120])
        sc = client_file("scope.json", {"client_id": cid, "client_secret": csec, "token_url": url, "scope": "https://h/mcp/read"})
        rc, out, err = helper(sc)
        accepted("form mint with the optional scope", rc, out, err)
        # auth-method: basic-only
        S["mode"] = "basic-only"
        rc, out, err = helper(good)
        refused("auth-method: a Basic-only endpoint is refused without --allow-basic; the child never runs", "auth-method",
                rc, out, err, "client_secret_basic" in err and "auth.rs" in err)
        rc, out, err = helper(good, "--allow-basic")
        accepted("auth-method: Basic-only accepted with --allow-basic as (auth basic, NOT partner-ready)", rc, out, err,
                 auth="auth basic, NOT partner-ready")
        case("auth-method: the --allow-basic run leaks neither the token nor the secret", clean(out, err))
        S["mode"] = "basic-fails-too"
        rc, out, err = helper(good)
        refused("http: invalid_client with form and with basic -> m2m-http naming both", "http", rc, out, err,
                "with form and HTTP 401 (invalid_client) with basic" in err)
        # redirects: every 3xx refused, the second listener never reached
        for code in (301, 302, 303, 307, 308):
            S["mode"], S["location"], S["other_hits"] = f"redirect:{code}", f"http://127.0.0.1:{other}/oauth2/token", 0
            rc, out, err = helper(good)
            refused(f"redirect {code} to a second loopback listener is refused; that listener saw 0 requests", "redirect",
                    rc, out, err, S["other_hits"] == 0 and f"HTTP {code}; redirects are never followed" in err)
        S["mode"], S["location"], S["other_hits"] = "redirect:302", f"http://127.0.0.1:{other}/collect", 0
        rc, out, err = helper(good)
        refused("redirect to an http:// Location (the downgrade form) is refused; the second listener saw 0 requests",
                "redirect", rc, out, err, S["other_hits"] == 0)
        S["mode"] = "form"
        # file mode table (fstat of the opened descriptor)
        for mode, ok_mode in ((0o600, True), (0o400, True), (0o700, False), (0o640, False), (0o644, False)):
            p = client_file(f"mode-{mode:o}.json", mode=mode)
            rc, out, err = helper(p)
            if ok_mode:
                accepted(f"file mode {mode:04o} accepted", rc, out, err)
            else:
                refused(f"file mode {mode:04o} refused m2m-file-mode", "file-mode", rc, out, err, f"mode {mode:04o}" in err)
        link = os.path.join(tmp, "link.json")
        os.symlink(good, link)
        rc, out, err = helper(link)
        refused("file type: a symlink to a good file is refused through O_NOFOLLOW", "file-type", rc, out, err, "symlink" in err)
        d = os.path.join(tmp, "a-dir")
        os.mkdir(d, 0o700)
        rc, out, err = helper(d)
        refused("file type: a directory is refused (fstat: not a regular file)", "file-type", rc, out, err)
        big = client_file("big.json", raw=" " * (MAX_FILE_BYTES + 1))
        rc, out, err = helper(big)
        refused(f"file type: a file over {MAX_FILE_BYTES} bytes is refused", "file-type", rc, out, err)
        foreign = next((p for p in ("/etc/hosts", "/etc/passwd", "/etc/group")
                        if os.path.isfile(p) and not os.path.islink(p) and os.stat(p).st_uid != os.getuid()), None)
        if foreign:
            rc, out, err = helper(foreign)
            refused(f"file owner: a file owned by another user ({foreign}) is refused", "file-owner", rc, out, err)
        else:
            case("file owner: no readable file owned by another user found to test with", False)
        # the remaining refusals
        rc, out, err = helper(os.path.join(tmp, "absent.json"))
        refused("file missing: refused", "file-missing", rc, out, err)
        rc, out, err = helper(client_file("list.json", raw="[]"))
        refused("file json: a non-object JSON is refused", "file-json", rc, out, err)
        rc, out, err = helper(client_file("broken.json", raw='{"client_id": '))
        refused("file json: invalid JSON is refused", "file-json", rc, out, err)
        rc, out, err = helper(client_file("missing.json", {"client_id": cid, "token_url": url}))
        refused("file keys: a missing client_secret is refused", "file-keys", rc, out, err, "client_secret" in err)
        rc, out, err = helper(client_file("unknown.json", {"client_id": cid, "client_secret": csec, "token_url": url, "extra": "x"}))
        refused("file keys: an unknown key is refused", "file-keys", rc, out, err, "extra" in err)
        rc, out, err = helper(client_file("empty.json", {"client_id": "", "client_secret": csec, "token_url": url}))
        refused("file keys: an empty client_id is refused", "file-keys", rc, out, err)
        rc, out, err = helper(good, loopback=False)
        refused("token url: http without --allow-loopback-http is refused", "token-url", rc, out, err)
        rc, out, err = helper(client_file("path.json", {"client_id": cid, "client_secret": csec,
                                                        "token_url": f"http://127.0.0.1:{port}/token"}))
        refused("token url: a path other than /oauth2/token is refused", "token-url", rc, out, err)
        rc, out, err = helper(client_file("other.json", {"client_id": cid, "client_secret": csec,
                                                         "token_url": "https://evil.example.com/oauth2/token"}),
                              expect_host="aprender-mcp-decide-motivation.eu-west.augury.pmcp.run", loopback=False)
        refused("host: a token_url host other than --expect-host is refused", "host", rc, out, err)
        S["mode"] = "status-invalid-scope"
        rc, out, err = helper(good)
        refused("http: HTTP 400 invalid_scope reported as the status and the code only", "http", rc, out, err,
                "HTTP 400 (invalid_scope)" in err and "DESCRIPTION-" not in err)
        S["mode"] = "nonjson"
        rc, out, err = helper(good)
        refused("response: a non-JSON token response is refused", "response", rc, out, err)
        S["mode"] = "token-type-mac"
        rc, out, err = helper(good)
        refused("response: token_type other than Bearer is refused", "response", rc, out, err)
        S["mode"], S["expires"] = "form", 600
        rc, out, err = helper(good)
        refused("expires: expires_in 600 below the default floor 900 is refused, naming the floor and LAYA_M2M_MIN_EXPIRES",
                "expires", rc, out, err, "below the floor 900 s" in err and "LAYA_M2M_MIN_EXPIRES" in err)
        rc, out, err = helper(good, "--min-expires", "300")
        accepted("expires: expires_in 600 accepted with --min-expires 300", rc, out, err)
        S["expires"] = 3600
        rc, out, err = helper(good, cmd=[])
        refused("usage: no command after -- is refused", "usage", rc, out, err)
        rc, out, err = helper(good, cmd=[sys.executable, "-I", "-c", CHILD, csec])
        refused("argv: a command line carrying the client secret is refused", "usage", rc, out, err)
        rc, out, err = helper(good, "--min-expires", "ninety")
        refused("usage: a non-integer --min-expires is refused", "usage", rc, out, err)
        # token-host precedence (the shared helper of the recipes)
        sd = os.path.join(tmp, "state")
        os.mkdir(sd)

        def th(eh, var=None):
            env = {k: v for k, v in os.environ.items() if k != "LAYA_M2M_TOKEN_HOST"}
            if var is not None:
                env["LAYA_M2M_TOKEN_HOST"] = var
            p = subprocess.run([sys.executable, "-I", os.path.abspath(__file__), "--token-host", "--state-dir", sd,
                                "--endpoint-host", eh], capture_output=True, text=True, timeout=30, env=env)
            return p.returncode, p.stdout.strip(), p.stderr.strip()

        ep = "aprender-mcp-decide-motivation.eu-west.augury.pmcp.run"
        cog = "augury-fixture.auth.eu-west-1.amazoncognito.com"
        case("token host: neither variable nor note -> the endpoint host", th(ep)[:2] == (0, ep))
        case("token host: LAYA_M2M_TOKEN_HOST wins over the endpoint host", th(ep, cog)[:2] == (0, cog))
        with open(os.path.join(sd, "m2m-token-host"), "w") as fh:
            fh.write(cog + "\n")
        case("token host: the m2m-token-host note wins over the endpoint host", th(ep)[:2] == (0, cog))
        r = th(ep, "other-fixture.auth.eu-west-1.amazoncognito.com")
        case("token host: the variable and the note differ -> refused token-host conflict", r[0] == 2 and "token-host conflict" in r[2])
        with open(os.path.join(sd, "m2m-token-host"), "w") as fh:
            fh.write("evil.example.com\n")
        r = th(ep)
        case("token host: a note outside pmcp.run / amazoncognito.com is refused", r[0] == 2 and "REFUSED probe-auth:" in r[2])
    finally:
        for s in servers:
            s.shutdown()
        shutil.rmtree(tmp, ignore_errors=True)
    print("%d case(s) failed" % len(failures) if failures else "all cases ok")
    if not failures:
        print("M2M TOKEN SELFTEST OK")
    return 1 if failures else 0


def main(argv):
    if argv[:1] == ["--selftest"]:
        return selftest()
    if argv[:1] == ["--stub-server"]:
        return run_stub(argv[1:])
    if argv[:1] == ["--token-host"]:
        return run_token_host(argv[1:])
    try:
        run_mint_and_exec(argv)
    except Refused as r:
        print(f"REFUSED m2m-{r.rule}: {r.msg}", file=sys.stderr)
        return 2
    except Exception as e:  # never a traceback: its text could quote a response body
        print(f"REFUSED m2m-usage: internal error ({type(e).__name__})", file=sys.stderr)
        return 2
    return 2  # execvpe does not return


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
