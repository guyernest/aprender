#!/usr/bin/env python3
"""Pull ONE finished Laya training run from S3 into a local, sha256-verified directory (plan 14-11, D-20).

    python3 -I scripts/laya_deploy/laya_fetch.py NAME TASK_ID BUCKET [--dest DIR] [--profile ze-kasher-dev]
    python3 -I scripts/laya_deploy/laya_fetch.py --selftest

`just laya-fetch` is a thin wrapper over this file. The run lives under s3://BUCKET/runs/NAME/TASK_ID/ and is
described by its manifest.json (schema laya-train-manifest-v1, written by the finalizer, 14-05). The fetch gives the
owner exactly the bytes that manifest names, or nothing:

  1. NAME, TASK_ID, BUCKET and the profile are validated, and DEST must be absent or an empty directory.
  2. manifest.json is read first (it is not an entry of itself); its schema, task id and name must match.
  3. EVERY entry path is checked (no absolute path, `..`, empty segment) and mapped by dest_for(), the one closed
     mapping below; the WHOLE destination tree is validated (duplicates, file-vs-directory conflicts) BEFORE any
     entry is downloaded.
  4. The entries are downloaded into a sibling staging directory `.<NAME>.fetch-<pid>`, each checked for size and
     sha256 as it lands, and only then is the staging directory renamed to DEST in one step.

The destination mapping (dest_for):

    manifest.json (fetched first)        -> DEST/manifest.json
    verdict.json                         -> DEST/verdict.json
    <NAME>.apr                           -> DEST/<NAME>.apr
    run/...                              -> DEST/run/...
    data/...                             -> DEST/data/...
    any other TOP-LEVEL entry (no slash) -> DEST/logs/<entry>
    any other nested entry               -> refused `fetch-layout`

DEST/run, DEST/data and DEST/<NAME>.apr are exactly what the existing, unchanged `just laya-verify` and
`just laya-deploy` take; on success this prints those two commands (owner-run, never executed here) and
`FETCH OK phase <phase> apr <sha256|none>`. Every failure is one `REFUSED fetch-<rule>: <detail>` line, exit 2, the
staging directory removed and DEST absent or unchanged. No decide server is touched and nothing is deployed.

S3 reads go through ONE Runner (`aws s3api get-object`, three attempts) that --selftest replaces with a copy from a
fixture directory: the selftest never reaches the network. stdlib only.
"""

import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time

SCHEMA = "laya-train-manifest-v1"
DEFAULT_PROFILE = "ze-kasher-dev"
ALLOWED_PROFILES = (DEFAULT_PROFILE,)
BASE_REVISION = "55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851"
BASE_HF_REPO = "convaiinnovations/laya"
MAX_MANIFEST_BYTES = 4 * 1024 * 1024
MAX_ENTRIES = 10000
MAX_PATH_LEN = 512
NAME_RE = re.compile(r"[a-z0-9][a-z0-9-]{0,47}")
TASK_RE = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
BUCKET_RE = re.compile(r"[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]")
IPV4_RE = re.compile(r"[0-9]{1,3}(\.[0-9]{1,3}){3}")
SHA_RE = re.compile(r"[0-9a-f]{64}")
CTRL_RE = re.compile(r"[\x00-\x1f\x7f\\]")
RULES = ("usage", "profile", "name", "task", "bucket", "dest", "manifest", "path", "layout", "dup", "conflict",
         "incomplete", "size", "sha256")
EXPECTED_ENTRY_KEYS = {"path", "size", "sha256"}
CHUNK = 1 << 20
RUN = subprocess.run  # the ONE subprocess entry point; the selftest swaps it for a tripwire


class Refused(Exception):
    def __init__(self, rule, msg):
        super().__init__(rule)
        self.rule = rule
        self.msg = msg


def refuse(rule, msg):
    assert rule in RULES, rule
    raise Refused(rule, msg)


class RunnerError(Exception):
    def __init__(self, kind, detail):
        super().__init__(kind)
        self.kind = kind  # "missing" | "error"
        self.detail = detail


# ---- the S3 reader ----------------------------------------------------------------------------------------

class AwsRunner:
    """`aws s3api get-object --profile P --bucket B --key K FILE`, three attempts; argv only, never a shell."""

    def __init__(self, profile):
        self.profile = profile

    def get(self, bucket, key, dest_path):
        last = "no attempt ran"
        for attempt in range(3):
            try:
                proc = RUN(["aws", "s3api", "get-object", "--profile", self.profile, "--bucket", bucket, "--key", key,
                            os.path.abspath(dest_path)], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                           stderr=subprocess.PIPE, text=True, timeout=3600)
            except FileNotFoundError:
                raise RunnerError("error", "the aws CLI is not installed")
            except subprocess.TimeoutExpired:
                last = "aws timed out"
                continue
            if proc.returncode == 0:
                return
            err = (proc.stderr or "").strip()
            if "NoSuchKey" in err or "Not Found" in err or "(404)" in err:
                raise RunnerError("missing", "the object does not exist")
            last = (err.splitlines() or ["aws exited %d" % proc.returncode])[-1][:200]
            time.sleep(attempt + 1)
        raise RunnerError("error", last)


