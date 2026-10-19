"""Reference proxy: platform invocation (API Gateway v2 event) -> private GPU host -> v2 response.

This Lambda is the ONLY gate in front of a host whose CUDA-path routers ignore the bearer key
(paiml/aprender#5478): there is no Function URL, the host admits only this function's security
group, and who may invoke it is decided by IAM (the platform's allowlist). It is a reference for the
short GPU test, meant to be adopted or replaced by the platform; it adds no authentication of its own.

It forwards three paths and refuses everything else, never logs a request or response body, and
does not stream (the platform's `lambda://` call is request/response).
"""
import base64
import json
import os
import urllib.error
import urllib.request

ALLOWED = {("POST", "/v1/chat/completions"), ("GET", "/v1/models"), ("GET", "/health")}
TIMEOUT_S = float(os.environ.get("PROXY_TIMEOUT_S", "280"))


def _response(status, body, content_type="application/json"):
    return {"statusCode": status, "headers": {"content-type": content_type},
            "body": body, "isBase64Encoded": False}


def handler(event, _context=None):
    http = event.get("requestContext", {}).get("http", {})
    method, path = http.get("method", ""), event.get("rawPath", "")
    if (method, path) not in ALLOWED:
        return _response(404, json.dumps({"error": {"message": "not found", "type": "proxy"}}))
    base = os.environ["HOST_URL"].rstrip("/")
    body = event.get("body") or ""
    data = None
    if method == "POST":
        data = base64.b64decode(body) if event.get("isBase64Encoded") else body.encode()
    req = urllib.request.Request(base + path, data=data, method=method,
                                 headers={"content-type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT_S) as resp:
            return _response(resp.status, resp.read().decode("utf-8", "replace"),
                             resp.headers.get("content-type", "application/json"))
    except urllib.error.HTTPError as err:  # the host answered: pass its status through
        return _response(err.code, err.read().decode("utf-8", "replace"),
                         err.headers.get("content-type", "application/json"))
    except (urllib.error.URLError, TimeoutError, OSError) as err:  # the host did not
        return _response(502, json.dumps({"error": {"message": f"host unreachable: {type(err).__name__}",
                                                     "type": "proxy"}}))
