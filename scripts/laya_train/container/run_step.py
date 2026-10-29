#!/usr/bin/env python3
"""The training container's one program: SageMaker runs `run_step.py train` and nothing else (plan 14-06); the
arm64 pack image runs `run_step.py pack` (plan 14-07, the PACK RULES below).

    run_step.py train            (SageMaker's single argument; the effective mode is env LAYA_STEP, default train)
    run_step.py pack             (the pack image's CMD; the effective mode is env LAYA_STEP, default the argument)
    run_step.py --selftest       (offline; a fake trainer and a fake S3 runner; prints RUN STEP SELFTEST OK)

DECLARED RULES (written before the code; the selftest asserts each one):

    mode      LAYA_STEP is `train` (default), `probe`, `probe-sigterm` or `pack`; anything else is `REFUSED env:` (exit 2).
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

PACK RULES (plan 14-07; D-14, D-15, D-04, D-19; the selftest asserts each one):

    where     pack and verify run on linux/arm64 only, in the pack image, with `pack_laya` (built as an example) at
              /src/target/release/examples/pack_laya and its policy at /src/contracts (its compile-time path).
    policy    THE POLICY IS NOT AN ARGUMENT: the child is `pack_laya pack --run R --data D --base B --out X.apr`, then
              `pack_laya verify X.apr --run R --data D --base B`, path arguments only, and the child environment is
              stripped to PATH, HOME, LANG and TMPDIR. The binary and contracts paths are constants; only a selftest
              function parameter can change them (no env var or argument does).
    env       LAYA_TASK_ID, LAYA_NAME, LAYA_BUCKET, LAYA_RUN_PREFIX as in train mode, plus LAYA_PACK_IMAGE_DIGEST
              (`sha256:` + 64 lowercase hex), written as pack-outcome.json pack_image_digest on every recorded outcome.
              Missing or malformed is `REFUSED env:` (exit 2) before any download.
    skip      pack runs for outcome `pass`, and for `gate_fail` only when the run's gate-report.json (downloaded and
              re-hashed first) carries a `pilot_waiver` claim (train.py never lets the claim change `pass` or the exit
              status; pack and verify decide). Anything else (gate_fail without a claim, refused, crash, timeout, or
              device_ok false) is recorded `{ran false, skipped <exit_class>}`, exit 0, with no base download.
    inputs    exactly the training-manifest entries under run/ and data/ are downloaded and re-hashed (a mismatch is
              exit 1 `integrity`); the base tar is checked against base-stage.json before safe_extract.
    accept    accepted = pack exit 0 AND verify exit 0 AND deploy_eligible true AND the PACKED sha256 equals the local
              hash of the written file AND PACKED `waived=` agrees with the verify JSON `waived`. Only then is the
              .apr uploaded. Every other way of ending is `accepted false`, no .apr, the reports untouched, exit 0.
    upload    .apr (accepted only), pack.log, verify.log, pack-manifest.json, pack-outcome.json LAST, each tagged
              laya-retention=expire; an upload failure or an infrastructure error (download, integrity, base pin) is
              exit 1.

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
from pathlib import Path, PurePosixPath

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

# D-12: ONE absolute deadline after SIGTERM. t0 = the signal; SIGTERM to the trainer's group at t0, SIGKILL at
# t0 + CHILD_GRACE_S; small-file uploads stop at t0 + SHUTDOWN_DEADLINE_S - FINAL_RESERVE_S; partial-manifest.json
# and outcome.json {timeout} take the reserved last seconds; the process exits by t0 + SHUTDOWN_DEADLINE_S.
# 25 s is [ASSUMED] to fit SageMaker's stop grace; plan 14-12's probe-sigterm measures the real one.
SHUTDOWN_DEADLINE_S = 25
CHILD_GRACE_S = 5
FINAL_RESERVE_S = 5
SYNC_INTERVAL_S = 300                                # periodic upload of changed small files + partial-manifest
GPU_SAMPLE_INTERVAL_S = 30
HEARTBEAT_INTERVAL_S = 2                             # probe-sigterm
PROBE_TRAIN_STEP_TOKENS = 1024                       # above the decide servers' 800 built-token cap: an upper bound
PROBE_ID_RE = re.compile(r"^[a-z0-9][a-z0-9-]{0,47}$")

# D-02 / D-13: torch 2.14 Linux wheels are CUDA 13; AWS documents driver 580 for CUDA 13 and the compat-package
# switch for older host drivers, so the compat libraries go on LD_LIBRARY_PATH only below this major.
COMPAT_BELOW_MAJOR = 580
COMPAT_DIR = "/usr/local/cuda/compat"
PROC_NVIDIA_VERSION = Path("/proc/driver/nvidia/version")
SMI_VERSION_CMD = ["nvidia-smi", "--query-gpu=driver_version", "--format=csv,noheader"]
SMI_SAMPLE_CMD = ["nvidia-smi", "--query-gpu=memory.used,utilization.gpu", "--format=csv,noheader,nounits"]
DRIVER_RE = re.compile(r"\b(\d{3,4}\.\d+(?:\.\d+)?)\b")

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


def small_files(work):
    """list_files() restricted to what a mid-run sync or a SIGTERM shutdown may upload: at most 5 MiB and never
    anything under a checkpoint directory (the checkpoints are uploaded only by the normal finish)."""
    out = []
    for rel, path in list_files(work):
        try:
            if path.stat().st_size <= SMALL_FILE_BYTES and "checkpoint" not in rel.split("/"):
                out.append((rel, path))
        except OSError:
            pass
    return out


def shutdown_priority(rel):
    """train.log first, then the reports (gate-report / recipe / per-seed JSON), then the rest."""
    if rel == "train.log":
        return 0
    if rel.startswith("run/") and rel.endswith(".json"):
        return 1
    return 2


def parse_driver(text):
    m = DRIVER_RE.search(text or "")
    return m.group(1) if m else None


def host_driver(proc_path=PROC_NVIDIA_VERSION, smi_cmd=None):
    """The host NVIDIA driver version string: /proc/driver/nvidia/version first, `nvidia-smi` second, else "unknown"."""
    try:
        drv = parse_driver(Path(proc_path).read_text(errors="replace"))
        if drv:
            return drv
    except OSError:
        pass
    try:
        r = subprocess.run(list(smi_cmd or SMI_VERSION_CMD), capture_output=True, text=True, timeout=15)
        drv = parse_driver(r.stdout) if r.returncode == 0 else None
        if drv:
            return drv
    except (OSError, subprocess.SubprocessError):
        pass
    return "unknown"


def decide_compat(driver):
    """True only when the driver's major version is known and below COMPAT_BELOW_MAJOR."""
    m = re.match(r"^(\d+)\.", driver or "")
    return bool(m) and int(m.group(1)) < COMPAT_BELOW_MAJOR


def with_compat(env, compat_used):
    """env with /usr/local/cuda/compat first on LD_LIBRARY_PATH when compat_used; otherwise a plain copy."""
    env = dict(env)
    if compat_used:
        old = env.get("LD_LIBRARY_PATH", "")
        env["LD_LIBRARY_PATH"] = COMPAT_DIR + (":" + old if old else "")
    return env


def stage_channel_base(base_tar, pin_path, hf_home):
    """The `base` channel's laya-base.tar -> HF_HOME/hub. Its sha256 must equal base-stage.json's pin BEFORE anything is
    extracted (T-14-21), then stage_base.safe_extract refuses absolute names, `..` members, hardlinks and links that
    resolve outside the destination. Offline from here on (HF_HUB_OFFLINE=1 in the child's env): a missing cache can
    never silently download the base. Raises on any failure; returns hf_home."""
    sys.path.insert(0, str(HERE))
    import stage_base
    pin = json.loads(Path(pin_path).read_text(encoding="utf-8"))
    if pin.get("schema") != "laya-base-stage-v1" or not HEX64_RE.match(str(pin.get("tar_sha256", ""))):
        raise RuntimeError("base-stage.json is not a laya-base-stage-v1 pin")
    got = sha256_file(base_tar)
    if got != pin["tar_sha256"]:
        raise RuntimeError("the base channel tar's sha256 does not match base-stage.json (refusing to extract)")
    n = stage_base.safe_extract(base_tar, Path(hf_home) / "hub")
    say("base staged: %d entries under %s" % (n, Path(hf_home) / "hub"))
    return Path(hf_home)


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
        self.rc = None
        self.exited = threading.Event()

    def start(self):
        self.started = time.monotonic()
        self.proc = subprocess.Popen(self.argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=self.env,
                                     start_new_session=True)
        self.pump = threading.Thread(target=self._pump, daemon=True)
        self.pump.start()
        threading.Thread(target=self._reap, daemon=True).start()

    def _reap(self):
        # the ONLY place that waits on the Popen: a SIGTERM handler that ran Popen.wait/poll would find
        # Popen's waitpid lock held by the interrupted frame and see a dead trainer as alive, so everything
        # else (the main wait, the handler) goes through `exited`.
        self.rc = self.proc.wait()
        self.exited.set()

    def kill_group(self, sig=signal.SIGKILL):
        if self.exited.is_set():
            return
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
        self.exited.wait()
        self.pump.join(10)
        return self.rc

    def seconds(self):
        return time.monotonic() - self.started if self.started is not None else 0.0


class Sampler(threading.Thread):
    """Samples `nvidia-smi --query-gpu=memory.used,utilization.gpu` every `interval` s into <work>/gpu-samples.json
    (rewritten atomically, so a mid-run sync never reads half a file)."""

    def __init__(self, work, cmd, interval):
        super().__init__(daemon=True)
        self.path, self.cmd, self.interval = Path(work) / "gpu-samples.json", list(cmd), interval
        self.stop = threading.Event()
        self.samples = []
        self.peak = None

    def sample(self):
        row = {"t": now_iso()}
        try:
            r = subprocess.run(self.cmd, capture_output=True, text=True, timeout=10)
            if r.returncode != 0:
                raise RuntimeError("exit %d" % r.returncode)
            mem, util = [int(x.strip()) for x in r.stdout.strip().splitlines()[0].split(",")[:2]]
            row.update({"memory_used_mib": mem, "utilization_pct": util})
            self.peak = mem if self.peak is None else max(self.peak, mem)
        except Exception as e:                           # a missing or failing nvidia-smi is a recorded gap
            row["error"] = "%s: %s" % (type(e).__name__, str(e)[:100])
        self.samples = (self.samples + [row])[-4000:]
        self.write()

    def write(self):
        obj = {"schema": "laya-train-gpu-samples-v1", "interval_s": self.interval, "samples": self.samples,
               "peak_memory_used_mib": self.peak}
        tmp = self.path.with_name(self.path.name + ".tmp")
        tmp.write_text(json.dumps(obj, indent=2) + "\n", encoding="utf-8")
        os.replace(tmp, self.path)

    def run(self):
        while True:
            self.sample()
            if self.stop.wait(self.interval):
                break

    def finish(self):
        self.stop.set()
        self.join(15)