# ---- validation -------------------------------------------------------------------------------------------

def check_inputs(name, task_id, bucket, profile):
    if profile not in ALLOWED_PROFILES:
        refuse("profile", "profile %r is not allowed in this phase (only %s)" % (profile, DEFAULT_PROFILE))
    if not NAME_RE.fullmatch(name or ""):
        refuse("name", "NAME must match [a-z0-9][a-z0-9-]{0,47}")
    if not TASK_RE.fullmatch(task_id or ""):
        refuse("task", "TASK_ID must be a 36-character lower-hex UUID (8-4-4-4-12)")
    if not BUCKET_RE.fullmatch(bucket or "") or ".." in bucket or IPV4_RE.fullmatch(bucket) or bucket.startswith("xn--"):
        refuse("bucket", "BUCKET is not a valid S3 bucket name")


def check_entry_path(p):
    if not isinstance(p, str) or not p:
        refuse("path", "a manifest entry path is not a non-empty string")
    if len(p) > MAX_PATH_LEN or CTRL_RE.search(p):
        refuse("path", "entry path %r has a control character, a backslash or is over %d characters" % (p[:60], MAX_PATH_LEN))
    if p.startswith("/"):
        refuse("path", "entry path %r is absolute" % p)
    for seg in p.split("/"):
        if seg == "":
            refuse("path", "entry path %r has an empty segment" % p)
        if seg in (".", ".."):
            refuse("path", "entry path %r contains %r" % (p, seg))


def dest_for(entry_path, name):
    """The ONE closed mapping from a manifest entry path to its place under DEST (pure)."""
    if entry_path == "manifest.json":
        refuse("layout", "manifest.json lists itself; it is fetched first and is not an entry")
    if entry_path == "verdict.json":
        return "verdict.json"
    if entry_path == name + ".apr":
        return entry_path
    if entry_path.startswith("run/") or entry_path.startswith("data/"):
        return entry_path
    if "/" not in entry_path:
        return "logs/" + entry_path
    refuse("layout", "entry %r is nested outside run/ and data/" % entry_path)


def validate_tree(entries, name):
    """Map every entry, then refuse duplicate destinations and file-vs-directory conflicts. Returns [(entry, dest)]."""
    mapped = []
    for e in entries:
        mapped.append((e, dest_for(e["path"], name)))
    seen = {}
    for e, d in mapped:
        k = d.casefold()
        if k in seen:
            refuse("dup", "entries %r and %r map to one destination %r" % (seen[k], e["path"], d))
        seen[k] = e["path"]
    dirs = {}
    for e, d in mapped:
        parts = d.casefold().split("/")
        for i in range(1, len(parts)):
            dirs.setdefault("/".join(parts[:i]), e["path"])
    for e, d in mapped:
        k = d.casefold()
        if k in dirs:
            refuse("conflict", "%r is a file but %r needs it to be a directory" % (e["path"], dirs[k]))
    return mapped


def _no_dup_keys(pairs):
    d = {}
    for k, v in pairs:
        if k in d:
            raise ValueError("duplicate key %r" % k)
        d[k] = v
    return d


def parse_manifest(raw, name, task_id):
    try:
        m = json.loads(raw.decode("utf-8"), object_pairs_hook=_no_dup_keys)
    except (UnicodeDecodeError, ValueError) as ex:
        refuse("manifest", "manifest.json is not valid JSON (%s)" % str(ex)[:80])
    if not isinstance(m, dict):
        refuse("manifest", "manifest.json is not an object")
    if m.get("schema") != SCHEMA:
        refuse("manifest", "manifest schema is %r, expected %r" % (m.get("schema"), SCHEMA))
    if m.get("task_id") != task_id:
        refuse("manifest", "manifest task_id %r is not the requested %r" % (m.get("task_id"), task_id))
    if m.get("name") != name:
        refuse("manifest", "manifest name %r is not the requested %r" % (m.get("name"), name))
    ents = m.get("entries")
    if not isinstance(ents, list):
        refuse("manifest", "manifest entries is not a list")
    if not ents:
        refuse("incomplete", "the manifest lists no entries: the run was not finalized")
    if len(ents) > MAX_ENTRIES:
        refuse("manifest", "the manifest lists more than %d entries" % MAX_ENTRIES)
    for e in ents:
        if not isinstance(e, dict) or set(e) != EXPECTED_ENTRY_KEYS:
            refuse("manifest", "an entry does not have exactly path, size and sha256")
        check_entry_path(e["path"])
        if isinstance(e["size"], bool) or not isinstance(e["size"], int) or e["size"] < 0:
            refuse("manifest", "entry %r has a bad size" % e["path"])
        if not isinstance(e["sha256"], str) or not SHA_RE.fullmatch(e["sha256"]):
            refuse("manifest", "entry %r has a bad sha256" % e["path"])
    unlisted = m.get("unlisted", 0)
    return ents, (unlisted if isinstance(unlisted, int) and not isinstance(unlisted, bool) else 0)


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            b = f.read(CHUNK)
            if not b:
                return h.hexdigest()
            h.update(b)


