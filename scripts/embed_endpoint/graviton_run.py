#!/usr/bin/env python3
"""Owner-run Graviton measurement driver for the Phase 11 probe (plan 11-01).

usage:
  graviton_run.py run --env <dev|prod> --profile <p> --label <l> --memory <MB> \
      --package-record <record.json> --out-dir <dir>

THIS MAKES AWS CALLS. It is started by `just embed-probe-run`, after that recipe has
refused every problem it can see offline (environment, positions, memory, asset,
package record). The driver refuses again on the live account before it measures:

  REFUSED function  the function is absent, or does not read back arm64, the
                    requested memory, or State Active
  REFUSED code      the live CodeSha256 differs from the package record's
                    code_sha256_b64, so the number would not describe the tested bytes
  REFUSED ingress   a function URL exists, or a resource policy grants any statement

It forces a cold start before each cold sample by changing the function description to
a unique stamp and waiting for the update, and it restores the original description on
every exit path. Each sample keeps every attempt: the chosen one and the discarded ones
(a cold sample without an Init Duration, a warm sample that shows one).

It writes the RAW record only: deploy/embed-endpoint/.ids/graviton-<label>-<UTC>.json.
The evidence is made by graviton_gate.py normalize. The raw record never holds an ARN,
an account id or a role name, and a 12-digit sequence is refused before the write.
Error text printed to the console is scrubbed of account ids and ARNs.

Standard library only, plus graviton_gate.py from this directory.
"""

import argparse
import base64
import datetime
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import uuid

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from graviton_gate import RAW_SCHEMA, TWELVE_DIGITS, parse_report  # noqa: E402

PROBE_PREFIX = "aprender-embed-probe"
ARCH = "arm64"
MAX_ATTEMPTS = 3
EV1 = {"n": 1, "micro_batch": 8, "words": 700}
EV32 = {"n": 32, "micro_batch": 8, "words": 700}
EV64 = {"n": 64, "micro_batch": 8, "words": 700}
EVTOK = {"tokenize_bytes": 1048576, "tokenize_kind": "prose"}
EVTOK_ADV = {"tokenize_bytes": 1048576, "tokenize_kind": "adversarial"}
# (kind, event, forced cold before each attempt, count).
# 3+5+2+3+2+3+1+1 = 20 nominal invokes (the 32-input counts mirror the 64-input counts).
PLAN = [
    ("cold-1", EV1, True, 3),
    ("warm-1", EV1, False, 5),
    ("cold-32", EV32, True, 2),
    ("warm-32", EV32, False, 3),
    ("cold-64", EV64, True, 2),
    ("warm-64", EV64, False, 3),
    ("tokenize-1mib", EVTOK, False, 1),
    ("tokenize-1mib", EVTOK_ADV, False, 1),
]
ARN_RE = re.compile(r"arn:aws[\w-]*:[^\s\"']+")


class Refused(Exception):
    def __init__(self, check, message):
        super().__init__(message)
        self.check = check


def round_floats(value, places=6):
    """Timings are doubles whose repr can end in exactly 12 fractional digits, which the
    account-id guard reads as a 12-digit run (a false positive, ~22% of realistic timings).
    Six places keeps microsecond resolution and leaves the account-id check unchanged."""
    if isinstance(value, float):
        return round(value, places)
    if isinstance(value, dict):
        return {k: round_floats(v, places) for k, v in value.items()}
    if isinstance(value, list):
        return [round_floats(v, places) for v in value]
    return value


def scrub(text):
    """Console text with account ids and ARNs replaced; the record never sees them anyway."""
    text = ARN_RE.sub("<arn>", text or "")
    return TWELVE_DIGITS.sub("<account>", text)


class Aws:
    def __init__(self, profile, region):
        self.profile = profile
        self.region = region

    def run(self, args):
        cmd = ["aws", "--profile", self.profile, "--region", self.region, "--output", "json"] + args
        return subprocess.run(cmd, capture_output=True, text=True)

    def json(self, args):
        proc = self.run(args)
        if proc.returncode != 0:
            raise RuntimeError(f"aws {' '.join(args[:2])} failed: {scrub(proc.stderr.strip())[:300]}")
        return json.loads(proc.stdout) if proc.stdout.strip() else {}

    def ok(self, args):
        proc = self.run(args)
        if proc.returncode != 0:
            raise RuntimeError(f"aws {' '.join(args[:2])} failed: {scrub(proc.stderr.strip())[:300]}")


