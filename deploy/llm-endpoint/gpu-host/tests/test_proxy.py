"""Offline tests for the reference proxy against a local stub host (no AWS)."""
import base64
import http.server
import json
import os
import sys
import threading
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "proxy"))
import lambda_function as lf  # noqa: E402

SEEN = []


class Stub(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):  # keep test output clean
        pass

    def do_POST(self):
        n = int(self.headers.get("content-length", 0))
        SEEN.append(("POST", self.path, self.rfile.read(n)))
        code = 422 if self.path.endswith("/bad") else 200
        out = json.dumps({"ok": True}).encode()
        self.send_response(code); self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(out))); self.end_headers(); self.wfile.write(out)

    def do_GET(self):
        SEEN.append(("GET", self.path, b""))
        out = b'{"status":"ok"}'
        self.send_response(200); self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(out))); self.end_headers(); self.wfile.write(out)


def ev(method, path, body="", b64=False):
    return {"rawPath": path, "body": body, "isBase64Encoded": b64,
            "requestContext": {"http": {"method": method, "path": path}}}


class ProxyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Stub)
        threading.Thread(target=cls.srv.serve_forever, daemon=True).start()
        os.environ["HOST_URL"] = f"http://127.0.0.1:{cls.srv.server_address[1]}"

    @classmethod
    def tearDownClass(cls):
        cls.srv.shutdown()

    def test_chat_is_forwarded_with_body_intact(self):
        SEEN.clear()
        r = lf.handler(ev("POST", "/v1/chat/completions", '{"a":1}'))
        self.assertEqual((r["statusCode"], json.loads(r["body"])), (200, {"ok": True}))
        self.assertEqual(SEEN, [("POST", "/v1/chat/completions", b'{"a":1}')])

    def test_base64_body_is_decoded(self):
        SEEN.clear()
        lf.handler(ev("POST", "/v1/chat/completions", base64.b64encode(b'{"b":2}').decode(), True))
        self.assertEqual(SEEN[0][2], b'{"b":2}')

    def test_unlisted_path_is_refused_without_reaching_the_host(self):
        SEEN.clear()
        for m, p in [("POST", "/v1/completions"), ("GET", "/v1/gpu/status"), ("POST", "/v1/gpu/warmup"),
                     ("GET", "/v1/chat/completions")]:
            self.assertEqual(lf.handler(ev(m, p))["statusCode"], 404, (m, p))
        self.assertEqual(SEEN, [])

    def test_host_error_status_passes_through(self):
        lf.ALLOWED.add(("POST", "/bad"))
        try:
            self.assertEqual(lf.handler(ev("POST", "/bad", "{}"))["statusCode"], 422)
        finally:
            lf.ALLOWED.discard(("POST", "/bad"))

    def test_unreachable_host_is_502_not_a_crash(self):
        old = os.environ["HOST_URL"]; os.environ["HOST_URL"] = "http://127.0.0.1:1"
        try:
            self.assertEqual(lf.handler(ev("GET", "/health"))["statusCode"], 502)
        finally:
            os.environ["HOST_URL"] = old

    def test_the_repo_sample_event_round_trips(self):
        p = os.path.join(os.path.dirname(__file__), "..", "..", "events", "apigw-v2-no-auth.json")
        with open(p) as f:
            ev_ = json.load(f)
        self.assertEqual(lf.handler(ev_)["statusCode"], 200)


if __name__ == "__main__":
    unittest.main()