def default_base():
    env = os.environ.get("LAYA_MODEL_DIR")
    if env:
        return env
    return os.path.join(os.path.expanduser("~"), ".cache", "huggingface", "hub", "models--convaiinnovations--laya",
                        "snapshots", BASE_REVISION)


def display_path(p):
    home = os.path.expanduser("~")
    if p == home or p.startswith(home + os.sep):
        return "~" + p[len(home):]
    return p


def q(p):
    """Quote a path for the printed command, keeping a leading ~ expandable."""
    if p.startswith("~/"):
        return "~/" + shlex.quote(p[2:]) if re.search(r"[^A-Za-z0-9_./@%+=:,-]", p[2:]) else p
    return shlex.quote(p)


# ---- the fetch --------------------------------------------------------------------------------------------

def check_dest(dest):
    if os.path.lexists(dest):
        if os.path.islink(dest) or not os.path.isdir(dest):
            refuse("dest", "%s exists and is not a plain directory; it is never overwritten" % dest)
        if os.listdir(dest):
            refuse("dest", "%s is not empty; it is never merged or overwritten" % dest)


def fetch(name, task_id, bucket, dest, profile, runner, emit, base_dir=None):
    """Fetch the run; returns nothing, raises Refused. Staging is always removed on failure."""
    check_inputs(name, task_id, bucket, profile)
    check_dest(dest)
    parent = os.path.dirname(os.path.abspath(dest))
    os.makedirs(parent, exist_ok=True)
    staging = os.path.join(parent, ".%s.fetch-%d" % (name, os.getpid()))
    if os.path.lexists(staging):
        shutil.rmtree(staging, ignore_errors=True)
    os.mkdir(staging)
    prefix = "runs/%s/%s/" % (name, task_id)
    try:
        mpath = os.path.join(staging, "manifest.json")
        try:
            runner.get(bucket, prefix + "manifest.json", mpath)
        except RunnerError as ex:
            if ex.kind == "missing":
                refuse("manifest", "s3://%s/%smanifest.json does not exist: the run is not finalized" % (bucket, prefix))
            refuse("incomplete", "could not read manifest.json: %s" % ex.detail)
        if os.path.getsize(mpath) > MAX_MANIFEST_BYTES:
            refuse("manifest", "manifest.json is larger than %d bytes" % MAX_MANIFEST_BYTES)
        with open(mpath, "rb") as f:
            raw = f.read()
        entries, unlisted = parse_manifest(raw, name, task_id)
        mapped = validate_tree(entries, name)
        for e, d in sorted(mapped, key=lambda x: x[1]):
            target = os.path.join(staging, *d.split("/"))
            os.makedirs(os.path.dirname(target), exist_ok=True)
            try:
                runner.get(bucket, prefix + e["path"], target)
            except RunnerError as ex:
                refuse("incomplete", "%s: %s" % (e["path"], ex.detail if ex.kind == "error" else "the object is missing"))
            if not os.path.isfile(target) or os.path.islink(target):
                refuse("incomplete", "%s did not arrive as a regular file" % e["path"])
            got = os.path.getsize(target)
            if got != e["size"]:
                refuse("size", "%s is %d bytes, the manifest says %d" % (e["path"], got, e["size"]))
            if sha256_of(target) != e["sha256"]:
                refuse("sha256", "%s does not match the manifest sha256" % e["path"])
        by_path = {e["path"]: e for e in entries}
        if "verdict.json" not in by_path:
            refuse("incomplete", "the manifest lists no verdict.json: the run was not finalized")
        try:
            with open(os.path.join(staging, "verdict.json"), "rb") as f:
                verdict = json.loads(f.read().decode("utf-8"))
            phase = verdict["phase"]
            if not isinstance(phase, str) or not re.fullmatch(r"[a-z_]{1,32}", phase):
                raise ValueError("phase")
        except (ValueError, KeyError, TypeError, UnicodeDecodeError):
            refuse("manifest", "verdict.json has no readable phase")
        apr = by_path.get(name + ".apr")
        try:
            os.rename(staging, dest)
        except OSError as ex:
            refuse("dest", "could not move the verified run into %s (%s)" % (dest, ex.strerror))
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    if unlisted:
        emit("NOTE: %d object(s) under the run prefix are not in the manifest and were NOT fetched" % unlisted)
    if apr is not None:
        base = display_path(base_dir if base_dir is not None else default_base())
        base_abs = os.path.expanduser(base)
        if not os.path.isdir(base_abs):
            emit("NOTE: the base snapshot directory %s is absent; fetch revision %s with: "
                 "hf download %s --revision %s" % (base, BASE_REVISION, BASE_HF_REPO, BASE_REVISION))
        d = dest.rstrip("/") or dest
        aprp, runp, datap = q(os.path.join(d, name + ".apr")), q(os.path.join(d, "run")), q(os.path.join(d, "data"))
        emit("owner-run, not executed: just laya-verify %s %s %s %s" % (aprp, runp, datap, q(base)))
        emit("owner-run, not executed: just laya-deploy %s %s %s %s SERVER" % (aprp, runp, datap, q(base)))
    emit("FETCH OK phase %s apr %s" % (phase, apr["sha256"] if apr is not None else "none"))


