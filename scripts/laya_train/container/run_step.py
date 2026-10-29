#!/usr/bin/env python3
"""The training container's one program: SageMaker runs `run_step.py train` and nothing else (plan 14-06).

    run_step.py train            (SageMaker's single argument; the effective mode is env LAYA_STEP, default train)
    run_step.py --selftest       (offline; a fake trainer and a fake S3 runner; prints RUN STEP SELFTEST OK)

DECLARED RULES (written before the code; the selftest asserts each one):

    mode      LAYA_STEP is `train` (default), `probe` or `probe-sigterm`; anything else is `REFUSED env:` (exit 2).
    trainer   the pinned trainer runs UNCHANGED: `train.py --data <work>/data --out <work>/run --device cuda
              [--epochs N]` and nothing else; never a seeds, stopping, base or variant flag. This program never
              packs, verifies or recomputes a metric; x86_64 touches only torch.
    exit      train.py exit 0 -> pass, 3 -> gate_fail (the job exits 0 so the pack stage runs), 2 -> refused, any
              other exit or a signal -> crash (the job exits 1 AFTER uploading). outcome.json is written LAST: it
              is the completion marker.
    device    train.py logs `DEVICE seed=N requested=cuda used=<dev> torch=<v>`; the first line whose `used=` is not
              a cuda device kills the trainer (a CPU fallback must never burn the 6 h ceiling). After any exit the
              run's gate-report.json device_used must start with `cuda`, else outcome device_ok is false.
    inputs    LAYA_EXPECTED_INPUTS (the canonical `task.json=<hex>;train.jsonl=<hex>;eval.jsonl=<hex>[;shift.jsonl=<hex>]`)
              is re-checked against the data channel BEFORE the trainer starts: a differing hash, or a file in one set
              and not the other, is outcome refused with refused_line `REFUSED inputs-changed: <file>`.
    upload    every object is a `put-object` carrying the tag `laya-retention=expire` through one Runner callable
              (default: the aws CLI v2; selftests inject a fake). Order: the files (checkpoints included),
              training-manifest.json (complete true), then outcome.json LAST. A file over 5 GiB is never uploaded.
    env       train mode needs LAYA_TASK_ID, LAYA_NAME, LAYA_BUCKET, LAYA_RUN_PREFIX (exactly runs/<name>/<task_id>/),
              LAYA_EPOCHS ("" or digits), LAYA_TRAIN_IMAGE_DIGEST (`sha256:` + 64 lowercase hex, recorded as
              training-manifest.json train_image_digest) and LAYA_EXPECTED_INPUTS. Anything missing or malformed is
              `REFUSED env: ...` (exit 2) before the trainer starts. Values are never echoed.
    never     no pip, no download of weights, no AWS call in the selftest, and no determinism switch: this program
              sets no deterministic-algorithms flag and no cuBLAS workspace variable (D-17).

Standard library only, Python >= 3.11 (it runs under the image's uv venv, and under `python3 -I` for the selftest).
"""
import contextlib
import hashlib
import io
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = Path(__file__).resolve().parents[3]          # the repo root, on the laptop and under /opt/laya in the image
TRAIN_PY = ROOT / "scripts" / "laya_train" / "train.py"
GATE_CONTRACT = ROOT / "contracts" / "laya-finetune-gate-v1.yaml"
FIXTURES = ROOT / "crates" / "aprender-mcp-laya-train" / "fixtures"

CHANNEL_DATA = Path("/opt/ml/input/data/data")
CHANNEL_BASE_TAR = Path("/opt/ml/input/data/base/laya-base.tar")
WORK_DIR = Path("/opt/ml/work")

SCHEMA_OUTCOME = "laya-train-outcome-v1"
SCHEMA_TRAINING_MANIFEST = "laya-train-training-manifest-v1"
RETENTION_TAG = "laya-retention=expire"
INPUT_FILES = ("task.json", "train.jsonl", "eval.jsonl", "shift.jsonl")
MAX_OBJECT_BYTES = 5 * 1024 ** 3
SMALL_FILE_BYTES = 5 * 1024 ** 2
MIN_CALL_S = 0.2                                     # an upload is not started with less time than this left

DEVICE_RE = re.compile(r"^DEVICE seed=\d+ requested=(\S+) used=(\S+) torch=")
NAME_RE = re.compile(r"^[a-z0-9][a-z0-9-]{0,47}$")
TASK_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9-]{0,63}$")
BUCKET_RE = re.compile(r"^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$")
DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")

QUIET = False
DEFAULT_RUNNER_CALLS = 0                              # the selftest asserts it stays 0 (no network)


def say(msg):
    if not QUIET:
        print("run_step: " + msg, flush=True)


class Refusal(Exception):
    """A refusal; str() is `<rule>: <detail>` and is printed as `REFUSED <rule>: <detail>`."""


# ------------------------------------------------------------------------------------------ pure helpers