def force_cold(aws, fn):
    """Change the description to a unique stamp, so the next invoke starts a new environment."""
    stamp = f"embed-probe-cold-{uuid.uuid4().hex[:16]}"
    aws.json(["lambda", "update-function-configuration", "--function-name", fn, "--description", stamp])
    aws.ok(["lambda", "wait", "function-updated-v2", "--function-name", fn])


def invoke(aws, fn, event, workdir):
    payload = os.path.join(workdir, "event.json")
    with open(payload, "w", encoding="utf-8") as handle:
        json.dump(event, handle)
    body = os.path.join(workdir, "out.json")
    started = time.monotonic()
    proc = aws.run([
        "lambda", "invoke", "--function-name", fn,
        "--payload", f"fileb://{payload}",
        "--cli-binary-format", "raw-in-base64-out",
        "--log-type", "Tail", body,
    ])
    wall_ms = (time.monotonic() - started) * 1000.0
    if proc.returncode != 0:
        raise RuntimeError(f"invoke failed: {scrub(proc.stderr.strip())[:300]}")
    meta = json.loads(proc.stdout)
    log_text = base64.b64decode(meta.get("LogResult", "") or "").decode("utf-8", "replace")
    probe = None
    if os.path.exists(body):
        with open(body, encoding="utf-8") as handle:
            probe = json.load(handle)
    return {
        "function_error": meta.get("FunctionError"),
        "client_wall_ms": wall_ms,
        "report": parse_report(log_text),
        "probe": probe,
    }


def accepted(kind, attempt):
    report = attempt["report"]
    if report is None:
        return False
    cold = report["init_duration_ms"] is not None
    return cold if kind.startswith("cold") else not cold


def take_sample(aws, fn, kind, event, cold, workdir):
    """Run one sample, retrying at most MAX_ATTEMPTS times until it is the kind it claims to be."""
    log = []
    for _ in range(MAX_ATTEMPTS):
        if cold:
            force_cold(aws, fn)
        attempt = invoke(aws, fn, event, workdir)
        log.append(attempt)
        if accepted(kind, attempt):
            break
    chosen = log[-1]
    return {
        "kind": kind,
        "event": event,
        "function_error": chosen["function_error"],
        "client_wall_ms": chosen["client_wall_ms"],
        "attempts": len(log),
        "report": chosen["report"],
        "probe": chosen["probe"],
        "discarded_attempts": [
            {"function_error": a["function_error"], "client_wall_ms": a["client_wall_ms"],
             "report": a["report"], "probe": a["probe"]}
            for a in log[:-1]
        ],
    }


def resolve_region(profile):
    """The region the CDK deploy uses: AWS_REGION, then the profile's region, then us-east-1."""
    if os.environ.get("AWS_REGION"):
        return os.environ["AWS_REGION"]
    proc = subprocess.run(["aws", "configure", "get", "region", "--profile", profile],
                          capture_output=True, text=True)
    value = proc.stdout.strip() if proc.returncode == 0 else ""
    return value or "us-east-1"


def check_function(aws, fn, memory):
    try:
        cfg = aws.json(["lambda", "get-function-configuration", "--function-name", fn])
    except RuntimeError as error:
        raise Refused("function", f"{fn} does not exist or cannot be read ({error})")
    if cfg.get("Architectures") != [ARCH]:
        raise Refused("function", f"{fn} does not read back {ARCH}")
    if cfg.get("MemorySize") != memory:
        raise Refused("function", f"{fn} memory is {cfg.get('MemorySize')} MB, requested {memory} MB")
    if cfg.get("State") != "Active":
        raise Refused("function", f"{fn} State is {cfg.get('State')}, not Active")
    return cfg


def check_code(cfg, record):
    live = cfg.get("CodeSha256")
    expected = record.get("code_sha256_b64")
    if not live or live != expected:
        raise Refused("code", "the live CodeSha256 differs from the package record: the tested bytes are not deployed")