# ---- CLI --------------------------------------------------------------------------------------------------

def parse_args(argv):
    pos, dest, profile = [], None, DEFAULT_PROFILE
    i = 0
    while i < len(argv):
        a = argv[i]
        if a in ("--dest", "--profile"):
            if i + 1 >= len(argv):
                refuse("usage", "%s needs a value" % a)
            if a == "--dest":
                dest = argv[i + 1]
            else:
                profile = argv[i + 1]
            i += 2
        elif a.startswith("--dest=") or a.startswith("--profile="):
            k, v = a.split("=", 1)
            if k == "--dest":
                dest = v
            else:
                profile = v
            i += 1
        elif a.startswith("-") and a != "-":
            refuse("usage", "unknown option %s" % a)
        else:
            pos.append(a)
            i += 1
    if len(pos) != 3:
        refuse("usage", "usage: laya_fetch.py NAME TASK_ID BUCKET [--dest DIR] [--profile P]")
    return pos[0], pos[1], pos[2], dest, profile


def main(argv, runner=None, emit=None, base_dir=None):
    emit = emit or (lambda s: print(s, flush=True))
    try:
        name, task_id, bucket, dest, profile = parse_args(argv)
        if dest is None:
            dest = os.path.join("models", "decide", name)
        if not dest:
            refuse("dest", "--dest is empty")
        fetch(name, task_id, bucket, dest, profile, runner or AwsRunner(profile), emit, base_dir)
    except Refused as r:
        print("REFUSED fetch-%s: %s" % (r.rule, r.msg), file=sys.stderr, flush=True)
        return 2
    return 0


# ---- selftest ---------------------------------------------------------------------------------------------

class FixtureRunner:
    """Serves objects from a directory laid out as ROOT/<bucket>/<key>; records every get."""

    def __init__(self, root):
        self.root = root
        self.calls = []

    def get(self, bucket, key, dest_path):
        self.calls.append(key)
        src = os.path.join(self.root, bucket, *key.split("/"))
        if not os.path.isfile(src):
            raise RunnerError("missing", "no such object")
        shutil.copyfile(src, dest_path)