class Syncer(threading.Thread):
    """Every `interval` s: upload the changed small files, THEN partial-manifest.json (complete false) listing
    every file uploaded so far, each only after its own upload returned success. A run killed hard still leaves
    recipe.json, the per-seed files and an inventory that names them."""

    def __init__(self, run, interval):
        super().__init__(daemon=True)
        self.run_, self.interval = run, interval
        self.stop = threading.Event()

    def sync_once(self):
        up = self.run_.uploader
        sent = False
        for rel, path in small_files(self.run_.work):
            if self.stop.is_set():
                return
            sent |= up.put(rel, path, attempts=2, timeout=60.0) == "uploaded"
        if sent and not self.stop.is_set():
            self.run_.put_partial_manifest(attempts=2, timeout=60.0)

    def run(self):
        while not self.stop.wait(self.interval):
            self.sync_once()

    def finish(self):
        self.stop.set()
        self.join(180)


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
    sync_interval: float = SYNC_INTERVAL_S
    sample_interval: float = GPU_SAMPLE_INTERVAL_S
    sampler_cmd: tuple = tuple(SMI_SAMPLE_CMD)
    deadline: float = SHUTDOWN_DEADLINE_S
    grace: float = CHILD_GRACE_S
    reserve: float = FINAL_RESERVE_S
    proc_version: Path = PROC_NVIDIA_VERSION
    smi_version_cmd: tuple = tuple(SMI_VERSION_CMD)
    install_signals: bool = True                      # the selftest runs in-process with False


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
        self.sampler = None
        self.syncer = None
        self.shutting_down = False
        self.completed = False                           # outcome.json is uploaded: a later SIGTERM must not replace it
        self.exit_code = 1
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
        if self.knobs.install_signals:
            signal.signal(signal.SIGTERM, self.on_sigterm)
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
        driver = host_driver(self.knobs.proc_version, self.knobs.smi_version_cmd)
        compat = decide_compat(driver)
        env = with_compat(child_env(hf_home), compat)
        self.run_env.update({"driver": driver, "compat_used": compat,
                             "compat_ld_library_path": env["LD_LIBRARY_PATH"] if compat else None})
        self.write_run_env()
        self.sampler = Sampler(self.work, self.knobs.sampler_cmd, self.knobs.sample_interval)
        self.sampler.start()
        self.syncer = Syncer(self, self.knobs.sync_interval)
        self.syncer.start()
        argv = trainer_argv(self.knobs.python, self.knobs.trainer, self.work, self.cfg.epochs)
        self.trainer = Trainer(argv, env, self.log_path, self.knobs.echo)
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
        if self.syncer is not None:
            self.syncer.finish()
        if self.sampler is not None:
            self.sampler.finish()
            self.run_env["peak_gpu_memory_mib"] = self.sampler.peak
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
        sent = up.put("outcome.json", out_path, attempts=k.attempts, timeout=k.upload_timeout, record=False)
        failed = dict(up.failed)
        if failed:
            say("upload failures: %s" % ", ".join("%s (%s)" % (p, e) for p, e in sorted(failed.items())))
        code = job_exit_code(exit_class)
        code = 1 if failed and code == 0 else code
        self.exit_code, self.completed = code, sent != "failed"
        return code

    # ------------------------------------------------------------------ partial inventory and SIGTERM
    def put_partial_manifest(self, attempts, timeout, deadline=None):
        """partial-manifest.json: a TrainingManifest with complete false listing exactly the files uploaded so far."""
        path = self.work / "partial-manifest.json"
        write_json(path, manifest_obj(self.cfg, self.gate_sha, False, self.uploader.entries()))
        return self.uploader.put("partial-manifest.json", path, attempts=attempts, timeout=timeout,
                                 deadline=deadline, record=False)

    def on_sigterm(self, signum, frame):
        """SIGTERM (MaxRuntime or a stop): do the whole bounded shutdown here, in the main thread, then exit. The
        deadline is anchored to the moment the handler starts."""
        t0 = time.monotonic()
        if self.shutting_down:
            return
        self.shutting_down = True
        code = self.exit_code if self.completed else self.shutdown(t0)
        sys.stdout.flush()
        sys.stderr.flush()
        os._exit(code)

    def shutdown(self, t0):
        k = self.knobs
        end = t0 + k.deadline
        uploads_stop = end - k.reserve
        for th in (self.syncer, self.sampler):
            if th is not None:
                th.stop.set()
        tr = self.trainer
        if tr is not None and tr.proc is not None and not tr.exited.is_set():
            tr.kill_group(signal.SIGTERM)
            if not tr.exited.wait(max(0.0, t0 + k.grace - time.monotonic())):
                tr.kill_group(signal.SIGKILL)
                tr.exited.wait(max(0.1, min(1.0, uploads_stop - time.monotonic())))
            tr.pump.join(0.5)
        up = self.uploader
        for rel, path in sorted(small_files(self.work), key=lambda rp: shutdown_priority(rp[0])):
            if time.monotonic() >= uploads_stop - MIN_CALL_S:
                break
            up.put(rel, path, attempts=1, timeout=10.0, deadline=uploads_stop)
        log_dev = tr.devices[0] if tr is not None and tr.devices else None
        dev = read_gate_device(self.work / "run") or log_dev
        self.run_env["peak_gpu_memory_mib"] = self.sampler.peak if self.sampler is not None else None
        self.write_run_env()
        outcome = {"schema": SCHEMA_OUTCOME, "task_id": self.cfg.task_id, "exit_class": "timeout", "train_rc": None,
                   "device_used": dev or "none", "device_ok": bool(dev and dev.startswith("cuda")),
                   "refused_line": None, "started_at": self.started_at, "finished_at": now_iso(),
                   "train_seconds": round(tr.seconds(), 1) if tr is not None else 0.0}
        out_path = self.work / "outcome.json"
        write_json(out_path, outcome)
        # the reserve is shared: the inventory may use its first half, outcome.json (the completion marker) is
        # always left the rest, so a slow runner cannot starve the marker
        self.put_partial_manifest(attempts=1, timeout=10.0, deadline=end - k.reserve / 2)
        up.put("outcome.json", out_path, attempts=1, timeout=10.0, deadline=end, record=False)
        return 1


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


# ------------------------------------------------------------------------------------------ probe modes

PROBE_KEYS = ("schema", "driver", "nvidia_smi", "compat_used", "torch_cuda_available", "torch_version_cuda",
              "device_name", "matmul_ok", "base_load_ok", "base_forward_seconds", "max_memory_allocated_bytes",
              "train_step_ok", "train_step_seconds", "train_step_peak_bytes", "host_rss_peak_bytes",
              "train_step_batch", "train_step_tokens", "tiny_fixture_rc", "tiny_fixture_device_used", "finished_at")
HELPER_KEYS = ("torch_cuda_available", "torch_version_cuda", "device_name", "matmul_ok", "base_load_ok",
               "base_forward_seconds", "max_memory_allocated_bytes", "train_step_ok", "train_step_seconds",
               "train_step_peak_bytes", "host_rss_peak_bytes", "train_step_batch", "train_step_tokens")
BOOL_PROBE_KEYS = ("torch_cuda_available", "matmul_ok", "base_load_ok", "train_step_ok")

# The CUDA half of the probe, run by the venv interpreter (`python -c`): torch availability, one matmul, the real
# base loaded OFFLINE through train.py's own loader, one forward, then ONE bounded real-model training step
# (forward, backward, one AdamW step over the trainable parameters on a synthetic batch). A memory and kernel
# check only, never a training numeric. It prints one `PROBE_JSON {...}` line and always exits 0: a failure is a
# false field plus an `error`, because the finding is the record.
PROBE_HELPER_PY = r"""
import argparse, json, os, resource, sys, time
ap = argparse.ArgumentParser()
ap.add_argument("--device", default="cuda")
ap.add_argument("--matmul-n", type=int, default=4096)
ap.add_argument("--batch", type=int, default=0)
ap.add_argument("--tokens", type=int, default=1024)
ap.add_argument("--base-dir", default="")
ap.add_argument("--base-sha256", default="")
ap.add_argument("--repo", required=True)
a = ap.parse_args()
res = {}
def emit(**kv):
    res.update(kv)
    print("PROBE_JSON " + json.dumps(res), flush=True)
def rss():
    r = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return r * 1024 if sys.platform.startswith("linux") else r
try:
    import torch
    dev = torch.device(a.device)
    cuda = dev.type == "cuda"
    res["torch_cuda_available"] = bool(torch.cuda.is_available())
    res["torch_version_cuda"] = torch.version.cuda
    if cuda and not torch.cuda.is_available():
        emit(matmul_ok=False, error="cuda requested but torch.cuda.is_available() is false")
        sys.exit(0)
    res["device_name"] = torch.cuda.get_device_name(0) if cuda else a.device
    x = torch.randn(a.matmul_n, a.matmul_n, device=dev)
    y = x @ x
    if cuda:
        torch.cuda.synchronize()
    res["matmul_ok"] = bool(torch.isfinite(y).all().item())
    del x, y
except SystemExit:
    raise
except BaseException as e:
    emit(matmul_ok=False, error="matmul: %s: %s" % (type(e).__name__, str(e)[:300]))
    sys.exit(0)
sys.path.insert(0, os.path.join(a.repo, "scripts", "laya_train"))
k = 3
try:
    import contract, train
    from laya.common import QTYPES
    if a.base_dir:
        src, rev, digest = a.base_dir, None, a.base_sha256
    else:
        b = contract.base()
        src, rev, digest = b["repo"], b["revision"], b["model_safetensors_sha256"]
    batch = a.batch or contract.recipe_number("batch_size", "int")
    if cuda:
        torch.cuda.reset_peak_memory_stats()
    agent, used = train.load_agent(src, a.device, digest, rev)
    if cuda and not used.startswith("cuda"):
        raise RuntimeError("the base landed on %s, not cuda" % used)
    model, dv = agent.model, agent.device
    def tensors(n, length):
        ids = torch.randint(5, max(8, len(agent.tok)), (n, length), device=dv)
        att = torch.ones((n, length), dtype=torch.long, device=dv)
        mpos = torch.tensor([[1, 2, 3]] * n, dtype=torch.long, device=dv)
        mm = torch.ones((n, k), dtype=torch.bool, device=dv)
        qt = torch.full((n,), QTYPES["choice"], dtype=torch.long, device=dv)
        return ids, att, mpos, mm, qt
    model.eval()
    t = time.time()
    with torch.no_grad():
        z, _ = model(*tensors(1, 64))
    if cuda:
        torch.cuda.synchronize()
    res["base_forward_seconds"] = round(time.time() - t, 4)
    res["base_load_ok"] = bool(torch.isfinite(z.float()).all().item())
    res["max_memory_allocated_bytes"] = int(torch.cuda.max_memory_allocated()) if cuda else 0
except BaseException as e:
    emit(base_load_ok=False, error="base: %s: %s" % (type(e).__name__, str(e)[:300]))
    sys.exit(0)
try:
    model.train()
    params = [p for p in model.parameters() if p.requires_grad]
    opt = torch.optim.AdamW(params, lr=1e-6)
    if cuda:
        torch.cuda.reset_peak_memory_stats()
    t = time.time()
    z, act = model(*tensors(batch, a.tokens))
    loss = z[:, :k].float().logsumexp(-1).mean() + 0.0 * act.float().sum()
    opt.zero_grad()
    loss.backward()
    opt.step()
    if cuda:
        torch.cuda.synchronize()
    res.update(train_step_ok=bool(torch.isfinite(loss).item()), train_step_seconds=round(time.time() - t, 3),
               train_step_peak_bytes=int(torch.cuda.max_memory_allocated()) if cuda else 0,
               train_step_batch=batch, train_step_tokens=a.tokens, host_rss_peak_bytes=rss())
except BaseException as e:
    res.update(train_step_ok=False, train_step_batch=batch, train_step_tokens=a.tokens,
               train_step_peak_bytes=int(torch.cuda.max_memory_allocated()) if cuda else 0,
               host_rss_peak_bytes=rss(), error="train step: %s: %s" % (type(e).__name__, str(e)[:300]))
emit()
"""


def run_text(cmd, timeout=60, env=None):
    """(returncode, merged output text); a command that cannot run is (None, the error)."""
    try:
        r = subprocess.run(list(cmd), capture_output=True, text=True, timeout=timeout, env=env, errors="replace")
        return r.returncode, (r.stdout or "") + (r.stderr or "")
    except (OSError, subprocess.SubprocessError) as e:
        return None, "%s: %s" % (type(e).__name__, str(e)[:200])


def parse_probe_env(env):
    bucket, pid = env.get("LAYA_BUCKET", ""), env.get("LAYA_PROBE_ID", "")
    if not BUCKET_RE.match(bucket):
        raise Refusal("env: LAYA_BUCKET is required and must be a bucket name")
    if not PROBE_ID_RE.match(pid):
        raise Refusal("env: LAYA_PROBE_ID must match ^[a-z0-9][a-z0-9-]{0,47}$")
    return bucket, pid


def run_probe(bucket, probe_id, paths, runner, knobs, helper_cmd=None, tiny_cmd=None, smi_cmd=None, tiny_out=None):
    """The paid probe's answer sheet (plan 14-12): nvidia-smi, the host driver and the compat decision, the CUDA
    helper (matmul, offline real-base load and forward, ONE real-model training step) and a synthetic-fixture
    train.py run on cuda, written to probe.json and uploaded under runs/probe/<probe_id>/ with the logs."""
    work = Path(paths.work) / "probe"
    work.mkdir(parents=True, exist_ok=True)
    snap = Path(tempfile.mkdtemp(prefix="laya-probe-snap-"))
    up = Uploader(runner, bucket, "runs/probe/%s/" % probe_id, snap, knobs.backoff)
    _, smi_text = run_text(smi_cmd or ["nvidia-smi"], timeout=30)
    driver = host_driver(knobs.proc_version, knobs.smi_version_cmd)
    compat = decide_compat(driver)
    hf_home = paths.hf_home
    if knobs.prepare_base is not None:
        try:
            hf_home = knobs.prepare_base()
        except Exception as e:
            (work / "base-staging.log").write_text("BASE STAGING FAILED: %s\n" % str(e)[:300])
    env = with_compat(child_env(hf_home), compat)
    tiny = ROOT / "crates" / "aprender-decide" / "tests" / "fixtures" / "laya_tiny"
    tiny_out = Path(tiny_out) if tiny_out else work / "tiny-run"
    if helper_cmd is None:
        helper_cmd = [knobs.python, "-c", PROBE_HELPER_PY, "--device", "cuda", "--repo", str(ROOT),
                      "--tokens", str(PROBE_TRAIN_STEP_TOKENS)]
    if tiny_cmd is None:
        tiny_cmd = [knobs.python, str(knobs.trainer), "--data", str(tiny / "data"), "--out", str(tiny_out),
                    "--variant", "synthetic-fixture", "--base", str(tiny / "checkpoint"), "--base-sha256",
                    sha256_file(tiny / "checkpoint" / "model.safetensors"), "--device", "cuda", "--epochs", "1",
                    "--stopping", "fixed_epochs"]
    h_rc, h_text = run_text(helper_cmd, timeout=3600, env=env)
    helper = {}
    for line in h_text.splitlines():
        if line.startswith("PROBE_JSON "):
            try:
                helper = json.loads(line[len("PROBE_JSON "):])
            except ValueError:
                pass
    t_rc, t_text = run_text(tiny_cmd, timeout=1800, env=env)
    tiny_dev = read_gate_device(tiny_out)
    probe = {"schema": "laya-train-probe-v1", "driver": driver, "nvidia_smi": smi_text[:16384], "compat_used": compat}
    for key in HELPER_KEYS:
        probe[key] = helper.get(key, False if key in BOOL_PROBE_KEYS else None)
    probe.update({"tiny_fixture_rc": t_rc, "tiny_fixture_device_used": tiny_dev, "finished_at": now_iso()})
    probe = {key: probe[key] for key in PROBE_KEYS}
    (work / "nvidia-smi.txt").write_text(smi_text, encoding="utf-8")
    (work / "helper.log").write_text("rc=%s\n%s" % (h_rc, h_text[-200000:]), encoding="utf-8")
    (work / "tiny-fixture.log").write_text("rc=%s\n%s" % (t_rc, t_text[-200000:]), encoding="utf-8")
    write_json(work / "probe.json", probe)
    for name in ("nvidia-smi.txt", "helper.log", "tiny-fixture.log", "probe.json"):
        up.put(name, work / name, attempts=knobs.attempts, timeout=knobs.upload_timeout)
    return 0 if "probe.json" in up.uploaded else 1