def sha256_bytes(b):
    return hashlib.sha256(b).hexdigest()


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def now_iso():
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def write_json(path, obj):
    Path(path).write_text(json.dumps(obj, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def parse_inputs_string(s):
    """Mirror of layout::parse_inputs_string (crates/aprender-mcp-laya-train/src/layout.rs): the canonical
    `task.json=<hex>;train.jsonl=<hex>;eval.jsonl=<hex>[;shift.jsonl=<hex>]`, each file once, in that order,
    64 lowercase hex, the first three required. Raises ValueError naming what is wrong (never a digest)."""
    out, last = {}, -1
    for part in s.split(";"):
        if "=" not in part:
            raise ValueError("an entry has no '='")
        name, _, hexd = part.partition("=")
        if name not in INPUT_FILES:
            raise ValueError("unknown input file %r" % name)
        rank = INPUT_FILES.index(name)
        if rank <= last:
            raise ValueError("%s is repeated or out of canonical order" % name)
        last = rank
        if not HEX64_RE.match(hexd):
            raise ValueError("%s digest is not 64 lowercase hex characters" % name)
        out[name] = hexd
    for need in INPUT_FILES[:3]:
        if need not in out:
            raise ValueError("%s is missing" % need)
    return out


def classify(rc):
    """train.py's exit status -> the outcome exit_class (rc None or negative = a signal or a launch failure)."""
    if rc == 0:
        return "pass"
    if rc == 3:
        return "gate_fail"
    if rc == 2:
        return "refused"
    return "crash"


def job_exit_code(exit_class):
    """pass and gate_fail exit 0 (the pack stage runs on a gate_fail); everything else exits 1."""
    return 0 if exit_class in ("pass", "gate_fail") else 1


def first_refused_line(log_path):
    try:
        with open(log_path, "rb") as f:
            for raw in f:
                line = raw.decode("utf-8", "replace").rstrip("\r\n")
                if line.startswith("REFUSED "):
                    return line
    except OSError:
        pass
    return None


def read_gate_device(run_dir):
    """gate-report.json device_used, or None when the report is absent or unreadable."""
    try:
        rep = json.loads((Path(run_dir) / "gate-report.json").read_text(encoding="utf-8"))
        dev = rep.get("device_used")
        return dev if isinstance(dev, str) else None
    except (OSError, ValueError):
        return None


def trainer_argv(python, trainer, work, epochs):
    """The trainer's argv: the pinned CLI, unchanged. `epochs` is "" or digits."""
    argv = [python, str(trainer), "--data", str(Path(work) / "data"), "--out", str(Path(work) / "run"),
            "--device", "cuda"]
    if epochs:
        argv += ["--epochs", epochs]
    return argv


@dataclass(frozen=True)
class Cfg:
    task_id: str
    name: str
    bucket: str
    run_prefix: str
    epochs: str
    image_digest: str
    expected_inputs: dict


def parse_env(env):
    """The train-mode env contract. Raises Refusal("env: ...") on anything missing or malformed."""
    def need(key):
        val = env.get(key, "")
        if not val:
            raise Refusal("env: %s is required" % key)
        return val

    task_id = need("LAYA_TASK_ID")
    if not TASK_RE.match(task_id):
        raise Refusal("env: LAYA_TASK_ID is malformed")
    name = need("LAYA_NAME")
    if not NAME_RE.match(name):
        raise Refusal("env: LAYA_NAME is malformed")
    bucket = need("LAYA_BUCKET")
    if not BUCKET_RE.match(bucket):
        raise Refusal("env: LAYA_BUCKET is malformed")
    prefix = need("LAYA_RUN_PREFIX")
    if prefix != "runs/%s/%s/" % (name, task_id):
        raise Refusal("env: LAYA_RUN_PREFIX must be runs/<LAYA_NAME>/<LAYA_TASK_ID>/")
    epochs = env.get("LAYA_EPOCHS", "")
    if epochs != "" and (not re.fullmatch(r"[0-9]{1,4}", epochs) or int(epochs) < 1):
        raise Refusal("env: LAYA_EPOCHS must be empty or a positive integer")
    digest = need("LAYA_TRAIN_IMAGE_DIGEST")
    if not DIGEST_RE.match(digest):
        raise Refusal("env: LAYA_TRAIN_IMAGE_DIGEST must be sha256: followed by 64 lowercase hex")
    try:
        expected = parse_inputs_string(need("LAYA_EXPECTED_INPUTS"))
    except ValueError as e:
        raise Refusal("env: LAYA_EXPECTED_INPUTS is unparseable: %s" % e)
    return Cfg(task_id, name, bucket, prefix, epochs, digest, expected)


def channel_hashes(channel):
    """{name: sha256} of the dataset files present in the data channel."""
    out = {}
    for name in INPUT_FILES:
        p = Path(channel) / name
        if p.is_file():
            out[name] = sha256_file(p)
    return out


def check_inputs(channel, expected):
    """None when the channel holds exactly the expected files with the expected hashes; else the name of the
    first (canonical order) file that differs, is present only in the channel, or is missing from it."""
    have = channel_hashes(channel)
    for name in INPUT_FILES:
        if have.get(name) != expected.get(name):
            return name
    return None


def list_files(work):
    """[(relpath, path)] of everything the run uploads: train.log, run-env.json, gpu-samples.json, then every
    regular file under run/ and data/ (sorted). outcome.json and the manifests are written by finish()."""
    work = Path(work)
    out = []
    for name in ("train.log", "run-env.json", "gpu-samples.json"):
        p = work / name
        if p.is_file():
            out.append((name, p))
    for top in ("run", "data"):
        root = work / top
        if root.is_dir():
            for p in sorted(root.rglob("*")):
                if p.is_file() and not p.is_symlink():
                    out.append((p.relative_to(work).as_posix(), p))
    return out


# ------------------------------------------------------------------------------------------ S3 transfer

def aws_runner(argv, timeout):
    """The default Runner: the aws CLI v2. Raises on a non-zero exit or a timeout (subprocess kills the child)."""
    global DEFAULT_RUNNER_CALLS
    DEFAULT_RUNNER_CALLS += 1
    r = subprocess.run(argv, capture_output=True, timeout=timeout)
    if r.returncode != 0:
        raise RuntimeError("aws exited %d: %s" % (r.returncode, r.stderr[-300:].decode("utf-8", "replace")))


class Uploader:
    """Tagged put-object through one Runner. `uploaded` holds {relpath: (size, sha256)} for exactly the files whose
    upload RETURNED success; a manifest lists nothing else. Small files are snapshotted first so the recorded hash
    is the hash of the uploaded bytes even while the trainer is still writing."""

    def __init__(self, runner, bucket, prefix, snap_dir, backoff=(1.0, 4.0), sleep=time.sleep):
        self.runner, self.bucket, self.prefix = runner, bucket, prefix
        self.snap_dir, self.backoff, self.sleep = Path(snap_dir), backoff, sleep
        self.uploaded = {}
        self.failed = {}
        self._lock = threading.RLock()
        self._n = 0

    def argv(self, rel, path):
        return ["aws", "s3api", "put-object", "--bucket", self.bucket, "--key", self.prefix + rel,
                "--body", str(path), "--tagging", RETENTION_TAG]

    def _snapshot(self, path):
        with self._lock:
            self._n += 1
            snap = self.snap_dir / ("%d.snap" % self._n)
        data = Path(path).read_bytes()
        snap.write_bytes(data)
        return snap, len(data), sha256_bytes(data)

    def put(self, rel, path, attempts=3, timeout=900.0, deadline=None, record=True):
        """One object. Returns "uploaded", "same" (already uploaded with these bytes) or "failed". `deadline` is
        an absolute time.monotonic() value: no call starts after it and every call's timeout is capped by it."""
        try:
            size = Path(path).stat().st_size
            if size > MAX_OBJECT_BYTES:
                self.failed[rel] = "over 5 GiB"
                return "failed"
            if size <= SMALL_FILE_BYTES:
                src, size, sha = self._snapshot(path)
            else:
                src, sha = Path(path), sha256_file(path)
        except OSError as e:
            self.failed[rel] = "read: %s" % e
            return "failed"
        with self._lock:
            if record and self.uploaded.get(rel) == (size, sha):
                return "same"
        err = "no attempt"
        for i in range(attempts):
            remaining = None if deadline is None else deadline - time.monotonic()
            if remaining is not None and remaining < MIN_CALL_S:
                err = "deadline"
                break
            call_timeout = timeout if remaining is None else min(timeout, remaining)
            try:
                self.runner(self.argv(rel, src), call_timeout)
                if record:
                    with self._lock:
                        self.uploaded[rel] = (size, sha)
                        self.failed.pop(rel, None)
                return "uploaded"
            except Exception as e:                       # a Runner failure of any kind is one failed attempt
                err = "%s: %s" % (type(e).__name__, str(e)[:200])
            if i + 1 < attempts and deadline is None:
                self.sleep(self.backoff[min(i, len(self.backoff) - 1)])
        self.failed[rel] = err
        return "failed"

    def entries(self):
        with self._lock:
            return [{"path": p, "size": s, "sha256": h} for p, (s, h) in sorted(self.uploaded.items())]


def manifest_obj(cfg, gate_sha, complete, entries):
    return {"schema": SCHEMA_TRAINING_MANIFEST, "task_id": cfg.task_id, "complete": complete,
            "train_image_digest": cfg.image_digest, "gate_contract_sha256": gate_sha,
            "entries": sorted(entries, key=lambda e: e["path"])}


# ------------------------------------------------------------------------------------------ the trainer child

class Trainer:
    """The trainer subprocess in its own process group. A pump thread appends its merged stdout/stderr to
    train.log, echoes it (CloudWatch) and kills the group at the first non-cuda `used=` DEVICE line."""

    def __init__(self, argv, env, log_path, echo=True):
        self.argv, self.env, self.log_path, self.echo = argv, env, Path(log_path), echo
        self.proc = None
        self.pump = None
        self.cpu_fallback = False
        self.devices = []
        self.started = None

    def start(self):
        self.started = time.monotonic()
        self.proc = subprocess.Popen(self.argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=self.env,
                                     start_new_session=True)
        self.pump = threading.Thread(target=self._pump, daemon=True)
        self.pump.start()

    def kill_group(self, sig=signal.SIGKILL):
        try:
            os.killpg(self.proc.pid, sig)
        except (ProcessLookupError, PermissionError):
            pass

    def _pump(self):
        with open(self.log_path, "ab") as log:
            for raw in iter(self.proc.stdout.readline, b""):
                log.write(raw)
                log.flush()
                if self.echo:
                    sys.stdout.buffer.write(raw)
                    sys.stdout.buffer.flush()
                m = DEVICE_RE.match(raw.decode("utf-8", "replace"))
                if m:
                    self.devices.append(m.group(2))
                    if not m.group(2).startswith("cuda") and not self.cpu_fallback:
                        self.cpu_fallback = True
                        say("DEVICE line says used=%s: killing the trainer (no silent CPU fallback)" % m.group(2))
                        self.kill_group()

    def wait(self):
        """The trainer's exit status (negative = killed by a signal); joins the pump."""
        rc = self.proc.wait()
        self.pump.join(10)
        return rc

    def seconds(self):
        return time.monotonic() - self.started if self.started is not None else 0.0


# ------------------------------------------------------------------------------------------ the run

@dataclass
class Paths:
    data_channel: Path = CHANNEL_DATA
    base_tar: Path = CHANNEL_BASE_TAR
    work: Path = WORK_DIR

    @property
    def hf_home(self):
        return Path(self.work) / "hf"


@dataclass
class Knobs:
    python: str = sys.executable
    trainer: Path = TRAIN_PY
    contract_path: Path = GATE_CONTRACT
    backoff: tuple = (1.0, 4.0)
    attempts: int = 3
    upload_timeout: float = 900.0
    echo: bool = True
    prepare_base: object = None                       # callable() -> HF_HOME Path (the base channel's staging)


def child_env(hf_home):
    env = dict(os.environ)
    env.update({"HF_HOME": str(hf_home), "HF_HUB_OFFLINE": "1", "TRANSFORMERS_OFFLINE": "1",
                "PYTHONUNBUFFERED": "1"})
    return env


class Run:
    def __init__(self, cfg, paths, runner, knobs):
        self.cfg, self.paths, self.knobs = cfg, paths, knobs
        self.work = Path(paths.work)
        self.work.mkdir(parents=True, exist_ok=True)
        self.snap = Path(tempfile.mkdtemp(prefix="laya-snap-"))
        self.uploader = Uploader(runner, cfg.bucket, cfg.run_prefix, self.snap, knobs.backoff)
        self.log_path = self.work / "train.log"
        self.log_path.write_bytes(b"")
        self.started_at = now_iso()
        self.trainer = None
        self.gate_sha = sha256_file(knobs.contract_path)
        self.run_env = {"schema": "laya-train-run-env-v1", "python": sys.version.split()[0],
                        "hf_hub_offline": True, "peak_gpu_memory_mib": None}

    # ------------------------------------------------------------------ steps
    def append_log(self, line):
        with open(self.log_path, "ab") as f:
            f.write((line + "\n").encode("utf-8"))
        say(line)

    def write_run_env(self):
        write_json(self.work / "run-env.json", self.run_env)

    def copy_data(self):
        dst = self.work / "data"
        dst.mkdir(parents=True, exist_ok=True)
        for name in INPUT_FILES:
            src = Path(self.paths.data_channel) / name
            if src.is_file():
                shutil.copyfile(src, dst / name)

    def execute(self):
        bad = check_inputs(self.paths.data_channel, self.cfg.expected_inputs)
        if bad is not None:
            line = "REFUSED inputs-changed: %s" % bad
            self.append_log(line)
            print(line, file=sys.stderr, flush=True)
            return self.finish("refused", None, 0.0, None, False)
        self.copy_data()
        hf_home = self.paths.hf_home
        if self.knobs.prepare_base is not None:
            try:
                hf_home = self.knobs.prepare_base()
            except Exception as e:                       # base staging failed: no trainer, the run is a crash
                self.append_log("BASE STAGING FAILED: %s" % str(e)[:300])
                return self.finish("crash", None, 0.0, None, False)
        self.write_run_env()
        argv = trainer_argv(self.knobs.python, self.knobs.trainer, self.work, self.cfg.epochs)
        self.trainer = Trainer(argv, child_env(hf_home), self.log_path, self.knobs.echo)
        try:
            self.trainer.start()
        except OSError as e:
            self.append_log("TRAINER LAUNCH FAILED: %s" % str(e)[:300])
            return self.finish("crash", None, 0.0, None, False)
        rc = self.trainer.wait()
        return self.finish_after_trainer(rc)

    def finish_after_trainer(self, rc):
        tr = self.trainer
        exit_class = "crash" if tr.cpu_fallback else classify(rc)
        train_rc = rc if rc is not None and rc >= 0 and not tr.cpu_fallback else None
        return self.finish(exit_class, train_rc, tr.seconds(), first_refused_line(self.log_path), tr.cpu_fallback)

    def finish(self, exit_class, train_rc, train_seconds, refused_line_hint, cpu_fallback):
        """Classify, hash, upload every file, then training-manifest.json, then outcome.json LAST."""
        report_dev = read_gate_device(self.work / "run")
        log_dev = self.trainer.devices[0] if self.trainer is not None and self.trainer.devices else None
        dev = report_dev
        device_used = dev if dev else (log_dev if cpu_fallback and log_dev else "none")
        device_ok = bool(dev and dev.startswith("cuda") and not cpu_fallback)
        refused_line = refused_line_hint if exit_class == "refused" else None
        if exit_class == "refused" and refused_line is None:
            refused_line = first_refused_line(self.log_path)
        self.write_run_env()
        outcome = {"schema": SCHEMA_OUTCOME, "task_id": self.cfg.task_id, "exit_class": exit_class,
                   "train_rc": train_rc, "device_used": device_used, "device_ok": device_ok,
                   "refused_line": refused_line, "started_at": self.started_at, "finished_at": now_iso(),
                   "train_seconds": round(train_seconds, 1)}
        out_path = self.work / "outcome.json"
        write_json(out_path, outcome)
        up, k = self.uploader, self.knobs
        for rel, path in list_files(self.work):
            up.put(rel, path, attempts=k.attempts, timeout=k.upload_timeout)
        entries = up.entries()
        entries.append({"path": "outcome.json", "size": out_path.stat().st_size, "sha256": sha256_file(out_path)})
        man_path = self.work / "training-manifest.json"
        write_json(man_path, manifest_obj(self.cfg, self.gate_sha, True, entries))
        up.put("training-manifest.json", man_path, attempts=k.attempts, timeout=k.upload_timeout, record=False)
        up.put("outcome.json", out_path, attempts=k.attempts, timeout=k.upload_timeout, record=False)
        failed = dict(up.failed)
        if failed:
            say("upload failures: %s" % ", ".join("%s (%s)" % (p, e) for p, e in sorted(failed.items())))
        code = job_exit_code(exit_class)
        return 1 if failed and code == 0 else code


def run_train(cfg, paths, runner, knobs):
    return Run(cfg, paths, runner, knobs).execute()


def entry_train(env, paths, runner, knobs):
    """Parse the env contract, then run. A refusal prints `REFUSED env: ...` on stderr and returns 2."""
    try:
        cfg = parse_env(env)
    except Refusal as e:
        print("REFUSED " + str(e), file=sys.stderr, flush=True)
        return 2
    return run_train(cfg, paths, runner, knobs)


# ------------------------------------------------------------------------------------------ selftest

FAKE_TRAINER = r'''
import json, os, pathlib, signal, sys, time
mode = os.environ.get("FAKE_MODE", "pass")
args = sys.argv[1:]
def opt(n):
    return args[args.index(n) + 1] if n in args else None
out = pathlib.Path(opt("--out"))
mark = os.environ.get("FAKE_MARK")
if mark:
    pathlib.Path(mark, "trainer.started").write_text("1")
out.mkdir(parents=True, exist_ok=True)
(out / "argv.json").write_text(json.dumps(args))
def device_line(dev):
    print("DEVICE seed=13 requested=cuda used=%s torch=x" % dev, flush=True)
def report(dev="cuda:0"):
    (out / "gate-report.json").write_text(json.dumps({"pass": mode == "pass", "device_used": dev, "device_is_cpu": False}))
if mode in ("pass", "gate3"):
    device_line("cuda:0")
    (out / "recipe.json").write_text('{"fake":true}')
    (out / "checkpoint").mkdir()
    (out / "checkpoint" / "model.safetensors").write_bytes(b"w" * 1024)
    report()
    print("GATE PASS" if mode == "pass" else "GATE FAIL", flush=True)
    sys.exit(0 if mode == "pass" else 3)
if mode == "refused":
    print("starting", flush=True)
    print("REFUSED data: fake refusal", file=sys.stderr, flush=True)
    sys.exit(2)
if mode == "rc7":
    device_line("cuda:0")
    sys.exit(7)
if mode == "sigkill":
    device_line("cuda:0")
    os.kill(os.getpid(), signal.SIGKILL)
if mode == "cpu":
    device_line("cpu")
    time.sleep(60)
'''


def selftest():
    global QUIET, DEFAULT_RUNNER_CALLS
    for need in (FIXTURES / "outcome-v1.json", FIXTURES / "training-manifest-v1.json", GATE_CONTRACT):
        if not need.is_file():
            print("RUN STEP SELFTEST CANNOT RUN: %s is missing (ROOT=%s)" % (need, ROOT))
            return 2
    QUIET = True
    DEFAULT_RUNNER_CALLS = 0
    failures = []

    def case(name, ok, detail=""):
        print("  %-4s %-72s %s" % ("ok" if ok else "FAIL", name, detail))
        if not ok:
            failures.append(name)

    outcome_keys = set(json.loads((FIXTURES / "outcome-v1.json").read_text()))
    manifest_keys = set(json.loads((FIXTURES / "training-manifest-v1.json").read_text()))
    task = "00000000-0000-4000-8000-000000000001"
    digest = "sha256:" + "ab" * 32

    class FakeRunner:
        """Copies the --body into a temp 'bucket' dir and records the call (key, tag, time, state)."""

        def __init__(self, bucket, marker_dir=None, delay=0.0, fail=None):
            self.bucket, self.marker_dir, self.delay, self.fail = Path(bucket), marker_dir, delay, fail
            self.calls = []

        def __call__(self, argv, timeout):
            a = dict(zip(argv[3::2], argv[4::2])) if argv[:3] == ["aws", "s3api", "put-object"] else None
            if a is None:
                raise RuntimeError("not a put-object argv")
            key, body = a["--key"], a["--body"]
            rec = {"key": key, "tagging": a.get("--tagging"), "t": time.monotonic(), "ok": False,
                   "trainer_done": bool(self.marker_dir and (Path(self.marker_dir) / "trainer.done").exists())}
            self.calls.append(rec)
            if self.delay:
                time.sleep(min(self.delay, timeout))
                if self.delay > timeout:
                    raise TimeoutError("fake runner timed out")
            if self.fail and self.fail(key):
                raise RuntimeError("fake runner failure")
            dst = self.bucket / key
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(body, dst)
            rec["ok"] = True

    class Env:
        def __init__(self, **kv):
            self.kv, self.old = kv, {}

        def __enter__(self):
            for k, v in self.kv.items():
                self.old[k] = os.environ.get(k)
                os.environ[k] = v

        def __exit__(self, *exc):
            for k, v in self.old.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v

    def make_case(tmp, name, mode, shift=False, epochs="", extra_files=None, **runner_kw):
        d = Path(tmp) / name
        chan, bucket, marks = d / "channel", d / "bucket", d / "marks"
        for p in (chan, bucket, marks):
            p.mkdir(parents=True)
        (chan / "task.json").write_bytes(b'{"task":"t"}\n')
        (chan / "train.jsonl").write_bytes(b'{"text":"a","label":"x"}\n')
        (chan / "eval.jsonl").write_bytes(b'{"text":"b","label":"x"}\n')
        if shift:
            (chan / "shift.jsonl").write_bytes(b'{"text":"c","label":"x"}\n')
        for k, v in (extra_files or {}).items():
            (chan / k).write_bytes(v)
        hashes = channel_hashes(chan)
        expected = ";".join("%s=%s" % (n, hashes[n]) for n in INPUT_FILES if n in hashes)
        env = {"LAYA_TASK_ID": task, "LAYA_NAME": "motiv-1", "LAYA_BUCKET": "test-bucket",
               "LAYA_RUN_PREFIX": "runs/motiv-1/%s/" % task, "LAYA_EPOCHS": epochs,
               "LAYA_TRAIN_IMAGE_DIGEST": digest, "LAYA_EXPECTED_INPUTS": expected}
        trainer = d / "fake_trainer.py"
        trainer.write_text(FAKE_TRAINER)
        paths = Paths(data_channel=chan, base_tar=d / "none.tar", work=d / "work")
        knobs = Knobs(trainer=trainer, backoff=(0.0,), echo=False)
        runner = FakeRunner(bucket / "b", marker_dir=marks, **runner_kw)
        return {"env": env, "paths": paths, "knobs": knobs, "runner": runner, "bucket": bucket / "b",
                "marks": marks, "chan": chan, "dir": d, "mode": mode}

    def run_case(c, quiet_err=True):
        err = io.StringIO()
        with Env(FAKE_MODE=c["mode"], FAKE_MARK=str(c["marks"])), contextlib.redirect_stderr(err):
            t0 = time.monotonic()
            rc = entry_train(c["env"], c["paths"], c["runner"], c["knobs"])
            c["seconds"] = time.monotonic() - t0
        c["stderr"] = err.getvalue()
        return rc

    def bucket_file(c, rel):
        return c["bucket"] / ("runs/motiv-1/%s/%s" % (task, rel))

    with tempfile.TemporaryDirectory(prefix="run-step-selftest-") as tmp:
        print("pure functions:")
        ok_inputs = ";".join("%s=%s" % (n, "%x" % (i + 10) * 64) for i, n in enumerate(INPUT_FILES))
        try:
            parsed = parse_inputs_string(ok_inputs)
            case("parse_inputs_string accepts the canonical string", set(parsed) == set(INPUT_FILES))
        except ValueError as e:
            case("parse_inputs_string accepts the canonical string", False, str(e))

        def bad_inputs(s):
            try:
                parse_inputs_string(s)
                return False
            except ValueError:
                return True
        h = lambda ch: ch * 64                                                       # noqa: E731
        base3 = "task.json=%s;train.jsonl=%s;eval.jsonl=%s" % (h("a"), h("b"), h("c"))
        case("parse_inputs_string refuses empty, missing, order, unknown, uppercase",
             all(bad_inputs(s) for s in ("", "task.json=%s;train.jsonl=%s" % (h("a"), h("b")),
                                          "train.jsonl=%s;task.json=%s;eval.jsonl=%s" % (h("b"), h("a"), h("c")),
                                          base3 + ";other.json=" + h("d"),
                                          "task.json=%s;train.jsonl=%s;eval.jsonl=%s" % (h("A"), h("b"), h("c")))))
        case("classify: 0 pass, 3 gate_fail, 2 refused, 7/-9/None crash",
             [classify(x) for x in (0, 3, 2, 7, -9, None)] == ["pass", "gate_fail", "refused", "crash", "crash", "crash"])
        case("job_exit_code: pass and gate_fail exit 0, the rest 1",
             [job_exit_code(x) for x in ("pass", "gate_fail", "refused", "crash", "timeout")] == [0, 0, 1, 1, 1])
        case("trainer argv shape", trainer_argv("py", "t.py", "/w", "") ==
             ["py", "t.py", "--data", "/w/data", "--out", "/w/run", "--device", "cuda"])

        print("train-pass:")
        c = make_case(tmp, "pass", "pass")
        rc = run_case(c)
        calls = c["runner"].calls
        out = json.loads(bucket_file(c, "outcome.json").read_text())
        case("train-pass exit 0", rc == 0, "rc=%s stderr=%s" % (rc, c["stderr"][-120:]))
        case("train-pass outcome exit_class pass, device_ok true, device cuda:0",
             out["exit_class"] == "pass" and out["device_ok"] is True and out["device_used"] == "cuda:0"
             and out["train_rc"] == 0 and out["refused_line"] is None, str(out)[:100])
        want = {"run/gate-report.json", "run/recipe.json", "run/argv.json", "run/checkpoint/model.safetensors",
                "data/task.json", "data/train.jsonl", "data/eval.jsonl", "train.log"}
        got = {x["key"].split(task + "/", 1)[1] for x in calls if x["ok"]}
        case("train-pass every file under run/ and data/ and train.log uploaded (checkpoint included)",
             want <= got, "missing %s" % sorted(want - got))
        case("train-pass every upload carries --tagging laya-retention=expire",
             bool(calls) and all(x["tagging"] == "laya-retention=expire" for x in calls))
        man = json.loads(bucket_file(c, "training-manifest.json").read_text())
        bad = [e["path"] for e in man["entries"] if e["path"] != "outcome.json" and
               (sha256_file(bucket_file(c, e["path"])) != e["sha256"]
                or bucket_file(c, e["path"]).stat().st_size != e["size"])]
        paths_in = {e["path"] for e in man["entries"]}
        case("train-pass manifest lists run/, data/ and train.log with hashlib's sha256 and sizes",
             not bad and want <= paths_in and "outcome.json" in paths_in, "mismatch %s" % bad)
        case("train-pass outcome.json is the last upload and the manifest the one before",
             calls[-1]["key"].endswith("/outcome.json") and calls[-2]["key"].endswith("/training-manifest.json"))
        case("train-pass outcome.json entry hash equals the uploaded outcome",
             any(e["path"] == "outcome.json" and e["sha256"] == sha256_file(bucket_file(c, "outcome.json"))
                 for e in man["entries"]))

        print("exit mapping:")
        c = make_case(tmp, "gate3", "gate3")
        rc = run_case(c)
        out = json.loads(bucket_file(c, "outcome.json").read_text())
        case("gate_fail: trainer exit 3 -> exit_class gate_fail, job exit 0",
             rc == 0 and out["exit_class"] == "gate_fail" and out["train_rc"] == 3, "rc=%s %s" % (rc, out["exit_class"]))
        c = make_case(tmp, "refused", "refused")
        rc = run_case(c)
        out = json.loads(bucket_file(c, "outcome.json").read_text())
        case("refused: trainer exit 2 -> refused, job exit 1, refused_line is the first REFUSED line",
             rc == 1 and out["exit_class"] == "refused" and out["refused_line"] == "REFUSED data: fake refusal"
             and out["device_ok"] is False, "rc=%s %s" % (rc, out["refused_line"]))
        c = make_case(tmp, "rc7", "rc7")
        rc = run_case(c)
        out = json.loads(bucket_file(c, "outcome.json").read_text())
        case("crash: trainer exit 7 -> crash, job exit 1, train_rc 7",
             rc == 1 and out["exit_class"] == "crash" and out["train_rc"] == 7)
        c = make_case(tmp, "sigkill", "sigkill")
        rc = run_case(c)
        out = json.loads(bucket_file(c, "outcome.json").read_text())
        case("crash: a SIGKILL'd trainer -> crash, job exit 1, train_rc null",
             rc == 1 and out["exit_class"] == "crash" and out["train_rc"] is None)

        print("cpu-fallback:")
        c = make_case(tmp, "cpu", "cpu")
        rc = run_case(c)
        out = json.loads(bucket_file(c, "outcome.json").read_text())
        case("cpu-fallback: the trainer is killed within 5 s of `used=cpu`", c["seconds"] < 5.0,
             "%.2f s" % c["seconds"])
        case("cpu-fallback: exit_class crash, device_ok false, device_used cpu, job exit 1",
             rc == 1 and out["exit_class"] == "crash" and out["device_ok"] is False and out["device_used"] == "cpu")

        print("argv:")
        c = make_case(tmp, "argv12", "pass", epochs="12")
        run_case(c)
        argv = json.loads(bucket_file(c, "run/argv.json").read_text())
        case("argv: LAYA_EPOCHS 12 -> the trainer argv ends --device cuda --epochs 12", argv[-4:] == ["--device", "cuda", "--epochs", "12"], str(argv))
        c = make_case(tmp, "argv0", "pass", epochs="")
        run_case(c)
        argv = json.loads(bucket_file(c, "run/argv.json").read_text())
        case("argv: LAYA_EPOCHS empty -> no --epochs", "--epochs" not in argv and argv[-2:] == ["--device", "cuda"])
        banned = ("--seeds", "--stopping", "--base", "--base-sha256", "--variant")
        case("argv: never contains --seeds, --stopping, --base or --variant", not any(b in argv for b in banned))

        print("keys:")
        c = make_case(tmp, "keys", "pass")
        run_case(c)
        case("keys: written outcome.json has exactly the golden fixture's key set",
             set(json.loads(bucket_file(c, "outcome.json").read_text())) == outcome_keys)
        case("keys: written training-manifest.json has exactly the golden fixture's key set",
             set(json.loads(bucket_file(c, "training-manifest.json").read_text())) == manifest_keys)
        man = json.loads(bucket_file(c, "training-manifest.json").read_text())
        case("partial-manifest: the written manifest is complete true with the fixture's entry shape",
             man["complete"] is True and all(set(e) == {"path", "size", "sha256"} for e in man["entries"]))

        print("env refusals:")
        good = make_case(tmp, "envgood", "pass")["env"]

        def refuses(label, mutate):
            c = make_case(tmp, "env-" + re.sub(r"\W", "", label), "pass")
            env = dict(c["env"])
            mutate(env)
            c["env"] = env
            rc = run_case(c)
            started = (c["marks"] / "trainer.started").exists()
            case("env refusal: %s -> REFUSED env:, exit 2, trainer not started, no upload" % label,
                 rc == 2 and "REFUSED env:" in c["stderr"] and not started and not c["runner"].calls
                 and digest not in c["stderr"], c["stderr"].strip()[:90])
        refuses("missing LAYA_TASK_ID", lambda e: e.pop("LAYA_TASK_ID"))
        refuses("missing LAYA_RUN_PREFIX", lambda e: e.pop("LAYA_RUN_PREFIX"))
        refuses("LAYA_RUN_PREFIX not matching name and task", lambda e: e.update(LAYA_RUN_PREFIX="runs/other/%s/" % task))
        refuses("LAYA_RUN_PREFIX with a path escape", lambda e: e.update(LAYA_RUN_PREFIX="runs/motiv-1/../x/"))
        refuses("non-numeric LAYA_EPOCHS", lambda e: e.update(LAYA_EPOCHS="twelve"))
        refuses("missing LAYA_TRAIN_IMAGE_DIGEST", lambda e: e.pop("LAYA_TRAIN_IMAGE_DIGEST"))
        refuses("malformed LAYA_TRAIN_IMAGE_DIGEST", lambda e: e.update(LAYA_TRAIN_IMAGE_DIGEST="sha256:" + "AB" * 32))
        refuses("missing LAYA_EXPECTED_INPUTS", lambda e: e.pop("LAYA_EXPECTED_INPUTS"))
        refuses("unparseable LAYA_EXPECTED_INPUTS", lambda e: e.update(LAYA_EXPECTED_INPUTS="task.json=nothex"))
        case("env refusal: the good env parses", parse_env(good).task_id == task)

        print("image-digest:")
        c = make_case(tmp, "digest", "pass")
        run_case(c)
        man = json.loads(bucket_file(c, "training-manifest.json").read_text())
        case("image-digest: LAYA_TRAIN_IMAGE_DIGEST is written verbatim as train_image_digest",
             man["train_image_digest"] == digest)
        case("image-digest: gate_contract_sha256 is hashlib's sha256 of contracts/laya-finetune-gate-v1.yaml",
             man["gate_contract_sha256"] == hashlib.sha256(GATE_CONTRACT.read_bytes()).hexdigest())

        print("inputs-changed:")
        c = make_case(tmp, "inputs-ok", "pass", shift=True)
        rc = run_case(c)
        case("inputs-changed: the real hashes (shift.jsonl included) let the trainer run",
             rc == 0 and (c["marks"] / "trainer.started").exists())
        c = make_case(tmp, "inputs-byte", "pass")
        (c["chan"] / "train.jsonl").write_bytes(b'{"text":"a","label":"y"}\n')
        rc = run_case(c)
        out = json.loads(bucket_file(c, "outcome.json").read_text())
        case("inputs-changed: one changed byte in train.jsonl -> refused, REFUSED inputs-changed: train.jsonl, exit 1",
             rc == 1 and out["exit_class"] == "refused" and out["refused_line"] == "REFUSED inputs-changed: train.jsonl",
             str(out["refused_line"]))
        case("inputs-changed: the trainer never started", not (c["marks"] / "trainer.started").exists())
        case("inputs-changed: the refusal also reaches train.log and the manifest is complete",
             "REFUSED inputs-changed: train.jsonl" in bucket_file(c, "train.log").read_text()
             and json.loads(bucket_file(c, "training-manifest.json").read_text())["complete"] is True)
        c = make_case(tmp, "inputs-extra", "pass")
        (c["chan"] / "shift.jsonl").write_bytes(b'{"text":"c","label":"x"}\n')
        rc = run_case(c)
        out = json.loads(bucket_file(c, "outcome.json").read_text())
        case("inputs-changed: a shift.jsonl in the channel but not in the expected set -> names shift.jsonl",
             rc == 1 and out["refused_line"] == "REFUSED inputs-changed: shift.jsonl"
             and not (c["marks"] / "trainer.started").exists())
        c = make_case(tmp, "inputs-missing", "pass", shift=True)
        (c["chan"] / "shift.jsonl").unlink()
        rc = run_case(c)
        out = json.loads(bucket_file(c, "outcome.json").read_text())
        case("inputs-changed: a shift.jsonl expected but absent from the channel -> names shift.jsonl",
             rc == 1 and out["refused_line"] == "REFUSED inputs-changed: shift.jsonl")

        print("no network:")
        case("the default aws Runner was never called", DEFAULT_RUNNER_CALLS == 0, "calls=%d" % DEFAULT_RUNNER_CALLS)

    if failures:
        print("RUN STEP SELFTEST FAILED: %s" % ", ".join(failures))
        return 1
    print("RUN STEP SELFTEST OK")
    return 0


# ------------------------------------------------------------------------------------------ entry

def main(argv):
    if argv[1:] == ["--selftest"]:
        return selftest()
    if len(argv) != 2 or argv[1] != "train":
        print("usage: run_step.py train | --selftest", file=sys.stderr)
        return 2
    step = os.environ.get("LAYA_STEP", "") or "train"
    if step != "train":
        print("REFUSED env: LAYA_STEP %r is not a mode of this image" % step, file=sys.stderr, flush=True)
        return 2
    return entry_train(os.environ, Paths(), aws_runner, Knobs())


if __name__ == "__main__":
    sys.exit(main(sys.argv))