def check_ingress(aws, fn):
    """Absent is an error from the service, not a silent pass: anything else refuses."""
    url = aws.run(["lambda", "get-function-url-config", "--function-name", fn])
    if url.returncode == 0:
        raise Refused("ingress", "a function URL exists on the probe")
    if "ResourceNotFoundException" not in url.stderr:
        raise Refused("ingress", f"the function URL could not be confirmed absent: {scrub(url.stderr.strip())[:200]}")
    policy = aws.run(["lambda", "get-policy", "--function-name", fn])
    if policy.returncode != 0:
        if "ResourceNotFoundException" not in policy.stderr:
            raise Refused("ingress", f"the resource policy could not be confirmed absent: {scrub(policy.stderr.strip())[:200]}")
        return "absent", "absent"
    statements = json.loads(json.loads(policy.stdout).get("Policy", "{}")).get("Statement", [])
    if statements:
        raise Refused("ingress", "the function has a resource-based policy with statements")
    return "absent", "absent"


def run(args):
    with open(args.package_record, encoding="utf-8") as handle:
        record = json.load(handle)
    fn = f"{PROBE_PREFIX}-{args.env}"
    region = resolve_region(args.profile)
    aws = Aws(args.profile, region)

    cfg = check_function(aws, fn, args.memory)
    check_code(cfg, record)
    url_state, policy_state = check_ingress(aws, fn)

    original = cfg.get("Description", "")
    changed = False
    samples = []
    workdir = tempfile.mkdtemp(prefix="graviton-run-")
    try:
        for kind, event, cold, count in PLAN:
            for _ in range(count):
                if cold:
                    changed = True
                samples.append(take_sample(aws, fn, kind, event, cold, workdir))
                print(f"SAMPLE {kind} attempts={samples[-1]['attempts']}", flush=True)
    finally:
        if changed:
            aws.json(["lambda", "update-function-configuration", "--function-name", fn,
                      "--description", original])
            aws.ok(["lambda", "wait", "function-updated-v2", "--function-name", fn])
        for name in ("out.json", "event.json"):
            path = os.path.join(workdir, name)
            if os.path.exists(path):
                os.remove(path)
        os.rmdir(workdir)

    env_vars = (cfg.get("Environment") or {}).get("Variables") or {}
    raw = {
        "schema": RAW_SCHEMA,
        "label": args.label,
        "function": {
            "name": fn,
            "region": region,
            "memory_mb": cfg.get("MemorySize"),
            "timeout_s": cfg.get("Timeout"),
            "architecture": ARCH,
            "rayon_threads_env": env_vars.get("RAYON_NUM_THREADS"),
            "code_sha256": cfg.get("CodeSha256"),
            "expected_code_sha256": record.get("code_sha256_b64"),
        },
        "ingress": {"function_url": url_state, "policy": policy_state},
        "samples": samples,
    }
    text = json.dumps(round_floats(raw), indent=2, sort_keys=True) + "\n"
    if TWELVE_DIGITS.search(text):
        raise RuntimeError("refusing to write a raw record that contains a 12-digit sequence")
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    os.makedirs(args.out_dir, exist_ok=True)
    out = os.path.join(args.out_dir, f"graviton-{args.label}-{stamp}.json")
    with open(out, "w", encoding="utf-8") as handle:
        handle.write(text)
    attempts = sum(s["attempts"] for s in samples)
    print(f"RAW RECORD WRITTEN {out}")
    print(f"INVOKES nominal {sum(c for _, _, _, c in PLAN)} attempts {attempts}")
    return 0


def main(argv):
    parser = argparse.ArgumentParser(description="owner-run Graviton measurement driver")
    sub = parser.add_subparsers(dest="command", required=True)
    r = sub.add_parser("run")
    r.add_argument("--env", required=True, choices=["dev", "prod"])
    r.add_argument("--profile", required=True)
    r.add_argument("--label", required=True)
    r.add_argument("--memory", required=True, type=int)
    r.add_argument("--package-record", required=True)
    r.add_argument("--out-dir", required=True)
    args = parser.parse_args(argv[1:])
    try:
        return run(args)
    except Refused as refusal:
        print(f"REFUSED {refusal.check}: {refusal}", file=sys.stderr)
        return 2
    except (RuntimeError, OSError, ValueError) as error:
        print(f"GRAVITON RUN FAILED: {scrub(str(error))}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