def entry_probe(env, paths, runner, knobs):
    try:
        bucket, pid = parse_probe_env(env)
    except Refusal as e:
        print("REFUSED " + str(e), file=sys.stderr, flush=True)
        return 2
    return run_probe(bucket, pid, paths, runner, knobs)


def run_probe_sigterm(bucket, probe_id, paths, runner, knobs, ready_path=None):
    """Measures SageMaker's SIGTERM grace: after SIGTERM it uploads sigterm-heartbeat-<n>.json {seconds_since_sigterm}
    every 2 s until the container is killed, so the last heartbeat bounds the grace from below."""
    work = Path(paths.work) / "probe-sigterm"
    work.mkdir(parents=True, exist_ok=True)
    up = Uploader(runner, bucket, "runs/probe/%s/" % probe_id, Path(tempfile.mkdtemp(prefix="laya-hb-")), knobs.backoff)
    state = {"in_handler": False}

    def beat(n, since):
        name = "sigterm-heartbeat-%d.json" % n
        write_json(work / name, {"schema": "laya-train-probe-sigterm-v1", "n": n, "seconds_since_sigterm": round(since, 3)})
        up.put(name, work / name, attempts=1, timeout=5.0, record=False)

    def handler(signum, frame):
        if state["in_handler"]:
            return
        state["in_handler"] = True
        t0 = time.monotonic()
        n = 0
        while True:
            beat(n, time.monotonic() - t0)
            n += 1
            time.sleep(max(0.0, t0 + n * HEARTBEAT_INTERVAL_S - time.monotonic()))

    signal.signal(signal.SIGTERM, handler)
    write_json(work / "sigterm-ready.json", {"schema": "laya-train-probe-sigterm-v1", "started_at": now_iso()})
    up.put("sigterm-ready.json", work / "sigterm-ready.json", attempts=3, timeout=30.0, record=False)
    if ready_path:
        Path(ready_path).write_text("1")
    say("probe-sigterm ready; waiting for SIGTERM")
    while True:
        time.sleep(1.0)


def entry_probe_sigterm(env, paths, runner, knobs):
    try:
        bucket, pid = parse_probe_env(env)
    except Refusal as e:
        print("REFUSED " + str(e), file=sys.stderr, flush=True)
        return 2
    return run_probe_sigterm(bucket, pid, paths, runner, knobs)


# ------------------------------------------------------------------------------------------ pack mode (plan 14-07)

PACK_BIN = Path("/src/target/release/examples/pack_laya")      # the example built in the pack image's build stage
PACK_CONTRACTS = Path("/src/contracts")                        # pack_laya's compile-time ../../contracts, from /src/crates/aprender-decide
PACK_WORK = Path("/work")
PACK_GATE_CONTRACT = "laya-finetune-gate-v1.yaml"
PACK_PARITY_CONTRACT = "laya-parity-v1.yaml"
SCHEMA_PACK_OUTCOME = "laya-train-pack-outcome-v1"
SCHEMA_BASE_STAGE = "laya-base-stage-v1"
EXIT_CLASSES = ("pass", "gate_fail", "refused", "crash", "timeout")
PACK_CHILD_ENV_KEYS = ("PATH", "HOME", "LANG", "TMPDIR")      # the whole environment the pack and verify children get
PACK_CHILD_TIMEOUT_S = 3 * 3600                                # the Fargate task's own 3 h taskTimeout is the real bound
LINE_CLIP = 4000


class PackError(Exception):
    """An infrastructure error of the pack step (download, integrity, base pin, an unrunnable binary): exit 1, no
    outcome recorded. str() is `<kind>: <detail>`; kind is one of outcome, manifest, download, integrity, base, child."""

    def __init__(self, kind, detail):
        super().__init__("%s: %s" % (kind, detail))
        self.kind = kind


@dataclass(frozen=True)
class PackCfg:
    task_id: str
    name: str
    bucket: str
    run_prefix: str
    image_digest: str


def parse_pack_env(env):
    """The pack-mode env contract. Raises Refusal("env: ...") on anything missing or malformed (values never echoed)."""
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
    digest = need("LAYA_PACK_IMAGE_DIGEST")
    if not DIGEST_RE.match(digest):
        raise Refusal("env: LAYA_PACK_IMAGE_DIGEST must be sha256: followed by 64 lowercase hex")
    return PackCfg(task_id, name, bucket, prefix, digest)


@dataclass
class PackKnobs:
    """Everything a selftest may change. The defaults are the image's constants; no env var or argument reaches here."""
    pack_cmd: tuple = (str(PACK_BIN),)
    contracts_dir: Path = PACK_CONTRACTS
    work: Path = PACK_WORK
    pin_path: Path = HERE / "base-stage.json"
    backoff: tuple = (1.0, 4.0)
    attempts: int = 3
    transfer_timeout: float = 1800.0
    upload_timeout: float = 900.0
    child_timeout: float = PACK_CHILD_TIMEOUT_S
    sleep: object = time.sleep


def pack_child_env():
    """The pack and verify children's whole environment: PATH, HOME, LANG and TMPDIR when set, nothing else."""
    return {k: os.environ[k] for k in PACK_CHILD_ENV_KEYS if k in os.environ}


def base_key_from_pin(pin):
    """The base tar's S3 key, derived from the pin: base/laya-<first 8 of the revision>/<tar_name> (the value of
    layout::BASE_KEY in crates/aprender-mcp-laya-train; the selftest compares them when that source is present)."""
    rev, tar_name = str(pin.get("revision", "")), str(pin.get("tar_name", ""))
    if not re.fullmatch(r"[0-9a-z]{8,64}", rev):
        raise PackError("base", "base-stage.json has no usable revision")
    if not re.fullmatch(r"[A-Za-z0-9._-]+", tar_name):
        raise PackError("base", "base-stage.json has no usable tar_name")
    return "base/laya-%s/%s" % (rev[:8], tar_name)