def selftest():
    failures = []
    calls = {"real": 0}

    def case(label, ok, detail=""):
        print("  %-4s %-72s %s" % ("ok" if ok else "FAIL", label, detail))
        if not ok:
            failures.append(label)

    def tripwire(*a, **k):
        calls["real"] += 1
        raise AssertionError("the selftest reached the real aws runner")

    global RUN
    saved_run = RUN
    RUN = tripwire
    work = tempfile.mkdtemp(prefix="laya-fetch-selftest-")
    NAME = "fixture-model"
    TASK = "00000000-0000-4000-8000-000000000001"
    BUCKET = "fixture-bucket"
    here = os.path.dirname(os.path.abspath(__file__))
    fixture = os.path.join(here, "..", "..", "crates", "aprender-mcp-laya-train", "fixtures", "manifest-v1.json")
    counter = {"n": 0}

    def sha(b):
        return hashlib.sha256(b).hexdigest()

    def files_ok(phase="completed", with_apr=True, extra=None, drop=()):
        f = {
            "verdict.json": json.dumps({"phase": phase, "verdict": {"pass": phase == "completed"}}).encode(),
            "outcome.json": b'{"exit_class":"pass"}\n',
            "training-manifest.json": b'{"complete":true}\n',
            "pack-outcome.json": b'{"accepted":true}\n',
            "train.log": b"epoch 1\nepoch 2\n",
            "run/gate-report.json": b'{"pass":true,"margin":0.17}\n',
            "run/recipe.json": b'{"recipe":"x"}\n',
            "run/checkpoint/model.safetensors": bytes(range(256)) * 64,
            "data/task.json": b'{"task":"x"}\n',
            "data/train.jsonl": b'{"text":"a","label":0}\n',
            "data/eval.jsonl": b'{"text":"b","label":1}\n',
        }
        if with_apr:
            f[NAME + ".apr"] = b"APR1" + bytes(range(256)) * 16
        f.update(extra or {})
        for k in drop:
            f.pop(k, None)
        return f

    def make_bucket(files, manifest_over=None, entries_over=None, tamper=None, raw_manifest=None, omit=()):
        """Build ROOT/<bucket>/runs/NAME/TASK/..., return the runner. tamper = {path: new bytes}, omit = paths not stored."""
        counter["n"] += 1
        root = os.path.join(work, "bucket%d" % counter["n"])
        pre = os.path.join(root, BUCKET, "runs", NAME, TASK)
        ents = [{"path": p, "size": len(b), "sha256": sha(b)} for p, b in sorted(files.items())]
        if entries_over is not None:
            ents = entries_over(ents)
        man = {"schema": SCHEMA, "task_id": TASK, "name": NAME, "images": {"train": "sha256:" + "0" * 64},
               "contracts": {"train_gate": "0" * 64}, "entries": ents, "unlisted": 0}
        man.update(manifest_over or {})
        store = dict(files)
        store.update(tamper or {})
        for p in omit:
            store.pop(p, None)
        for p, b in store.items():
            fp = os.path.join(pre, *p.split("/"))
            os.makedirs(os.path.dirname(fp), exist_ok=True)
            with open(fp, "wb") as fh:
                fh.write(b)
        os.makedirs(pre, exist_ok=True)
        with open(os.path.join(pre, "manifest.json"), "wb") as fh:
            fh.write(raw_manifest if raw_manifest is not None else json.dumps(man, indent=2).encode())
        return FixtureRunner(root)

    def siblings(dest):
        parent = os.path.dirname(dest)
        return [x for x in os.listdir(parent) if x.startswith(".") and ".fetch-" in x]

    def go(runner, dest=None, base_dir=None, name=NAME, task=TASK, bucket=BUCKET, profile=None, preexisting=None):
        counter["n"] += 1
        d = dest or os.path.join(work, "out%d" % counter["n"], "dest")
        if preexisting is not None:
            os.makedirs(d)
            for p, b in preexisting.items():
                with open(os.path.join(d, p), "wb") as fh:
                    fh.write(b)
        argv = [name, task, bucket, "--dest", d] + (["--profile", profile] if profile else [])
        out = []
        err = io_capture()
        rc = main(argv, runner=runner, emit=out.append, base_dir=base_dir if base_dir is not None else work)
        return rc, out, err(), d

    def io_capture():
        import io
        buf = io.StringIO()
        old = sys.stderr
        sys.stderr = buf

        def done():
            sys.stderr = old
            return buf.getvalue()
        return done

    def gets(r):
        return [k for k in r.calls if not k.endswith("/manifest.json")]

    def refused_with(rule, rc, err):
        return rc == 2 and err.startswith("REFUSED fetch-%s:" % rule)

    try:
        # --- fetch-ok -------------------------------------------------------------------------------------
        files = files_ok()
        r = make_bucket(files)
        rc, out, err, d = go(r)
        mapping_ok = rc == 0
        want = {"manifest.json", "verdict.json", NAME + ".apr", "run/gate-report.json", "run/recipe.json",
                "run/checkpoint/model.safetensors", "data/task.json", "data/train.jsonl", "data/eval.jsonl",
                "logs/outcome.json", "logs/training-manifest.json", "logs/pack-outcome.json", "logs/train.log"}
        have = set()
        for dp, _, fs in os.walk(d):
            for fn in fs:
                have.add(os.path.relpath(os.path.join(dp, fn), d).replace(os.sep, "/"))
        case("fetch-ok: the destination holds exactly the mapped files", mapping_ok and have == want,
             "" if have == want else "extra %s missing %s" % (sorted(have - want), sorted(want - have)))
        bytes_ok = all(sha256_of(os.path.join(d, *k.split("/"))) == sha(files[k] if k in files else b"")
                       for k in files if k in ("verdict.json", NAME + ".apr", "run/gate-report.json", "data/train.jsonl")) \
            and open(os.path.join(d, "logs", "train.log"), "rb").read() == files["train.log"]
        case("fetch-ok: every fetched file equals the manifest bytes", bytes_ok)
        apr_sha = sha(files[NAME + ".apr"])
        case("fetch-ok: stdout ends FETCH OK phase completed apr <sha>", out[-1] == "FETCH OK phase completed apr " + apr_sha, out[-1][:60])
        vcmd = [x for x in out if "just laya-verify" in x]
        dcmd = [x for x in out if "just laya-deploy" in x]
        case("fetch-ok: prints exactly one verify and one deploy command, both marked not executed",
             len(vcmd) == 1 and len(dcmd) == 1 and "owner-run, not executed" in vcmd[0] and "owner-run, not executed" in dcmd[0])
        case("fetch-ok: verify command names the apr, run, data and the pinned base revision",
             len(vcmd) == 1 and d + "/" + NAME + ".apr" in vcmd[0] and d + "/run " in vcmd[0] and d + "/data " in vcmd[0])
        case("fetch-ok: the verify command precedes the deploy command", out.index(vcmd[0]) < out.index(dcmd[0]) < len(out) - 1)
        case("fetch-ok: no staging directory is left behind", siblings(d) == [])
        case("fetch-ok: the manifest was read first", r.calls[0] == "runs/%s/%s/manifest.json" % (NAME, TASK))

        # --- fetch-base-note ------------------------------------------------------------------------------
        absent = os.path.join(work, "no-such-base-dir")
        rc, out, err, d = go(make_bucket(files_ok()), base_dir=absent)
        vi = [i for i, x in enumerate(out) if "just laya-verify" in x]
        notes = [i for i, x in enumerate(out) if x.startswith("NOTE:") and absent in x]
        case("fetch-base-note: an absent base dir gets one note line naming it, before the verify command",
             rc == 0 and len(notes) == 1 and len(vi) == 1 and notes[0] == vi[0] - 1 and BASE_REVISION in out[notes[0]])
        rc, out, err, d = go(make_bucket(files_ok()), base_dir=work)
        case("fetch-base-note: a present base dir prints no note", rc == 0 and not any(x.startswith("NOTE:") for x in out))

        # --- fetch-failed-run -----------------------------------------------------------------------------
        r = make_bucket(files_ok(phase="gate_failed", with_apr=False, drop=("pack-outcome.json",)))
        rc, out, err, d = go(r)
        case("fetch-failed-run: FETCH OK phase gate_failed apr none, no verify command",
             rc == 0 and out[-1] == "FETCH OK phase gate_failed apr none" and not any("laya-verify" in x for x in out)
             and not any("laya-deploy" in x for x in out))
        case("fetch-failed-run: the reports are present", os.path.isfile(os.path.join(d, "run", "gate-report.json"))
             and os.path.isfile(os.path.join(d, "verdict.json")) and not os.path.exists(os.path.join(d, NAME + ".apr")))

        # --- fetch-sha256 / size --------------------------------------------------------------------------
        files = files_ok()
        flipped = bytearray(files["run/gate-report.json"])
        flipped[3] ^= 0x01
        r = make_bucket(files, tamper={"run/gate-report.json": bytes(flipped)})
        rc, out, err, d = go(r)
        case("fetch-sha256: one flipped byte is refused naming the path, DEST absent, staging removed",
             refused_with("sha256", rc, err) and "run/gate-report.json" in err and not os.path.lexists(d) and siblings(d) == [], err.strip()[:80])
        r = make_bucket(files, tamper={"train.log": files["train.log"] + b"x"})
        rc, out, err, d = go(r)
        case("fetch-sha256: a size mismatch is refused (fetch-size)", refused_with("size", rc, err) and not os.path.lexists(d), err.strip()[:70])

        # --- fetch-incomplete -----------------------------------------------------------------------------
        r = make_bucket(files_ok(), omit=("run/recipe.json",))
        rc, out, err, d = go(r)
        case("fetch-incomplete: a listed object that is missing is refused, DEST absent",
             refused_with("incomplete", rc, err) and "run/recipe.json" in err and not os.path.lexists(d) and siblings(d) == [])
        rc, out, err, d = go(make_bucket(files_ok(), entries_over=lambda e: []))
        case("fetch-incomplete: a manifest listing nothing is refused", refused_with("incomplete", rc, err) and not os.path.lexists(d))
        rc, out, err, d = go(make_bucket(files_ok(drop=("verdict.json",))))
        case("fetch-incomplete: a manifest without verdict.json is refused", refused_with("incomplete", rc, err) and not os.path.lexists(d))

        # --- fetch-path -----------------------------------------------------------------------------------
        for label, bad in (("absolute", "/etc/passwd"), ("dotdot", "run/../../x"), ("empty-segment", "run//x"),
                           ("leading-dot-segment", "./train.log"), ("backslash", "run\\x"), ("empty", "")):
            def over(e, bad=bad):
                return e + [{"path": bad, "size": 1, "sha256": "0" * 64}]
            r = make_bucket(files_ok(), entries_over=over)
            rc, out, err, d = go(r)
            case("fetch-path: %s entry path is refused, nothing downloaded" % label,
                 refused_with("path", rc, err) and gets(r) == [] and not os.path.lexists(d), err.strip()[:70])

        # --- fetch-manifest -------------------------------------------------------------------------------
        for label, over in (("wrong-schema", {"schema": "laya-train-manifest-v0"}),
                            ("other-task", {"task_id": "00000000-0000-4000-8000-000000000002"}),
                            ("other-name", {"name": "other-model"})):
            r = make_bucket(files_ok(), manifest_over=over)
            rc, out, err, d = go(r)
            case("fetch-manifest: %s is refused, nothing downloaded" % label,
                 refused_with("manifest", rc, err) and gets(r) == [] and not os.path.lexists(d), err.strip()[:70])
        r = make_bucket(files_ok(), raw_manifest=b"{not json")
        rc, out, err, d = go(r)
        case("fetch-manifest: an unparseable manifest is refused", refused_with("manifest", rc, err) and gets(r) == [])
        r = make_bucket(files_ok(), raw_manifest=b'{"schema":"%s","schema":"%s"}' % (SCHEMA.encode(), SCHEMA.encode()))
        rc, out, err, d = go(r)
        case("fetch-manifest: a duplicate JSON key is refused", refused_with("manifest", rc, err))
        r = make_bucket(files_ok(), entries_over=lambda e: [dict(e[0], size=-1)] + e[1:])
        rc, out, err, d = go(r)
        case("fetch-manifest: a negative size is refused", refused_with("manifest", rc, err) and gets(r) == [])
        r = make_bucket(files_ok(), entries_over=lambda e: [dict(e[0], sha256="XYZ")] + e[1:])
        rc, out, err, d = go(r)
        case("fetch-manifest: a malformed sha256 is refused", refused_with("manifest", rc, err) and gets(r) == [])
        r = make_bucket(files_ok(), entries_over=lambda e: [dict(e[0], extra=1)] + e[1:])
        rc, out, err, d = go(r)
        case("fetch-manifest: an entry with an extra key is refused", refused_with("manifest", rc, err) and gets(r) == [])
        # a run whose manifest is simply absent
        r = make_bucket(files_ok())
        os.remove(os.path.join(r.root, BUCKET, "runs", NAME, TASK, "manifest.json"))
        rc, out, err, d = go(r)
        case("fetch-manifest: an absent manifest.json is refused", refused_with("manifest", rc, err) and not os.path.lexists(d))

        # --- fetch-dup / conflict / layout ----------------------------------------------------------------
        r = make_bucket(files_ok(), entries_over=lambda e: e + [dict(e[0])])
        rc, out, err, d = go(r)
        case("fetch-dup: two entries with one destination are refused, nothing downloaded",
             refused_with("dup", rc, err) and gets(r) == [] and not os.path.lexists(d), err.strip()[:70])
        r = make_bucket(files_ok(extra={"run/Gate-Report.json": b"x"}))
        rc, out, err, d = go(r)
        case("fetch-dup: destinations equal ignoring case are refused (case-insensitive filesystems)",
             refused_with("dup", rc, err) and gets(r) == [])
        r = make_bucket(files_ok(extra={"run/a": b"x", "run/a/b": b"y"}), omit=("run/a",))  # a filesystem cannot hold both; S3 can
        rc, out, err, d = go(r)
        case("fetch-conflict: run/a and run/a/b are refused, nothing downloaded",
             refused_with("conflict", rc, err) and gets(r) == [] and not os.path.lexists(d), err.strip()[:70])
        r = make_bucket(files_ok(extra={"extra/x.json": b"x"}))
        rc, out, err, d = go(r)
        case("fetch-layout: a nested entry outside run/ and data/ is refused",
             refused_with("layout", rc, err) and gets(r) == [] and not os.path.lexists(d), err.strip()[:70])
        r = make_bucket(files_ok(), entries_over=lambda e: e + [{"path": "manifest.json", "size": 1, "sha256": "0" * 64}])
        rc, out, err, d = go(r)
        case("fetch-layout: a manifest listing manifest.json is refused", refused_with("layout", rc, err) and gets(r) == [])
        table = [("verdict.json", "verdict.json"), (NAME + ".apr", NAME + ".apr"), ("run/x/y.json", "run/x/y.json"),
                 ("data/train.jsonl", "data/train.jsonl"), ("outcome.json", "logs/outcome.json"),
                 ("train.log", "logs/train.log"), ("other.apr", "logs/other.apr"), ("run", "logs/run")]
        bad = [(e, dest_for(e, NAME), w) for e, w in table if dest_for(e, NAME) != w]
        case("dest_for: the closed mapping table (%d rows) holds" % len(table), not bad, str(bad))
        nested_refused = 0
        for e in ("a/b", "logs/x", "runs/x", "run", "datax/y"):
            try:
                dest_for(e, NAME)
                if e == "run":
                    nested_refused += 1  # `run` alone is a top-level file -> logs/run
            except Refused as ex:
                nested_refused += ex.rule == "layout"
        case("dest_for: every other nested entry is refused (fetch-layout)", nested_refused == 5, str(nested_refused))

        # --- fetch-dest -----------------------------------------------------------------------------------
        r = make_bucket(files_ok())
        rc, out, err, d = go(r, preexisting={"keep.txt": b"mine"})
        case("fetch-dest: a non-empty DEST is refused, unchanged, nothing downloaded",
             refused_with("dest", rc, err) and r.calls == [] and os.listdir(d) == ["keep.txt"]
             and open(os.path.join(d, "keep.txt"), "rb").read() == b"mine")
        r = make_bucket(files_ok())
        rc, out, err, d = go(r, preexisting={})
        case("fetch-dest: an existing EMPTY directory is accepted", rc == 0 and out[-1].startswith("FETCH OK"))
        counter["n"] += 1
        fpath = os.path.join(work, "file%d" % counter["n"])
        open(fpath, "wb").write(b"x")
        r = make_bucket(files_ok())
        rc, out, err, d = go(r, dest=fpath)
        case("fetch-dest: a DEST that is a file is refused", refused_with("dest", rc, err) and r.calls == [])
        counter["n"] += 1
        tgt = os.path.join(work, "tgt%d" % counter["n"])
        os.makedirs(tgt)
        lnk = os.path.join(work, "lnk%d" % counter["n"])
        os.symlink(tgt, lnk)
        r = make_bucket(files_ok())
        rc, out, err, d = go(r, dest=lnk)
        case("fetch-dest: a DEST that is a symlink is refused", refused_with("dest", rc, err) and r.calls == [])

        # --- fetch-name / task / bucket / profile / usage -------------------------------------------------
        for label, kw in (("uppercase", {"name": "Bad"}), ("slash", {"name": "a/b"}), ("dot-dot", {"name": ".."}),
                          ("too-long", {"name": "a" * 49}), ("empty", {"name": ""})):
            r = make_bucket(files_ok())
            rc, out, err, d = go(r, **kw)
            case("fetch-name: %s is refused, no call" % label, refused_with("name", rc, err) and r.calls == [])
        try:
            check_inputs("-x", TASK, BUCKET, DEFAULT_PROFILE)
            lead = False
        except Refused as ex:
            lead = ex.rule == "name"
        case("fetch-name: a leading hyphen is refused by the grammar (the CLI also reads it as an option)", lead)
        for label, kw in (("short", {"task": "1234"}), ("uppercase", {"task": TASK.upper().replace("0", "A")}),
                          ("no-hyphens", {"task": TASK.replace("-", "")}), ("traversal", {"task": "../" + TASK[3:]})):
            r = make_bucket(files_ok())
            rc, out, err, d = go(r, **kw)
            case("fetch-task: %s is refused, no call" % label, refused_with("task", rc, err) and r.calls == [])
        for label, b in (("uppercase", "Bucket"), ("underscore", "a_b_c"), ("short", "ab"), ("ip", "10.1.2.3"), ("dots", "a..b")):
            r = make_bucket(files_ok())
            rc, out, err, d = go(r, bucket=b)
            case("fetch-bucket: %s is refused, no call" % label, refused_with("bucket", rc, err) and r.calls == [])
        for p in ("augury-dev", "default", ""):
            r = make_bucket(files_ok())
            rc, out, err, d = go(r, profile=p or "x y")
            case("fetch-profile: %r is refused, no call" % (p or "x y"), refused_with("profile", rc, err) and r.calls == [])
        r = make_bucket(files_ok())
        rc, out, err, d = go(r, profile=DEFAULT_PROFILE)
        case("fetch-profile: ze-kasher-dev is accepted", rc == 0)
        e2 = io_capture()
        rc = main(["only-one"], runner=make_bucket(files_ok()), emit=lambda s: None)
        err = e2()
        case("fetch-usage: the wrong number of positionals is refused", refused_with("usage", rc, err))

        # --- fetch-keys -----------------------------------------------------------------------------------
        try:
            with open(fixture, "rb") as fh:
                golden = json.loads(fh.read().decode())
            r = make_bucket(files_ok())
            mine = json.load(open(os.path.join(r.root, BUCKET, "runs", NAME, TASK, "manifest.json")))
            case("fetch-keys: the selftest manifest key set equals fixtures/manifest-v1.json's",
                 set(mine) == set(golden) and set(mine["entries"][0]) == set(golden["entries"][0])
                 and mine["schema"] == golden["schema"] == SCHEMA,
                 "" if set(mine) == set(golden) else "%s vs %s" % (sorted(mine), sorted(golden)))
            case("fetch-keys: the golden fixture's name and task id pass this tool's grammars",
                 bool(NAME_RE.fullmatch(golden["name"])) and bool(TASK_RE.fullmatch(golden["task_id"])))
            ents_ok = True
            for e in golden["entries"]:
                try:
                    check_entry_path(e["path"])
                    dest_for(e["path"], golden["name"])
                except Refused:
                    ents_ok = False
            case("fetch-keys: every golden fixture entry maps through dest_for", ents_ok)
            dests = [x[1] for x in validate_tree(golden["entries"], golden["name"])]
            case("fetch-keys: the golden fixture's whole destination tree validates (%d entries)" % len(dests), len(dests) == len(golden["entries"]))
        except (OSError, ValueError, KeyError) as ex:
            case("fetch-keys: the golden fixture is readable", False, "%s: %s" % (fixture, ex))

        # --- fetch-no-network -----------------------------------------------------------------------------
        case("fetch-no-network: the real aws runner was never invoked by any fetch case", calls["real"] == 0)
        e3 = io_capture()
        probe = AwsRunner(DEFAULT_PROFILE)
        try:
            probe.get(BUCKET, "k", os.path.join(work, "never"))
            tripped = False
        except AssertionError:
            tripped = True
        e3()
        case("fetch-no-network: the tripwire guards the real runner (control: it fires when called)", tripped and calls["real"] == 1)
    finally:
        RUN = saved_run
        shutil.rmtree(work, ignore_errors=True)

    case("fetch-no-network: the real subprocess entry point is restored afterwards", RUN is saved_run)
    if failures:
        print("LAYA FETCH SELFTEST FAILED: %d case(s): %s" % (len(failures), "; ".join(failures)))
        return 1
    print("LAYA FETCH SELFTEST OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        sys.exit(selftest())
    sys.exit(main(sys.argv[1:]))