def s3_get(runner, bucket, key, dest, knobs):
    """Download one object to `dest` through the Runner (`aws s3api get-object`), with retries. Raises PackError."""
    dest = Path(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    err = "no attempt"
    for i in range(knobs.attempts):
        dest.unlink(missing_ok=True)
        try:
            runner(["aws", "s3api", "get-object", "--bucket", bucket, "--key", key, str(dest)], knobs.transfer_timeout)
            if dest.is_file():
                return dest
            err = "no file was written"
        except Exception as e:                            # a Runner failure of any kind is one failed attempt
            err = "%s: %s" % (type(e).__name__, str(e)[:200])
        if i + 1 < knobs.attempts:
            knobs.sleep(knobs.backoff[min(i, len(knobs.backoff) - 1)])
    raise PackError("download", "%s (%s)" % (key, err))


def read_json_object(path, what):
    try:
        obj = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        raise PackError("manifest" if "manifest" in what else "outcome", "%s is not readable JSON (%s)" % (what, type(e).__name__))
    if not isinstance(obj, dict):
        raise PackError("manifest" if "manifest" in what else "outcome", "%s is not a JSON object" % what)
    return obj


def parse_training_manifest(obj, task_id):
    """-> {path: (size, sha256)} after refusing a malformed manifest, a foreign task, an unsafe or repeated path."""
    if obj.get("schema") != SCHEMA_TRAINING_MANIFEST or obj.get("task_id") != task_id:
        raise PackError("manifest", "schema or task_id does not match this run")
    out = {}
    entries = obj.get("entries")
    if not isinstance(entries, list):
        raise PackError("manifest", "entries is not a list")
    for e in entries:
        if not isinstance(e, dict) or set(e) != {"path", "size", "sha256"}:
            raise PackError("manifest", "an entry does not have exactly path, size and sha256")
        path, size, sha = e["path"], e["size"], e["sha256"]
        if (not isinstance(path, str) or not isinstance(size, int) or isinstance(size, bool) or size < 0
                or not isinstance(sha, str) or not HEX64_RE.match(sha)):
            raise PackError("manifest", "an entry has a malformed field")
        pp = PurePosixPath(path)
        if (not path or path.startswith("/") or "\\" in path or "\0" in path or not pp.parts
                or any(part in ("", ".", "..") for part in path.split("/"))):
            raise PackError("integrity", "the manifest names an unsafe path %r" % path[:80])
        if path in out:
            raise PackError("integrity", "the manifest names %s twice" % path[:80])
        out[path] = (size, sha)
    return out


def fetch_verified(runner, cfg, rel, size_sha, dest, knobs):
    """Download run-prefix/<rel> to `dest` and re-hash it against the manifest entry; raises PackError integrity."""
    s3_get(runner, cfg.bucket, cfg.run_prefix + rel, dest, knobs)
    size, sha = size_sha
    if Path(dest).stat().st_size != size or sha256_file(dest) != sha:
        raise PackError("integrity", "%s does not match training-manifest.json (size or sha256)" % rel)
    return dest


def last_line_with(path, prefix):
    """The LAST line of the file that starts with `prefix` (the child's final answer), else None."""
    found = None
    try:
        with open(path, "rb") as f:
            for raw in f:
                line = raw.decode("utf-8", "replace").rstrip("\r\n")
                if line.startswith(prefix):
                    found = line
    except OSError:
        pass
    return found


def parse_packed_line(line):
    """PACKED ... sha256=<64 hex> ... shipped_seed=<n|null> ... waived=<id|none> -> (sha256, shipped_seed, waived)
    or None when the line is not that shape. waived is None for `none`."""
    if line is None or not line.startswith("PACKED "):
        return None
    m_sha = re.search(r"\bsha256=([0-9a-f]{64})\b", line)
    m_seed = re.search(r"\bshipped_seed=(null|-?[0-9]+)(?=\s|$)", line)
    m_waived = re.search(r"\bwaived=(\S+)\s*$", line)
    if not (m_sha and m_seed and m_waived):
        return None
    seed = None if m_seed.group(1) == "null" else int(m_seed.group(1))
    waived = None if m_waived.group(1) == "none" else m_waived.group(1)
    return m_sha.group(1), seed, waived


def parse_verify_line(path):
    """The last JSON object line of verify.log with a deploy_eligible key -> (line, dict) or (None, None)."""
    found = (None, None)
    try:
        with open(path, "rb") as f:
            for raw in f:
                line = raw.decode("utf-8", "replace").strip()
                if line.startswith("{"):
                    try:
                        obj = json.loads(line)
                    except ValueError:
                        continue
                    if isinstance(obj, dict) and "deploy_eligible" in obj:
                        found = (line, obj)
    except OSError:
        pass
    return found


def run_child(cmd, log_path, timeout):
    """Run a pack_laya child with the stripped environment, stdout and stderr to `log_path`. Returns (rc, seconds);
    rc is None when the child timed out (it is killed). A child that cannot start is a PackError."""
    t0 = time.monotonic()
    with open(log_path, "wb") as log:
        try:
            r = subprocess.run(list(cmd), stdout=log, stderr=subprocess.STDOUT, env=pack_child_env(), timeout=timeout)
            rc = r.returncode
        except subprocess.TimeoutExpired:
            log.write(b"\nrun_step: child killed after the timeout\n")
            rc = None
        except OSError as e:
            raise PackError("child", "%s cannot start (%s)" % (cmd[0], type(e).__name__))
    return rc, time.monotonic() - t0


def new_pack_outcome(cfg, gate_sha, parity_sha, **kv):
    out = {"schema": SCHEMA_PACK_OUTCOME, "task_id": cfg.task_id, "pack_image_digest": cfg.image_digest,
           "gate_contract_sha256": gate_sha, "parity_contract_sha256": parity_sha, "ran": False, "skipped": None,
           "exit_code": None, "accepted": False, "apr_sha256": None, "shipped_seed": None, "waived": None,
           "packed_line": None, "verify_line": None, "deploy_eligible": False, "pack_seconds": None,
           "verify_seconds": None}
    out.update(kv)
    return out


def publish_pack(cfg, work, up, outcome, knobs):
    """pack-manifest.json (a JSON array of {path,size,sha256}: everything uploaded so far plus pack-outcome.json),
    then pack-outcome.json LAST. Returns True when both uploads returned success."""
    out_path = Path(work) / "pack-outcome.json"
    write_json(out_path, outcome)
    entries = up.entries()
    entries.append({"path": "pack-outcome.json", "size": out_path.stat().st_size, "sha256": sha256_file(out_path)})
    man_path = Path(work) / "pack-manifest.json"
    write_json(man_path, sorted(entries, key=lambda e: e["path"]))
    a = knobs.attempts
    m = up.put("pack-manifest.json", man_path, attempts=a, timeout=knobs.upload_timeout, record=False)
    o = up.put("pack-outcome.json", out_path, attempts=a, timeout=knobs.upload_timeout, record=False)
    return m != "failed" and o != "failed"


def decide_skip(runner, cfg, outcome, work, knobs):
    """None when pack should run, else the exit_class string to record as `skipped`. Raises PackError."""
    ec = outcome.get("exit_class")
    if ec not in EXIT_CLASSES:
        raise PackError("outcome", "outcome.json has an unknown exit_class")
    if outcome.get("device_ok") is not True or ec not in ("pass", "gate_fail"):
        return ec
    if ec == "pass":
        return None
    # gate_fail: pack runs only for a pilot-waiver claim in the (re-hashed) gate-report.json
    manifest = None
    for name in ("training-manifest.json", "partial-manifest.json"):
        try:
            got = s3_get(runner, cfg.bucket, cfg.run_prefix + name, Path(work) / "dl" / name, knobs)
        except PackError:
            continue
        manifest = parse_training_manifest(read_json_object(got, name), cfg.task_id)
        break
    entry = (manifest or {}).get("run/gate-report.json")
    if entry is None:
        return ec
    path = fetch_verified(runner, cfg, "run/gate-report.json", entry, Path(work) / "dl" / "gate-report.json", knobs)
    try:
        report = json.loads(path.read_text(encoding="utf-8"))
    except ValueError:
        raise PackError("integrity", "run/gate-report.json is not JSON")
    claim = report.get("pilot_waiver") if isinstance(report, dict) else None
    if isinstance(claim, dict) and isinstance(claim.get("id"), str) and claim["id"]:
        return None
    return ec


def run_pack(cfg, runner, knobs):
    work = Path(knobs.work)
    work.mkdir(parents=True, exist_ok=True)
    (work / "dl").mkdir(parents=True, exist_ok=True)
    snap = Path(tempfile.mkdtemp(prefix="laya-pack-snap-"))
    up = Uploader(runner, cfg.bucket, cfg.run_prefix, snap, knobs.backoff, sleep=knobs.sleep)
    gate_sha = sha256_file(Path(knobs.contracts_dir) / PACK_GATE_CONTRACT)
    parity_sha = sha256_file(Path(knobs.contracts_dir) / PACK_PARITY_CONTRACT)

    got = s3_get(runner, cfg.bucket, cfg.run_prefix + "outcome.json", work / "dl" / "outcome.json", knobs)
    outcome = read_json_object(got, "outcome.json")
    if outcome.get("schema") != SCHEMA_OUTCOME or outcome.get("task_id") != cfg.task_id:
        raise PackError("outcome", "outcome.json schema or task_id does not match this run")

    skipped = decide_skip(runner, cfg, outcome, work, knobs)
    if skipped is not None:
        say("pack skipped: the run ended %s" % skipped)
        rec = new_pack_outcome(cfg, gate_sha, parity_sha, skipped=skipped)
        return 0 if publish_pack(cfg, work, up, rec, knobs) else 1

    # ---- the inputs: exactly the manifest's run/ and data/ entries, each re-hashed
    got = s3_get(runner, cfg.bucket, cfg.run_prefix + "training-manifest.json", work / "dl" / "training-manifest.json", knobs)
    mobj = read_json_object(got, "training-manifest.json")
    if mobj.get("complete") is not True:
        raise PackError("manifest", "training-manifest.json is not complete")
    manifest = parse_training_manifest(mobj, cfg.task_id)
    for rel in sorted(manifest):
        if rel.split("/", 1)[0] in ("run", "data") and "/" in rel:
            fetch_verified(runner, cfg, rel, manifest[rel], work / rel, knobs)
    run_dir, data_dir = work / "run", work / "data"
    if not run_dir.is_dir() or not data_dir.is_dir():
        raise PackError("manifest", "the manifest lists no run/ or no data/ files")

    # ---- the base: pinned by sha256 BEFORE safe_extract, offline layout under <hf>/hub
    try:
        pin = json.loads(Path(knobs.pin_path).read_text(encoding="utf-8"))
        base_key = base_key_from_pin(pin)
        base_tar = s3_get(runner, cfg.bucket, base_key, work / "base.tar", knobs)
        hf_home = stage_channel_base(base_tar, knobs.pin_path, work / "hf")
        base_tar.unlink(missing_ok=True)                 # the extracted copy is the one used; free the disk
    except PackError:
        raise
    except Exception as e:
        raise PackError("base", "%s: %s" % (type(e).__name__, str(e)[:200]))
    sys.path.insert(0, str(HERE))
    import stage_base
    base_dir = Path(hf_home) / "hub" / stage_base.CACHE_DIR_NAME / "snapshots" / str(pin["revision"])
    if not base_dir.is_dir():
        raise PackError("base", "the staged base has no snapshot directory")

    # ---- pack, then (only on 0) verify: path arguments only, each timed
    apr = work / "out" / ("%s.apr" % cfg.name)
    apr.parent.mkdir(parents=True, exist_ok=True)
    pack_log, verify_log = work / "pack.log", work / "verify.log"
    pack_argv = list(knobs.pack_cmd) + ["pack", "--run", str(run_dir), "--data", str(data_dir), "--base", str(base_dir),
                                        "--out", str(apr)]
    verify_argv = list(knobs.pack_cmd) + ["verify", str(apr), "--run", str(run_dir), "--data", str(data_dir),
                                          "--base", str(base_dir)]
    pack_rc, pack_s = run_child(pack_argv, pack_log, knobs.child_timeout)
    verify_rc, verify_s = None, None
    if pack_rc == 0:
        verify_rc, verify_s = run_child(verify_argv, verify_log, knobs.child_timeout)

    packed_line = last_line_with(pack_log, "PACKED ") or last_line_with(pack_log, "REFUSED ")
    packed = parse_packed_line(last_line_with(pack_log, "PACKED ")) if pack_rc == 0 else None
    verify_json_line, verify_obj = parse_verify_line(verify_log) if verify_rc == 0 else (None, None)
    verify_line = verify_json_line or (last_line_with(verify_log, "REFUSED ") if verify_rc is not None else None)
    eligible = bool(verify_obj is not None and verify_obj.get("deploy_eligible") is True)
    local_sha = sha256_file(apr) if apr.is_file() else None
    verify_waived = verify_obj.get("waived") if verify_obj is not None else None
    waived_ok = packed is not None and (verify_waived is None or isinstance(verify_waived, str)) \
        and packed[2] == verify_waived
    accepted = bool(pack_rc == 0 and verify_rc == 0 and packed is not None and eligible
                    and local_sha is not None and packed[0] == local_sha and waived_ok)

    # ---- upload: the .apr only for an accepted run; a failed .apr upload un-accepts it
    if accepted:
        if up.put("%s.apr" % cfg.name, apr, attempts=knobs.attempts, timeout=knobs.upload_timeout) != "uploaded":
            say("the .apr upload failed; recording accepted false")
            accepted = False
    for rel, path in (("pack.log", pack_log), ("verify.log", verify_log)):
        if path.is_file():
            up.put(rel, path, attempts=knobs.attempts, timeout=knobs.upload_timeout)
    exit_code = pack_rc if pack_rc != 0 else verify_rc     # the first child that did not exit 0, else 0
    rec = new_pack_outcome(
        cfg, gate_sha, parity_sha, ran=True, exit_code=exit_code, accepted=accepted,
        apr_sha256=local_sha if accepted else None,
        shipped_seed=packed[1] if accepted and packed is not None else None,
        waived=packed[2] if accepted and packed is not None else None,
        packed_line=packed_line[:LINE_CLIP] if packed_line else None,
        verify_line=verify_line[:LINE_CLIP] if verify_line else None,
        deploy_eligible=eligible and verify_rc == 0,
        pack_seconds=round(pack_s, 2), verify_seconds=round(verify_s, 2) if verify_s is not None else None)
    published = publish_pack(cfg, work, up, rec, knobs)
    failed = dict(up.failed)
    if failed:
        say("upload failures: %s" % ", ".join("%s (%s)" % (p, e) for p, e in sorted(failed.items())))
    return 0 if published and not failed else 1


def entry_pack(env, runner, knobs):
    """Parse the env contract, check the contracts exist, then run. A refusal is `REFUSED env: ...` (exit 2) before
    any download; an infrastructure error is `ERROR <kind>: <detail>` (exit 1)."""
    try:
        cfg = parse_pack_env(env)
    except Refusal as e:
        print("REFUSED " + str(e), file=sys.stderr, flush=True)
        return 2
    for name in (PACK_GATE_CONTRACT, PACK_PARITY_CONTRACT):
        if not (Path(knobs.contracts_dir) / name).is_file():
            print("REFUSED contracts: %s is missing from the pack image" % name, file=sys.stderr, flush=True)
            return 2
    try:
        return run_pack(cfg, runner, knobs)
    except PackError as e:
        print("ERROR %s" % e, file=sys.stderr, flush=True)
        return 1


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
(out / "env.json").write_text(json.dumps({k: os.environ.get(k) for k in ("LD_LIBRARY_PATH", "HF_HUB_OFFLINE", "HF_HOME")}))
def device_line(dev):
    print("DEVICE seed=13 requested=cuda used=%s torch=x" % dev, flush=True)
def report(dev="cuda:0"):
    (out / "gate-report.json").write_text(json.dumps({"pass": mode == "pass", "device_used": dev, "device_is_cpu": False}))
def checkpoint(root):
    (root / "checkpoint").mkdir(parents=True, exist_ok=True)
    (root / "checkpoint" / "model.safetensors").write_bytes(b"w" * 1024)
if mode in ("pass", "gate3"):
    device_line("cuda:0")
    (out / "recipe.json").write_text('{"fake":true}')
    checkpoint(out)
    report()
    print("GATE PASS" if mode == "pass" else "GATE FAIL", flush=True)
    sys.exit(0 if mode == "pass" else 3)
if mode == "slow":
    time.sleep(0.5)
    (out / "recipe.json").write_text('{"fake":true}')
    (out / "big.bin").write_bytes(b"b" * (6 * 1024 * 1024))
    time.sleep(3.0)
    device_line("cuda:0")
    report()
    pathlib.Path(mark, "trainer.done").write_text("1")
    sys.exit(0)
if mode == "sigterm":
    got = pathlib.Path(mark, "got-sigterm")
    signal.signal(signal.SIGTERM, lambda s, f: got.write_text("1"))
    (out / "recipe.json").write_text('{"fake":true}')
    checkpoint(out)
    (out / "seeds" / "seed-13").mkdir(parents=True)
    checkpoint(out / "seeds" / "seed-13")
    (out / "seeds" / "seed-13" / "eval-probs.json").write_text("[]")
    device_line("cuda:0")
    pathlib.Path(mark, "pid").write_text(str(os.getpid()))
    pathlib.Path(mark, "ready").write_text("1")
    while True:
        time.sleep(0.1)
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


FAKE_PACK = r'''
import hashlib, json, os, pathlib, sys, time
here = pathlib.Path(__file__).resolve().parent
cfg = json.loads((here / "cfg.json").read_text())
args = sys.argv[1:]
def opt(n):
    return args[args.index(n) + 1] if n in args else None
base = opt("--base")
rec = {"argv": args, "env_keys": sorted(os.environ), "run_ok": os.path.isdir(opt("--run") or "/nonexistent"),
       "data_ok": os.path.isdir(opt("--data") or "/nonexistent"),
       "base_ok": os.path.isfile(os.path.join(base or "/nonexistent", "model.safetensors")),
       "files": sorted(str(p.relative_to(opt("--run"))) for p in pathlib.Path(opt("--run")).rglob("*") if p.is_file())
       if opt("--run") and os.path.isdir(opt("--run")) else []}
with open(here / "calls.jsonl", "a") as f:
    f.write(json.dumps(rec) + "\n")
print("fake pack_laya noise on stderr", file=sys.stderr, flush=True)
time.sleep(0.02)
if args[0] == "pack":
    rc = cfg["pack_rc"]
    if rc != 0:
        print("REFUSED %s fake refusal (nothing written)" % ("GateFailed" if rc == 3 else "BadInput"), flush=True)
        sys.exit(rc)
    out = pathlib.Path(opt("--out"))
    out.parent.mkdir(parents=True, exist_ok=True)
    data = b"APR-FAKE-BYTES" * 10
    out.write_bytes(data)
    sha = cfg.get("packed_sha") or hashlib.sha256(data).hexdigest()
    print("PACKED %s sha256=%s rescore_max_abs=1e-6 zs_rescore_max_abs=1e-6 rescore_bound=1e-5 zs_rescore_bound=1e-5 "
          "noise=null zs_noise=null shipped_seed=%s argmax=10/10 waived=%s" % (out, sha, cfg["seed"], cfg["pack_waived"]), flush=True)
    sys.exit(0)
if args[0] == "verify":
    rc = cfg["verify_rc"]
    if rc != 0:
        print("REFUSED ParityExceeded fake verify refusal (nothing written)", flush=True)
        sys.exit(rc)
    print(json.dumps({"artifact_sha256": "x", "deploy_eligible": cfg["eligible"], "waived": cfg["verify_waived"]}), flush=True)
    sys.exit(0)
sys.exit(2)
'''


class FakeRunner:
    """The selftest's S3: copies the --body into a temp 'bucket' dir and records every call (key, tag, times, whether
    the fake trainer had finished). `delay` is how long an upload takes (it honours the call's timeout, like
    subprocess.run), `fail(key)` makes chosen uploads raise, `log_path` persists the calls for a child process."""

    def __init__(self, bucket, marker_dir=None, delay=0.0, fail=None, log_path=None):
        self.bucket, self.marker_dir, self.delay, self.fail = Path(bucket), marker_dir, delay, fail
        self.log_path = Path(log_path) if log_path else None
        self.calls = []

    def _get(self, argv):
        """aws s3api get-object --bucket B --key K <outfile>: copy the fake bucket's object to the outfile."""
        a = dict(zip(argv[3:7:2], argv[4:7:2]))
        key = a["--key"]
        rec = {"op": "get", "key": key, "tagging": None, "t": time.monotonic(), "wall": time.time(), "ok": False,
               "trainer_done": False}
        self.calls.append(rec)
        try:
            if self.fail and self.fail(key):
                raise RuntimeError("fake runner failure")
            src = self.bucket / key
            if not src.is_file():
                raise RuntimeError("NoSuchKey: %s" % key)
            shutil.copyfile(src, argv[7])
            rec["ok"] = True
        finally:
            if self.log_path:
                with open(self.log_path, "a") as f:
                    f.write(json.dumps(rec) + "\n")

    def __call__(self, argv, timeout):
        if argv[:3] == ["aws", "s3api", "get-object"] and len(argv) == 8:
            return self._get(argv)
        a = dict(zip(argv[3::2], argv[4::2])) if argv[:3] == ["aws", "s3api", "put-object"] else None
        if a is None:
            raise RuntimeError("not a put-object argv")
        key, body = a["--key"], a["--body"]
        rec = {"op": "put", "key": key, "tagging": a.get("--tagging"), "t": time.monotonic(), "wall": time.time(), "ok": False,
               "trainer_done": bool(self.marker_dir and (Path(self.marker_dir) / "trainer.done").exists())}
        self.calls.append(rec)
        try:
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
            if key.endswith("partial-manifest.json"):
                rec["body"] = json.loads(Path(body).read_text())
        finally:
            if self.log_path:
                with open(self.log_path, "a") as f:
                    f.write(json.dumps(rec) + "\n")


NO_GPU = {"proc_version": Path("/nonexistent/nvidia-version"), "smi_version_cmd": ("/nonexistent/nvidia-smi",),
          "sampler_cmd": ("/nonexistent/nvidia-smi",), "install_signals": False}


def selftest_child(spec):
    """A child process of the selftest: the real run_step with the real SIGTERM handler, a fake trainer and a fake
    S3, so the parent can send it SIGTERM and time the shutdown. `spec` is JSON on the command line."""
    global QUIET
    QUIET = True
    fail = None
    if spec.get("fail") == "small":
        fail = lambda key: not key.endswith(("partial-manifest.json", "outcome.json"))   # noqa: E731
    runner = FakeRunner(spec["bucket"], spec["marks"], spec.get("delay", 0.0), fail, spec["log"])
    knobs = Knobs(trainer=Path(spec["trainer"]), backoff=(0.0,), echo=False, deadline=spec["deadline"],
                  grace=spec["grace"], reserve=spec["reserve"], sync_interval=3600.0, sample_interval=3600.0,
                  **{**NO_GPU, "install_signals": True})
    paths = Paths(data_channel=Path(spec["chan"]), base_tar=Path(spec["work"]) / "none.tar", work=Path(spec["work"]))
    if spec["kind"] == "probe-sigterm":
        bucket, pid = parse_probe_env({"LAYA_BUCKET": "test-bucket", "LAYA_PROBE_ID": "probe-1"})
        return run_probe_sigterm(bucket, pid, paths, runner, knobs, ready_path=spec["ready"])
    return entry_train(spec["env"], paths, runner, knobs)


def selftest():
    global QUIET, DEFAULT_RUNNER_CALLS
    for need in (FIXTURES / "outcome-v1.json", FIXTURES / "training-manifest-v1.json", FIXTURES / "pack-outcome-v1.json",
                 GATE_CONTRACT, ROOT / "contracts" / PACK_PARITY_CONTRACT):
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

    def make_case(tmp, name, mode, shift=False, epochs="", extra_files=None, knob_kw=None, **runner_kw):
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
        knob_kw = {**NO_GPU, "sync_interval": 3600.0, "sample_interval": 3600.0, **(knob_kw or {})}
        knobs = Knobs(trainer=trainer, backoff=(0.0,), echo=False, **knob_kw)
        runner = FakeRunner(bucket / "b", marker_dir=marks, **runner_kw)
        return {"env": env, "paths": paths, "knobs": knobs, "runner": runner, "bucket": bucket / "b",
                "marks": marks, "chan": chan, "dir": d, "mode": mode, "trainer": trainer}

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

        print("compat:")
        proc = Path(tmp) / "nvidia-version"
        proc.write_text("NVRM version: NVIDIA UNIX x86_64 Kernel Module  570.172.08  Mon Jul 14 2025\n")
        case("compat: driver string 570.172.08 is parsed from the /proc text", host_driver(proc, ("/nonexistent",)) == "570.172.08")
        case("compat: decide_compat table 570.172.08 true, 579.99 true, 580.65.06 false, 590.1 false, unknown false",
             [decide_compat(x) for x in ("570.172.08", "579.99", "580.65.06", "590.1", "unknown")] == [True, True, False, False, False])
        e570 = with_compat({"LD_LIBRARY_PATH": "/x"}, decide_compat("570.172.08"))
        e580 = with_compat({"LD_LIBRARY_PATH": "/x"}, decide_compat("580.65.06"))
        case("compat: 570 puts /usr/local/cuda/compat first on LD_LIBRARY_PATH, 580 leaves it unchanged",
             e570["LD_LIBRARY_PATH"] == "/usr/local/cuda/compat:/x" and e580["LD_LIBRARY_PATH"] == "/x"
             and with_compat({}, True)["LD_LIBRARY_PATH"] == "/usr/local/cuda/compat")
        case("compat: an unreadable /proc version and no nvidia-smi -> unknown, compat_used false",
             host_driver("/nonexistent/v", ("/nonexistent/smi",)) == "unknown" and not decide_compat("unknown"))
        smi_fake = (sys.executable, "-c", "print('570.172.08')")
        case("compat: nvidia-smi is the fallback when /proc is unreadable", host_driver("/nonexistent/v", smi_fake) == "570.172.08")
        old_ld = os.environ.get("LD_LIBRARY_PATH")
        c = make_case(tmp, "compat570", "pass", knob_kw={"proc_version": proc})
        run_case(c)
        child_env_seen = json.loads(bucket_file(c, "run/env.json").read_text())
        renv = json.loads(bucket_file(c, "run-env.json").read_text())
        case("compat: a 570 host -> the trainer child sees LD_LIBRARY_PATH starting /usr/local/cuda/compat, recorded in run-env.json",
             (child_env_seen["LD_LIBRARY_PATH"] or "").startswith("/usr/local/cuda/compat") and renv["compat_used"] is True
             and renv["driver"] == "570.172.08" and child_env_seen["HF_HUB_OFFLINE"] == "1", str(child_env_seen))
        proc580 = Path(tmp) / "nvidia-version-580"
        proc580.write_text("NVRM version: NVIDIA UNIX x86_64 Kernel Module  580.65.06  Mon Aug 11 2025\n")
        c = make_case(tmp, "compat580", "pass", knob_kw={"proc_version": proc580})
        run_case(c)
        child_env_seen = json.loads(bucket_file(c, "run/env.json").read_text())
        renv = json.loads(bucket_file(c, "run-env.json").read_text())
        case("compat: a 580 host -> compat_used false and the child's LD_LIBRARY_PATH is the inherited one",
             renv["compat_used"] is False and child_env_seen["LD_LIBRARY_PATH"] == old_ld and renv["compat_ld_library_path"] is None)
        c = make_case(tmp, "compat-unknown", "pass")
        run_case(c)
        renv = json.loads(bucket_file(c, "run-env.json").read_text())
        case("compat: no readable driver -> run-env.json driver unknown, compat_used false",
             renv["driver"] == "unknown" and renv["compat_used"] is False)

        print("gpu sampler:")
        c = make_case(tmp, "sampler", "slow", knob_kw={"sampler_cmd": (sys.executable, "-c", "print('1234, 56')"),
                                                        "sample_interval": 0.4})
        run_case(c)
        gs = json.loads(bucket_file(c, "gpu-samples.json").read_text())
        ok_rows = [r for r in gs["samples"] if "memory_used_mib" in r]
        case("gpu sampler: gpu-samples.json is uploaded with parsed rows and a peak (nvidia-smi is a fake)",
             len(ok_rows) >= 3 and ok_rows[0]["memory_used_mib"] == 1234 and ok_rows[0]["utilization_pct"] == 56
             and gs["peak_memory_used_mib"] == 1234)
        case("gpu sampler: the peak lands in run-env.json peak_gpu_memory_mib",
             json.loads(bucket_file(c, "run-env.json").read_text())["peak_gpu_memory_mib"] == 1234)

        print("periodic-sync:")
        c = make_case(tmp, "sync", "slow", knob_kw={"sync_interval": 1.0})
        rc = run_case(c)
        calls = c["runner"].calls
        idx = lambda pred: next((i for i, x in enumerate(calls) if pred(x)), None)             # noqa: E731
        i_rec = idx(lambda x: x["key"].endswith("run/recipe.json") and x["ok"] and not x["trainer_done"])
        i_man = idx(lambda x: x["key"].endswith("partial-manifest.json") and x["ok"] and not x["trainer_done"])
        case("periodic-sync: recipe.json written mid-run is uploaded before the trainer exits", i_rec is not None, "rc=%s" % rc)
        case("periodic-sync: a partial-manifest.json follows it, still before the trainer exits",
             i_man is not None and i_rec is not None and i_man > i_rec)
        body = calls[i_man]["body"] if i_man is not None else {}
        rec_sha = sha256_file(bucket_file(c, "run/recipe.json"))
        case("periodic-sync: the partial manifest is complete false and lists recipe.json with its sha256",
             body.get("complete") is False and set(body) == manifest_keys
             and any(e["path"] == "run/recipe.json" and e["sha256"] == rec_sha for e in body.get("entries", [])))
        big = [x for x in calls if x["key"].endswith("run/big.bin")]
        case("periodic-sync: a file over 5 MiB is not synced mid-run (only after the trainer exits)",
             bool(big) and all(x["trainer_done"] for x in big))
        case("periodic-sync: the final training-manifest.json supersedes it (complete true)",
             json.loads(bucket_file(c, "training-manifest.json").read_text())["complete"] is True)
        c = make_case(tmp, "sync-fail", "slow", knob_kw={"sync_interval": 1.0},
                      fail=lambda key: key.endswith("run/recipe.json"))
        run_case(c)
        mid = [x for x in c["runner"].calls if x["key"].endswith("partial-manifest.json") and not x["trainer_done"] and "body" in x]
        case("periodic-sync: a partial manifest never lists a file whose upload failed",
             all(e["path"] != "run/recipe.json" for x in c["runner"].calls if "body" in x for e in x["body"]["entries"])
             and not any(x["ok"] for x in c["runner"].calls if x["key"].endswith("run/recipe.json")),
             "mid-run partials %d" % len(mid))

        print("sigterm:")
        import signal as _signal

        def sigterm_run(name, deadline, grace, reserve, delay=0.0, fail=None, wait_after=0.0):
            c = make_case(tmp, name, "sigterm")
            log = c["dir"] / "calls.jsonl"
            spec = {"kind": "train", "env": c["env"], "chan": str(c["chan"]), "work": str(c["paths"].work),
                    "trainer": str(c["trainer"]), "bucket": str(c["bucket"]), "marks": str(c["marks"]),
                    "log": str(log), "delay": delay, "fail": fail, "deadline": deadline, "grace": grace,
                    "reserve": reserve}
            env = dict(os.environ, FAKE_MODE="sigterm", FAKE_MARK=str(c["marks"]))
            proc = subprocess.Popen([sys.executable, "-I", str(Path(__file__).resolve()), "--selftest-child",
                                     json.dumps(spec)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            t_end = time.monotonic() + 20
            while not (c["marks"] / "ready").exists() and time.monotonic() < t_end and proc.poll() is None:
                time.sleep(0.05)
            time.sleep(0.3)                                      # a mid-run state for the trainer to be killed in
            wall = time.time()
            t_sig = time.monotonic()
            proc.send_signal(_signal.SIGTERM)
            try:
                rc = proc.wait(timeout=deadline + 15)
            except subprocess.TimeoutExpired:
                proc.kill()
                rc = None
            c.update(rc=rc, took=time.monotonic() - t_sig, wall=wall)
            c["calls"] = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            return c

        def trainer_dead(c):
            pid = int((c["marks"] / "pid").read_text())
            for _ in range(20):
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    return True
                time.sleep(0.1)
            return False

        c = sigterm_run("sigterm", deadline=6, grace=1, reserve=2)
        keys = [x["key"].split(task + "/", 1)[1] for x in c["calls"]]
        outc = json.loads(bucket_file(c, "outcome.json").read_text()) if bucket_file(c, "outcome.json").exists() else {}
        case("sigterm: the trainer got SIGTERM, then was SIGKILLed after the child grace (it ignores SIGTERM)",
             (c["marks"] / "got-sigterm").exists() and trainer_dead(c))
        case("sigterm: run_step exits 1 within deadline + 1 s of the signal", c["rc"] == 1 and c["took"] <= 7.0,
             "rc=%s took %.2f s" % (c["rc"], c["took"]))
        case("sigterm: small files go first (train.log, then reports), then partial-manifest.json, then outcome.json last",
             keys[0] == "train.log" and keys[-2:] == ["partial-manifest.json", "outcome.json"]
             and {"run/recipe.json", "run/seeds/seed-13/eval-probs.json"} <= set(keys), str(keys))
        case("sigterm: no checkpoint file is uploaded (run/checkpoint, run/seeds/*/checkpoint)",
             not any("checkpoint" in k for k in keys))
        pm = json.loads(bucket_file(c, "partial-manifest.json").read_text())
        okset = {x["key"].split(task + "/", 1)[1] for x in c["calls"] if x["ok"]} - {"partial-manifest.json", "outcome.json"}
        case("sigterm: outcome.json is exit_class timeout and the partial manifest lists exactly the uploaded files",
             outc.get("exit_class") == "timeout" and pm["complete"] is False
             and {e["path"] for e in pm["entries"]} == okset, "%s vs %s" % (sorted(e["path"] for e in pm["entries"]), sorted(okset)))

        print("sigterm-slow:")
        c = sigterm_run("sigterm-slow", deadline=9, grace=1, reserve=5, delay=2.0)
        keys = [x["key"].split(task + "/", 1)[1] for x in c["calls"]]
        case("sigterm-slow: a 2 s-per-upload runner still exits within deadline + 1 s", c["rc"] == 1 and c["took"] <= 10.0,
             "rc=%s took %.2f s" % (c["rc"], c["took"]))
        case("sigterm-slow: outcome.json is the last call, preceded by the partial manifest", keys[-2:] == ["partial-manifest.json", "outcome.json"], str(keys))
        man_call = next(x for x in c["calls"] if x["key"].endswith("partial-manifest.json"))
        before_ok = {x["key"].split(task + "/", 1)[1] for x in c["calls"] if x["ok"] and x["t"] < man_call["t"]}
        pm = json.loads(bucket_file(c, "partial-manifest.json").read_text()) if bucket_file(c, "partial-manifest.json").exists() else None
        case("sigterm-slow: the partial manifest lists exactly the files whose upload returned before it",
             pm is not None and {e["path"] for e in pm["entries"]} == before_ok and len(before_ok) >= 1,
             "%s vs %s" % (None if pm is None else sorted(e["path"] for e in pm["entries"]), sorted(before_ok)))
        case("sigterm-slow: outcome.json reached the bucket as timeout",
             bucket_file(c, "outcome.json").exists() and json.loads(bucket_file(c, "outcome.json").read_text())["exit_class"] == "timeout")

        print("sigterm-failing:")
        c = sigterm_run("sigterm-failing", deadline=6, grace=1, reserve=2, fail="small")
        keys = [x["key"].split(task + "/", 1)[1] for x in c["calls"]]
        small = [k for k in keys if k not in ("partial-manifest.json", "outcome.json")]
        case("sigterm-failing: every failing small file is attempted exactly once (no retry past the signal)",
             bool(small) and len(small) == len(set(small)), str(keys))
        case("sigterm-failing: partial-manifest.json and outcome.json are still attempted and exit is within deadline + 1 s",
             keys[-2:] == ["partial-manifest.json", "outcome.json"] and c["rc"] == 1 and c["took"] <= 7.0,
             "rc=%s took %.2f s" % (c["rc"], c["took"]))
        pm = json.loads(bucket_file(c, "partial-manifest.json").read_text()) if bucket_file(c, "partial-manifest.json").exists() else None
        case("sigterm-failing: the partial manifest lists nothing (no file uploaded) and no call starts after the deadline",
             pm is not None and pm["entries"] == [] and max(x["wall"] for x in c["calls"]) <= c["wall"] + 6 + 1.0)

        print("probe:")
        tiny_out = Path(tmp) / "tiny-fixture-run"
        helper_json = {"torch_cuda_available": True, "torch_version_cuda": "13.0", "device_name": "NVIDIA A10G",
                       "matmul_ok": True, "base_load_ok": True, "base_forward_seconds": 0.12,
                       "max_memory_allocated_bytes": 1000, "train_step_ok": True, "train_step_seconds": 1.5,
                       "train_step_peak_bytes": 2000, "host_rss_peak_bytes": 3000, "train_step_batch": 8,
                       "train_step_tokens": PROBE_TRAIN_STEP_TOKENS}
        helper_cmd = [sys.executable, "-c", "import json; print('noise'); print('PROBE_JSON ' + json.dumps(%r))" % helper_json]
        tiny_cmd = [sys.executable, "-c", "import json,sys,pathlib; o=pathlib.Path(sys.argv[1]); o.mkdir(parents=True); "
                    "(o/'gate-report.json').write_text(json.dumps({'device_used':'cuda:0'}))", str(tiny_out)]
        smi_cmd = [sys.executable, "-c", "print('FAKE NVIDIA-SMI Driver Version: 570.172.08')"]
        pdir = Path(tmp) / "probe"
        pdir.mkdir()
        prunner = FakeRunner(pdir / "bucket")
        pknobs = Knobs(backoff=(0.0,), echo=False, **{**NO_GPU, "proc_version": proc})
        ppaths = Paths(data_channel=pdir, base_tar=pdir / "none", work=pdir / "work")
        rc = run_probe("test-bucket", "probe-1", ppaths, prunner, pknobs, helper_cmd=helper_cmd, tiny_cmd=tiny_cmd,
                       smi_cmd=smi_cmd, tiny_out=tiny_out)
        pj = json.loads((pdir / "bucket" / "runs/probe/probe-1/probe.json").read_text())
        case("probe: probe.json has exactly the declared key set and schema laya-train-probe-v1",
             set(pj) == set(PROBE_KEYS) and len(PROBE_KEYS) == 20 and pj["schema"] == "laya-train-probe-v1", str(sorted(set(pj) ^ set(PROBE_KEYS))))
        case("probe: it records nvidia-smi, the driver, compat_used, the CUDA helper's fields and the tiny fixture's device",
             rc == 0 and "FAKE NVIDIA-SMI" in pj["nvidia_smi"] and pj["driver"] == "570.172.08" and pj["compat_used"] is True
             and pj["torch_version_cuda"] == "13.0" and pj["matmul_ok"] is True and pj["train_step_ok"] is True
             and pj["train_step_peak_bytes"] == 2000 and pj["host_rss_peak_bytes"] == 3000
             and pj["train_step_batch"] == 8 and pj["train_step_tokens"] == 1024
             and pj["tiny_fixture_rc"] == 0 and pj["tiny_fixture_device_used"] == "cuda:0")
        case("probe: probe.json, the logs and nvidia-smi.txt are uploaded under runs/probe/<probe_id>/ with the expire tag",
             {"runs/probe/probe-1/" + n for n in ("probe.json", "helper.log", "tiny-fixture.log", "nvidia-smi.txt")}
             <= {x["key"] for x in prunner.calls if x["ok"]} and all(x["tagging"] == "laya-retention=expire" for x in prunner.calls))
        bad_runner = FakeRunner(pdir / "bucket-bad")
        rc = run_probe("test-bucket", "probe-2", ppaths, bad_runner, pknobs, helper_cmd=[sys.executable, "-c", "raise SystemExit(1)"],
                       tiny_cmd=[sys.executable, "-c", "raise SystemExit(5)"], smi_cmd=smi_cmd, tiny_out=tiny_out / "none")
        pj = json.loads((pdir / "bucket-bad" / "runs/probe/probe-2/probe.json").read_text())
        case("probe: a failing helper and tiny fixture are recorded as false / null / rc, never as a crash of the probe",
             rc == 0 and pj["train_step_ok"] is False and pj["base_load_ok"] is False and pj["matmul_ok"] is False
             and pj["tiny_fixture_rc"] == 5 and pj["tiny_fixture_device_used"] is None and set(pj) == set(PROBE_KEYS))
        errbuf = io.StringIO()
        with contextlib.redirect_stderr(errbuf):
            prc = entry_probe({"LAYA_BUCKET": "test-bucket", "LAYA_PROBE_ID": "Bad_ID"}, ppaths, prunner, pknobs)
        case("probe: a LAYA_PROBE_ID outside ^[a-z0-9][a-z0-9-]{0,47}$ is REFUSED env: (exit 2)",
             prc == 2 and "REFUSED env:" in errbuf.getvalue())

        print("probe-sigterm:")
        pd = Path(tmp) / "probe-sigterm"
        (pd / "marks").mkdir(parents=True)
        spec = {"kind": "probe-sigterm", "chan": str(pd), "work": str(pd / "work"), "trainer": str(TRAIN_PY),
                "bucket": str(pd / "bucket"), "marks": str(pd / "marks"), "log": str(pd / "calls.jsonl"),
                "deadline": 25, "grace": 5, "reserve": 5, "ready": str(pd / "marks" / "ready")}
        proc = subprocess.Popen([sys.executable, "-I", str(Path(__file__).resolve()), "--selftest-child", json.dumps(spec)],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        t_end = time.monotonic() + 20
        while not (pd / "marks" / "ready").exists() and time.monotonic() < t_end and proc.poll() is None:
            time.sleep(0.05)
        proc.send_signal(_signal.SIGTERM)
        time.sleep(5.5)
        alive_after = proc.poll() is None
        proc.kill()
        proc.wait()
        hb = sorted((pd / "bucket" / "runs/probe/probe-1").glob("sigterm-heartbeat-*.json"),
                    key=lambda q: int(q.stem.rsplit("-", 1)[1]))
        secs = [json.loads(q.read_text())["seconds_since_sigterm"] for q in hb]
        case("probe-sigterm: it announced itself (sigterm-ready.json) before the signal",
             (pd / "bucket" / "runs/probe/probe-1/sigterm-ready.json").exists())
        case("probe-sigterm: heartbeats keep arriving every 2 s after SIGTERM until it is killed (0, 2, 4 s)",
             alive_after and len(secs) >= 3 and secs[0] < 1.0 and all(1.5 <= b - a <= 3.0 for a, b in zip(secs, secs[1:])),
             "seconds %s" % secs)

        print("no-determinism:")
        src = Path(__file__).read_text(encoding="utf-8")
        banned = ["use_determin" + "istic_algorithms", "CUBLAS_WORKSPACE" + "_CONFIG"]
        case("no-determinism: the module source contains neither the deterministic-algorithms call nor the cuBLAS workspace variable",
             not any(b in src for b in banned) and all(len(b) > 10 for b in banned))
        d_img = ROOT / "scripts" / "laya_train" / "container" / "Dockerfile.train"
        case("no-determinism: neither appears in Dockerfile.train when it exists",
             not d_img.exists() or not any(b in d_img.read_text() for b in banned))

        print("base channel:")
        sys.path.insert(0, str(HERE))
        import stage_base as sb
        bcache, brev = sb._synthetic_cache(Path(tmp) / "bc")
        btar = Path(tmp) / "laya-base.tar"
        info = sb.build(bcache, btar, brev, expected_model_sha256=None)
        pin = Path(tmp) / "base-stage.json"
        pin.write_text(json.dumps({"schema": "laya-base-stage-v1", "tar_sha256": info["sha256"]}))
        hf = Path(tmp) / "hf-ok"
        got_hf = stage_channel_base(btar, pin, hf)
        case("base: a tar whose sha256 equals the pin is extracted into HF_HOME/hub",
             got_hf == hf and (hf / "hub" / sb.CACHE_DIR_NAME / "snapshots" / brev / "model.safetensors").read_bytes() == b"MODEL-BYTES" * 100)
        flipped = Path(tmp) / "flipped.tar"
        raw = bytearray(btar.read_bytes())
        raw[len(raw) // 2] ^= 0xFF
        flipped.write_bytes(bytes(raw))
        hf2 = Path(tmp) / "hf-flipped"
        try:
            stage_channel_base(flipped, pin, hf2)
            refused_flip = False
        except RuntimeError as e:
            refused_flip = "sha256" in str(e)
        case("base: one flipped byte -> refused on the pin BEFORE anything is extracted", refused_flip and not hf2.exists())
        evil = Path(tmp) / "evil.tar"
        sb._tar_with(evil, [("../escape", "file", b"x")])
        evil_pin = Path(tmp) / "evil-pin.json"
        evil_pin.write_text(json.dumps({"schema": "laya-base-stage-v1", "tar_sha256": sha256_file(evil)}))
        try:
            stage_channel_base(evil, evil_pin, Path(tmp) / "hf-evil")
            refused_evil = False
        except sb.StageError:
            refused_evil = not (Path(tmp) / "escape").exists()
        case("base: a tar with the RIGHT sha256 but a `..` member is still refused by safe_extract", refused_evil)
        c = make_case(tmp, "base-fail", "pass", knob_kw={"prepare_base": lambda: stage_channel_base(flipped, pin, Path(tmp) / "hf3")})
        rc = run_case(c)
        out = json.loads(bucket_file(c, "outcome.json").read_text())
        case("base: a failed staging is a crash outcome with the trainer never started and the reason in train.log",
             rc == 1 and out["exit_class"] == "crash" and not (c["marks"] / "trainer.started").exists()
             and "BASE STAGING FAILED" in bucket_file(c, "train.log").read_text())
        c = make_case(tmp, "base-ok", "pass", knob_kw={"prepare_base": lambda: stage_channel_base(btar, pin, Path(tmp) / "hf4")})
        rc = run_case(c)
        env_seen = json.loads(bucket_file(c, "run/env.json").read_text())
        case("base: after staging the trainer runs with HF_HOME at the staged home and HF_HUB_OFFLINE=1",
             rc == 0 and env_seen["HF_HOME"] == str(Path(tmp) / "hf4") and env_seen["HF_HUB_OFFLINE"] == "1")

        print("pack:")
        pack_fix = json.loads((FIXTURES / "pack-outcome-v1.json").read_text())
        entry_keys = set(json.loads((FIXTURES / "training-manifest-v1.json").read_text())["entries"][0])
        pdigest = "sha256:" + "cd" * 32
        ppin = Path(tmp) / "pack-pin.json"
        ppin.write_text(json.dumps({"schema": SCHEMA_BASE_STAGE, "tar_sha256": sha256_file(btar), "revision": brev,
                                    "tar_name": "laya-base.tar"}))
        pbase_key = base_key_from_pin(json.loads(ppin.read_text()))
        pcontracts = ROOT / "contracts"
        want_gate = hashlib.sha256((pcontracts / PACK_GATE_CONTRACT).read_bytes()).hexdigest()
        want_parity = hashlib.sha256((pcontracts / PACK_PARITY_CONTRACT).read_bytes()).hexdigest()
        prefix = "runs/motiv-1/%s/" % task
        claim_w = {"id": "W-1", "waives": "ece_post"}

        def make_pack_case(name, exit_class="pass", device_ok=True, claim=None, drop_gate_entry=False, partial=False,
                           corrupt=None, base_bytes=None, runner_kw=None, **fake):
            d = Path(tmp) / ("pack-" + name)
            bucket = d / "bucket"
            root = bucket / prefix
            report = {"schema": "laya-gate-report-v1", "pass": exit_class == "pass", "device_used": "cuda:0"}
            if claim:
                report["pilot_waiver"] = claim
            files = {"run/gate-report.json": json.dumps(report).encode(), "run/recipe.json": b'{"fake":true}',
                     "run/checkpoint/model.safetensors": b"w" * 2048, "data/task.json": b'{"task":"t"}\n',
                     "data/train.jsonl": b'{"a":1}\n', "data/eval.jsonl": b'{"b":2}\n', "train.log": b"log\n"}
            out_obj = {"schema": SCHEMA_OUTCOME, "task_id": task, "exit_class": exit_class,
                       "train_rc": {"pass": 0, "gate_fail": 3, "refused": 2}.get(exit_class), "device_used": "cuda:0",
                       "device_ok": device_ok, "refused_line": None, "started_at": "t", "finished_at": "t",
                       "train_seconds": 1.0}
            files["outcome.json"] = (json.dumps(out_obj) + "\n").encode()
            entries = [{"path": rel, "size": len(b), "sha256": sha256_bytes(b)} for rel, b in sorted(files.items())]
            if drop_gate_entry:
                entries = [e for e in entries if e["path"] != "run/gate-report.json"]
            man = {"schema": SCHEMA_TRAINING_MANIFEST, "task_id": task, "complete": not partial,
                   "train_image_digest": digest, "gate_contract_sha256": "0" * 64, "entries": entries}
            for rel, b in files.items():
                (root / rel).parent.mkdir(parents=True, exist_ok=True)
                (root / rel).write_bytes(b)
            (root / ("partial-manifest.json" if partial else "training-manifest.json")).write_text(json.dumps(man))
            for rel, b in (corrupt or {}).items():
                (root / rel).write_bytes(b)
            (bucket / pbase_key).parent.mkdir(parents=True, exist_ok=True)
            (bucket / pbase_key).write_bytes(base_bytes if base_bytes is not None else btar.read_bytes())
            fdir = d / "fake"
            fdir.mkdir(parents=True)
            (fdir / "fake_pack.py").write_text(FAKE_PACK)
            (fdir / "cfg.json").write_text(json.dumps({"pack_rc": 0, "verify_rc": 0, "eligible": True,
                                                       "pack_waived": "none", "verify_waived": None,
                                                       "seed": 20260927, "packed_sha": None, **fake}))
            knobs = PackKnobs(pack_cmd=(sys.executable, "-I", str(fdir / "fake_pack.py")), contracts_dir=pcontracts,
                              work=d / "work", pin_path=ppin, backoff=(0.0,), attempts=2, sleep=lambda s: None,
                              child_timeout=60.0)
            env = {"LAYA_TASK_ID": task, "LAYA_NAME": "motiv-1", "LAYA_BUCKET": "test-bucket",
                   "LAYA_RUN_PREFIX": prefix, "LAYA_PACK_IMAGE_DIGEST": pdigest}
            return {"d": d, "bucket": bucket, "root": root, "fdir": fdir, "knobs": knobs, "env": env, "files": files,
                    "runner": FakeRunner(bucket, **(runner_kw or {}))}

        def run_pack_case(c):
            err = io.StringIO()
            with Env(LAYA_POLICY_PROBE="1", LAYA_STEP="pack", PACK_POLICY_OVERRIDE="x", AWS_ACCESS_KEY_ID="not-a-key"), \
                    contextlib.redirect_stderr(err):
                rc = entry_pack(c["env"], c["runner"], c["knobs"])
            c["stderr"] = err.getvalue()
            return rc

        def puts(c):
            return [x for x in c["runner"].calls if x["op"] == "put" and x["ok"]]

        def gets(c):
            return [x["key"][len(prefix):] if x["key"].startswith(prefix) else x["key"]
                    for x in c["runner"].calls if x["op"] == "get"]

        def fake_calls(c):
            p = c["fdir"] / "calls.jsonl"
            return [json.loads(line) for line in p.read_text().splitlines()] if p.exists() else []

        def pobj(c, rel):
            return json.loads((c["root"] / rel).read_text())

        def put_keys(c):
            return [x["key"][len(prefix):] for x in puts(c)]

        def no_apr(c):
            return not any(x["key"].endswith(".apr") for x in c["runner"].calls if x["op"] == "put")

        c = make_pack_case("accepted")
        rc = run_pack_case(c)
        po = pobj(c, "pack-outcome.json") if (c["root"] / "pack-outcome.json").exists() else {}
        apr_obj = c["root"] / "motiv-1.apr"
        calls = puts(c)
        case("pack-accepted: exit 0 and an outcome that says ran, accepted and deploy_eligible, nothing skipped",
             rc == 0 and po.get("ran") is True and po.get("accepted") is True and po.get("deploy_eligible") is True
             and po.get("skipped") is None and po.get("exit_code") == 0, "rc=%s %s" % (rc, c["stderr"][-120:]))
        case("pack-accepted: apr_sha256 is hashlib's sha256 of the uploaded .apr and shipped_seed is the PACKED one",
             apr_obj.is_file() and po.get("apr_sha256") == hashlib.sha256(apr_obj.read_bytes()).hexdigest()
             and po.get("shipped_seed") == 20260927 and po.get("waived") is None)
        case("pack-accepted: the PACKED line and the verify JSON line are recorded",
             str(po.get("packed_line")).startswith("PACKED ") and str(po.get("verify_line")).startswith("{")
             and json.loads(po["verify_line"])["deploy_eligible"] is True)
        case("pack-accepted: the .apr, both logs, the manifest and the outcome are uploaded, each tagged laya-retention=expire",
             {"motiv-1.apr", "pack.log", "verify.log", "pack-manifest.json", "pack-outcome.json"} <= set(put_keys(c))
             and all(x["tagging"] == "laya-retention=expire" for x in calls), str(put_keys(c)))
        case("pack-accepted: upload order is .apr, pack.log, verify.log, pack-manifest.json, pack-outcome.json last",
             put_keys(c) == ["motiv-1.apr", "pack.log", "verify.log", "pack-manifest.json", "pack-outcome.json"],
             str(put_keys(c)))
        pm = pobj(c, "pack-manifest.json")
        bad = [e["path"] for e in pm if e["path"] != "pack-outcome.json" and
               (sha256_file(c["root"] / e["path"]) != e["sha256"] or (c["root"] / e["path"]).stat().st_size != e["size"])]
        case("pack-accepted: pack-manifest lists the .apr, pack.log, verify.log and pack-outcome.json with hashlib's sha256",
             isinstance(pm, list) and not bad
             and {e["path"] for e in pm} == {"motiv-1.apr", "pack.log", "verify.log", "pack-outcome.json"}
             and any(e["path"] == "pack-outcome.json" and e["sha256"] == sha256_file(c["root"] / "pack-outcome.json")
                     for e in pm), "mismatch %s" % bad)
        want_get = {"outcome.json", "run/gate-report.json", "run/recipe.json", "run/checkpoint/model.safetensors",
                    "data/task.json", "data/train.jsonl", "data/eval.jsonl", "training-manifest.json", pbase_key}
        case("pack-accepted: exactly the manifest's run/ and data/ files, the manifest, the outcome and the base are downloaded",
             set(gets(c)) == want_get and "train.log" not in gets(c), str(sorted(set(gets(c)) ^ want_get)))
        fc = fake_calls(c)
        case("pack-accepted: pack_laya saw the run and data files and the staged base (offline layout) when it ran",
             len(fc) == 2 and all(x["run_ok"] and x["data_ok"] and x["base_ok"] for x in fc)
             and {"gate-report.json", "recipe.json", "checkpoint/model.safetensors"} <= set(fc[0]["files"]))

        print("pack-keys:")
        case("pack-keys: pack-outcome.json has exactly the key set of 14-01's pack-outcome-v1.json fixture",
             set(po) == set(pack_fix), str(sorted(set(po) ^ set(pack_fix))))
        case("pack-keys: pack-manifest.json is a JSON array whose entries have exactly the fixture entry keys",
             isinstance(pm, list) and all(set(e) == entry_keys for e in pm) and po.get("schema") == SCHEMA_PACK_OUTCOME
             and po.get("task_id") == task)

        print("pack-argv:")
        want_pack = ["pack", "--run", str(c["knobs"].work / "run"), "--data", str(c["knobs"].work / "data"), "--base",
                     fc[0]["argv"][6] if fc else "", "--out", str(c["knobs"].work / "out" / "motiv-1.apr")]
        want_verify = ["verify", str(c["knobs"].work / "out" / "motiv-1.apr"), "--run", str(c["knobs"].work / "run"),
                       "--data", str(c["knobs"].work / "data"), "--base", fc[0]["argv"][6] if fc else ""]
        case("pack-argv: the children got exactly pack --run R --data D --base B --out <name>.apr, then verify <name>.apr --run R --data D --base B",
             len(fc) == 2 and fc[0]["argv"] == want_pack and fc[1]["argv"] == want_verify
             and fc[0]["argv"][6].endswith("/snapshots/" + brev), str(fc[0]["argv"] if fc else None))
        allowed = set(PACK_CHILD_ENV_KEYS) | {"LC_CTYPE", "__CF_USER_TEXT_ENCODING"}
        seen = {k for x in fc for k in x["env_keys"]}
        case("pack-argv: the children's environment has no LAYA_*, *POLICY*, PACK_* or AWS_* variable (only PATH, HOME, LANG, TMPDIR)",
             bool(fc) and not any(k.startswith(("LAYA_", "PACK_", "AWS_")) or "POLICY" in k for k in seen)
             and seen <= allowed, str(sorted(seen - allowed)))
        with Env(LAYA_POLICY_PROBE="1", HOME="/home/x", PATH="/p", LANG="C", TMPDIR="/t", SECRET_TOKEN="s"):
            stripped = pack_child_env()
        case("pack-argv: pack_child_env keeps exactly PATH, HOME, LANG and TMPDIR",
             stripped == {"PATH": "/p", "HOME": "/home/x", "LANG": "C", "TMPDIR": "/t"})
        case("pack-argv: the defaults are the image constants and no env var names a binary, contracts or policy override",
             PackKnobs().pack_cmd == (str(PACK_BIN),) and PackKnobs().contracts_dir == PACK_CONTRACTS
             and str(PACK_BIN) == "/src/target/release/examples/pack_laya" and str(PACK_CONTRACTS) == "/src/contracts"
             and not re.search(r"environ(\.get)?[\[(]\s*[\"'][A-Z_]*(BIN|CONTRACT|POLICY)", Path(__file__).read_text(encoding="utf-8")))

        print("pack-contracts / pack-timings:")
        case("pack-contracts: gate_contract_sha256 and parity_contract_sha256 equal hashlib's sha256 of the two contract files",
             po["gate_contract_sha256"] == want_gate and po["parity_contract_sha256"] == want_parity
             and want_gate != want_parity)
        case("pack-image-env: LAYA_PACK_IMAGE_DIGEST is written verbatim as pack_image_digest (accepted outcome)",
             po["pack_image_digest"] == pdigest)
        case("pack-timings: pack_seconds and verify_seconds are non-negative numbers when both children ran",
             isinstance(po["pack_seconds"], (int, float)) and po["pack_seconds"] >= 0
             and isinstance(po["verify_seconds"], (int, float)) and po["verify_seconds"] >= 0)

        print("pack refusals (no .apr):")

        def refused_case(label, tag, expect_exit, expect_verify_ran, **kw):
            cc = make_pack_case(tag, **kw)
            rcc = run_pack_case(cc)
            o = pobj(cc, "pack-outcome.json") if (cc["root"] / "pack-outcome.json").exists() else {}
            case("%s: exit 0, accepted false, ran true, no .apr uploaded and none in the bucket" % label,
                 rcc == 0 and o.get("accepted") is False and o.get("ran") is True and no_apr(cc)
                 and not (cc["root"] / "motiv-1.apr").exists() and o.get("apr_sha256") is None and o.get("waived") is None,
                 "rc=%s %s" % (rcc, cc["stderr"][-80:]))
            case("%s: exit_code %s, verify %s, pack-outcome is the last upload" % (
                label, expect_exit, "ran" if expect_verify_ran else "did not run"),
                 o.get("exit_code") == expect_exit and (o.get("verify_seconds") is not None) == expect_verify_ran
                 and puts(cc)[-1]["key"].endswith("/pack-outcome.json") and len(fake_calls(cc)) == (2 if expect_verify_ran else 1),
                 str(o.get("exit_code")))
            return cc, o

        cc, o = refused_case("pack-gate-failed", "gate3", 3, False, pack_rc=3)
        case("pack-gate-failed: the REFUSED line is recorded, pack_seconds is a number, verify_seconds null, pack.log uploaded",
             str(o.get("packed_line")).startswith("REFUSED GateFailed") and isinstance(o.get("pack_seconds"), float)
             and o.get("verify_seconds") is None and "pack.log" in put_keys(cc) and "verify.log" not in put_keys(cc))
        cc, o = refused_case("pack-refused", "refused2", 2, False, pack_rc=2)
        case("pack-timings: a pack that refused has pack_seconds and a null verify_seconds",
             isinstance(o["pack_seconds"], (int, float)) and o["verify_seconds"] is None)
        cc, o = refused_case("verify-refused", "verify2", 2, True, verify_rc=2)
        case("verify-refused: the verify REFUSED line is recorded and deploy_eligible is false; pack ran and exited 0",
             str(o.get("verify_line")).startswith("REFUSED ") and o.get("deploy_eligible") is False
             and str(o.get("packed_line")).startswith("PACKED "))
        cc, o = refused_case("verify-not-eligible", "noteligible", 0, True, eligible=False)
        case("verify-not-eligible: verify exit 0 with deploy_eligible false is accepted false", o.get("deploy_eligible") is False)
        cc, o = refused_case("sha-mismatch", "shamismatch", 0, True, packed_sha="f" * 64)
        case("sha-mismatch: a PACKED sha256 that differs from the written file's hash is accepted false",
             str(o.get("packed_line")).count("f" * 64) == 1 and o.get("apr_sha256") is None)

        print("pack-skip:")
        skips = [("gate_fail without a claim", dict(exit_class="gate_fail"), "gate_fail"),
                 ("refused", dict(exit_class="refused", device_ok=False), "refused"),
                 ("crash", dict(exit_class="crash", device_ok=False), "crash"),
                 ("timeout", dict(exit_class="timeout", device_ok=False), "timeout"),
                 ("pass with device_ok false", dict(exit_class="pass", device_ok=False), "pass"),
                 ("gate_fail with a claim but device_ok false", dict(exit_class="gate_fail", device_ok=False, claim=claim_w), "gate_fail"),
                 ("gate_fail whose partial manifest lacks the gate report", dict(exit_class="gate_fail", partial=True, drop_gate_entry=True, claim=claim_w), "gate_fail")]
        for i, (label, kw, want_skip) in enumerate(skips):
            cc = make_pack_case("skip%d" % i, **kw)
            rcc = run_pack_case(cc)
            o = pobj(cc, "pack-outcome.json") if (cc["root"] / "pack-outcome.json").exists() else {}
            pmn = pobj(cc, "pack-manifest.json") if (cc["root"] / "pack-manifest.json").exists() else []
            case("pack-skip: %s -> exit 0, ran false, skipped %s, nothing packed, no base download, no .apr" % (label, want_skip),
                 rcc == 0 and o.get("ran") is False and o.get("skipped") == want_skip and o.get("accepted") is False
                 and pbase_key not in gets(cc) and not fake_calls(cc) and no_apr(cc)
                 and o.get("pack_seconds") is None and o.get("verify_seconds") is None and o.get("exit_code") is None,
                 "rc=%s %s %s" % (rcc, o.get("skipped"), cc["stderr"][-80:]))
            case("pack-skip: %s -> pack-image-env / pack-contracts: the skip is uploaded as pack-manifest [pack-outcome.json] then pack-outcome with the image digest and both contract hashes" % label,
                 [e["path"] for e in pmn] == ["pack-outcome.json"] and put_keys(cc) == ["pack-manifest.json", "pack-outcome.json"]
                 and o.get("pack_image_digest") == pdigest and o.get("gate_contract_sha256") == want_gate
                 and o.get("parity_contract_sha256") == want_parity and set(o) == set(pack_fix))

        print("pack-waiver:")
        cc = make_pack_case("waiver-claim", exit_class="gate_fail", claim=claim_w, pack_waived="W-1", verify_waived="W-1")
        rcc = run_pack_case(cc)
        o = pobj(cc, "pack-outcome.json") if (cc["root"] / "pack-outcome.json").exists() else {}
        case("pack-waiver-claim: a gate_fail whose gate-report carries pilot_waiver is packed; PACKED waived=W-1 and verify waived W-1 -> accepted true, waived W-1",
             rcc == 0 and o.get("ran") is True and o.get("accepted") is True and o.get("waived") == "W-1"
             and o.get("deploy_eligible") is True and o.get("skipped") is None, "rc=%s %s" % (rcc, cc["stderr"][-80:]))
        case("pack-waiver-claim: the .apr is uploaded tagged expire, the gate-report was downloaded and re-hashed before packing",
             (cc["root"] / "motiv-1.apr").is_file() and all(x["tagging"] == "laya-retention=expire" for x in puts(cc))
             and "run/gate-report.json" in gets(cc) and len(fake_calls(cc)) == 2)
        cc = make_pack_case("waiver-refused", exit_class="gate_fail", claim=claim_w, pack_rc=3)
        rcc = run_pack_case(cc)
        o = pobj(cc, "pack-outcome.json") if (cc["root"] / "pack-outcome.json").exists() else {}
        case("pack-waiver-refused: the same claim with pack exit 3 -> accepted false, waived null, exit_code 3, no .apr, exit 0",
             rcc == 0 and o.get("ran") is True and o.get("accepted") is False and o.get("waived") is None
             and o.get("exit_code") == 3 and no_apr(cc))
        cc = make_pack_case("waiver-disagree", exit_class="gate_fail", claim=claim_w, pack_waived="W-1", verify_waived=None)
        rcc = run_pack_case(cc)
        o = pobj(cc, "pack-outcome.json") if (cc["root"] / "pack-outcome.json").exists() else {}
        case("pack-waiver-disagree: PACKED waived=W-1 but verify JSON waived null -> accepted false, waived null, no .apr",
             rcc == 0 and o.get("accepted") is False and o.get("waived") is None and no_apr(cc)
             and o.get("deploy_eligible") is True)
        cc = make_pack_case("waiver-disagree2", exit_class="gate_fail", claim=claim_w, pack_waived="none", verify_waived="W-1")
        rcc = run_pack_case(cc)
        o = pobj(cc, "pack-outcome.json") if (cc["root"] / "pack-outcome.json").exists() else {}
        case("pack-waiver-disagree: PACKED waived=none but verify waived W-1 -> accepted false too",
             rcc == 0 and o.get("accepted") is False and no_apr(cc))

        print("pack-integrity / pack-base-pin:")
        cc = make_pack_case("integrity", corrupt={"run/recipe.json": b'{"fake":false}'})
        rcc = run_pack_case(cc)
        case("pack-integrity: a run file whose sha256 differs from training-manifest -> exit 1 `integrity`, nothing packed, nothing uploaded",
             rcc == 1 and "integrity" in cc["stderr"] and not fake_calls(cc) and not puts(cc) and pbase_key not in gets(cc),
             cc["stderr"].strip()[:100])
        cc = make_pack_case("integrity-gate", exit_class="gate_fail", claim=claim_w,
                            corrupt={"run/gate-report.json": b'{"pilot_waiver":{"id":"X"}}'})
        rcc = run_pack_case(cc)
        case("pack-integrity: a gate-report.json whose hash differs is refused (exit 1) BEFORE its pilot_waiver is read",
             rcc == 1 and "integrity" in cc["stderr"] and not fake_calls(cc) and not puts(cc))
        cc = make_pack_case("integrity-size", corrupt={"data/train.jsonl": b'{"a":1}\n\n'})
        rcc = run_pack_case(cc)
        case("pack-integrity: a data file with the wrong size is refused too", rcc == 1 and "integrity" in cc["stderr"])
        flip = bytearray(btar.read_bytes())
        flip[len(flip) // 2] ^= 0xFF
        cc = make_pack_case("base-pin", base_bytes=bytes(flip))
        rcc = run_pack_case(cc)
        case("pack-base-pin: a base tar whose sha256 differs from the pin -> exit 1, nothing packed, nothing uploaded, nothing extracted",
             rcc == 1 and "base" in cc["stderr"] and not fake_calls(cc) and not puts(cc)
             and not (cc["knobs"].work / "hf").exists(), cc["stderr"].strip()[:100])
        cc = make_pack_case("unsafe-path")
        man = pobj(cc, "training-manifest.json")
        man["entries"].append({"path": "run/../../escape", "size": 1, "sha256": "a" * 64})
        (cc["root"] / "training-manifest.json").write_text(json.dumps(man))
        rcc = run_pack_case(cc)
        case("pack-integrity: a manifest path with `..` is refused (exit 1), nothing downloaded to it",
             rcc == 1 and "integrity" in cc["stderr"] and not (cc["knobs"].work.parent / "escape").exists() and not fake_calls(cc))
        cc = make_pack_case("partial-pass", partial=True)
        rcc = run_pack_case(cc)
        case("pack: an outcome pass with no complete training-manifest.json (only the partial one) is exit 1, never packed",
             rcc == 1 and "manifest" in cc["stderr"] and not fake_calls(cc) and not puts(cc))
        cc = make_pack_case("no-outcome")
        (cc["root"] / "outcome.json").unlink()
        rcc = run_pack_case(cc)
        case("pack: an absent outcome.json is exit 1 (download), nothing recorded", rcc == 1 and "download" in cc["stderr"] and not puts(cc))
        cc = make_pack_case("apr-upload-fails", runner_kw={"fail": lambda key: key.endswith(".apr")})
        rcc = run_pack_case(cc)
        o = pobj(cc, "pack-outcome.json") if (cc["root"] / "pack-outcome.json").exists() else {}
        case("pack: a failed .apr upload records accepted false (no model is claimed that is not in the bucket) and exits 1",
             rcc == 1 and o.get("accepted") is False and o.get("apr_sha256") is None and not (cc["root"] / "motiv-1.apr").exists())

        print("pack-image-env:")
        for label, mutate in (("missing", lambda e: e.pop("LAYA_PACK_IMAGE_DIGEST")),
                              ("uppercase hex", lambda e: e.update(LAYA_PACK_IMAGE_DIGEST="sha256:" + "AB" * 32)),
                              ("short", lambda e: e.update(LAYA_PACK_IMAGE_DIGEST="sha256:" + "ab" * 31)),
                              ("no prefix", lambda e: e.update(LAYA_PACK_IMAGE_DIGEST="ab" * 32)),
                              ("run prefix mismatch", lambda e: e.update(LAYA_RUN_PREFIX="runs/other/%s/" % task)),
                              ("missing LAYA_BUCKET", lambda e: e.pop("LAYA_BUCKET"))):
            cc = make_pack_case("env-" + re.sub(r"\W", "", label))
            mutate(cc["env"])
            rcc = run_pack_case(cc)
            case("pack-image-env: %s -> REFUSED env:, exit 2, no download and no upload" % label,
                 rcc == 2 and "REFUSED env:" in cc["stderr"] and not cc["runner"].calls and pdigest not in cc["stderr"],
                 cc["stderr"].strip()[:90])
        cc = make_pack_case("env-contracts")
        cc["knobs"] = PackKnobs(**{**cc["knobs"].__dict__, "contracts_dir": Path(tmp) / "no-contracts"})
        rcc = run_pack_case(cc)
        case("pack-image-env: a pack image without its contracts is REFUSED contracts: (exit 2) before any download",
             rcc == 2 and "REFUSED contracts:" in cc["stderr"] and not cc["runner"].calls)
        errbuf = io.StringIO()
        with Env(LAYA_STEP="pack"), contextlib.redirect_stderr(errbuf):
            saved = {k: os.environ.pop(k, None) for k in ("LAYA_TASK_ID", "LAYA_NAME", "LAYA_BUCKET", "LAYA_RUN_PREFIX", "LAYA_PACK_IMAGE_DIGEST")}
            try:
                mrc = main(["run_step.py", "pack"])
            finally:
                for k, v in saved.items():
                    if v is not None:
                        os.environ[k] = v
        case("pack: `run_step.py pack` dispatches to pack mode (an empty env is REFUSED env:, exit 2)",
             mrc == 2 and "REFUSED env:" in errbuf.getvalue())
        rust_layout = ROOT / "crates" / "aprender-mcp-laya-train" / "src" / "layout.rs"
        real_key = base_key_from_pin(json.loads((HERE / "base-stage.json").read_text()))
        if rust_layout.is_file():
            m_key = re.search(r'pub const BASE_KEY: &str = "([^"]+)";', rust_layout.read_text(encoding="utf-8"))
            case("pack: the base key derived from base-stage.json equals layout::BASE_KEY in the Rust crate",
                 bool(m_key) and m_key.group(1) == real_key, real_key)
        else:
            case("pack: the base key derived from base-stage.json has the base/laya-<8 hex>/<tar> shape (Rust source absent)",
                 re.fullmatch(r"base/laya-[0-9a-f]{8}/laya-base\.tar", real_key) is not None, real_key)
        case("pack: parse_packed_line reads the real PACKED shape and refuses a malformed one",
             parse_packed_line("PACKED /w/x.apr sha256=%s rescore_max_abs=1 shipped_seed=20260927 argmax=3/3 waived=W-1" % ("a" * 64))
             == ("a" * 64, 20260927, "W-1")
             and parse_packed_line("PACKED /w/x.apr sha256=%s shipped_seed=null argmax=3/3 waived=none" % ("b" * 64))
             == ("b" * 64, None, None)
             and parse_packed_line("PACKED waived=none") is None and parse_packed_line("REFUSED X y") is None)

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
    if len(argv) == 3 and argv[1] == "--selftest-child":
        return selftest_child(json.loads(argv[2]))
    if len(argv) != 2 or argv[1] not in ("train", "pack"):
        print("usage: run_step.py train | pack | --selftest", file=sys.stderr)
        return 2
    step = os.environ.get("LAYA_STEP", "") or argv[1]
    if step == "pack":
        return entry_pack(os.environ, aws_runner, PackKnobs())
    paths = Paths()
    knobs = Knobs(prepare_base=lambda: stage_channel_base(paths.base_tar, HERE / "base-stage.json", paths.hf_home))
    if step == "train":
        return entry_train(os.environ, paths, aws_runner, knobs)
    if step == "probe":
        return entry_probe(os.environ, paths, aws_runner, knobs)
    if step == "probe-sigterm":
        return entry_probe_sigterm(os.environ, paths, aws_runner, knobs)
    print("REFUSED env: LAYA_STEP %r is not a mode of this image" % step, file=sys.stderr, flush=True)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
