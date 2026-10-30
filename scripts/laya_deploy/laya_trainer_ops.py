#!/usr/bin/env python3
"""The owner-run AWS recipes of the Laya training server's first deployment (plans 14-09 and 14-10; D-01, D-02, D-03, D-07, D-10, D-13, D-16).

    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] image  WHICH ENV PROFILE [--list-files]
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] infra  ACTION ENV PROFILE
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] stage  base ENV PROFILE [--tar FILE]
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] stage  dataset DIR PREFIX ENV PROFILE [--expect motivation-v1]
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] probe  cuda|sigterm ENV PROFILE
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] config ENV PROFILE [--template FILE   (DRY_RUN=1 only)]
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] deploy ENV PROFILE
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] grant  ENV PROFILE        (no URL, by design)
    python3 -I scripts/laya_deploy/laya_trainer_ops.py finalizer-provenance write|verify ASSET_DIR [--target TRIPLE]
    python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state DIR [--env dev|prod]
    python3 -I scripts/laya_deploy/laya_trainer_ops.py --selftest

`just laya-trainer-image | laya-trainer-infra | laya-trainer-stage | laya-trainer-probe` are thin wrappers over this file
(ACTION is diff-images | deploy-images | outputs-images | diff | deploy | outputs; WHICH is train | pack). `--state-dir`
may stand anywhere on the command line.

EVERY recipe, by construction:

  * accepts only profile ze-kasher-dev and env dev|prod; anything else is `REFUSED profile:` / `REFUSED env:` (exit 2)
    before any command runs (D-01);
  * because a profile NAME is not an account identity, runs `just _laya-account-check <profile> <region>` as its first
    command, and its refusal stops the subcommand with exit 2 before any other AWS call. The region is read from
    `just _laya-target-check` (a pure local read);
  * under DRY_RUN=1 prints every command it would run (`+ ...`) and executes none of the AWS or CDK ones. Local read-only
    commands (`git ls-tree`, `git status`, `git rev-parse`, the two `just` checks) do run, so a dry run reports what a
    live run would refuse. A dry run ends `AWS CALLS: 0`;
  * prints no 12-digit number: ECR URIs, bucket names and role ARNs embed the account id, so every printed line passes
    through redact() (repository NAMES and digests are printed; URIs live only in the gitignored state files).

State lives under --state-dir (default models/laya-trainer, gitignored by the root-anchored /models). A state dir inside the
repository that `git check-ignore` does not ignore is `REFUSED state:`; one outside the repository is accepted. Files:

    outputs-images-<env>.json   {schema, env, outputs: {ContextBucketName, TrainRepoUri, PackRepoUri, TrainProjectName, PackProjectName}}
    outputs-<env>.json          {schema, env, trainer: {the 12 trainer stack outputs}, images: {the 5 images stack outputs}}
    images-<env>.json           {schema, env, train: {repo_uri, repo_name, digest, commit, build_id, image_bytes}, pack: {...}}
    datasets-<env>.json         {schema, env, datasets: {<PREFIX>: {prefix, files: {name: sha256}}}}
    probe-<id>.json             {schema, mode, probe_id, job_name, final_status, failure_reason, billable_seconds, ..., probe}

LIVE RUNS THAT OUTLAST THE BASH TOOL: `image` (a CodeBuild wait of up to 90 min) and `probe` (a training job of up to 30 min
plus start-up) run longer than the Bash tool's 10-minute foreground cap. Start them with run_in_background and judge them by
their LAST line, never by the wrapper's exit status: `IMAGE OK <which> <digest>` or `IMAGE FAILED <which> <status>`;
`PROBE DONE <mode> <final status>` or `PROBE FAILED <mode> <detail>`; `INFRA OK <action>` or `INFRA FAILED <action> ...`.

Image context. `image` builds its zip from `git archive HEAD -- <paths>` (the committed bytes the commit tag names), never the
working tree: the paths are the image's own context list (train: scripts/laya_train/container/context-paths.txt; pack:
deploy-extensions/container/pack-arm64/context-paths.txt) plus the image's Dockerfile, which CodeBuild needs at the context
root (`docker build -f $DOCKERFILE .`) and which the pack list does not name. The clean-tree check is scoped to exactly those
paths, so a modified .planning/ file never refuses a build. Zip entries are sorted with fixed timestamps.

Finalizer provenance (plan 14-19, WR-04). `just laya-trainer-synth` runs `finalizer-provenance write` after it copies the
finalizer into deploy-extensions/assets/laya-finalizer/, which writes bootstrap.build.json beside it: the commit, the source
closure (the workspace directories `cargo tree -p aprender-mcp-laya-train-lambda -e normal --prefix none --format {p} --target
<triple>` names with a local path, plus Cargo.lock, Cargo.toml and rust-toolchain.toml), whether those paths were clean, the
binary's sha256, the target and the UTC time. A dirty closure is recorded, not refused, at write time. `infra diff` and `infra
deploy` then refuse (`REFUSED infra: finalizer provenance: <reason>`; a dry run notes it) unless the asset's sha256 is the
recorded one, the build was clean, the recorded commit is an ancestor of HEAD, no closure path differs between it and HEAD and
none is dirty now. The check runs after the account check and before any cdk call. It attests what was built from what; it is
not byte reproducibility (a re-synth of unchanged source writes a different binary and a new sidecar).

Publishing the request function (plan 14-10, D-10). `config` fills the tracked deploy template's three placeholders from
outputs-<env>.json into <state-dir>/deploy-config-<env>.toml (mode 600); the template declares NO [[iam.statements]] and
`[auth] enabled = false`, and a template that declares IAM is refused. `deploy` first revokes any inline grant
aprender-laya-train-<env> from the function's role, FAIL-CLOSED (a `--no-oauth` redeploy may reset OAuth): an absent
function or policy proceeds, a found policy is deleted and must read back NoSuchEntity, and anything else refuses with
`REFUSED deploy: cannot confirm the grant is absent`. It then runs `_laya-crates-root-swap crates <cfg> <snap> cargo pmcp
deploy --manifest-path crates ... --no-oauth` (the shared crates root comes back byte-identical), requires the build log to
name aprender-mcp-laya-train-lambda (`REFUSED deploy: wrong package`), parses the endpoint from the log, requires its host's
first label to be aprender-mcp-laya-train and the tenant suffix of the profile, and writes <state-dir>/deploy-<env>.json
{server_id, package, endpoint, deployed_at} (mode 600). A live deploy needs LAYA_TRAINER_APPROVED_DEPLOY=<env>.

`grant` attaches the stack's RequestLambdaPolicy as the inline policy aprender-laya-train-<env> on the role of function
aprender-mcp-laya-train, and only after it has OBSERVED OAuth itself, in the same invocation, on an endpoint bound to that
function: it takes NO URL (an extra argument is refused as unknown); the endpoint comes from the verified deploy state
<state-dir>/deploy-<env>.json, its host's first label must be the server id and GET /health on it must answer 200 naming
aprender-mcp-laya-train (`REFUSED grant: endpoint not bound to aprender-mcp-laya-train`); then `just _laya-auth-refusal-check
<endpoint> 0 train_status` must see tools/list AND a train_status tools/call refused with 401 (`REFUSED grant: OAuth not
confirmed (tools/list answered <code>)`, or `REFUSED grant: endpoint unreachable (transport error; retry)` when curl could not
connect), and only then `_laya-account-check`, the role lookup and the put-role-policy run. A live grant needs
LAYA_TRAINER_APPROVED_GRANT=<env>. A 401 seen on an unrelated endpoint can therefore never authorize this function's grant.

stdlib only. Every external command goes through ONE Runner (aws, npx cdk, git, just, cargo tree); --selftest replaces it with a
simulated world and never reaches AWS, the network or the real account.
"""

import contextlib
import hashlib
import io
import json
import os
import re
import shlex
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile
from datetime import datetime, timezone
from pathlib import Path

PROFILE = "ze-kasher-dev"
ENVS = ("dev", "prod")
REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_STATE_DIR = "models/laya-trainer"
STATE_PROBE_NAME = ".laya-state-probe"
CRED_VARS = ("AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN", "AWS_SECURITY_TOKEN")

ACCT_RE = re.compile(r"(?<![0-9A-Za-z])[0-9]{12}(?![0-9A-Za-z])")
DIGEST_RE = re.compile(r"sha256:[0-9a-f]{64}")
COMMIT_RE = re.compile(r"[0-9a-f]{40}")
SHA_RE = re.compile(r"[0-9a-f]{64}")
REGION_RE = re.compile(r"[a-z]{2}-[a-z]+-[0-9]")
BUILD_ID_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.:-]{3,200}")
BUCKET_RE = re.compile(r"[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]")
ZERO_DIGEST = "sha256:" + "0" * 64
CTRL_RE = re.compile(r"[\x00-\x1f\x7f\\]")

IMAGES = {
    "train": {
        "paths_file": "scripts/laya_train/container/context-paths.txt",
        "dockerfile": "scripts/laya_train/container/Dockerfile.train",
        "repo_key": "TrainRepoUri",
        "project_key": "TrainProjectName",
    },
    "pack": {
        "paths_file": "deploy-extensions/container/pack-arm64/context-paths.txt",
        "dockerfile": "deploy-extensions/container/pack-arm64/Dockerfile",
        "repo_key": "PackRepoUri",
        "project_key": "PackProjectName",
    },
}
BUILD_POLL_S = 30
BUILD_CAP_S = 90 * 60
BUILD_TERMINAL = ("SUCCEEDED", "FAILED", "FAULT", "TIMED_OUT", "STOPPED")

IMAGES_STACK = "aprender-laya-trainer-images-%s"
TRAINER_STACK = "aprender-laya-trainer-%s"
IMAGES_OUTPUT_KEYS = ("ContextBucketName", "TrainRepoUri", "PackRepoUri", "TrainProjectName", "PackProjectName")
TRAINER_OUTPUT_KEYS = ("BucketName", "TableName", "StateMachineArn", "FinalizerName", "SageMakerRoleArn", "TrainRepoUri",
                       "PackTaskDefinitionArn", "ClusterArn", "SubnetIds", "SecurityGroupId", "FinalizerDlqUrl",
                       "RequestLambdaPolicy")
FINALIZER_DIR = "deploy-extensions/assets/laya-finalizer"
FINALIZER_ASSET = FINALIZER_DIR + "/bootstrap"
INFRA_ACTIONS = ("diff-images", "deploy-images", "outputs-images", "diff", "deploy", "outputs")


class Refused(Exception):
    """An input or state this tool will not act on: one `REFUSED <rule>: <detail>` line, exit 2."""

    def __init__(self, rule, msg):
        super().__init__(rule)
        self.rule = rule
        self.msg = msg


class Failure(Exception):
    """An infrastructure error (exit 1): `last` is the deterministic last line a backgrounded run is judged by."""

    def __init__(self, msg, last=None):
        super().__init__(msg)
        self.msg = msg
        self.last = last


def refuse(rule, msg):
    raise Refused(rule, msg)


def redact(text):
    return ACCT_RE.sub("<account>", text)


# ---- the Runner seam ----------------------------------------------------------------------------------------

class Result:
    def __init__(self, rc=0, out="", err=""):
        self.rc = rc
        self.out = out
        self.err = err


class Runner:
    """The ONE place a command is run. Under dry mode every command is printed and only `local` ones execute."""

    def __init__(self, dry, say):
        self.dry = dry
        self.say = say
        self.aws_calls = 0

    def run(self, argv, local=False, dry_out="", env=None, cwd=None, stream=False, timeout=None, quiet=False):
        argv = [str(a) for a in argv]
        if not quiet:
            self.say("+ " + shlex.join(argv))
        if self.dry and not local:
            return Result(0, dry_out, "")
        if argv[0] == "aws" or argv[:2] == ["npx", "cdk"]:
            self.aws_calls += 1
        return self._exec(argv, env or {}, cwd, stream, timeout)

    def _exec(self, argv, env, cwd, stream, timeout):
        child = {k: v for k, v in os.environ.items() if k not in CRED_VARS}
        for k, v in env.items():
            if v is None:
                child.pop(k, None)
            else:
                child[k] = v
        if self.dry:
            child["DRY_RUN"] = "1"
        try:
            if stream:
                proc = subprocess.Popen(argv, cwd=cwd, env=child, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                        stderr=subprocess.STDOUT, text=True, errors="replace")
                lines = []
                for line in proc.stdout:
                    lines.append(line)
                    self.say(line.rstrip("\n"))
                proc.wait()
                return Result(proc.returncode, "".join(lines), "")
            p = subprocess.run(argv, cwd=cwd, env=child, stdin=subprocess.DEVNULL, capture_output=True, text=True,
                               errors="replace", timeout=timeout)
            return Result(p.returncode, p.stdout, p.stderr)
        except FileNotFoundError:
            return Result(127, "", "%s: command not found" % argv[0])
        except subprocess.TimeoutExpired:
            return Result(124, "", "%s timed out" % argv[0])


class Ctx:
    """Everything a subcommand touches besides its arguments, so --selftest can replace all of it."""

    def __init__(self, runner=None, env=None, stdout=None, stderr=None, sleep=None, utcnow=None, repo_root=None,
                 is_ignored=None, file_exists=None):
        self.env = dict(os.environ) if env is None else env
        self.stdout = stdout or sys.stdout
        self.stderr = stderr or sys.stderr
        self.dry = self.env.get("DRY_RUN", "") == "1"
        self.runner = runner or Runner(self.dry, self.say)
        self.sleep = sleep or time.sleep
        self.clock = time.monotonic
        self.utcnow = utcnow or (lambda: datetime.now(timezone.utc))
        self.repo_root = Path(repo_root) if repo_root else REPO_ROOT
        self.is_ignored = is_ignored or self._git_ignored
        self.file_exists = file_exists or os.path.isfile
        self.base_stage = str(self.repo_root / BASE_STAGE_JSON)
        self.finalizer_dir = self.repo_root / FINALIZER_DIR
        self.state = None
        self.target = ""
        self.suffix = ""

    def say(self, line):
        print(redact(line), file=self.stdout, flush=True)

    def warn(self, line):
        print(redact(line), file=self.stderr, flush=True)

    def _git_ignored(self, rel):
        p = subprocess.run(["git", "-C", str(self.repo_root), "check-ignore", "-q", "--", rel],
                           stdin=subprocess.DEVNULL, capture_output=True)
        return p.returncode == 0


# ---- validation and state -----------------------------------------------------------------------------------

def check_profile_env(profile, env):
    if profile != PROFILE:
        refuse("profile", "profile %r is not allowed in this phase (only %s)" % (profile[:40], PROFILE))
    if env not in ENVS:
        refuse("env", "environment must be dev or prod, got %r" % env[:40])


def state_dir(ctx, opt, create):
    p = Path(opt).resolve() if opt else (ctx.repo_root / DEFAULT_STATE_DIR).resolve()
    root = ctx.repo_root.resolve()
    if p == root or root in p.parents:
        rel = str(p.relative_to(root) / STATE_PROBE_NAME) if p != root else STATE_PROBE_NAME
        if not ctx.is_ignored(rel):
            refuse("state", "state dir %s is inside the repository and `git check-ignore` does not ignore it "
                            "(it would hold account ids); use models/laya-trainer or a directory outside the repository"
                   % (p.relative_to(root) if p != root else "."))
    if create and not p.exists():
        os.makedirs(p, mode=0o700, exist_ok=True)
        os.chmod(p, 0o700)
    ctx.state = p
    return p


def read_state(ctx, name, hint):
    path = ctx.state / name
    try:
        raw = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        refuse("state", "%s is missing; %s" % (name, hint))
    except OSError as e:
        refuse("state", "%s cannot be read (%s)" % (name, type(e).__name__))
    try:
        data = json.loads(raw)
    except ValueError:
        refuse("state", "%s is not valid JSON" % name)
    if not isinstance(data, dict):
        refuse("state", "%s is not a JSON object" % name)
    return data


def read_state_optional(ctx, name):
    if not (ctx.state / name).exists():
        return None
    return read_state(ctx, name, "")


def write_state(ctx, name, data):
    path = ctx.state / name
    tmp = ctx.state / (".%s.tmp-%d" % (name, os.getpid()))
    fd = os.open(str(tmp), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2, sort_keys=True)
        fh.write("\n")
    os.replace(tmp, path)
    os.chmod(path, 0o600)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def passthrough_refusal(res, default_rule):
    for line in (res.err + "\n" + res.out).splitlines():
        m = re.match(r"REFUSED ([A-Za-z0-9_-]+): (.*)", line)
        if m:
            refuse(m.group(1), m.group(2))
    refuse(default_rule, "the check exited %d without a REFUSED line" % res.rc)


def target_region(ctx, profile):
    """The pure local half of the account gate: the region (and the tenant host suffix) of the profile's pmcp.run target."""
    root = str(ctx.repo_root)
    t = ctx.runner.run(["just", "_laya-target-check", profile, ""], local=True, cwd=root, quiet=True)
    if t.rc != 0:
        passthrough_refusal(t, "target")
    parts = t.out.split()
    if len(parts) < 2 or not REGION_RE.fullmatch(parts[1]):
        refuse("target", "the target check answered no region")
    ctx.target = parts[0]
    ctx.suffix = parts[2] if len(parts) > 2 else ""
    return parts[1]


def account_gate(ctx, profile, region):
    a = ctx.runner.run(["just", "_laya-account-check", profile, region], local=True, cwd=str(ctx.repo_root))
    if a.rc != 0:
        passthrough_refusal(a, "target-account")


def connect(ctx, profile):
    """The account gate: region from the target table, then `_laya-account-check` before any other AWS call."""
    region = target_region(ctx, profile)
    account_gate(ctx, profile, region)
    return region


def aws(ctx, profile, region, *args, dry_out="", stream=False, timeout=None):
    """One aws call, always with an explicit profile and region."""
    argv = ["aws", *args, "--profile", profile, "--region", region]
    return ctx.runner.run(argv, dry_out=dry_out, env=child_env(profile, region), stream=stream, timeout=timeout)


def child_env(profile, region):
    return {"AWS_PROFILE": profile, "AWS_REGION": region, "AWS_DEFAULT_REGION": region}


def finish_dry(ctx, what):
    ctx.say("DRY RUN %s: every command above was printed, none was executed" % what)
    ctx.say("AWS CALLS: %d" % ctx.runner.aws_calls)


# ---- placeholder state (for dry runs that must not fake the real state) ---------------------------------------

def placeholder_state(env):
    acct = "123456789012"
    images_outputs = {
        "ContextBucketName": "aprender-laya-build-context-%s-%s" % (env, acct),
        "TrainRepoUri": "%s.dkr.ecr.us-east-1.amazonaws.com/aprender-laya-train-%s" % (acct, env),
        "PackRepoUri": "%s.dkr.ecr.us-east-1.amazonaws.com/aprender-laya-pack-%s" % (acct, env),
        "TrainProjectName": "aprender-laya-train-image-%s" % env,
        "PackProjectName": "aprender-laya-pack-image-%s" % env,
    }
    trainer = {
        "BucketName": "aprender-laya-train-%s-%s" % (env, acct),
        "TableName": "aprender-laya-train-%s" % env,
        "StateMachineArn": "arn:aws:states:us-east-1:%s:stateMachine:aprender-laya-train-%s" % (acct, env),
        "FinalizerName": "aprender-laya-train-finalizer-%s" % env,
        "SageMakerRoleArn": "arn:aws:iam::%s:role/aprender-laya-sagemaker-%s" % (acct, env),
        "TrainRepoUri": images_outputs["TrainRepoUri"],
        "PackTaskDefinitionArn": "arn:aws:ecs:us-east-1:%s:task-definition/aprender-laya-pack-%s:1" % (acct, env),
        "ClusterArn": "arn:aws:ecs:us-east-1:%s:cluster/aprender-laya-%s" % (acct, env),
        "SubnetIds": "subnet-00000001,subnet-00000002",
        "SecurityGroupId": "sg-00000001",
        "FinalizerDlqUrl": "https://sqs.us-east-1.amazonaws.com/%s/aprender-laya-train-finalizer-dlq-%s" % (acct, env),
        "RequestLambdaPolicy": "{}",
    }
    images = {
        "schema": "laya-trainer-images-v1",
        "env": env,
        "train": {"repo_uri": images_outputs["TrainRepoUri"], "repo_name": "aprender-laya-train-%s" % env,
                  "digest": "sha256:" + "a" * 64, "commit": "1" * 40, "build_id": "aprender-laya-train-image-%s:placeholder" % env,
                  "image_bytes": 1},
        "pack": {"repo_uri": images_outputs["PackRepoUri"], "repo_name": "aprender-laya-pack-%s" % env,
                 "digest": "sha256:" + "b" * 64, "commit": "1" * 40, "build_id": "aprender-laya-pack-image-%s:placeholder" % env,
                 "image_bytes": 1},
    }
    return {
        "outputs-images-%s.json" % env: {"schema": "laya-trainer-images-outputs-v1", "env": env, "outputs": images_outputs},
        "outputs-%s.json" % env: {"schema": "laya-trainer-outputs-v1", "env": env, "trainer": trainer, "images": images_outputs},
        "images-%s.json" % env: images,
    }


def write_placeholder_state(ctx, dest, env):
    """Placeholder state files for a dry run, written only into a directory that is not the real state dir's content."""
    if env not in ENVS:
        refuse("env", "environment must be dev or prod, got %r" % env[:40])
    p = state_dir(ctx, dest, create=True)
    files = placeholder_state(env)
    for name in files:
        if (p / name).exists():
            refuse("state", "%s already exists in %s; placeholder state is never written over a state file" % (name, dest))
    for name, data in files.items():
        write_state(ctx, name, data)
    ctx.say("placeholder state for env %s written to %s (%s)" % (env, dest, ", ".join(sorted(files))))
    return 0


# ---- image ---------------------------------------------------------------------------------------------------

def read_context_paths(ctx, spec):
    path = ctx.repo_root / spec["paths_file"]
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        refuse("image", "the context list %s cannot be read" % spec["paths_file"])
    paths = []
    for raw in lines:
        s = raw.strip()
        if not s or s.startswith("#"):
            continue
        if s.startswith("/") or s.startswith(":") or ".." in s.split("/") or CTRL_RE.search(s) or re.search(r"[*?\[]", s):
            refuse("image", "context path %r in %s is not a plain repository-relative path" % (s[:60], spec["paths_file"]))
        paths.append(s)
    if not paths:
        refuse("image", "the context list %s names no path" % spec["paths_file"])
    return paths


def covers(path, name):
    return name == path or name.startswith(path.rstrip("/") + "/")


def tar_to_zip(tar_path, zip_path):
    """A deterministic zip of a tar's files and symlinks: sorted names, fixed timestamps, normalized modes."""
    names = []
    with tarfile.open(tar_path) as tf, zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as zf:
        members = sorted((m for m in tf.getmembers() if m.isfile() or m.issym()), key=lambda m: m.name)
        for m in members:
            n = m.name
            if n.startswith("/") or ".." in n.split("/") or not n:
                raise ValueError("archive entry %r escapes the context" % n)
            zi = zipfile.ZipInfo(n, (1980, 1, 1, 0, 0, 0))
            zi.create_system = 3
            zi.compress_type = zipfile.ZIP_DEFLATED
            if m.issym():
                zi.external_attr = (stat.S_IFLNK | 0o777) << 16
                zf.writestr(zi, m.linkname)
            else:
                zi.external_attr = (stat.S_IFREG | (0o755 if m.mode & 0o111 else 0o644)) << 16
                with tf.extractfile(m) as src, zf.open(zi, "w") as dst:
                    shutil.copyfileobj(src, dst)
            names.append(n)
    return names


def cmd_image(ctx, pos, opts):
    if len(pos) != 3:
        refuse("usage", "image WHICH ENV PROFILE (WHICH is train or pack)")
    which, env, profile = pos
    check_profile_env(profile, env)
    if which not in IMAGES:
        refuse("usage", "image WHICH must be train or pack, got %r" % which[:40])
    spec = IMAGES[which]
    paths = read_context_paths(ctx, spec)
    pathspec = list(paths)
    if not any(covers(p, spec["dockerfile"]) for p in paths):
        pathspec.append(spec["dockerfile"])
    state_dir(ctx, opts.get("state_dir"), create=not ctx.dry)
    region = connect(ctx, profile)
    root = str(ctx.repo_root)
    git = ["git", "--literal-pathspecs"]
    head = ctx.runner.run(git + ["rev-parse", "HEAD"], local=True, cwd=root).out.strip()
    if not COMMIT_RE.fullmatch(head):
        raise Failure("git rev-parse HEAD did not answer a 40-hex commit", "IMAGE FAILED %s NO_COMMIT" % which)
    listing = ctx.runner.run(git + ["ls-tree", "-r", "--name-only", "HEAD", "--", *pathspec], local=True, cwd=root)
    files = sorted(n for n in listing.out.splitlines() if n)
    ctx.say("image %s: commit %s; context %d files from %d paths" % (which, head, len(files), len(pathspec)))
    for p in pathspec:
        n = sum(1 for f in files if covers(p, f))
        ctx.say("  context path %s (%d files at HEAD)" % (p, n))
        if n == 0:
            refuse("image", "context path %s has no file at HEAD" % p)
    if opts.get("list_files"):
        for f in files:
            ctx.say("  file %s" % f)
    status = ctx.runner.run(git + ["status", "--porcelain", "--", *pathspec], local=True, cwd=root)
    dirty = [ln for ln in status.out.splitlines() if ln.strip()]
    if dirty:
        first = dirty[0][3:] if len(dirty[0]) > 3 else dirty[0]
        msg = "context path %s has uncommitted changes (%d entries); commit them first" % (first, len(dirty))
        if ctx.dry:
            ctx.say("DRY RUN NOTE: a live run would be REFUSED image: " + msg)
        else:
            refuse("image", msg)
    out = read_state(ctx, "outputs-images-%s.json" % env, "run `just laya-trainer-infra outputs-images %s %s` first" % (env, profile))
    o = out.get("outputs") if isinstance(out.get("outputs"), dict) else {}
    bucket, repo_uri, project = (o.get("ContextBucketName", ""), o.get(spec["repo_key"], ""), o.get(spec["project_key"], ""))
    if not BUCKET_RE.fullmatch(bucket) or "/" not in repo_uri or not project:
        refuse("state", "outputs-images-%s.json lacks ContextBucketName, %s or %s" % (env, spec["repo_key"], spec["project_key"]))
    repo_name = repo_uri.split("/", 1)[1]
    key = "context/%s-%s.zip" % (which, head)
    tar_path = ctx.state / (".context-%s-%s.tar" % (which, head[:12]))
    zip_path = ctx.state / ("context-%s-%s.zip" % (which, head[:12]))
    ctx.runner.run(git + ["archive", "--format=tar", "-o", str(tar_path), "HEAD", "--", *pathspec], cwd=root)
    if not ctx.dry:
        try:
            names = tar_to_zip(tar_path, zip_path)
        except (OSError, tarfile.TarError, ValueError) as e:
            raise Failure("could not assemble the context zip: %s" % str(e)[:120], "IMAGE FAILED %s NO_CONTEXT" % which)
        finally:
            with contextlib.suppress(OSError):
                tar_path.unlink()
        if sorted(names) != files or spec["dockerfile"] not in names:
            with contextlib.suppress(OSError):
                zip_path.unlink()
            raise Failure("the archive differs from the HEAD tree list", "IMAGE FAILED %s CONTEXT_MISMATCH" % which)
        ctx.say("context zip: %d entries, %d bytes (sorted, fixed timestamps)" % (len(names), zip_path.stat().st_size))
    cp = aws(ctx, profile, region, "s3", "cp", str(zip_path), "s3://%s/%s" % (bucket, key), "--only-show-errors")
    if cp.rc != 0:
        raise Failure("the context upload failed: %s" % first_line(cp.err), "IMAGE FAILED %s UPLOAD" % which)
    with contextlib.suppress(OSError):
        zip_path.unlink()
    envs = ["name=DOCKERFILE,value=%s,type=PLAINTEXT" % spec["dockerfile"], "name=REPO_URI,value=%s,type=PLAINTEXT" % repo_uri,
            "name=GIT_COMMIT,value=%s,type=PLAINTEXT" % head]
    start = aws(ctx, profile, region, "codebuild", "start-build", "--project-name", project, "--source-type-override", "S3",
                "--source-location-override", "%s/%s" % (bucket, key), "--environment-variables-override", *envs,
                "--query", "build.id", "--output", "text", dry_out="<build-id>\n")
    build_id = start.out.strip()
    if start.rc != 0 or (not ctx.dry and not BUILD_ID_RE.fullmatch(build_id)):
        raise Failure("start-build failed: %s" % first_line(start.err), "IMAGE FAILED %s START" % which)
    ctx.say("build id: %s" % build_id)
    if ctx.dry:
        aws(ctx, profile, region, "codebuild", "batch-get-builds", "--ids", build_id, "--query", "builds[0].buildStatus", "--output", "text")
        aws(ctx, profile, region, "ecr", "describe-images", "--repository-name", repo_name, "--image-ids", "imageTag=%s" % head,
            "--output", "json")
        ctx.say("would poll the build every %d s for up to %d min, then record %s in images-%s.json" % (BUILD_POLL_S, BUILD_CAP_S // 60, which, env))
        finish_dry(ctx, "image %s" % which)
        return 0
    deadline = ctx.clock() + BUILD_CAP_S
    last = None
    while True:
        st = aws(ctx, profile, region, "codebuild", "batch-get-builds", "--ids", build_id, "--query", "builds[0].buildStatus",
                 "--output", "text")
        status = st.out.strip() if st.rc == 0 else "UNKNOWN"
        if status != last:
            ctx.say("build %s: %s" % (build_id, status))
            last = status
        if status in BUILD_TERMINAL:
            break
        if ctx.clock() > deadline:
            ctx.say("build id %s is still running after %d minutes; it was NOT stopped" % (build_id, BUILD_CAP_S // 60))
            ctx.say("IMAGE FAILED %s POLL_TIMEOUT" % which)
            return 1
        ctx.sleep(BUILD_POLL_S)
    if status != "SUCCEEDED":
        ctx.say("build id: %s (see its CodeBuild log)" % build_id)
        ctx.say("IMAGE FAILED %s %s" % (which, status))
        return 1
    di = aws(ctx, profile, region, "ecr", "describe-images", "--repository-name", repo_name, "--image-ids", "imageTag=%s" % head,
             "--output", "json")
    digest, size = None, None
    if di.rc == 0:
        try:
            d0 = json.loads(di.out)["imageDetails"][0]
            digest, size = d0.get("imageDigest"), d0.get("imageSizeInBytes")
        except (ValueError, KeyError, IndexError, TypeError):
            pass
    if not isinstance(digest, str) or not DIGEST_RE.fullmatch(digest):
        ctx.say("build id: %s" % build_id)
        ctx.say("IMAGE FAILED %s NO_DIGEST" % which)
        return 1
    imgs = read_state_optional(ctx, "images-%s.json" % env) or {}
    imgs.update({"schema": "laya-trainer-images-v1", "env": env})
    imgs[which] = {"repo_uri": repo_uri, "repo_name": repo_name, "digest": digest, "commit": head, "build_id": build_id,
                   "image_bytes": size if isinstance(size, int) else None}
    write_state(ctx, "images-%s.json" % env, imgs)
    ctx.say("image %s: repository %s digest %s commit %s" % (which, repo_name, digest, head))
    ctx.say("IMAGE OK %s %s" % (which, digest))
    return 0


def first_line(text):
    lines = [ln for ln in (text or "").splitlines() if ln.strip()]
    return lines[-1][:160] if lines else "no error text"


# ---- finalizer provenance (plan 14-19; WR-04, 14-SECURITY W-3) ------------------------------------------------
#
# The finalizer is the one writer of the terminal record, the verdict, the manifest, the tags and the lock release,
# and its Lambda asset is an UNTRACKED binary (deploy-extensions/assets/ is gitignored) written by `just
# laya-trainer-synth` from whatever the working tree held. `finalizer-provenance write` records, beside the asset,
# what it was built from; `infra diff` and `infra deploy` refuse an asset that record cannot attribute to a commit on
# HEAD's history and a source closure that is clean and unchanged since. This is ATTESTATION of what was built from
# what, not byte reproducibility (W-3 stays open): a re-synth of unchanged source yields a different binary and a new
# sidecar. The closure is DERIVED with cargo tree (never a hard-coded list) and is scoped, so a commit touching only
# other paths (planning documents) never refuses, while a commit touching any closure crate, Laya or not, does.

FINALIZER_SIDECAR = "bootstrap.build.json"
FINALIZER_SCHEMA = "laya-finalizer-build-v1"
FINALIZER_PACKAGE = "aprender-mcp-laya-train-lambda"
FINALIZER_TARGET = "aarch64-unknown-linux-gnu"
CLOSURE_ROOT_FILES = ("Cargo.lock", "Cargo.toml", "rust-toolchain.toml")
TARGET_RE = re.compile(r"[a-z0-9_]+(?:-[a-z0-9_.]+){1,3}")
CLOSURE_PATH_RE = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_.-]*(?:/[A-Za-z0-9_][A-Za-z0-9_.-]*)*")
TREE_GROUP_RE = re.compile(r"\(([^()]*)\)")
NAME_RE = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_-]*")


def cargo_tree_argv(target):
    """The ONE closure derivation: every package the finalizer's package links, over normal edges, for the build target."""
    return ["cargo", "tree", "-p", FINALIZER_PACKAGE, "-e", "normal", "--prefix", "none", "--format", "{p}", "--target", target]


def parse_closure(tree_text, repo_root):
    """cargo tree `{p}` lines -> ({crate name: repository-relative dir}, [local paths outside the repository]).

    A line names a package and, for a path dependency, its absolute directory in parentheses; a registry or git
    dependency carries no absolute path (`(proc-macro)` and `(*)` are markers, a git source is a URL). Duplicates
    collapse. Only the absolute-path group is read, so a marker can never be mistaken for a crate."""
    root = Path(repo_root).resolve()
    crates, outside = {}, set()
    for raw in tree_text.splitlines():
        line = raw.strip()
        if not line:
            continue
        name = line.split(" ", 1)[0]
        for grp in TREE_GROUP_RE.findall(line):
            if not grp.startswith("/"):
                continue
            p = Path(grp).resolve()
            try:
                rel = p.relative_to(root).as_posix()
            except ValueError:
                outside.add(grp)
                continue
            if NAME_RE.fullmatch(name):
                crates[name] = rel or "."
    return crates, sorted(outside)


def closure_paths(crates):
    return sorted(set(crates.values()) | set(CLOSURE_ROOT_FILES))


def porcelain_paths(text):
    names = []
    for ln in text.splitlines():
        if len(ln) < 4:
            continue
        n = ln[3:]
        if " -> " in n:
            n = n.split(" -> ", 1)[1]
        names.append(n.strip('"'))
    return sorted(set(names))


def git_head(ctx):
    return ctx.runner.run(["git", "--literal-pathspecs", "rev-parse", "HEAD"], local=True, cwd=str(ctx.repo_root)).out.strip()


def git_dirty(ctx, paths):
    """Closure paths with uncommitted changes (untracked files included); None when git could not answer."""
    r = ctx.runner.run(["git", "--literal-pathspecs", "status", "--porcelain", "--untracked-files=all", "--", *paths],
                       local=True, cwd=str(ctx.repo_root))
    return porcelain_paths(r.out) if r.rc == 0 else None


def git_is_ancestor(ctx, commit):
    r = ctx.runner.run(["git", "--literal-pathspecs", "merge-base", "--is-ancestor", commit, "HEAD"], local=True, cwd=str(ctx.repo_root))
    return r.rc == 0


def git_changed_since(ctx, commit, paths):
    """Closure files that differ between the build commit and HEAD; None when git could not answer."""
    r = ctx.runner.run(["git", "--literal-pathspecs", "diff", "--name-only", commit, "HEAD", "--", *paths],
                       local=True, cwd=str(ctx.repo_root))
    return sorted(n for n in r.out.splitlines() if n) if r.rc == 0 else None


def sidecar_shape(sc):
    """The first malformed field of a sidecar of the right schema, or None."""
    if not isinstance(sc.get("commit"), str) or not COMMIT_RE.fullmatch(sc["commit"]):
        return "commit is not a 40-hex commit"
    cl = sc.get("closure")
    if not isinstance(cl, list) or not cl or not all(isinstance(p, str) and CLOSURE_PATH_RE.fullmatch(p) and ".." not in p.split("/")
                                                     for p in cl):
        return "closure is not a non-empty list of repository-relative paths"
    if not isinstance(sc.get("closure_clean"), bool):
        return "closure_clean is not a boolean"
    if not isinstance(sc.get("dirty"), list) or not all(isinstance(p, str) for p in sc["dirty"]):
        return "dirty is not a list of paths"
    if not isinstance(sc.get("bootstrap_sha256"), str) or not SHA_RE.fullmatch(sc["bootstrap_sha256"]):
        return "bootstrap_sha256 is not 64 hex"
    if not isinstance(sc.get("target"), str) or not TARGET_RE.fullmatch(sc["target"]):
        return "target is not a target triple"
    if not isinstance(sc.get("built_at"), str) or not sc["built_at"]:
        return "built_at is absent"
    return None


def first_of(names):
    return names[0] + (" (and %d more)" % (len(names) - 1) if len(names) > 1 else "")


def check_provenance(sidecar, facts):
    """The pure rule: the first reason the asset cannot be attributed, or None. Every reason opens with its own phrase.

    facts = {asset_sha256, is_ancestor, changed (list | None), dirty (list | None)}, gathered from the asset and git."""
    if sidecar is None:
        return "sidecar absent: no %s beside the asset" % FINALIZER_SIDECAR
    if not isinstance(sidecar, dict) or sidecar.get("schema") != FINALIZER_SCHEMA:
        return "schema: %s is not a %s record" % (FINALIZER_SIDECAR, FINALIZER_SCHEMA)
    bad = sidecar_shape(sidecar)
    if bad:
        return "schema: %s %s" % (FINALIZER_SIDECAR, bad)
    if facts.get("asset_sha256") != sidecar["bootstrap_sha256"]:
        return "sha mismatch: the asset's sha256 %s is not the recorded %s" % (str(facts.get("asset_sha256"))[:16], sidecar["bootstrap_sha256"][:16])
    if not sidecar["closure_clean"]:
        return "built from a dirty closure: %s" % first_of(sorted(sidecar["dirty"]) or ["(no path recorded)"])
    if not facts.get("is_ancestor"):
        return "commit not an ancestor of HEAD: the asset was built at %s" % sidecar["commit"][:12]
    if facts.get("changed") is None:
        return "closure path changed since the build: git could not diff %s..HEAD" % sidecar["commit"][:12]
    if facts["changed"]:
        return "closure path changed since the build: %s" % first_of(facts["changed"])
    if facts.get("dirty") is None:
        return "closure path dirty: git status could not answer"
    if facts["dirty"]:
        return "closure path dirty: %s" % first_of(facts["dirty"])
    return None


def read_sidecar(asset_dir):
    path = Path(asset_dir) / FINALIZER_SIDECAR
    try:
        raw = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return None
    except OSError:
        return "<unreadable>"
    try:
        return json.loads(raw)
    except ValueError:
        return "<not JSON>"


def gather_provenance(ctx, asset_dir):
    """The thin, impure half: (sidecar, facts). Git is asked only about a sidecar whose shape check_provenance accepts."""
    sidecar = read_sidecar(asset_dir)
    boot = Path(asset_dir) / "bootstrap"
    facts = {"asset_sha256": sha256_file(boot) if boot.is_file() else None, "is_ancestor": False, "changed": None, "dirty": None}
    if isinstance(sidecar, dict) and sidecar.get("schema") == FINALIZER_SCHEMA and sidecar_shape(sidecar) is None:
        facts["is_ancestor"] = git_is_ancestor(ctx, sidecar["commit"])
        facts["changed"] = git_changed_since(ctx, sidecar["commit"], sidecar["closure"]) if facts["is_ancestor"] else None
        facts["dirty"] = git_dirty(ctx, sidecar["closure"])
    return sidecar, facts


def build_sidecar(ctx, asset_dir, target, crates):
    """The record `write` stores. It does not refuse a dirty closure (an offline synth from a dirty tree stays possible);
    it records closure_clean false and the dirty paths, and the deploy path refuses that record."""
    boot = Path(asset_dir) / "bootstrap"
    if not boot.is_file():
        refuse("provenance", "no finalizer binary at %s; the synth must copy it there first" % boot)
    head = git_head(ctx)
    if not COMMIT_RE.fullmatch(head):
        refuse("provenance", "git rev-parse HEAD did not answer a 40-hex commit")
    paths = closure_paths(crates)
    dirty = git_dirty(ctx, paths)
    if dirty is None:
        refuse("provenance", "git status over the closure failed")
    return {"schema": FINALIZER_SCHEMA, "commit": head, "closure": paths, "closure_crates": sorted(crates),
            "closure_clean": not dirty, "dirty": dirty, "bootstrap_sha256": sha256_file(boot), "target": target,
            "built_at": ctx.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ"),
            "note": "attestation of what was built from what, not byte reproducibility; carries no secret"}


def derive_closure(ctx, target):
    r = ctx.runner.run(cargo_tree_argv(target), local=True, cwd=str(ctx.repo_root))
    if r.rc != 0:
        refuse("provenance", "cargo tree failed (rc %d): %s" % (r.rc, first_line(r.err)))
    crates, outside = parse_closure(r.out, ctx.repo_root)
    if outside:
        refuse("provenance", "the closure has a path dependency outside the repository (%s); git cannot attest it" % outside[0])
    if FINALIZER_PACKAGE not in crates:
        refuse("provenance", "cargo tree did not name %s with a local path" % FINALIZER_PACKAGE)
    return crates


def write_provenance(ctx, asset_dir, target):
    sc = build_sidecar(ctx, asset_dir, target, derive_closure(ctx, target))
    path = Path(asset_dir) / FINALIZER_SIDECAR
    tmp = Path(asset_dir) / (".%s.tmp-%d" % (FINALIZER_SIDECAR, os.getpid()))
    tmp.write_text(json.dumps(sc, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.chmod(tmp, 0o644)
    os.replace(tmp, path)
    return sc


def verify_provenance(ctx, asset_dir=None):
    sidecar, facts = gather_provenance(ctx, asset_dir or ctx.finalizer_dir)
    return check_provenance(sidecar, facts)


def cmd_provenance(ctx, pos, opts):
    """`finalizer-provenance write ASSET_DIR [--target T]` (run by just laya-trainer-synth) | `verify ASSET_DIR`. No AWS call."""
    if len(pos) != 2 or pos[0] not in ("write", "verify"):
        refuse("usage", "finalizer-provenance write|verify ASSET_DIR [--target TRIPLE]")
    verb, asset_dir = pos[0], Path(pos[1]).resolve()
    target = opts.get("target", FINALIZER_TARGET)
    if not TARGET_RE.fullmatch(target):
        refuse("usage", "--target is not a target triple: %r" % target[:40])
    if verb == "write":
        sc = write_provenance(ctx, asset_dir, target)
        ctx.say("FINALIZER PROVENANCE WRITTEN commit=%s closure=%d paths (%d crates) clean=%s sha256=%s"
                % (sc["commit"][:12], len(sc["closure"]), len(sc["closure_crates"]), str(sc["closure_clean"]).lower(), sc["bootstrap_sha256"][:16]))
        if not sc["closure_clean"]:
            ctx.warn("WARNING: built from a dirty closure (%s); `infra diff` and `infra deploy` will refuse this asset until a "
                     "clean re-synth" % first_of(sc["dirty"]))
        return 0
    if opts.get("target"):
        refuse("usage", "--target is for write only")
    why = verify_provenance(ctx, asset_dir)
    if why:
        refuse("provenance", "finalizer provenance: %s" % why)
    ctx.say("FINALIZER PROVENANCE OK")
    return 0


# ---- infra ---------------------------------------------------------------------------------------------------

def load_image_digests(ctx, env):
    imgs = read_state_optional(ctx, "images-%s.json" % env)
    got = {}
    for which in ("train", "pack"):
        e = imgs.get(which) if isinstance(imgs, dict) else None
        d = e.get("digest") if isinstance(e, dict) else None
        if not isinstance(d, str) or not DIGEST_RE.fullmatch(d) or d == ZERO_DIGEST:
            refuse("infra", "no built image digests: images-%s.json holds no real digest for the %s image "
                            "(run `just laya-trainer-image %s %s %s` first)" % (env, which, which, env, PROFILE))
        got[which] = d
    return got


def cdk_argv(action, env, profile, stack, digests=None):
    argv = ["npx", "cdk", action, "--context", "env=%s" % env, "--context", "stack=%s" % stack]
    if digests:
        argv += ["--context", "trainImageDigest=%s" % digests["train"], "--context", "packImageDigest=%s" % digests["pack"]]
    argv += ["--profile", profile, "--no-notices", "--no-version-reporting"]
    if action == "deploy":
        argv += ["--require-approval", "never"]
    return argv


def describe_stack(ctx, profile, region, stack):
    r = aws(ctx, profile, region, "cloudformation", "describe-stacks", "--stack-name", stack, "--query", "Stacks[0].Outputs",
            "--output", "json", dry_out="[]")
    if r.rc != 0:
        if "does not exist" in r.err:
            return None
        raise Failure("describe-stacks of %s failed: %s" % (stack, first_line(r.err)), "INFRA FAILED outputs DESCRIBE")
    try:
        items = json.loads(r.out)
    except ValueError:
        raise Failure("describe-stacks of %s answered no JSON" % stack, "INFRA FAILED outputs DESCRIBE")
    if items is None:
        items = []
    got = {}
    for it in items if isinstance(items, list) else []:
        if isinstance(it, dict) and isinstance(it.get("OutputKey"), str) and isinstance(it.get("OutputValue"), str):
            got[it["OutputKey"]] = it["OutputValue"]
    return got


def need_keys(outs, keys, stack):
    missing = [k for k in keys if k not in outs]
    if missing:
        refuse("infra", "stack %s lacks the outputs %s" % (stack, ", ".join(missing)))
    return {k: outs[k] for k in keys}


def cmd_infra(ctx, pos, opts):
    if len(pos) != 3:
        refuse("usage", "infra ACTION ENV PROFILE (ACTION is %s)" % " | ".join(INFRA_ACTIONS))
    action, env, profile = pos
    check_profile_env(profile, env)
    if action not in INFRA_ACTIONS:
        refuse("usage", "infra ACTION must be one of %s, got %r" % (", ".join(INFRA_ACTIONS), action[:40]))
    state_dir(ctx, opts.get("state_dir"), create=not ctx.dry)
    digests = None
    asset_present = False
    if action in ("diff", "deploy"):
        digests = load_image_digests(ctx, env)
        asset_present = ctx.file_exists(str(ctx.finalizer_dir / "bootstrap"))
        if not asset_present:
            msg = "the finalizer asset %s is absent; run `just laya-trainer-synth %s` to build it" % (FINALIZER_ASSET, env)
            if ctx.dry:
                ctx.say("DRY RUN NOTE: a live run would be REFUSED infra: " + msg)
            else:
                refuse("infra", msg)
    region = connect(ctx, profile)
    if asset_present:
        # After the account check (which stays the first command), before any cdk call: the asset must be attributable
        # to a commit on HEAD's history and a closure clean and unchanged since (plan 14-19, WR-04).
        why = verify_provenance(ctx)
        if why:
            msg = ("finalizer provenance: %s; re-run `just laya-trainer-synth %s` on a clean, committed closure, then diff, "
                   "then deploy" % (why, env))
            if ctx.dry:
                ctx.say("DRY RUN NOTE: a live run would be REFUSED infra: " + msg)
            else:
                refuse("infra", msg)
    cwd = str(ctx.repo_root / "deploy-extensions")
    cenv = child_env(profile, region)

    def cdk(act, stack, dg):
        r = ctx.runner.run(cdk_argv(act, env, profile, stack, dg), env=cenv, cwd=cwd, stream=True)
        if r.rc != 0:
            raise Failure("cdk %s %s exited %d" % (act, stack, r.rc), "INFRA FAILED %s cdk-%s rc=%d" % (action, act, r.rc))

    if action in ("diff-images", "deploy-images", "diff", "deploy"):
        trainer = action in ("diff", "deploy")
        stack_ctx = "laya-trainer" if trainer else "laya-trainer-images"
        cdk("diff", stack_ctx, digests)
        if action.startswith("deploy"):
            cdk("deploy", stack_ctx, digests)
        if ctx.dry:
            finish_dry(ctx, "infra %s" % action)
            return 0
        ctx.say("INFRA OK %s" % action)
        return 0
    # outputs-images / outputs
    img_stack = IMAGES_STACK % env
    if action == "outputs-images":
        outs = describe_stack(ctx, profile, region, img_stack)
        if ctx.dry:
            ctx.say("would write outputs-images-%s.json from the %s outputs" % (env, img_stack))
            finish_dry(ctx, "infra %s" % action)
            return 0
        if outs is None:
            refuse("infra", "images stack %s absent; run `just laya-trainer-infra deploy-images %s %s` first" % (img_stack, env, profile))
        sel = need_keys(outs, IMAGES_OUTPUT_KEYS, img_stack)
        write_state(ctx, "outputs-images-%s.json" % env, {"schema": "laya-trainer-images-outputs-v1", "env": env, "outputs": sel})
        ctx.say("wrote outputs-images-%s.json (%s)" % (env, ", ".join(sorted(sel))))
        ctx.say("INFRA OK %s" % action)
        return 0
    tr_stack = TRAINER_STACK % env
    trainer_outs = describe_stack(ctx, profile, region, tr_stack)
    images_outs = describe_stack(ctx, profile, region, img_stack)
    if ctx.dry:
        ctx.say("would write outputs-%s.json from the %s and %s outputs" % (env, tr_stack, img_stack))
        finish_dry(ctx, "infra %s" % action)
        return 0
    if trainer_outs is None:
        refuse("infra", "trainer stack %s absent; deploy it first (`just laya-trainer-infra deploy %s %s`)" % (tr_stack, env, profile))
    if images_outs is None:
        refuse("infra", "images stack %s absent" % img_stack)
    t_sel = need_keys(trainer_outs, TRAINER_OUTPUT_KEYS, tr_stack)
    i_sel = need_keys(images_outs, IMAGES_OUTPUT_KEYS, img_stack)
    write_state(ctx, "outputs-%s.json" % env, {"schema": "laya-trainer-outputs-v1", "env": env, "trainer": t_sel, "images": i_sel})
    ctx.say("wrote outputs-%s.json (%d trainer outputs, %d images outputs)" % (env, len(t_sel), len(i_sel)))
    ctx.say("INFRA OK %s" % action)
    return 0


# ---- stage ---------------------------------------------------------------------------------------------------

BASE_STAGE_JSON = "scripts/laya_train/container/base-stage.json"
DATASET_FILES = ("task.json", "train.jsonl", "eval.jsonl", "shift.jsonl")
DATASET_REQUIRED = ("task.json", "train.jsonl", "eval.jsonl")
SEGMENT_RE = re.compile(r"[a-z0-9][a-z0-9._-]{0,63}")
# Phase 12's inputs (12-GATE-RUN-EVIDENCE.json inputs_sha256): --expect motivation-v1 binds a staged dataset to them.
EXPECT = {
    "motivation-v1": {
        "task.json": "f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3",
        "train.jsonl": "f1d84f19ec38ed85356bcbcd1eae6ef8ad07fb97e014f61a17d37adc0d68d378",
        "eval.jsonl": "ee95d71fcb5d90d515b9fc6741e5df51087e763fa7f251ace83d9f11d5ccd575",
    },
}


def read_outputs(ctx, env, profile, keys):
    out = read_state(ctx, "outputs-%s.json" % env, "run `just laya-trainer-infra outputs %s %s` first" % (env, profile))
    t = out.get("trainer") if isinstance(out.get("trainer"), dict) else {}
    missing = [k for k in keys if not isinstance(t.get(k), str) or not t.get(k)]
    if missing:
        refuse("state", "outputs-%s.json lacks %s" % (env, ", ".join(missing)))
    if "BucketName" in keys and not BUCKET_RE.fullmatch(t["BucketName"]):
        refuse("state", "outputs-%s.json BucketName is not a bucket name" % env)
    return t


def read_base_stage(ctx):
    path = ctx.base_stage
    try:
        d = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        refuse("stage", "the base stage record %s cannot be read" % Path(path).name)
    if (not isinstance(d, dict) or d.get("schema") != "laya-base-stage-v1" or not SHA_RE.fullmatch(str(d.get("tar_sha256", "")))
            or not isinstance(d.get("tar_bytes"), int) or d["tar_bytes"] <= 0 or not COMMIT_RE.fullmatch(str(d.get("revision", "")))
            or not re.fullmatch(r"[A-Za-z0-9._-]+", str(d.get("tar_name", "")))):
        refuse("stage", "the base stage record %s is not a laya-base-stage-v1 with a sha256, a size, a revision and a tar name" % Path(path).name)
    return d


def check_dataset_prefix(prefix):
    """Mirror of crates/aprender-mcp-laya-train parse_dataset_uri: 1 to 4 segments of [a-z0-9._-], each starting with a
    letter or digit, at most 64 characters, no `%`. Returns the normalized segments."""
    p = prefix[:-1] if prefix.endswith("/") else prefix
    parts = p.split("/")
    if "%" in prefix or not p or not (1 <= len(parts) <= 4) or not all(SEGMENT_RE.fullmatch(x) for x in parts):
        refuse("stage", "PREFIX %r must be 1 to 4 segments of [a-z0-9._-], each starting with a letter or digit and at most 64 "
                        "characters (the trainer's dataset_uri grammar)" % prefix[:60])
    return parts


def head_size(ctx, profile, region, bucket, key):
    r = aws(ctx, profile, region, "s3api", "head-object", "--bucket", bucket, "--key", key, "--query", "ContentLength",
            "--output", "text", dry_out="0\n")
    try:
        return int(r.out.strip()) if r.rc == 0 else None
    except ValueError:
        return None


def stage_base(ctx, rest, opts):
    if len(rest) != 2:
        refuse("usage", "stage base ENV PROFILE [--tar FILE]")
    env, profile = rest
    check_profile_env(profile, env)
    if opts.get("expect"):
        refuse("usage", "--expect is for `stage dataset`")
    if opts.get("stage_json"):
        if not ctx.dry:
            refuse("usage", "--stage-json is honoured only under DRY_RUN=1 (the live pin is the committed base-stage.json)")
        ctx.base_stage = opts["stage_json"]
    pin = read_base_stage(ctx)
    state_dir(ctx, opts.get("state_dir"), create=not ctx.dry)
    tar = Path(opts["tar"]) if opts.get("tar") else ctx.state / "laya-base.tar"
    try:
        size = tar.stat().st_size
    except OSError:
        refuse("stage", "the base tar %s does not exist; build it with `stage_base.py build --out %s`" % (tar, tar))
    if size != pin["tar_bytes"]:
        refuse("stage", "%s is %d bytes, base-stage.json pins %d" % (tar.name, size, pin["tar_bytes"]))
    got = sha256_file(tar)
    if got != pin["tar_sha256"]:
        refuse("stage", "%s sha256 %s... is not base-stage.json's %s..." % (tar.name, got[:12], pin["tar_sha256"][:12]))
    bucket = read_outputs(ctx, env, profile, ("BucketName",))["BucketName"]
    key = "base/laya-%s/%s" % (pin["revision"][:8], pin["tar_name"])
    region = connect(ctx, profile)
    cp = aws(ctx, profile, region, "s3", "cp", str(tar), "s3://%s/%s" % (bucket, key), "--only-show-errors")
    if cp.rc != 0:
        raise Failure("the base upload failed: %s" % first_line(cp.err), "STAGE FAILED base UPLOAD")
    up = head_size(ctx, profile, region, bucket, key)
    if ctx.dry:
        finish_dry(ctx, "stage base")
        return 0
    if up != pin["tar_bytes"]:
        raise Failure("the uploaded object is %s bytes, expected %d" % (up, pin["tar_bytes"]), "STAGE FAILED base SIZE")
    ctx.say("staged %s (%d bytes, sha256 %s) at %s" % (pin["tar_name"], up, got, key))
    ctx.say("STAGE OK base %s" % key)
    return 0


def stage_dataset(ctx, rest, opts):
    if len(rest) != 4:
        refuse("usage", "stage dataset DIR PREFIX ENV PROFILE [--expect motivation-v1]")
    src, prefix, env, profile = rest
    check_profile_env(profile, env)
    if opts.get("tar") or opts.get("stage_json"):
        refuse("usage", "--tar and --stage-json are for `stage base`")
    expect = opts.get("expect")
    if expect is not None and expect not in EXPECT:
        refuse("usage", "--expect names %r; the only pin is %s" % (expect[:40], ", ".join(EXPECT)))
    parts = check_dataset_prefix(prefix)
    norm = "/".join(parts)
    d = Path(src)
    if not d.is_dir() or d.is_symlink():
        refuse("stage", "DIR %r is not a directory" % src[:80])
    names = sorted(os.listdir(d))
    for n in names:
        if n not in DATASET_FILES:
            refuse("stage", "DIR holds %r, which is not one of %s (nothing else is staged)" % (n[:60], ", ".join(DATASET_FILES)))
        st = os.lstat(d / n)
        if not stat.S_ISREG(st.st_mode):
            refuse("stage", "%s is not a regular file" % n)
    for n in DATASET_REQUIRED:
        if n not in names:
            refuse("stage", "DIR lacks the required %s" % n)
    hashes = {n: sha256_file(d / n) for n in names}
    if expect:
        for n, want in EXPECT[expect].items():
            if hashes.get(n) != want:
                refuse("stage", "%s is not Phase 12's (sha256 %s... instead of %s...); --expect %s binds the three files to the "
                                "inputs the gate run passed on" % (n, hashes.get(n, "?")[:12], want[:12], expect))
    state_dir(ctx, opts.get("state_dir"), create=not ctx.dry)
    bucket = read_outputs(ctx, env, profile, ("BucketName",))["BucketName"]
    region = connect(ctx, profile)
    base = "datasets/%s/" % norm
    ls = aws(ctx, profile, region, "s3api", "list-objects-v2", "--bucket", bucket, "--prefix", base, "--max-keys", "1", "--query",
             "length(Contents || `[]`)", "--output", "text", dry_out="0\n")
    try:
        held = int(ls.out.strip()) if ls.rc == 0 else None
    except ValueError:
        held = None
    if held is None:
        raise Failure("could not list %s: %s" % (base, first_line(ls.err)), "STAGE FAILED dataset LIST")
    if held > 0:
        refuse("stage", "datasets/%s/ already holds objects; use a new prefix (a dataset prefix is immutable, so a restage can "
                        "never leave a stale file behind)" % norm)
    for n in names:
        cp = aws(ctx, profile, region, "s3", "cp", str(d / n), "s3://%s/%s%s" % (bucket, base, n), "--only-show-errors")
        if cp.rc != 0:
            raise Failure("upload of %s failed: %s; %s now holds a partial dataset, use a NEW prefix" % (n, first_line(cp.err), base),
                          "STAGE FAILED dataset UPLOAD")
    for n in names:
        got = head_size(ctx, profile, region, bucket, base + n)
        if not ctx.dry and got != (d / n).stat().st_size:
            raise Failure("%s is %s bytes in S3, expected %d; %s is now unusable, use a NEW prefix" % (n, got, (d / n).stat().st_size, base),
                          "STAGE FAILED dataset SIZE")
    if ctx.dry:
        ctx.say("would record datasets/%s in datasets-%s.json with the sha256 of %s" % (norm, env, ", ".join(names)))
        finish_dry(ctx, "stage dataset")
        return 0
    rec = read_state_optional(ctx, "datasets-%s.json" % env) or {}
    rec.update({"schema": "laya-trainer-datasets-v1", "env": env})
    rec.setdefault("datasets", {})[norm] = {"prefix": base, "files": hashes, "staged_at": ctx.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ")}
    write_state(ctx, "datasets-%s.json" % env, rec)
    for n in names:
        ctx.say("  %s sha256 %s" % (n, hashes[n]))
    ctx.say("STAGE OK dataset %s" % norm)
    return 0


def cmd_stage(ctx, pos, opts):
    if not pos or pos[0] not in ("base", "dataset"):
        refuse("usage", "stage base ENV PROFILE | stage dataset DIR PREFIX ENV PROFILE")
    return (stage_base if pos[0] == "base" else stage_dataset)(ctx, pos[1:], opts)


# ---- probe ---------------------------------------------------------------------------------------------------

PROBE_INSTANCE = "ml.g5.xlarge"
PROBE_MAX_RUNTIME_S = 1800
PROBE_VOLUME_GB = 100
PROBE_POLL_S = 30
PROBE_CAP_S = 3600            # create to a terminal status, start-up included; past it the job is stopped
SIGTERM_AFTER_S = 120
SIGTERM_READY_CAP_S = 1200
JOB_TERMINAL = ("Completed", "Failed", "Stopped")
PROBE_MODES = {"cuda": "probe", "sigterm": "probe-sigterm"}


def build_probe_request(mode, probe_id, bucket, role_arn, image, base_prefix):
    """The one create-training-job request of a probe: one instance, MaxRuntime 1800, the base channel only."""
    return {
        "TrainingJobName": "laya-probe-%s" % probe_id,
        "RoleArn": role_arn,
        "AlgorithmSpecification": {"TrainingImage": image, "TrainingInputMode": "File"},
        "InputDataConfig": [{
            "ChannelName": "base",
            "DataSource": {"S3DataSource": {"S3DataType": "S3Prefix", "S3Uri": "s3://%s/%s" % (bucket, base_prefix),
                                            "S3DataDistributionType": "FullyReplicated"}},
        }],
        "OutputDataConfig": {"S3OutputPath": "s3://%s/sagemaker-output/" % bucket},
        "ResourceConfig": {"InstanceType": PROBE_INSTANCE, "InstanceCount": 1, "VolumeSizeInGB": PROBE_VOLUME_GB},
        "StoppingCondition": {"MaxRuntimeInSeconds": PROBE_MAX_RUNTIME_S},
        "Environment": {"LAYA_STEP": PROBE_MODES[mode], "LAYA_BUCKET": bucket, "LAYA_PROBE_ID": probe_id},
    }


def int_or_float(x):
    return int(x) if isinstance(x, (int, float)) and float(x).is_integer() else x


def describe_job(ctx, profile, region, name):
    bad = 0
    while True:
        r = aws(ctx, profile, region, "sagemaker", "describe-training-job", "--training-job-name", name, "--output", "json",
                dry_out="{}")
        try:
            job = json.loads(r.out) if r.rc == 0 else None
        except ValueError:
            job = None
        if isinstance(job, dict):
            return job
        bad += 1
        if bad >= 3:
            raise Failure("describe-training-job failed three times: %s" % first_line(r.err), "PROBE FAILED probe DESCRIBE")
        ctx.sleep(PROBE_POLL_S)


def fetch_json(ctx, profile, region, bucket, key):
    """get-object into a private temp file, parsed; None when the object is absent or not a JSON object."""
    tmp = Path(tempfile.mkdtemp(prefix=".probe-get-", dir=str(ctx.state)))
    try:
        out = tmp / "obj"
        r = aws(ctx, profile, region, "s3api", "get-object", "--bucket", bucket, "--key", key, str(out), "--output", "json")
        if r.rc != 0 or not out.exists():
            return None
        try:
            v = json.loads(out.read_text(encoding="utf-8"))
        except ValueError:
            return None
        return v if isinstance(v, dict) else None
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


# The CUDA probe's pass rule (plan 14-12, owner decision amend-criterion, 2026-10-29). The tiny fixture is a 12-row random-weight
# model: train.py ends it with exit 3 (GATE FAIL, margin 0.0 against 0.05) on EVERY device, cpu included (measured), so a clean
# fixture run is rc 0 or rc 3. rc 2 is a refusal of its inputs and any other rc is a crash: both still fail. The device must be a
# cuda one, since a fixture that fell back to cpu proves nothing about the GPU host.
FIXTURE_OK_RCS = (0, 3)


def probe_verdict(probe):
    """('pass'|'fail', [reasons]) from probe.json's measurements alone; a missing probe.json is a fail."""
    if not isinstance(probe, dict):
        return "fail", ["probe.json absent"]
    why = []
    for key in ("torch_cuda_available", "matmul_ok", "base_load_ok", "train_step_ok"):
        if probe.get(key) is not True:
            why.append("%s is not true" % key)
    rc = probe.get("tiny_fixture_rc")
    if isinstance(rc, bool) or rc not in FIXTURE_OK_RCS:
        why.append("tiny_fixture_rc %r is not in %s" % (rc, FIXTURE_OK_RCS))
    dev = probe.get("tiny_fixture_device_used")
    if not (isinstance(dev, str) and dev.startswith("cuda")):
        why.append("tiny_fixture_device_used %r does not start with cuda" % (dev,))
    return ("fail" if why else "pass"), why


def cmd_probe(ctx, pos, opts):
    if len(pos) != 3:
        refuse("usage", "probe cuda|sigterm ENV PROFILE")
    mode, env, profile = pos
    check_profile_env(profile, env)
    if mode not in PROBE_MODES:
        refuse("usage", "probe MODE must be cuda or sigterm, got %r" % mode[:40])
    if not ctx.dry and ctx.env.get("LAYA_TRAINER_APPROVED_SPEND", "") != "probe-%s" % mode:
        refuse("spend", "a live probe creates one %s training job (up to %d s, billed); set LAYA_TRAINER_APPROVED_SPEND=probe-%s to approve it"
               % (PROBE_INSTANCE, PROBE_MAX_RUNTIME_S, mode))
    state_dir(ctx, opts.get("state_dir"), create=not ctx.dry)
    t = read_outputs(ctx, env, profile, ("BucketName", "SageMakerRoleArn", "TrainRepoUri"))
    imgs = read_state_optional(ctx, "images-%s.json" % env)
    train = imgs.get("train") if isinstance(imgs, dict) and isinstance(imgs.get("train"), dict) else {}
    digest = train.get("digest")
    if not isinstance(digest, str) or not DIGEST_RE.fullmatch(digest) or digest == ZERO_DIGEST:
        refuse("probe", "images-%s.json holds no real train image digest (run `just laya-trainer-image train %s %s` first)" % (env, env, profile))
    if train.get("repo_uri") not in (None, t["TrainRepoUri"]):
        refuse("probe", "images-%s.json names another train repository than the trainer stack's TrainRepoUri" % env)
    pin = read_base_stage(ctx)
    base_prefix = "base/laya-%s/" % pin["revision"][:8]
    probe_id = "%s-%s" % (mode, ctx.utcnow().strftime("%Y%m%d%H%M%S"))
    req = build_probe_request(mode, probe_id, t["BucketName"], t["SageMakerRoleArn"], "%s@%s" % (t["TrainRepoUri"], digest), base_prefix)
    name = req["TrainingJobName"]
    region = connect(ctx, profile)
    ctx.say("probe %s: one %s job %s, MaxRuntimeInSeconds %d, the base channel only" % (mode, PROBE_INSTANCE, name, PROBE_MAX_RUNTIME_S))
    ctx.say("request: " + json.dumps(req, indent=2, sort_keys=True))
    bucket = t["BucketName"]
    prefix = "runs/probe/%s/" % probe_id
    create = aws(ctx, profile, region, "sagemaker", "create-training-job", "--cli-input-json", json.dumps(req, sort_keys=True))
    if create.rc != 0:
        raise Failure("create-training-job failed: %s" % first_line(create.err), "PROBE FAILED %s CREATE" % mode)
    if ctx.dry:
        describe_job(ctx, profile, region, name)
        if mode == "sigterm":
            aws(ctx, profile, region, "s3api", "head-object", "--bucket", bucket, "--key", prefix + "sigterm-ready.json")
            aws(ctx, profile, region, "sagemaker", "stop-training-job", "--training-job-name", name)
            aws(ctx, profile, region, "s3api", "list-objects-v2", "--bucket", bucket, "--prefix", prefix + "sigterm-heartbeat-",
                "--query", "Contents[].Key", "--output", "json")
        else:
            aws(ctx, profile, region, "s3api", "get-object", "--bucket", bucket, "--key", prefix + "probe.json", "<probe.json>")
        ctx.say("would poll every %d s (cap %d s)%s and record probe-%s.json" % (
            PROBE_POLL_S, PROBE_CAP_S, ", stop the job %d s after InProgress once the container is ready" % SIGTERM_AFTER_S if mode == "sigterm" else "", probe_id))
        finish_dry(ctx, "probe %s" % mode)
        return 0
    ctx.say("created %s; polling every %d s" % (name, PROBE_POLL_S))
    t0 = ctx.clock()
    in_progress_at = stop_at = None
    ready = False
    last = None
    while True:
        job = describe_job(ctx, profile, region, name)
        status, sec = job.get("TrainingJobStatus"), job.get("SecondaryStatus")
        if (status, sec) != last:
            ctx.say("job %s: %s / %s (%d s)" % (name, status, sec, ctx.clock() - t0))
            last = (status, sec)
        if status in JOB_TERMINAL:
            break
        now = ctx.clock()
        if status == "InProgress" and in_progress_at is None:
            in_progress_at = now
        if mode == "sigterm" and stop_at is None and in_progress_at is not None and now - in_progress_at >= SIGTERM_AFTER_S:
            h = aws(ctx, profile, region, "s3api", "head-object", "--bucket", bucket, "--key", prefix + "sigterm-ready.json")
            ready = h.rc == 0
            if ready or now - in_progress_at >= SIGTERM_AFTER_S + SIGTERM_READY_CAP_S:
                sp = aws(ctx, profile, region, "sagemaker", "stop-training-job", "--training-job-name", name)
                if sp.rc != 0:
                    raise Failure("stop-training-job failed: %s" % first_line(sp.err), "PROBE FAILED sigterm STOP")
                stop_at = now
                ctx.say("stop requested %d s after InProgress (container ready: %s)" % (now - in_progress_at, ready))
        if now - t0 > PROBE_CAP_S:
            aws(ctx, profile, region, "sagemaker", "stop-training-job", "--training-job-name", name)
            ctx.say("job %s did not finish within %d s; a stop was requested" % (name, PROBE_CAP_S))
            ctx.say("PROBE FAILED %s POLL_TIMEOUT" % mode)
            return 1
        ctx.sleep(PROBE_POLL_S)
    final = describe_job(ctx, profile, region, name)
    status = final.get("TrainingJobStatus")
    rec = {
        "schema": "laya-trainer-probe-record-v1", "mode": mode, "probe_id": probe_id, "job_name": name, "instance_type": PROBE_INSTANCE,
        "max_runtime_seconds": PROBE_MAX_RUNTIME_S, "final_status": status, "failure_reason": final.get("FailureReason"),
        "secondary_status": final.get("SecondaryStatus"), "training_time_seconds": final.get("TrainingTimeInSeconds"),
        "billable_seconds": final.get("BillableTimeInSeconds"),
        "secondary_status_transitions": final.get("SecondaryStatusTransitions", []),
        "recorded_at": ctx.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ"), "probe": None,
    }
    if mode == "cuda":
        probe = fetch_json(ctx, profile, region, bucket, prefix + "probe.json")
        rec["probe"] = probe
        rec["verdict"], rec["fail_reasons"] = probe_verdict(probe)
        ctx.say("probe verdict: %s%s" % (rec["verdict"], "" if rec["verdict"] == "pass" else " (%s)" % "; ".join(rec["fail_reasons"])))
        ctx.say("probe.json: %s" % ("downloaded (train_step_ok %s)" % probe.get("train_step_ok") if probe else "absent (the container wrote none)"))
    else:
        keys = []
        ls = aws(ctx, profile, region, "s3api", "list-objects-v2", "--bucket", bucket, "--prefix", prefix + "sigterm-heartbeat-",
                 "--query", "Contents[].Key", "--output", "json")
        try:
            keys = [k for k in (json.loads(ls.out) or []) if isinstance(k, str)] if ls.rc == 0 else []
        except ValueError:
            keys = []
        secs = []
        for k in keys[:200]:
            hb = fetch_json(ctx, profile, region, bucket, k)
            v = hb.get("seconds_since_sigterm") if hb else None
            if isinstance(v, (int, float)) and not isinstance(v, bool):
                secs.append(float(v))
        rec["heartbeats"] = len(secs)
        rec["grace_seconds"] = int_or_float(max(secs)) if secs else 0
        rec["sigterm_ready"] = ready
        rec["stop_requested_after_in_progress_seconds"] = int_or_float(stop_at - in_progress_at) if stop_at is not None else None
        ctx.say("grace: %s s from %d heartbeats" % (rec["grace_seconds"], len(secs)))
    write_state(ctx, "probe-%s.json" % probe_id, rec)
    ctx.say("billable seconds: %s; failure reason: %s" % (rec["billable_seconds"], rec["failure_reason"]))
    ctx.say("recorded probe-%s.json" % probe_id)
    ctx.say("PROBE DONE %s %s" % (mode, status))
    return 0


# ---- config and deploy (plan 14-10: D-10, D-01) ---------------------------------------------------------------

SERVER_ID = "aprender-mcp-laya-train"
PACKAGE = "aprender-mcp-laya-train-lambda"
TEMPLATE = "crates/aprender-mcp-laya-train-lambda/.pmcp/deploy.toml.template"
BOOTSTRAP_SRC = "crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs"
UNSET = "UNSET-run-just-laya-trainer-config"
CONFIG_ENV_KEYS = (("APRENDER_LAYA_TRAIN_TABLE", "TableName"), ("APRENDER_LAYA_TRAIN_BUCKET", "BucketName"),
                   ("APRENDER_LAYA_TRAIN_STATE_MACHINE_ARN", "StateMachineArn"))
TABLE_RE = re.compile(r"[A-Za-z0-9_.-]{3,255}")
SFN_ARN_RE = re.compile(r"arn:aws:states:[a-z]{2}-[a-z]+-[0-9]:[0-9]{12}:stateMachine:[A-Za-z0-9_-]{1,80}")
ROLE_RE = re.compile(r"[A-Za-z0-9+=,.@_-]{1,64}")
POLICY_NAME = "aprender-laya-train-%s"
ENDPOINT_RE = re.compile(r"https://([a-z][a-z0-9-]{0,62})((?:\.[a-z0-9-]+)+)/mcp")
# The package-resolution check reads cargo's own `Compiling <crate> ` lines. The request function's one other deployable-looking
# dependency is the SetFit training crate it reuses; any OTHER -lambda crate in the log means another server's binary was built.
EXPECT_LAMBDA_CRATES = (PACKAGE, "aprender-setfit-train-lambda")


def write_text(ctx, name, text):
    path = ctx.state / name
    tmp = ctx.state / (".%s.tmp-%d" % (name, os.getpid()))
    fd = os.open(str(tmp), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        fh.write(text)
    os.replace(tmp, path)
    os.chmod(path, 0o600)
    return path


def noncomment(lines):
    return [(i, ln) for i, ln in enumerate(lines) if not ln.lstrip().startswith("#")]


def check_no_iam(lines, what):
    for _, ln in noncomment(lines):
        if re.match(r"\s*\[+\s*iam\b", ln):
            refuse("config", "%s declares IAM (%s): the request function is published with NO [[iam.statements]]; "
                             "access comes only from `just laya-trainer-grant` after a 401 is observed (D-10)" % (what, ln.strip()[:40]))


def parse_config_toml(text, what):
    import tomllib
    try:
        doc = tomllib.loads(text)
    except tomllib.TOMLDecodeError:
        refuse("config", "%s is not valid TOML" % what)
    if "iam" in doc:
        refuse("config", "%s declares IAM: the request function is published with NO [[iam.statements]] (D-10)" % what)
    if (doc.get("server") or {}).get("name") != SERVER_ID:
        refuse("config", "%s names a server other than %s" % (what, SERVER_ID))
    auth = doc.get("auth")
    if not isinstance(auth, dict) or auth.get("enabled") is not False:
        refuse("config", "%s does not declare [auth] enabled = false (cargo-pmcp cannot render OAuth for pmcp-run; D-10)" % what)
    return doc


def cmd_config(ctx, pos, opts):
    if len(pos) != 2:
        refuse("usage", "config ENV PROFILE")
    env, profile = pos
    check_profile_env(profile, env)
    tpl = opts.get("template")
    if tpl and not ctx.dry:
        refuse("usage", "--template is honoured only under DRY_RUN=1 (the live template is the tracked file)")
    state_dir(ctx, opts.get("state_dir"), create=True)
    path = Path(tpl) if tpl else ctx.repo_root / TEMPLATE
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        refuse("config", "the template %s cannot be read" % (tpl or TEMPLATE))
    lines = text.splitlines()
    check_no_iam(lines, "the template")
    unset = [i for i, ln in noncomment(lines) if UNSET in ln]
    slots = {}
    for key, _ in CONFIG_ENV_KEYS:
        hit = [i for i, ln in noncomment(lines) if re.fullmatch(r'\s*%s\s*=\s*"%s"\s*(#.*)?' % (re.escape(key), re.escape(UNSET)), ln)]
        if len(hit) != 1:
            refuse("config", "the template must hold exactly one %s placeholder line, found %d" % (key, len(hit)))
        slots[key] = hit[0]
    if sorted(unset) != sorted(slots.values()):
        refuse("config", "the template has %d placeholder lines for %d keys: every %s must belong to one of %s"
               % (len(unset), len(slots), UNSET, ", ".join(k for k, _ in CONFIG_ENV_KEYS)))
    out = read_outputs(ctx, env, profile, [name for _, name in CONFIG_ENV_KEYS])
    vals = {}
    for key, name in CONFIG_ENV_KEYS:
        v = out[name]
        ok = {"TableName": TABLE_RE, "BucketName": BUCKET_RE, "StateMachineArn": SFN_ARN_RE}[name].fullmatch(v)
        if not ok:
            refuse("state", "outputs-%s.json %s is not shaped like a %s" % (env, name, {"TableName": "table name", "BucketName": "bucket name",
                                                                                         "StateMachineArn": "state machine ARN"}[name]))
        vals[key] = v
    for key, i in slots.items():
        lines[i] = '%s = "%s"' % (key, vals[key])
    rendered = "\n".join(lines) + "\n"
    if UNSET in "\n".join(ln for _, ln in noncomment(rendered.splitlines())):
        refuse("config", "a placeholder survived the render")
    parse_config_toml(rendered, "the rendered config")
    name = "deploy-config-%s.toml" % env
    write_text(ctx, name, rendered)
    ctx.say("wrote %s (mode 600): %s filled from outputs-%s.json" % (name, ", ".join(k for k, _ in CONFIG_ENV_KEYS), env))
    if ctx.dry:
        finish_dry(ctx, "config")
    ctx.say("CONFIG OK %s" % name)
    return 0


def aws_error_code(err):
    m = re.search(r"\(([A-Za-z]+)\)", err or "")
    return m.group(1) if m else ""


def revoke_grant(ctx, profile, region, env):
    """Delete the function's inline grant before an OAuth-off redeploy, FAIL-CLOSED. An absent function or an absent
    policy proceeds; a found policy is deleted and must then read back NoSuchEntity; ANY other answer (AccessDenied,
    throttling, an unparseable reply) refuses, because "cannot see the grant" must never be read as "no grant"."""
    pol = POLICY_NAME % env
    cannot = "cannot confirm the grant is absent"

    def refuse_deploy(why):
        refuse("deploy", "%s: %s" % (cannot, why))

    fn = aws(ctx, profile, region, "lambda", "get-function", "--function-name", SERVER_ID, "--query", "Configuration.Role", "--output", "text")
    if ctx.dry:
        ctx.say("+ aws iam get-role-policy --role-name <role of %s> --policy-name %s" % (SERVER_ID, pol))
        ctx.say("DRY RUN: a live run deletes the inline policy %s first when it is found, and refuses when it cannot tell" % pol)
        return
    if fn.rc != 0:
        if aws_error_code(fn.err) == "ResourceNotFoundException":
            ctx.say("no function %s yet: nothing to revoke" % SERVER_ID)
            return
        refuse_deploy("get-function answered %s" % (aws_error_code(fn.err) or first_line(fn.err)))
    role = fn.out.strip().rsplit("/", 1)[-1]
    if not ROLE_RE.fullmatch(role):
        refuse_deploy("get-function answered no role name")

    def read():
        return aws(ctx, profile, region, "iam", "get-role-policy", "--role-name", role, "--policy-name", pol, "--output", "json")

    r = read()
    if r.rc != 0:
        if aws_error_code(r.err) == "NoSuchEntity":
            ctx.say("no inline grant %s on the function's role: nothing to revoke" % pol)
            return
        refuse_deploy("get-role-policy answered %s" % (aws_error_code(r.err) or first_line(r.err)))
    try:
        found = isinstance(json.loads(r.out), dict)
    except ValueError:
        found = False
    if not found:
        refuse_deploy("get-role-policy answered an unparseable reply")
    d = aws(ctx, profile, region, "iam", "delete-role-policy", "--role-name", role, "--policy-name", pol)
    if d.rc != 0:
        refuse_deploy("delete-role-policy answered %s" % (aws_error_code(d.err) or first_line(d.err)))
    r2 = read()
    if r2.rc == 0 or aws_error_code(r2.err) != "NoSuchEntity":
        refuse_deploy("the policy still reads back after the delete" if r2.rc == 0
                      else "the re-read after the delete answered %s" % (aws_error_code(r2.err) or first_line(r2.err)))
    ctx.say("GRANT REVOKED for redeploy")


def parse_endpoint(log):
    for pat in (r'^\s*mcp_endpoint:\s*"(https://[^"\s]+)"', r"^\s*Server URL:\s*(https://\S+)", r"MCP Endpoint:\s*\n\s*URL:\s*(https://\S+)"):
        m = re.search(pat, log, re.M)
        if m:
            return m.group(1)
    return None


def cmd_deploy(ctx, pos, opts):
    if len(pos) != 2:
        refuse("usage", "deploy ENV PROFILE")
    env, profile = pos
    check_profile_env(profile, env)
    state_dir(ctx, opts.get("state_dir"), create=not ctx.dry)
    cfg_name = "deploy-config-%s.toml" % env
    cfg = ctx.state / cfg_name
    if not cfg.is_file():
        refuse("deploy", "%s is missing; run `just laya-trainer-config %s %s` first" % (cfg_name, env, profile))
    try:
        cfg_text = cfg.read_text(encoding="utf-8")
    except OSError:
        refuse("deploy", "%s cannot be read" % cfg_name)
    check_no_iam(cfg_text.splitlines(), cfg_name)
    parse_config_toml(cfg_text, cfg_name)
    if any(UNSET in ln for _, ln in noncomment(cfg_text.splitlines())):
        refuse("deploy", "%s still holds a placeholder; regenerate it with `just laya-trainer-config`" % cfg_name)
    if not ctx.dry and ctx.env.get("LAYA_TRAINER_APPROVED_DEPLOY", "") != env:
        refuse("approval", "a live deploy needs LAYA_TRAINER_APPROVED_DEPLOY=%s (the owner's explicit go)" % env)
    region = target_region(ctx, profile)
    account_gate(ctx, profile, region)
    revoke_grant(ctx, profile, region, env)
    snap = ctx.state / ("deploy-%s.state" % env)
    cenv = child_env(profile, region)
    cenv.update({"PMCP_TARGET": ctx.target, "PMCP_API_URL": None})
    if not ctx.dry:
        shutil.rmtree(str(snap), ignore_errors=True)
        try:
            import resource
            soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
            resource.setrlimit(resource.RLIMIT_NOFILE, (min(65536, hard) if hard != resource.RLIM_INFINITY else 65536, hard))
        except (ImportError, ValueError, OSError):
            ctx.warn("WARNING: could not raise the fd limit; a link may fail with ProcessFdQuotaExceeded")
        # The touch makes the request function's crate ALWAYS recompile, so its absence from the log means cargo-pmcp built
        # something else (a cached build prints no Compiling line).
        ctx.runner.run(["touch", str(ctx.repo_root / BOOTSTRAP_SRC)], cwd=str(ctx.repo_root))
    argv = ["just", "_laya-crates-root-swap", "crates", str(cfg), str(snap), "cargo", "pmcp", "deploy", "--manifest-path", "crates",
            "--regenerate-stack", "--no-post-deploy-test", "--no-oauth", "--no-color"]
    res = ctx.runner.run(argv, env=cenv, cwd=str(ctx.repo_root), stream=True, local=False)
    if ctx.dry:
        finish_dry(ctx, "deploy")
        return 0
    log = res.out
    write_text(ctx, "deploy-%s.log" % env, log)
    if res.rc != 0:
        raise Failure("the deploy exited %d (log: %s/deploy-%s.log)" % (res.rc, ctx.state, env), "DEPLOY FAILED rc=%d" % res.rc)
    compiled = re.findall(r"Compiling ([A-Za-z0-9_-]+-lambda) ", log)
    if PACKAGE not in compiled:
        refuse("deploy", "wrong package: the build log does not name %s; nothing was recorded (the published function has no IAM, so it can reach nothing)" % PACKAGE)
    other = sorted({c for c in compiled if c not in EXPECT_LAMBDA_CRATES})
    if other:
        refuse("deploy", "wrong package: the build log also builds %s; nothing was recorded" % ", ".join(other))
    endpoint = parse_endpoint(log)
    m = ENDPOINT_RE.fullmatch(endpoint) if endpoint else None
    if not m:
        refuse("deploy", "the deploy log names no https://<server>.<host>/mcp endpoint; nothing was recorded")
    if m.group(1) != SERVER_ID:
        refuse("deploy", "the deployed endpoint names server %s, not %s; nothing was recorded" % (m.group(1), SERVER_ID))
    if ctx.suffix and m.group(2) != ctx.suffix:
        refuse("deploy", "the deployed endpoint host is not under *%s, the tenant of profile %s; nothing was recorded" % (ctx.suffix, profile))
    write_state(ctx, "deploy-%s.json" % env, {"schema": "laya-trainer-deploy-v1", "env": env, "server_id": SERVER_ID, "package": PACKAGE,
                                              "endpoint": endpoint, "deployed_at": ctx.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ")})
    ctx.say("endpoint: %s" % endpoint)
    ctx.say("recorded deploy-%s.json; next: the owner enables OAuth, then `just laya-trainer-grant %s %s`" % (env, env, profile))
    ctx.say("DEPLOY OK %s" % SERVER_ID)
    return 0


# ---- grant (plan 14-10: D-10, T-14-71) --------------------------------------------------------------------------------

def bound_endpoint(ctx, env):
    """The endpoint the grant may be decided on: read ONLY from the deploy state the `deploy` subcommand wrote from the log that
    named the package, and bound to the function being granted to by the server id in its host and in its own /health."""
    not_bound = lambda why: refuse("grant", "endpoint not bound to %s (%s)" % (SERVER_ID, why))
    rec = read_state_optional(ctx, "deploy-%s.json" % env)
    if rec is None:
        not_bound("deploy-%s.json is missing; run `just laya-trainer-deploy %s %s`" % (env, env, PROFILE))
    ep = rec.get("endpoint")
    m = ENDPOINT_RE.fullmatch(ep) if isinstance(ep, str) else None
    if not m or rec.get("server_id") != SERVER_ID or rec.get("package") != PACKAGE:
        not_bound("deploy-%s.json does not record a deployment of %s" % (env, PACKAGE))
    if m.group(1) != SERVER_ID:
        not_bound("the endpoint's host label is %s" % m.group(1))
    if ctx.suffix and m.group(2) != ctx.suffix:
        not_bound("the endpoint host is not under *%s" % ctx.suffix)
    return ep


def health_names_server(ctx, endpoint):
    base = endpoint[:-len("/mcp")]
    with tempfile.TemporaryDirectory(prefix="laya-grant-health-") as td:
        body_path = os.path.join(td, "health.json")
        r = ctx.runner.run(["curl", "-sS", "-m", "30", "-o", body_path, "-w", "%{http_code}", base + "/health"], local=True)
        if r.rc != 0:
            refuse("grant", "endpoint unreachable (transport error; retry)")
        code = r.out.strip()[-3:]
        if code != "200":
            refuse("grant", "endpoint not bound to %s (GET /health answered %s)" % (SERVER_ID, code if code.isdigit() else "no status"))
        try:
            with open(body_path, "r", encoding="utf-8", errors="replace") as fh:
                body = json.loads(fh.read(65536))
        except (OSError, ValueError):
            body = None
    if not isinstance(body, dict) or body.get("serverId") != SERVER_ID:
        refuse("grant", "endpoint not bound to %s (its /health names another server)" % SERVER_ID)


def classify_auth_refusal(res):
    text = res.err + "\n" + res.out
    if "REFUSED auth-check: transport" in text:
        refuse("grant", "endpoint unreachable (transport error; retry)")
    m = re.search(r"REFUSED auth-open: unauthenticated tools/list answered HTTP ([0-9]{3})", text)
    if m:
        refuse("grant", "OAuth not confirmed (tools/list answered %s)" % m.group(1))
    m = re.search(r"REFUSED auth-open: tools/call answered HTTP ([0-9]{3})", text)
    if m:
        refuse("grant", "OAuth not confirmed (tools/call answered %s)" % m.group(1))
    refuse("grant", "OAuth not confirmed (the auth check exited %d without a verdict)" % res.rc)


def cmd_grant(ctx, pos, opts):
    if len(pos) != 2:
        refuse("usage", "grant takes exactly ENV PROFILE: unknown argument %r (the endpoint is read from the verified deploy state, "
                        "never from the command line)" % (pos[2][:40] if len(pos) > 2 else ""))
    env, profile = pos
    check_profile_env(profile, env)
    state_dir(ctx, opts.get("state_dir"), create=False)
    if not ctx.dry and ctx.env.get("LAYA_TRAINER_APPROVED_GRANT", "") != env:
        refuse("approval", "a live grant needs LAYA_TRAINER_APPROVED_GRANT=%s (the owner's explicit go, after enabling OAuth)" % env)
    out = read_state(ctx, "outputs-%s.json" % env, "run `just laya-trainer-infra outputs %s %s` first" % (env, profile))
    policy = (out.get("trainer") or {}).get("RequestLambdaPolicy")
    try:
        doc = json.loads(policy) if isinstance(policy, str) else None
    except ValueError:
        doc = None
    if not isinstance(doc, dict):
        refuse("state", "outputs-%s.json holds no RequestLambdaPolicy document" % env)
    if not ctx.dry and not (isinstance(doc.get("Statement"), list) and doc["Statement"]):
        refuse("state", "outputs-%s.json RequestLambdaPolicy has no Statement (a placeholder, not the stack's policy)" % env)
    region = target_region(ctx, profile)
    endpoint = bound_endpoint(ctx, env)
    health_names_server(ctx, endpoint)
    # THE ORDER: the 401 is observed here, on the endpoint just bound to this function, immediately before the IAM call.
    auth = ctx.runner.run(["just", "_laya-auth-refusal-check", endpoint, "0", "train_status"], local=True, cwd=str(ctx.repo_root))
    if auth.rc != 0:
        classify_auth_refusal(auth)
    ctx.say("OAuth confirmed on %s: unauthenticated tools/list and a train_status tools/call both answered 401" % SERVER_ID)
    account_gate(ctx, profile, region)
    fn = aws(ctx, profile, region, "lambda", "get-function", "--function-name", SERVER_ID, "--query", "Configuration.Role", "--output", "text")
    role = "<role of %s>" % SERVER_ID
    if not ctx.dry:
        if fn.rc != 0:
            refuse("grant", "cannot read the role of function %s (%s); publish it first: just laya-trainer-deploy %s %s"
                   % (SERVER_ID, aws_error_code(fn.err) or first_line(fn.err), env, profile))
        role = fn.out.strip().rsplit("/", 1)[-1]
        if not ROLE_RE.fullmatch(role):
            refuse("grant", "get-function answered no role name")
    put = aws(ctx, profile, region, "iam", "put-role-policy", "--role-name", role, "--policy-name", POLICY_NAME % env, "--policy-document", policy)
    if ctx.dry:
        finish_dry(ctx, "grant")
        return 0
    if put.rc != 0:
        raise Failure("put-role-policy failed: %s" % (aws_error_code(put.err) or first_line(put.err)), "GRANT FAILED")
    ctx.say("policy: %s" % (POLICY_NAME % env))
    ctx.say("GRANT OK role %s" % role)
    return 0


# ---- dispatch ------------------------------------------------------------------------------------------------

VALUE_OPTS = {"--state-dir": "state_dir", "--tar": "tar", "--expect": "expect", "--stage-json": "stage_json", "--env": "env",
              "--template": "template", "--target": "target"}
FLAG_OPTS = {"--list-files": "list_files"}
COMMANDS = {"image": cmd_image, "infra": cmd_infra, "stage": cmd_stage, "probe": cmd_probe, "config": cmd_config, "deploy": cmd_deploy,
            "grant": cmd_grant, "finalizer-provenance": cmd_provenance}
SUBCOMMAND_OPTS = {"image": ("state_dir", "list_files"), "infra": ("state_dir",), "stage": ("state_dir", "tar", "expect", "stage_json"),
                   "probe": ("state_dir",), "config": ("state_dir", "template"), "deploy": ("state_dir",), "grant": ("state_dir",),
                   "finalizer-provenance": ("target",)}


def parse_args(argv):
    pos, opts = [], {}
    i = 0
    while i < len(argv):
        a = argv[i]
        if a.startswith("--"):
            name, eq, val = a.partition("=")
            if name in VALUE_OPTS:
                if not eq:
                    i += 1
                    if i >= len(argv):
                        refuse("usage", "%s needs a value" % name)
                    val = argv[i]
                if VALUE_OPTS[name] in opts:
                    refuse("usage", "%s given twice" % name)
                opts[VALUE_OPTS[name]] = val
            elif name in FLAG_OPTS and not eq:
                opts[FLAG_OPTS[name]] = True
            else:
                refuse("usage", "unknown option %s" % name[:40])
        else:
            pos.append(a)
        i += 1
    return pos, opts


def main(argv, ctx=None):
    ctx = ctx or Ctx()
    try:
        if "--selftest" in argv:
            return selftest()
        pos, opts = parse_args(argv)
        if not pos:
            refuse("usage", "usage: laya_trainer_ops.py [--state-dir DIR] image|infra|stage|probe ... (see the module docstring)")
        cmd, rest = pos[0], pos[1:]
        if cmd not in COMMANDS:
            refuse("usage", "unknown subcommand %r (image, infra, stage, probe, config, deploy, grant, finalizer-provenance)" % cmd[:40])
        allowed = SUBCOMMAND_OPTS[cmd]
        for k in opts:
            if k not in allowed:
                refuse("usage", "option for %s is not accepted by `%s`" % (k, cmd))
        return COMMANDS[cmd](ctx, rest, opts)
    except Refused as e:
        ctx.warn("REFUSED %s: %s" % (e.rule, e.msg))
        return 2
    except Failure as e:
        ctx.warn("ERROR: %s" % e.msg)
        if e.last:
            ctx.say(e.last)
        return 1


def entry(argv):
    if argv and argv[0] == "--write-placeholder-state":
        ctx = Ctx()
        try:
            pos, opts = parse_args(argv[1:])
            if len(pos) != 1:
                refuse("usage", "--write-placeholder-state DIR [--env dev|prod]")
            return write_placeholder_state(ctx, pos[0], opts.get("env", "dev"))
        except Refused as e:
            ctx.warn("REFUSED %s: %s" % (e.rule, e.msg))
            return 2
    return main(argv)


# ---- the simulated world and the selftest -----------------------------------------------------------------------

class Sim:
    """git, just, aws and npx as one scripted world. Never reaches the real ones (except read-only git when asked)."""

    HEAD = "c" * 40

    def __init__(self, tmp):
        self.tmp = Path(tmp)
        self.calls = []
        self.clock = [0.0]
        self.account_refusal = False
        self.dirty = []
        self.tree = []
        self.ancestor = True
        self.changed = []
        self.asset_dir = None
        self.real_git = False
        self.build_statuses = ["IN_PROGRESS", "IN_PROGRESS", "SUCCEEDED"]
        self.build_polls = 0
        self.digest = "sha256:" + "d" * 64
        self.stacks = {}
        self.objects = {}
        self.copied = {}
        self.cdk_rc = {}
        self.extra = {}
        self.existing = set()
        self.head_override = {}
        self.job = None
        self.job_create_rc = 0
        self.cuda_end = ("Completed", None, 600.0)
        self.hb_max = 28
        self.ready_after = 200.0
        self.ready_never = False
        self.swap_rc = 0
        self.swap_out = ""
        self.health = {"rc": 0, "code": "200", "body": '{"status":"healthy","serverId":"aprender-mcp-laya-train","hasDeployment":true}'}
        self.auth = {"rc": 0, "out": "auth-refusal ok: tools/list 401, tools/call 401 after 1 poll(s) (0 s)\n", "err": ""}
        self.iam = {"function": True, "role": "deploy-role", "fn_err": None, "policy": False, "inspect_err": None, "delete_err": None,
                    "sticky": False, "reread_err": None, "reads": 0, "put_err": None, "put_doc": None}
        self.probe_json = {"schema": "laya-train-probe-v1", "train_step_ok": True, "train_step_peak_bytes": 12345678901,
                           "train_step_batch": 4, "train_step_tokens": 1024, "host_rss_peak_bytes": 987654321,
                           "torch_cuda_available": True, "matmul_ok": True, "base_load_ok": True, "tiny_fixture_rc": 0,
                           "tiny_fixture_device_used": "cuda"}

    # -- helpers
    @staticmethod
    def opt(argv, flag, default=None):
        return argv[argv.index(flag) + 1] if flag in argv else default

    def handle(self, argv, env, cwd):
        self.calls.append(list(argv))
        exe = argv[0]
        if exe == "just":
            return self.just(argv)
        if exe == "git":
            return self.git(argv, cwd)
        if exe == "aws":
            return self.aws(argv)
        if exe == "touch":
            return Result(0, "", "")
        if exe == "curl":
            if self.health["rc"]:
                return Result(self.health["rc"], "", "curl: (7) Failed to connect")
            out = self.opt(argv, "-o")
            if out and self.health["body"] is not None:
                Path(out).write_text(self.health["body"])
            return Result(0, self.health["code"], "")
        if exe == "npx":
            rc = self.cdk_rc.get(argv[2], 0)
            return Result(rc, "cdk %s output\n" % argv[2], "")
        return Result(127, "", "fake world: unexpected command %s" % shlex.join(argv))

    def just(self, argv):
        if argv[1] == "_laya-target-check":
            return Result(0, "dev us-east-1 .us-east.true-mcp.com 000000000000\n", "target-check ok: fake\n")
        if argv[1] == "_laya-account-check":
            if self.account_refusal:
                return Result(2, "", "REFUSED target-account: profile ze-kasher-dev resolves to an account ending 0001, not the table's\n")
            return Result(0, "", "target-account ok: fake\n")
        if argv[1] == "_laya-auth-refusal-check":
            return Result(self.auth["rc"], self.auth["out"], self.auth["err"])
        if argv[1] == "_laya-crates-root-swap":
            return Result(self.swap_rc, self.swap_out, "")
        return Result(127, "", "fake world: unexpected just recipe")

    def git(self, argv, cwd):
        if self.real_git:
            p = subprocess.run(argv, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True, text=True)
            return Result(p.returncode, p.stdout, p.stderr)
        sub = argv[2] if argv[1] == "--literal-pathspecs" else argv[1]
        rest = argv[argv.index(sub) + 1:]
        specs = rest[rest.index("--") + 1:] if "--" in rest else []
        if sub == "rev-parse":
            return Result(0, self.HEAD + "\n", "")
        if sub == "ls-tree":
            return Result(0, "".join(n + "\n" for n in sorted(self.tree) if any(covers(s, n) for s in specs)), "")
        if sub == "status":
            return Result(0, "".join("%s %s\n" % (flag, n) for flag, n in self.dirty if any(covers(s, n) for s in specs)), "")
        if sub == "merge-base":
            return Result(0 if self.ancestor else 1, "", "")
        if sub == "diff":
            return Result(0, "".join(n + "\n" for n in sorted(self.changed) if any(covers(s, n) for s in specs)), "")
        if sub == "archive":
            out = rest[rest.index("-o") + 1]
            with tarfile.open(out, "w") as tf:
                for n in sorted(self.tree):
                    if any(covers(s, n) for s in specs):
                        data = ("content of %s\n" % n).encode()
                        ti = tarfile.TarInfo(n)
                        ti.size = len(data)
                        ti.mode = 0o755 if n.endswith(".sh") else 0o644
                        ti.mtime = 1234567 + len(n)
                        tf.addfile(ti, io.BytesIO(data))
            return Result(0, "", "")
        return Result(127, "", "fake world: unexpected git subcommand")

    def aws(self, argv):
        svc, op = argv[1], argv[2]
        if (svc, op) == ("s3", "cp"):
            src, dst = argv[3], argv[4]
            with open(src, "rb") as fh:
                self.copied[dst] = fh.read()
            self.objects[dst] = len(self.copied[dst])
            return Result(0, "", "")
        if (svc, op) == ("codebuild", "start-build"):
            self.build_polls = 0
            return Result(0, "aprender-laya-train-image-dev:11111111-2222-3333-4444-5555555555ab\n", "")
        if (svc, op) == ("codebuild", "batch-get-builds"):
            st = self.build_statuses[min(self.build_polls, len(self.build_statuses) - 1)]
            self.build_polls += 1
            return Result(0, st + "\n", "")
        if (svc, op) == ("ecr", "describe-images"):
            if self.digest is None:
                return Result(254, "", "An error occurred (ImageNotFoundException)")
            return Result(0, json.dumps({"imageDetails": [{"imageDigest": self.digest, "imageSizeInBytes": 123456}]}), "")
        if (svc, op) == ("cloudformation", "describe-stacks"):
            name = self.opt(argv, "--stack-name")
            if name not in self.stacks:
                return Result(254, "", "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id %s does not exist" % name)
            return Result(0, json.dumps([{"OutputKey": k, "OutputValue": v} for k, v in self.stacks[name].items()]), "")
        if svc == "s3api":
            return self.s3api(op, argv)
        if svc == "sagemaker":
            return self.sagemaker(op, argv)
        if svc in ("lambda", "iam"):
            return self.lambda_iam(svc, op, argv)
        h = self.extra.get((svc, op))
        if h:
            return h(argv)
        return Result(97, "", "fake aws: unexpected call: %s" % shlex.join(argv))

    def lambda_iam(self, svc, op, argv):
        st = self.iam
        if (svc, op) == ("lambda", "get-function"):
            if st["fn_err"]:
                return Result(254, "", st["fn_err"])
            if not st["function"]:
                return Result(254, "", "An error occurred (ResourceNotFoundException) when calling the GetFunction operation: Function not found")
            return Result(0, "arn:aws:iam::000000000001:role/service-role/%s\n" % st["role"], "")
        if (svc, op) == ("iam", "get-role-policy"):
            st["reads"] += 1
            if st["inspect_err"]:
                return Result(254, "", st["inspect_err"])
            if st["policy"]:
                return Result(0, json.dumps({"RoleName": st["role"], "PolicyName": self.opt(argv, "--policy-name"), "PolicyDocument": {}}), "")
            if st["reread_err"] and st["reads"] > 1:
                return Result(254, "", st["reread_err"])
            return Result(254, "", "An error occurred (NoSuchEntity) when calling the GetRolePolicy operation: The role policy cannot be found.")
        if (svc, op) == ("iam", "delete-role-policy"):
            if st["delete_err"]:
                return Result(254, "", st["delete_err"])
            st["policy"] = st["sticky"]
            return Result(0, "", "")
        if (svc, op) == ("iam", "put-role-policy"):
            if st["put_err"]:
                return Result(254, "", st["put_err"])
            st["put_doc"] = self.opt(argv, "--policy-document")
            st["policy"] = True
            return Result(0, "", "")
        return Result(97, "", "fake aws: unexpected %s call: %s" % (svc, op))

    def probe_id(self):
        return self.job["req"]["Environment"]["LAYA_PROBE_ID"]

    def ready_now(self):
        return self.job is not None and not self.ready_never and self.clock[0] - self.job["t0"] >= self.ready_after

    def s3api(self, op, argv):
        bucket, key = self.opt(argv, "--bucket"), self.opt(argv, "--key")
        if op == "head-object":
            if key.endswith("sigterm-ready.json"):
                return Result(0, "", "") if self.ready_now() else Result(254, "", "An error occurred (404) when calling the HeadObject operation: Not Found")
            uri = "s3://%s/%s" % (bucket, key)
            if key in self.head_override:
                return Result(0, "%d\n" % self.head_override[key], "")
            if uri in self.objects:
                return Result(0, "%d\n" % self.objects[uri], "")
            return Result(254, "", "An error occurred (404) when calling the HeadObject operation: Not Found")
        if op == "list-objects-v2":
            prefix = self.opt(argv, "--prefix")
            if "sigterm-heartbeat-" in prefix:
                if self.job is None or self.job.get("stop_at") is None:
                    return Result(0, "null\n", "")
                base = prefix[:-len("sigterm-heartbeat-")]
                return Result(0, json.dumps([base + "sigterm-heartbeat-%d.json" % n for n in range(0, self.hb_max // 2 + 1)]), "")
            held = {k for k in self.existing if k.startswith(prefix)} | {k[len("s3://%s/" % bucket):] for k in self.objects
                                                                       if k.startswith("s3://%s/%s" % (bucket, prefix))}
            return Result(0, "%d\n" % min(1, len(held)), "")
        if op == "get-object":
            out = argv[argv.index(key) + 1]
            if key.endswith("/probe.json"):
                if self.probe_json is None:
                    return Result(254, "", "An error occurred (NoSuchKey) when calling the GetObject operation")
                Path(out).write_text(json.dumps(self.probe_json))
                return Result(0, "{}", "")
            m = re.search(r"sigterm-heartbeat-(\d+)\.json$", key)
            if m:
                Path(out).write_text(json.dumps({"schema": "laya-train-probe-sigterm-v1", "n": int(m.group(1)),
                                                 "seconds_since_sigterm": 2.0 * int(m.group(1))}))
                return Result(0, "{}", "")
            return Result(254, "", "An error occurred (NoSuchKey) when calling the GetObject operation")
        return Result(97, "", "fake aws: unexpected s3api call: %s" % op)

    def sagemaker(self, op, argv):
        if op == "create-training-job":
            if self.job_create_rc:
                return Result(self.job_create_rc, "", "An error occurred (ResourceLimitExceeded) when calling the CreateTrainingJob operation")
            self.job = {"req": json.loads(self.opt(argv, "--cli-input-json")), "t0": self.clock[0], "stop_at": None}
            return Result(0, json.dumps({"TrainingJobArn": "arn:aws:sagemaker:us-east-1:000000000001:training-job/x"}), "")
        if op == "stop-training-job":
            if self.job is not None and self.job["stop_at"] is None:
                self.job["stop_at"] = self.clock[0]
            return Result(0, "", "")
        if op == "describe-training-job":
            j = self.job
            if j is None:
                return Result(254, "", "An error occurred (ValidationException): Requested resource not found")
            t = self.clock[0] - j["t0"]
            reason = None
            if j["req"]["Environment"]["LAYA_STEP"] == "probe-sigterm":
                done = j["stop_at"] is not None and self.clock[0] - j["stop_at"] >= self.hb_max + 2
                status = "Stopped" if done else "InProgress"
            else:
                end_status, end_reason, end_s = self.cuda_end
                done = t >= end_s
                status = end_status if done else "InProgress"
                reason = end_reason if done else None
            doc = {"TrainingJobName": j["req"]["TrainingJobName"], "TrainingJobStatus": status,
                   "SecondaryStatus": status if done else "Training", "TrainingTimeInSeconds": int(t) if done else None,
                   "BillableTimeInSeconds": int(t) + 60 if done else None,
                   "SecondaryStatusTransitions": [{"Status": "Starting", "StatusMessage": "Preparing the instances"},
                                                  {"Status": "Training", "StatusMessage": "Training image download completed"}]}
            if reason:
                doc["FailureReason"] = reason
            return Result(0, json.dumps(doc), "")
        return Result(97, "", "fake aws: unexpected sagemaker call: %s" % op)


class FakeRunner(Runner):
    def __init__(self, sim, dry, say):
        super().__init__(dry, say)
        self.sim = sim

    def _exec(self, argv, env, cwd, stream, timeout):
        res = self.sim.handle(argv, env, cwd)
        if stream and res.out:
            for ln in res.out.splitlines():
                self.say(ln)
        return res


def selftest():
    fails = []
    n_ok = [0]

    def case(label, cond, detail=""):
        print("  %-4s %-26s %s" % ("ok" if cond else "FAIL", label, detail), flush=True)
        if cond:
            n_ok[0] += 1
        else:
            fails.append(label)

    ctx0 = Ctx()
    TRAIN_PATHS = read_context_paths(ctx0, IMAGES["train"])
    PACK_PATHS = read_context_paths(ctx0, IMAGES["pack"])

    def files_for(paths):
        out = []
        for p in paths:
            out += [p + "a.txt", p + "sub/b.sh"] if p.endswith("/") else [p]
        return out

    DEFAULT_TREE = sorted(set(files_for(TRAIN_PATHS) + files_for(PACK_PATHS) + [IMAGES["train"]["dockerfile"], IMAGES["pack"]["dockerfile"]]
                              + ["crates/zz-other/unrelated.rs", ".planning/STATE.md", "docs/x.md", "models/m.bin"]))
    TRAIN_FILES = sorted(n for n in DEFAULT_TREE if any(covers(p, n) for p in TRAIN_PATHS))

    work = Path(tempfile.mkdtemp(prefix="laya-ops-selftest-"))
    counter = [0]

    def fresh(name):
        counter[0] += 1
        d = work / ("%s-%d" % (name, counter[0]))
        d.mkdir(parents=True)
        return d

    SIM_CRATES = {FINALIZER_PACKAGE: "crates/" + FINALIZER_PACKAGE, "aprender-core": "crates/aprender-core"}

    def sim_sidecar(adir, **over):
        sc = {"schema": FINALIZER_SCHEMA, "commit": Sim.HEAD, "closure": closure_paths(SIM_CRATES), "closure_crates": sorted(SIM_CRATES),
              "closure_clean": True, "dirty": [], "bootstrap_sha256": sha256_file(adir / "bootstrap"), "target": FINALIZER_TARGET,
              "built_at": "2026-10-29T12:34:56Z"}
        sc.update(over)
        return sc

    def world(real_git=False, stateless=False):
        d = fresh("w")
        sim = Sim(d)
        sim.real_git = real_git
        sim.tree = list(DEFAULT_TREE)
        adir = d / "asset"
        adir.mkdir()
        (adir / "bootstrap").write_bytes(b"\x7fELF finalizer bytes\n")
        (adir / FINALIZER_SIDECAR).write_text(json.dumps(sim_sidecar(adir)))
        sim.asset_dir = adir
        sdir = d / "state"
        sdir.mkdir(mode=0o700)
        if not stateless:
            for e in ENVS:
                for name, data in placeholder_state(e).items():
                    (sdir / name).write_text(json.dumps(data))
        return sim, sdir

    def go(sim, sdir, argv, dry=False, env_extra=None, file_exists=None, state_opt=True, base_stage=None):
        out, err = io.StringIO(), io.StringIO()
        env = {"DRY_RUN": "1" if dry else "0"}
        env.update(env_extra or {})
        box = {}

        def say(line):
            print(redact(line), file=out, flush=True)

        runner = FakeRunner(sim, dry, say)
        ctx = Ctx(runner=runner, env=env, stdout=out, stderr=err, sleep=lambda s: sim.clock.__setitem__(0, sim.clock[0] + s),
                  file_exists=file_exists or (lambda p: True))
        ctx.clock = lambda: sim.clock[0]
        if sim.asset_dir is not None:
            ctx.finalizer_dir = sim.asset_dir
        if base_stage:
            ctx.base_stage = str(base_stage)
        ctx.utcnow = lambda: datetime(2026, 10, 29, 12, 34, 56, tzinfo=timezone.utc)
        args = (["--state-dir", str(sdir)] if state_opt else []) + list(argv)
        rc = main(args, ctx)
        box["rc"], box["out"], box["err"], box["ctx"] = rc, out.getvalue(), err.getvalue(), ctx
        return box

    def aws_calls(sim):
        return [c for c in sim.calls if c[0] == "aws" or c[:2] == ["npx", "cdk"]]

    ok = ["image", "train", "dev", PROFILE]

    # ---- image-dry-run
    sim, sdir = world()
    r = go(sim, sdir, ok, dry=True)
    o = r["out"]
    cmds = [ln for ln in o.splitlines() if ln.startswith("+ ")]
    case("image-dry-run", r["rc"] == 0 and o.rstrip().endswith("AWS CALLS: 0") and not aws_calls(sim)
         and any(ln.startswith("+ git --literal-pathspecs archive --format=tar") for ln in cmds)
         and any(ln.startswith("+ aws s3 cp ") and "/context/train-%s.zip" % sim.HEAD in ln.replace(" s3://", "/") for ln in cmds)
         and any("name=DOCKERFILE,value=scripts/laya_train/container/Dockerfile.train" in ln and "name=GIT_COMMIT,value=%s" % sim.HEAD in ln
                 and ln.startswith("+ aws codebuild start-build") for ln in cmds),
         "archive, s3 cp and start-build printed; AWS CALLS: 0; %d aws/cdk executed" % len(aws_calls(sim)))
    case("image-dry-counts", "context %d files from %d paths" % (len(TRAIN_FILES), len(TRAIN_PATHS)) in o and "zz-other" not in o
         and ".planning/STATE" not in o and "docs/x" not in o, "the dry run counts only the committed context files (%d), not the other tracked ones" % len(TRAIN_FILES))

    # ---- image-pack-context: the REAL repository at HEAD, read-only git
    sim, sdir = world(real_git=True)
    r = go(sim, sdir, ["image", "pack", "dev", PROFILE, "--list-files"], dry=True)
    listed = sorted(ln[len("  file "):] for ln in r["out"].splitlines() if ln.startswith("  file "))
    spec = IMAGES["pack"]
    paths = read_context_paths(Ctx(), spec)
    allp = subprocess.run(["git", "-C", str(REPO_ROOT), "ls-tree", "-r", "--name-only", "HEAD"], capture_output=True, text=True).stdout.splitlines()
    want = sorted(n for n in allp if any(covers(p, n) for p in paths) or n == spec["dockerfile"])
    bad = [f for f in listed if f.startswith((".planning/", "docs/", "models/"))]
    case("image-pack-context", r["rc"] == 0 and listed == want and len(listed) > 20 and not bad and spec["dockerfile"] in listed,
         "%d files = the context list at HEAD plus the Dockerfile; none under .planning/, docs/ or models/ (%d bad)" % (len(listed), len(bad)))

    # ---- image-dirty
    sim, sdir = world()
    dirty_file = TRAIN_FILES[0]
    sim.dirty = [(" M", dirty_file)]
    r = go(sim, sdir, ok)
    case("image-dirty", r["rc"] == 2 and "REFUSED image: context path %s has uncommitted changes" % dirty_file in r["err"]
         and not [c for c in aws_calls(sim)], "a modified context file refuses before any aws call (exit 2)")
    sim, sdir = world()
    sim.dirty = [(" M", ".planning/config.json"), ("??", "docs/new.md")]
    r = go(sim, sdir, ok, dry=True)
    st2 = [c for c in sim.calls if c[0] == "git" and "status" in c][0]
    sp2 = st2[st2.index("--") + 1:]
    case("image-dirty-scope", r["rc"] == 0 and "DRY RUN NOTE" not in r["out"] and ".planning" not in " ".join(sp2) and "docs" not in " ".join(sp2)
         and sp2 == TRAIN_PATHS,
         "a dirty .planning/ file never refuses (the status call is scoped to exactly the context paths)")
    sim, sdir = world()
    sim.dirty = [(" M", dirty_file)]
    r = go(sim, sdir, ok, dry=True)
    case("image-dirty-dry", r["rc"] == 0 and "DRY RUN NOTE: a live run would be REFUSED image: context path %s" % dirty_file in r["out"],
         "a dry run reports the refusal a live run would make and still prints the plan")

    # ---- image-record
    sim, sdir = world()
    pack_before = json.loads((sdir / "images-dev.json").read_text())["pack"]
    (sdir / "images-dev.json").write_text(json.dumps({"schema": "laya-trainer-images-v1", "env": "dev", "pack": pack_before}))
    r = go(sim, sdir, ok)
    img = json.loads((sdir / "images-dev.json").read_text())
    t = img.get("train", {})
    last = r["out"].rstrip().splitlines()[-1] if r["out"].strip() else ""
    zkey = [k for k in sim.copied if k.endswith("/context/train-%s.zip" % sim.HEAD)]
    zip_ok, zip_detail = False, "no zip uploaded"
    if zkey:
        zp = work / "uploaded.zip"
        zp.write_bytes(sim.copied[zkey[0]])
        with zipfile.ZipFile(zp) as zf:
            names = zf.namelist()
            stamps = {i.date_time for i in zf.infolist()}
        zip_ok = names == TRAIN_FILES and stamps == {(1980, 1, 1, 0, 0, 0)} and IMAGES["train"]["dockerfile"] in names
        zip_detail = "%d entries, timestamps %s" % (len(names), sorted(stamps)[:1])
    case("image-record", r["rc"] == 0 and re.fullmatch(r"IMAGE OK train sha256:[0-9a-f]{64}", last) is not None
         and t.get("digest") == sim.digest and t.get("commit") == sim.HEAD and re.fullmatch(r"[0-9a-f]{64}", t.get("digest", "x")[7:]) is not None
         and t.get("repo_name") == "aprender-laya-train-dev" and t.get("repo_uri", "").startswith("123456789012.dkr.ecr.") and t.get("build_id")
         and img.get("pack") == pack_before and not ACCT_RE.search(r["out"]) and (sdir / "images-dev.json").stat().st_mode & 0o777 == 0o600,
         "train entry recorded, the pack entry kept, last line %r, stdout holds no 12-digit number (%s)" % (last[:40], "ok" if not ACCT_RE.search(r["out"]) else "LEAK"))
    case("image-zip", zip_ok, "uploaded context zip = the committed context files, sorted, fixed timestamps (%s)" % zip_detail)
    case("image-polls", sim.build_polls == 3 and sim.clock[0] == 60.0, "the build was polled %d times, 30 s apart (fake clock %d s)" % (sim.build_polls, sim.clock[0]))
    sim, sdir = world()
    sim.build_statuses = ["IN_PROGRESS", "FAILED"]
    r = go(sim, sdir, ok)
    last = r["out"].rstrip().splitlines()[-1]
    case("image-failed", r["rc"] == 1 and last == "IMAGE FAILED train FAILED" and "build id: " in r["out"]
         and json.loads((sdir / "images-dev.json").read_text()) == placeholder_state("dev")["images-dev.json"],
         "a FAILED build prints its id and ends %r (exit 1), nothing recorded" % last)
    sim, sdir = world()
    sim.build_statuses = ["IN_PROGRESS"]
    r = go(sim, sdir, ok)
    case("image-timeout", r["rc"] == 1 and r["out"].rstrip().splitlines()[-1] == "IMAGE FAILED train POLL_TIMEOUT" and sim.clock[0] > BUILD_CAP_S,
         "a build that never ends is abandoned (not stopped) after %d min on the fake clock" % (sim.clock[0] // 60))

    # ---- infra-deploy-guard
    sim, sdir = world()
    (sdir / "images-dev.json").unlink()
    r = go(sim, sdir, ["infra", "deploy", "dev", PROFILE])
    a = r["rc"] == 2 and "REFUSED infra: no built image digests" in r["err"] and not aws_calls(sim)
    sim, sdir = world()
    d = json.loads((sdir / "images-dev.json").read_text())
    d["train"]["digest"] = ZERO_DIGEST
    (sdir / "images-dev.json").write_text(json.dumps(d))
    r = go(sim, sdir, ["infra", "deploy", "dev", PROFILE])
    b = r["rc"] == 2 and "REFUSED infra: no built image digests" in r["err"] and "train image" in r["err"] and not aws_calls(sim)
    sim, sdir = world()
    r = go(sim, sdir, ["infra", "deploy", "dev", PROFILE], dry=True)
    sim2, sdir2 = world()
    d = json.loads((sdir2 / "images-dev.json").read_text())
    d["pack"]["digest"] = "sha256:" + "0" * 63 + "z"
    (sdir2 / "images-dev.json").write_text(json.dumps(d))
    r2 = go(sim2, sdir2, ["infra", "diff", "dev", PROFILE], dry=True)
    sim3, sdir3 = world()
    r3 = go(sim3, sdir3, ["infra", "deploy", "dev", PROFILE], file_exists=lambda p: False)
    r4 = go(sim3, sdir3, ["infra", "deploy", "dev", PROFILE], dry=True, file_exists=lambda p: False)
    case("infra-deploy-guard", a and b and r["rc"] == 0 and r["out"].rstrip().endswith("AWS CALLS: 0") and r2["rc"] == 2 and "no built image digests" in r2["err"]
         and r3["rc"] == 2 and "REFUSED infra: the finalizer asset" in r3["err"] and r4["rc"] == 0 and "DRY RUN NOTE" in r4["out"],
         "no images file, an all-zero digest and a malformed digest are refused (also under diff); an absent finalizer asset refuses live and is noted in a dry run")

    # ---- infra-provenance-tracer (plan 14-19, WR-04): the sidecar decides, end to end through go()
    def cdk_acts(sim):
        return [c[2] for c in sim.calls if c[:2] == ["npx", "cdk"]]

    sim_a, sdir_a = world()
    ra_dry = go(sim_a, sdir_a, ["infra", "deploy", "dev", PROFILE], dry=True)
    ra_live = go(sim_a, sdir_a, ["infra", "deploy", "dev", PROFILE])
    benign = (ra_dry["rc"] == 0 and ra_dry["out"].rstrip().endswith("AWS CALLS: 0") and "finalizer provenance" not in ra_dry["out"] + ra_dry["err"]
              and ra_live["rc"] == 0 and ra_live["out"].rstrip().endswith("INFRA OK deploy") and cdk_acts(sim_a) == ["diff", "deploy"]
              and any(c[:4] == ["git", "--literal-pathspecs", "merge-base", "--is-ancestor"] for c in sim_a.calls))
    sim_b, sdir_b = world()
    (sim_b.asset_dir / "bootstrap").write_bytes(b"\x7fELF a different finalizer\n")
    rb_live = go(sim_b, sdir_b, ["infra", "deploy", "dev", PROFILE])
    rb_diff = go(sim_b, sdir_b, ["infra", "diff", "dev", PROFILE])
    rb_dry = go(sim_b, sdir_b, ["infra", "deploy", "dev", PROFILE], dry=True)
    tampered = (rb_live["rc"] == 2 and "REFUSED infra: finalizer provenance: sha mismatch" in rb_live["err"] and "just laya-trainer-synth dev" in rb_live["err"]
                and rb_diff["rc"] == 2 and "finalizer provenance: sha mismatch" in rb_diff["err"] and cdk_acts(sim_b) == []
                and rb_dry["rc"] == 0 and "DRY RUN NOTE: a live run would be REFUSED infra: finalizer provenance: sha mismatch" in rb_dry["out"])
    sim_c, sdir_c = world()
    (sim_c.asset_dir / FINALIZER_SIDECAR).write_text(json.dumps(sim_sidecar(sim_c.asset_dir, closure_clean=False,
                                                                            dirty=["crates/aprender-core/src/lib.rs"])))
    rc_live = go(sim_c, sdir_c, ["infra", "deploy", "dev", PROFILE])
    rc_dry = go(sim_c, sdir_c, ["infra", "diff", "dev", PROFILE], dry=True)
    dirty_built = (rc_live["rc"] == 2 and "REFUSED infra: finalizer provenance: built from a dirty closure: crates/aprender-core/src/lib.rs" in rc_live["err"]
                   and cdk_acts(sim_c) == [] and rc_dry["rc"] == 0
                   and "DRY RUN NOTE: a live run would be REFUSED infra: finalizer provenance: built from a dirty closure" in rc_dry["out"])
    case("infra-provenance-tracer", benign and tampered and dirty_built,
         "a matching sidecar deploys (dry: AWS CALLS: 0); a changed asset byte (sha mismatch) and a dirty-closure build are refused live with no "
         "cdk call and noted under DRY_RUN (benign=%s tampered=%s dirty=%s)" % (benign, tampered, dirty_built))

    # ---- infra-order
    sim, sdir = world()
    r = go(sim, sdir, ["infra", "deploy", "dev", PROFILE])
    cdk = [c for c in sim.calls if c[:2] == ["npx", "cdk"]]
    acts = [c[2] for c in cdk]
    dg = json.loads((sdir / "images-dev.json").read_text())
    want_ctx = ["--context", "stack=laya-trainer", "--context", "trainImageDigest=%s" % dg["train"]["digest"],
                "--context", "packImageDigest=%s" % dg["pack"]["digest"]]
    flat = all(" ".join(want_ctx) in " ".join(c) for c in cdk)
    case("infra-order", r["rc"] == 0 and acts == ["diff", "deploy"] and flat and "--require-approval never" in " ".join(cdk[1])
         and "--require-approval" not in cdk[0] and r["out"].rstrip().endswith("INFRA OK deploy") and "--profile ze-kasher-dev" in " ".join(cdk[1]),
         "cdk diff runs before cdk deploy --require-approval never, both with the two digest contexts: %s" % acts)
    sim, sdir = world()
    sim.cdk_rc["diff"] = 1
    r = go(sim, sdir, ["infra", "deploy", "dev", PROFILE])
    case("infra-diff-fails", r["rc"] == 1 and [c[2] for c in sim.calls if c[:2] == ["npx", "cdk"]] == ["diff"]
         and r["out"].rstrip().endswith("INFRA FAILED deploy cdk-diff rc=1"), "a failing diff stops the deploy")
    sim, sdir = world()
    r = go(sim, sdir, ["infra", "deploy-images", "dev", PROFILE])
    cdk = [c for c in sim.calls if c[:2] == ["npx", "cdk"]]
    case("infra-images-stack", [c[2] for c in cdk] == ["diff", "deploy"] and "stack=laya-trainer-images" in cdk[1] and not any("Digest" in a for a in cdk[1]),
         "deploy-images diffs then deploys the images stack with no digest context")

    # ---- infra-outputs-images / infra-outputs
    img_outs = placeholder_state("dev")["outputs-images-dev.json"]["outputs"]
    tr_outs = placeholder_state("dev")["outputs-dev.json"]["trainer"]
    sim, sdir = world(stateless=True)
    sim.stacks = {IMAGES_STACK % "dev": dict(img_outs)}
    r = go(sim, sdir, ["infra", "outputs-images", "dev", PROFILE])
    f = json.loads((sdir / "outputs-images-dev.json").read_text()) if (sdir / "outputs-images-dev.json").exists() else {}
    r_b = go(sim, sdir, ["infra", "outputs", "dev", PROFILE])
    case("infra-outputs-images", r["rc"] == 0 and sorted(f.get("outputs", {})) == sorted(IMAGES_OUTPUT_KEYS)
         and not (sdir / "outputs-dev.json").exists() and r_b["rc"] == 2 and "REFUSED infra: trainer stack" in r_b["err"] and "absent" in r_b["err"],
         "the images stack alone writes outputs-images-dev.json (5 outputs); `outputs` before the trainer stack is refused")
    sim, sdir = world(stateless=True)
    r_c = go(sim, sdir, ["infra", "outputs-images", "dev", PROFILE])
    sim.stacks = {IMAGES_STACK % "dev": dict(img_outs), TRAINER_STACK % "dev": dict(tr_outs)}
    r = go(sim, sdir, ["infra", "outputs", "dev", PROFILE])
    f = json.loads((sdir / "outputs-dev.json").read_text()) if (sdir / "outputs-dev.json").exists() else {}
    sim.stacks[TRAINER_STACK % "dev"].pop("SubnetIds")
    r_d = go(sim, sdir, ["infra", "outputs", "dev", PROFILE])
    case("infra-outputs", r_c["rc"] == 2 and "images stack" in r_c["err"] and r["rc"] == 0 and sorted(f.get("trainer", {})) == sorted(TRAINER_OUTPUT_KEYS)
         and len(f["trainer"]) == 12 and sorted(f.get("images", {})) == sorted(IMAGES_OUTPUT_KEYS) and (sdir / "outputs-dev.json").stat().st_mode & 0o777 == 0o600
         and r_d["rc"] == 2 and "lacks the outputs SubnetIds" in r_d["err"] and not ACCT_RE.search(r["out"]),
         "both stacks -> outputs-dev.json with the 12 trainer and 5 images outputs, mode 600, nothing printed; a missing output is refused")

    # ---- the dry paths of outputs-images and outputs (found by the gate row, not by the cases above)
    sim, sdir = world()
    ra = go(sim, sdir, ["infra", "outputs-images", "dev", PROFILE], dry=True)
    rb = go(sim, sdir, ["infra", "outputs", "dev", PROFILE], dry=True)
    case("infra-outputs-dry", ra["rc"] == 0 and rb["rc"] == 0 and ra["out"].rstrip().endswith("AWS CALLS: 0") and rb["out"].rstrip().endswith("AWS CALLS: 0")
         and not aws_calls(sim) and "would write outputs-dev.json" in rb["out"], "both outputs commands print their describe-stacks and write nothing under DRY_RUN")

    # ---- account-check-first
    sim, sdir = world()
    r = go(sim, sdir, ok)
    first_aws = next((i for i, c in enumerate(sim.calls) if c[0] == "aws"), -1)
    acct_idx = next((i for i, c in enumerate(sim.calls) if c[:2] == ["just", "_laya-account-check"]), 10**6)
    exact = acct_idx < len(sim.calls) and sim.calls[acct_idx] == ["just", "_laya-account-check", PROFILE, "us-east-1"]
    first_non_just = next((i for i, c in enumerate(sim.calls) if c[0] != "just"), -1)
    sim_b, sdir_b = world()
    sim_b.account_refusal = True
    r_b = go(sim_b, sdir_b, ok)
    sim_c, sdir_c = world()
    sim_c.account_refusal = True
    r_c = go(sim_c, sdir_c, ["infra", "deploy", "dev", PROFILE])
    case("account-check-first", exact and acct_idx < first_aws and acct_idx < first_non_just and r_b["rc"] == 2
         and "REFUSED target-account: profile ze-kasher-dev resolves to an account ending 0001" in r_b["err"]
         and not aws_calls(sim_b) and sim_b.calls[-1][:2] == ["just", "_laya-account-check"] and r_c["rc"] == 2 and not aws_calls(sim_c),
         "`just _laya-account-check %s us-east-1` precedes every other command; its refusal exits 2 with no further call" % PROFILE)

    # ---- state-dir
    sim, sdir = world()
    inside = REPO_ROOT / "docs" / "zz-laya-ops-selftest-state"
    r1 = go(sim, inside, ok, dry=True)
    sim2, sdir2 = world()
    ignored = REPO_ROOT / "models" / "laya-trainer-selftest-probe"
    can = Ctx(env={"DRY_RUN": "1"}, stdout=io.StringIO(), stderr=io.StringIO())
    try:
        state_dir(can, str(ignored), create=False)
        ign_ok = can.state == ignored.resolve()
    except Refused:
        ign_ok = False
    default_ok = False
    try:
        state_dir(Ctx(env={"DRY_RUN": "1"}), None, create=False)
        default_ok = True
    except Refused:
        default_ok = False
    sim3, _unused = world(stateless=True)
    sdir3 = fresh("ns") / "state-new"
    sim3.stacks = {IMAGES_STACK % "dev": dict(img_outs)}
    r3 = go(sim3, sdir3, ["infra", "outputs-images", "dev", PROFILE])
    case("state-dir", r1["rc"] == 2 and "REFUSED state: state dir docs/zz-laya-ops-selftest-state is inside the repository" in r1["err"] and not sim.calls
         and ign_ok and default_ok and r3["rc"] == 0 and (sdir3 / "outputs-images-dev.json").exists() and oct(sdir3.stat().st_mode & 0o777) == "0o700",
         "a non-ignored in-repo dir is refused before any call; models/ (gitignored) and the default are accepted; a temp dir outside the repo holds every state file (mode 700)")

    # ---- profile-env
    bad_calls = 0
    results = []
    ds_good = fresh("ds-profile")
    for n_ in DATASET_REQUIRED:
        (ds_good / n_).write_text("x\n")
    for argv in (["image", "train"], ["image", "pack"], ["infra", "diff"], ["infra", "deploy"], ["infra", "outputs"], ["infra", "outputs-images"],
                 ["stage", "base"], ["stage", "dataset", str(ds_good), "p1"], ["probe", "cuda"], ["probe", "sigterm"]):
        for dry in (True, False):
            sim, sdir = world()
            rp = go(sim, sdir, argv + ["dev", "augury-dev"], dry=dry)
            re_ = go(sim, sdir, argv + ["staging", PROFILE], dry=dry)
            results.append(rp["rc"] == 2 and "REFUSED profile: " in rp["err"] and re_["rc"] == 2 and "REFUSED env: " in re_["err"])
            bad_calls += len(sim.calls)
    case("profile-env", all(results) and bad_calls == 0, "profile augury-dev and env staging refused for every subcommand, live and dry, with %d commands run" % bad_calls)
    sim, sdir = world()
    ru = go(sim, sdir, ["image", "gpu", "dev", PROFILE])
    rv = go(sim, sdir, ["infra", "destroy", "dev", PROFILE])
    rw = go(sim, sdir, ["image", "train", "dev"])
    rx = go(sim, sdir, ["frobnicate"])
    ry = go(sim, sdir, ["image", "train", "dev", PROFILE, "--tar", "x"])
    case("usage", all(x["rc"] == 2 and "REFUSED usage: " in x["err"] for x in (ru, rv, rw, rx, ry)) and not sim.calls,
         "an unknown image, action, subcommand, argument count or option is `REFUSED usage:`")


    # ---- stage base
    def sha_of(b):
        return hashlib.sha256(b).hexdigest()

    tiny = work / "tiny-base.tar"
    tiny.write_bytes(b"a tiny stand-in for the base tar\n" * 50)
    wrong_same_size = work / "wrong-base.tar"
    wrong_same_size.write_bytes(b"b" * tiny.stat().st_size)
    real_pin = json.loads((REPO_ROOT / BASE_STAGE_JSON).read_text())
    pin_ok = work / "base-stage-ok.json"
    pin_ok.write_text(json.dumps(dict(real_pin, tar_sha256=sha_of(tiny.read_bytes()), tar_bytes=tiny.stat().st_size)))
    base_key = "base/laya-%s/laya-base.tar" % real_pin["revision"][:8]

    sim, sdir = world()
    r = go(sim, sdir, ["stage", "base", "dev", PROFILE, "--tar", str(tiny), "--stage-json", str(pin_ok)], dry=True)
    cmds = [ln for ln in r["out"].splitlines() if ln.startswith("+ ")]
    a = (r["rc"] == 0 and r["out"].rstrip().endswith("AWS CALLS: 0") and not aws_calls(sim)
         and any(re.match(r"\+ aws s3 cp \S+\.tar s3://\S+/%s --only-show-errors" % re.escape(base_key), ln) for ln in cmds)
         and any(ln.startswith("+ aws s3api head-object ") and base_key in ln for ln in cmds))
    sim, sdir = world()
    r_b = go(sim, sdir, ["stage", "base", "dev", PROFILE, "--tar", str(wrong_same_size), "--stage-json", str(pin_ok)], dry=True)
    r_c = go(sim, sdir, ["stage", "base", "dev", PROFILE, "--tar", str(tiny)], dry=True)
    r_d = go(sim, sdir, ["stage", "base", "dev", PROFILE, "--tar", str(tiny), "--stage-json", str(pin_ok)])
    r_e = go(sim, sdir, ["stage", "base", "dev", PROFILE])
    sim2, sdir2 = world()
    r_f = go(sim2, sdir2, ["stage", "base", "dev", PROFILE, "--tar", str(tiny)], base_stage=pin_ok)
    calls_f = [c for c in sim2.calls if c[0] == "aws"]
    sim3, sdir3 = world()
    sim3.head_override[base_key] = 5
    r_g = go(sim3, sdir3, ["stage", "base", "dev", PROFILE, "--tar", str(tiny)], base_stage=pin_ok)
    case("stage-base", a and r_b["rc"] == 2 and "REFUSED stage: " in r_b["err"] and "is not base-stage.json's" in r_b["err"]
         and r_c["rc"] == 2 and "REFUSED stage: " in r_c["err"] and "bytes, base-stage.json pins" in r_c["err"]
         and r_d["rc"] == 2 and "REFUSED usage: --stage-json is honoured only under DRY_RUN=1" in r_d["err"]
         and r_e["rc"] == 2 and "does not exist" in r_e["err"] and not [c for c in sim.calls if c[0] == "aws"]
         and r_f["rc"] == 0 and [c[2] for c in calls_f] == ["cp", "head-object"] and r_f["out"].rstrip().endswith("STAGE OK base " + base_key)
         and r_g["rc"] == 1 and r_g["out"].rstrip().endswith("STAGE FAILED base SIZE"),
         "a wrong tar is refused (sha256 and size) before any call; the right one is copied to %s and its size verified; --stage-json is dry-run only" % base_key)

    # ---- stage dataset
    GOOD = {"task.json": b'{"task":"x"}\n', "train.jsonl": b'{"text":"a","label":0}\n', "eval.jsonl": b'{"text":"b","label":1}\n'}

    def mkds(files, links=None):
        d = fresh("ds")
        for n_, b_ in files.items():
            (d / n_).write_bytes(b_)
        for n_, tgt in (links or {}).items():
            os.symlink(tgt, d / n_)
        return d

    def stage_cmd(d, prefix, extra=()):
        return ["stage", "dataset", str(d), prefix, "dev", PROFILE, *extra]

    sim, sdir = world()
    d1 = mkds(GOOD)
    r = go(sim, sdir, stage_cmd(d1, "stance-64"))
    puts = [c for c in sim.calls if c[:3] == ["aws", "s3", "cp"]]
    rec = json.loads((sdir / "datasets-dev.json").read_text()) if (sdir / "datasets-dev.json").exists() else {}
    e = rec.get("datasets", {}).get("stance-64", {})
    a = (r["rc"] == 0 and len(puts) == 3 and all("/datasets/stance-64/" in c[4] for c in puts)
         and e.get("prefix") == "datasets/stance-64/" and e.get("files") == {n_: sha_of(b_) for n_, b_ in GOOD.items()}
         and r["out"].rstrip().endswith("STAGE OK dataset stance-64") and not ACCT_RE.search(r["out"]))
    d2 = mkds(dict(GOOD, **{"shift.jsonl": b'{"text":"c","label":1}\n'}))
    r2 = go(sim, sdir, stage_cmd(d2, "stance-64-shift/"))
    puts2 = [c for c in sim.calls if c[:3] == ["aws", "s3", "cp"]][3:]
    rec2 = json.loads((sdir / "datasets-dev.json").read_text())
    b = (r2["rc"] == 0 and len(puts2) == 4 and sorted(rec2["datasets"]) == ["stance-64", "stance-64-shift"]
         and "shift.jsonl" in rec2["datasets"]["stance-64-shift"]["files"])
    sim_x, sdir_x = world()
    refusals = []
    for label, files, links in (("extra file", dict(GOOD, **{"notes.txt": b"x"}), None), ("hidden file", dict(GOOD, **{".DS_Store": b"x"}), None),
                                ("missing eval", {k: v for k, v in GOOD.items() if k != "eval.jsonl"}, None),
                                ("symlinked train", {k: v for k, v in GOOD.items() if k != "train.jsonl"}, {"train.jsonl": str(d1 / "train.jsonl")})):
        rr = go(sim_x, sdir_x, stage_cmd(mkds(files, links), "ok-prefix"))
        refusals.append(rr["rc"] == 2 and "REFUSED stage: " in rr["err"])
    sub = mkds(GOOD)
    (sub / "task.json").unlink()
    (sub / "task.json").mkdir()
    refusals.append(go(sim_x, sdir_x, stage_cmd(sub, "ok-prefix"))["rc"] == 2)
    bad_prefixes = ["Upper", "a/../b", "a/b/c/d/e", "", "a%b", "/abs", "a//b", "-x", "a" * 65, ".hidden", "a/B"]
    pref = [go(sim_x, sdir_x, stage_cmd(d1, bp))["rc"] == 2 for bp in bad_prefixes]
    good_pref = go(sim_x, sdir_x, stage_cmd(d1, "x/y.z_1-2/"))
    case("stage-dataset", a and b and all(refusals) and all(pref) and good_pref["rc"] == 0
         and not [c for c in sim_x.calls if c[:2] == ["aws", "s3"] and "datasets/ok-prefix" in " ".join(c)] and "datasets/x/y.z_1-2/" in good_pref["out"].replace("\\", ""),
         "three or four files are put under datasets/<prefix>/ and recorded with their sha256; an extra, hidden, missing, symlinked or directory entry and %d bad prefixes are refused" % len(bad_prefixes))

    sim_z, sdir_z = world()
    sim_z.head_override["datasets/stance-64/train.jsonl"] = 3
    r_z = go(sim_z, sdir_z, stage_cmd(d1, "stance-64"))
    case("stage-dataset-size", r_z["rc"] == 1 and r_z["out"].rstrip().endswith("STAGE FAILED dataset SIZE") and not (sdir_z / "datasets-dev.json").exists()
         and "use a NEW prefix" in r_z["err"], "an uploaded file whose size differs from the local one fails the stage, records nothing and says to use a new prefix")

    # ---- stage-expect
    pins = {n_: sha_of(b_) for n_, b_ in GOOD.items()}
    sim, sdir = world()
    saved = EXPECT["motivation-v1"]
    try:
        # task.json and eval.jsonl are the pinned ones, train.jsonl is not: the refusal must name train.jsonl
        EXPECT["motivation-v1"] = dict(saved, **{"task.json": pins["task.json"], "eval.jsonl": pins["eval.jsonl"]})
        r_bad = go(sim, sdir, stage_cmd(d1, "motivation-v1", ("--expect", "motivation-v1")))
    finally:
        EXPECT["motivation-v1"] = saved
    r_unk = go(sim, sdir, stage_cmd(d1, "motivation-v1", ("--expect", "nope")))
    r_none = go(sim, sdir, stage_cmd(d1, "motivation-v1", ("--expect", "motivation-v1")))
    try:
        EXPECT["motivation-v1"] = pins
        r_ok = go(sim, sdir, stage_cmd(d1, "motivation-v1", ("--expect", "motivation-v1")))
    finally:
        EXPECT["motivation-v1"] = saved
    case("stage-expect", r_bad["rc"] == 2 and "REFUSED stage: train.jsonl is not Phase 12's" in r_bad["err"] and r_unk["rc"] == 2 and "REFUSED usage" in r_unk["err"]
         and r_none["rc"] == 2 and "is not Phase 12's" in r_none["err"] and r_ok["rc"] == 0 and saved["train.jsonl"].startswith("f1d84f19")
         and saved["task.json"].startswith("f74a3ad3") and saved["eval.jsonl"].startswith("ee95d71f") and not [c for c in sim.calls if c[:3] == ["aws", "s3", "cp"]][3:],
         "--expect motivation-v1 refuses a train.jsonl that is not Phase 12's before any call, and passes the files that are (pins f74a3ad3, f1d84f19, ee95d71f)")

    # ---- stage-immutable
    sim, sdir = world()
    sim.existing = {"datasets/stance-64/train.jsonl"}
    r = go(sim, sdir, stage_cmd(d1, "stance-64"))
    first = aws_calls(sim)[0] if aws_calls(sim) else []
    a = (r["rc"] == 2 and "REFUSED stage: datasets/stance-64/ already holds objects" in r["err"] and first[:3] == ["aws", "s3api", "list-objects-v2"]
         and not [c for c in sim.calls if c[:3] == ["aws", "s3", "cp"]] and "--max-keys 1" in " ".join(first))
    sim, sdir = world()
    r1 = go(sim, sdir, stage_cmd(d1, "stance-64"))
    r2 = go(sim, sdir, stage_cmd(d2, "stance-64"))
    n_puts = len([c for c in sim.calls if c[:3] == ["aws", "s3", "cp"]])
    case("stage-immutable", a and r1["rc"] == 0 and r2["rc"] == 2 and "already holds objects; use a new prefix" in r2["err"] and n_puts == 3,
         "an existing key under datasets/<prefix>/ refuses with no put (the listing is the first call after the account check); a restage of a staged prefix is refused")

    # ---- probe
    sim, sdir = world()
    r = go(sim, sdir, ["probe", "cuda", "dev", PROFILE], dry=True)
    o = r["out"]
    printed = json.JSONDecoder().raw_decode(o[o.index("request: ") + len("request: "):])[0] if "request: " in o else {}
    st = placeholder_state("dev")
    tr = st["outputs-dev.json"]["trainer"]
    want_req = build_probe_request("cuda", "cuda-20261029123456", tr["BucketName"], tr["SageMakerRoleArn"],
                                   "%s@%s" % (tr["TrainRepoUri"], st["images-dev.json"]["train"]["digest"]), "base/laya-%s/" % real_pin["revision"][:8])
    want_printed = json.loads(redact(json.dumps(want_req)))
    ch = printed.get("InputDataConfig", [{}])
    rs = printed.get("ResourceConfig", {})
    case("probe-request", r["rc"] == 0 and printed == want_printed and printed.get("TrainingJobName") == "laya-probe-cuda-20261029123456"
         and rs == {"InstanceType": "ml.g5.xlarge", "InstanceCount": 1, "VolumeSizeInGB": 100} and printed["StoppingCondition"] == {"MaxRuntimeInSeconds": 1800}
         and len(ch) == 1 and ch[0]["ChannelName"] == "base" and ch[0]["DataSource"]["S3DataSource"]["S3Uri"].endswith("/base/laya-55cf4c4e/")
         and printed["AlgorithmSpecification"]["TrainingImage"].endswith("@sha256:" + "a" * 64) and printed["RoleArn"].endswith(":role/aprender-laya-sagemaker-dev")
         and printed["Environment"]["LAYA_STEP"] == "probe" and printed["Environment"]["LAYA_PROBE_ID"] == "cuda-20261029123456" and "LAYA_BUCKET" in printed["Environment"]
         and o.rstrip().endswith("AWS CALLS: 0") and not aws_calls(sim) and any(ln.startswith("+ aws sagemaker create-training-job ") for ln in o.splitlines()),
         "dry run prints the one create-training-job request (ml.g5.xlarge x1, MaxRuntime 1800, the base channel only, the image by digest, LAYA_STEP probe); AWS CALLS: 0")
    sim_s, sdir_s = world()
    r_s = go(sim_s, sdir_s, ["probe", "sigterm", "dev", PROFILE], dry=True)
    ps = json.JSONDecoder().raw_decode(r_s["out"][r_s["out"].index("request: ") + len("request: "):])[0]
    case("probe-request-sigterm", ps["Environment"]["LAYA_STEP"] == "probe-sigterm" and ps["TrainingJobName"] == "laya-probe-sigterm-20261029123456"
         and len(ps["InputDataConfig"]) == 1 and not aws_calls(sim_s) and any(ln.startswith("+ aws sagemaker stop-training-job ") for ln in r_s["out"].splitlines()),
         "the sigterm dry run prints LAYA_STEP probe-sigterm, its job name and the stop-training-job command")

    # ---- probe-spend-guard
    sim, sdir = world()
    r1 = go(sim, sdir, ["probe", "cuda", "dev", PROFILE])
    r2 = go(sim, sdir, ["probe", "cuda", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-sigterm"})
    r3 = go(sim, sdir, ["probe", "sigterm", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-cuda"})
    r4 = go(sim, sdir, ["probe", "cuda", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-cuda "})
    refused_calls = len(sim.calls)
    sim2, sdir2 = world()
    r5 = go(sim2, sdir2, ["probe", "cuda", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-cuda"})
    case("probe-spend-guard", all(x["rc"] == 2 and "REFUSED spend: " in x["err"] for x in (r1, r2, r3, r4)) and refused_calls == 0
         and r5["rc"] == 0 and any(c[:3] == ["aws", "sagemaker", "create-training-job"] for c in sim2.calls),
         "a live probe without LAYA_TRAINER_APPROVED_SPEND=probe-<mode> (or with another probe's, or a padded value) is refused with no call; the exact value proceeds")

    # ---- probe-cuda-completed, probe-train-step
    sim, sdir = world()
    sim.cuda_end = ("Completed", None, 600.0)
    r = go(sim, sdir, ["probe", "cuda", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-cuda"})
    recf = sdir / "probe-cuda-20261029123456.json"
    rec = json.loads(recf.read_text()) if recf.exists() else {}
    pj = rec.get("probe") or {}
    keys5 = ("train_step_ok", "train_step_peak_bytes", "train_step_batch", "train_step_tokens", "host_rss_peak_bytes")
    case("probe-cuda-completed", r["rc"] == 0 and r["out"].rstrip().endswith("PROBE DONE cuda Completed") and rec.get("final_status") == "Completed"
         and rec.get("failure_reason") is None and rec.get("billable_seconds") == 660 and recf.stat().st_mode & 0o777 == 0o600
         and sim.job["req"]["TrainingJobName"] == "laya-probe-cuda-20261029123456" and sim.clock[0] == 600.0 and not ACCT_RE.search(r["out"]),
         "a Completed cuda probe is polled every 30 s, recorded (mode 600) and ends %r" % r["out"].rstrip().splitlines()[-1])
    case("probe-train-step", all(pj.get(k) == sim.probe_json[k] for k in keys5) and pj == sim.probe_json,
         "probe-<id>.json carries probe.json's train_step_ok, train_step_peak_bytes, train_step_batch, train_step_tokens and host_rss_peak_bytes unchanged")

    # ---- probe-verdict (owner decision amend-criterion): the fixture's rc is 0 or 3 on a cuda device; rc 2, a crash or a cpu device fails
    base_pj = dict(sim.probe_json)
    table = [("rc 0 on cuda", {"tiny_fixture_rc": 0, "tiny_fixture_device_used": "cuda:0"}, "pass"),
             ("rc 3 on cuda:0 (GATE FAIL, the measured outcome)", {"tiny_fixture_rc": 3, "tiny_fixture_device_used": "cuda:0"}, "pass"),
             ("rc 2 (refused) on cuda", {"tiny_fixture_rc": 2, "tiny_fixture_device_used": "cuda:0"}, "fail"),
             ("rc 1 (crash) on cuda", {"tiny_fixture_rc": 1, "tiny_fixture_device_used": "cuda:0"}, "fail"),
             ("rc 5 on cuda", {"tiny_fixture_rc": 5, "tiny_fixture_device_used": "cuda:0"}, "fail"),
             ("rc 3 on cpu", {"tiny_fixture_rc": 3, "tiny_fixture_device_used": "cpu"}, "fail"),
             ("rc 0 on cpu", {"tiny_fixture_rc": 0, "tiny_fixture_device_used": "cpu"}, "fail"),
             ("rc 3 with no device", {"tiny_fixture_rc": 3, "tiny_fixture_device_used": None}, "fail"),
             ("rc True", {"tiny_fixture_rc": True, "tiny_fixture_device_used": "cuda:0"}, "fail"),
             ("rc None", {"tiny_fixture_rc": None, "tiny_fixture_device_used": "cuda:0"}, "fail"),
             ("train step not ok", {"tiny_fixture_rc": 3, "tiny_fixture_device_used": "cuda:0", "train_step_ok": False}, "fail"),
             ("base load not ok", {"tiny_fixture_rc": 0, "tiny_fixture_device_used": "cuda:0", "base_load_ok": False}, "fail"),
             ("cuda unavailable", {"tiny_fixture_rc": 0, "tiny_fixture_device_used": "cuda:0", "torch_cuda_available": False}, "fail")]
    got = [(name, probe_verdict(dict(base_pj, **chg))[0], want) for name, chg, want in table]
    bad = [g for g in got if g[1] != g[2]]
    case("probe-verdict-table", not bad and probe_verdict(None)[0] == "fail" and probe_verdict({})[0] == "fail",
         "rc in {0,3} on a cuda device passes; rc 2, 1, 5, a bool or None rc, a cpu or absent device, a failed step and a missing probe.json fail (%d rows)%s"
         % (len(table), "" if not bad else "; WRONG: %r" % bad))
    sim3, sdir3 = world()
    sim3.probe_json = dict(base_pj, tiny_fixture_rc=3, tiny_fixture_device_used="cuda:0")
    r3 = go(sim3, sdir3, ["probe", "cuda", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-cuda"})
    rec3 = json.loads((sdir3 / "probe-cuda-20261029123456.json").read_text())
    sim2, sdir2 = world()
    sim2.probe_json = dict(base_pj, tiny_fixture_rc=2, tiny_fixture_device_used="cuda:0")
    r2 = go(sim2, sdir2, ["probe", "cuda", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-cuda"})
    rec2 = json.loads((sdir2 / "probe-cuda-20261029123456.json").read_text())
    case("probe-verdict-recorded", rec3.get("verdict") == "pass" and rec3.get("fail_reasons") == [] and rec2.get("verdict") == "fail"
         and any("tiny_fixture_rc" in x for x in rec2.get("fail_reasons", [])) and r3["rc"] == 0 and r2["rc"] == 0
         and r3["out"].rstrip().endswith("PROBE DONE cuda Completed") and r2["out"].rstrip().endswith("PROBE DONE cuda Completed"),
         "a recorded cuda probe carries verdict and fail_reasons: rc 3 on cuda:0 passes, rc 2 fails; the last line stays PROBE DONE cuda Completed")

    # ---- probe-maxruntime
    sim, sdir = world()
    sim.cuda_end = ("Stopped", "MaxRuntimeExceeded: Training job runtime exceeded MaxRuntimeInSeconds provided", 1800.0)
    r = go(sim, sdir, ["probe", "cuda", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-cuda"})
    rec = json.loads((sdir / "probe-cuda-20261029123456.json").read_text())
    sim_n, sdir_n = world()
    sim_n.cuda_end = ("Stopped", "MaxRuntimeExceeded: x", 1800.0)
    sim_n.probe_json = None
    r_n = go(sim_n, sdir_n, ["probe", "cuda", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-cuda"})
    rec_n = json.loads((sdir_n / "probe-cuda-20261029123456.json").read_text())
    case("probe-maxruntime", r["rc"] == 0 and r["out"].rstrip().endswith("PROBE DONE cuda Stopped") and rec["final_status"] == "Stopped"
         and rec["failure_reason"].startswith("MaxRuntimeExceeded") and rec["probe"] is not None
         and r_n["rc"] == 0 and r_n["out"].rstrip().endswith("PROBE DONE cuda Stopped") and rec_n["probe"] is None and rec_n["failure_reason"],
         "a cuda probe that ends Stopped with MaxRuntimeExceeded is recorded with its reason and the partial probe.json (or none), exit 0, last line PROBE DONE cuda Stopped")

    # ---- probe-sigterm-grace
    sim, sdir = world()
    r = go(sim, sdir, ["probe", "sigterm", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-sigterm"})
    rec = json.loads((sdir / "probe-sigterm-20261029123456.json").read_text())
    stop = [c for c in sim.calls if c[:3] == ["aws", "sagemaker", "stop-training-job"]]
    case("probe-sigterm-grace", r["rc"] == 0 and rec["grace_seconds"] == 28 and rec["final_status"] == "Stopped" and rec["heartbeats"] == 15
         and r["out"].rstrip().endswith("PROBE DONE sigterm Stopped") and len(stop) == 1 and rec["sigterm_ready"] is True
         and rec["stop_requested_after_in_progress_seconds"] >= 120 and sim.job["stop_at"] >= 200.0 and rec["probe"] is None,
         "heartbeats at 0, 2, ..., 28 s -> grace_seconds %s, final Stopped; the stop waited for the container's ready marker (stop at %d s)" % (rec["grace_seconds"], sim.job["stop_at"]))
    sim, sdir = world()
    sim.ready_never = True
    r = go(sim, sdir, ["probe", "sigterm", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-sigterm"})
    rec = json.loads((sdir / "probe-sigterm-20261029123456.json").read_text())
    case("probe-sigterm-noready", r["rc"] == 0 and rec["sigterm_ready"] is False and sim.job["stop_at"] >= SIGTERM_AFTER_S + SIGTERM_READY_CAP_S
         and rec["final_status"] == "Stopped", "a container that never shows its ready marker is still stopped, after %d s, and the record says it was not ready" % (SIGTERM_AFTER_S + SIGTERM_READY_CAP_S))
    sim, sdir = world()
    sim.job_create_rc = 254
    r = go(sim, sdir, ["probe", "cuda", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-cuda"})
    sim2, sdir2 = world()
    sim2.cuda_end = ("Completed", None, 10 ** 9)
    r2 = go(sim2, sdir2, ["probe", "cuda", "dev", PROFILE], env_extra={"LAYA_TRAINER_APPROVED_SPEND": "probe-cuda"})
    case("probe-failures", r["rc"] == 1 and r["out"].rstrip().endswith("PROBE FAILED cuda CREATE") and r2["rc"] == 1
         and r2["out"].rstrip().endswith("PROBE FAILED cuda POLL_TIMEOUT") and any(c[:3] == ["aws", "sagemaker", "stop-training-job"] for c in sim2.calls)
         and not list(sdir2.glob("probe-*.json")), "a failed create and a job that outlasts the cap end PROBE FAILED (the latter stops the job); nothing is recorded as a result")

    # ---- plan 14-10: config and deploy
    CFGARGS = ["config", "dev", PROFILE]
    DEPLOY_OK = {"LAYA_TRAINER_APPROVED_DEPLOY": "dev"}
    GOOD_LOG = ("SWAPPED crates: decide config installed alone\n   Compiling aprender-setfit-train-lambda v0.69.3 (/x)\n"
                "   Compiling aprender-mcp-laya-train-lambda v0.69.3 (/x)\n\U0001f680 Deploying to pmcp.run...\n"
                "   Server URL: https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp\n"
                "      mcp_endpoint: \"https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp\"\n"
                "RESTORED crates byte-identical (exit 0)\n")

    def with_config():
        sim, sdir = world()
        rc = go(sim, sdir, CFGARGS)
        return sim, sdir, rc

    def swaps(sim):
        return [c for c in sim.calls if c[:2] == ["just", "_laya-crates-root-swap"]]

    def iam_calls(sim):
        return [c[2] for c in sim.calls if c[0] == "aws" and c[1] in ("iam", "lambda")]

    # config-render: the REAL template, filled from the placeholder outputs, written once at mode 600, no command run
    sim, sdir = world()
    r = go(sim, sdir, CFGARGS)
    cfgp = sdir / "deploy-config-dev.toml"
    txt = cfgp.read_text() if cfgp.exists() else ""
    out_dev = json.loads((sdir / "outputs-dev.json").read_text())["trainer"]
    import tomllib as _tl
    doc = _tl.loads(txt) if txt else {}
    envd = doc.get("environment", {})
    case("config-render", r["rc"] == 0 and r["out"].rstrip().endswith("CONFIG OK deploy-config-dev.toml") and not sim.calls
         and stat.S_IMODE(cfgp.stat().st_mode) == 0o600 and envd.get("APRENDER_LAYA_TRAIN_TABLE") == out_dev["TableName"]
         and envd.get("APRENDER_LAYA_TRAIN_BUCKET") == out_dev["BucketName"]
         and envd.get("APRENDER_LAYA_TRAIN_STATE_MACHINE_ARN") == out_dev["StateMachineArn"] and UNSET not in txt
         and doc["server"]["name"] == SERVER_ID and doc["auth"]["enabled"] is False and "iam" not in doc and not ACCT_RE.search(r["out"]),
         "the tracked template filled from outputs-dev.json: three values, mode 600, auth off, no IAM table, no command, no account id printed")
    simd, sdird = world()
    rd = go(simd, sdird, CFGARGS, dry=True)
    case("config-render-dry", rd["rc"] == 0 and rd["out"].rstrip().endswith("CONFIG OK deploy-config-dev.toml") and "AWS CALLS: 0" in rd["out"]
         and not simd.calls, "a dry run renders the same file and reports AWS CALLS: 0")

    # config-iam-refused: a template copy holding an IAM table on a non-comment line, a changed placeholder count, --template outside a dry run
    tpl_text = (REPO_ROOT / TEMPLATE).read_text()
    iamdir = fresh("tpl")
    (iamdir / "iam.template").write_text(tpl_text + '\n[[iam.statements]]\neffect = "Allow"\nactions = ["s3:GetObject"]\nresources = ["*"]\n')
    (iamdir / "two.template").write_text(tpl_text.replace('APRENDER_LAYA_TRAIN_TABLE = "%s"\n' % UNSET, ""))
    (iamdir / "dup.template").write_text(tpl_text.replace('APRENDER_LAYA_TRAIN_TABLE = "%s"\n' % UNSET, 'APRENDER_LAYA_TRAIN_TABLE = "%s"\nAPRENDER_LAYA_TRAIN_TABLE = "%s"\n' % (UNSET, UNSET)))
    (iamdir / "extra.template").write_text(tpl_text.replace('RUST_LOG = "info"', 'RUST_LOG = "%s"' % UNSET))
    (iamdir / "authon.template").write_text(tpl_text.replace("[auth]\nenabled = false", "[auth]\nenabled = true"))
    sim, sdir = world()
    r1 = go(sim, sdir, CFGARGS + ["--template", str(iamdir / "iam.template")], dry=True)
    r2 = go(sim, sdir, CFGARGS + ["--template", str(iamdir / "two.template")], dry=True)
    r3 = go(sim, sdir, CFGARGS + ["--template", str(iamdir / "extra.template")], dry=True)
    r4 = go(sim, sdir, CFGARGS + ["--template", str(iamdir / "authon.template")], dry=True)
    r5 = go(sim, sdir, CFGARGS + ["--template", str(iamdir / "iam.template")])
    r6 = go(sim, sdir, CFGARGS + ["--template", str(iamdir / "dup.template")], dry=True)
    case("config-iam-refused", r1["rc"] == 2 and "REFUSED config: the template declares IAM" in r1["err"] and r2["rc"] == 2
         and "REFUSED config: the template must hold exactly one APRENDER_LAYA_TRAIN_TABLE" in r2["err"] and r3["rc"] == 2
         and "REFUSED config: " in r3["err"] and r4["rc"] == 2 and "enabled = false" in r4["err"] and r5["rc"] == 2 and "--template is honoured only under DRY_RUN=1" in r5["err"]
         and r6["rc"] == 2 and "exactly one APRENDER_LAYA_TRAIN_TABLE placeholder line, found 2" in r6["err"]
         and not (sdir / "deploy-config-dev.toml").exists(), "IAM on a non-comment line, a missing or extra placeholder, auth on, and --template outside a dry run each refuse; no file written")
    rp = go(world()[0], world()[1], ["config", "dev", "augury-dev"], dry=True)
    rp2 = go(world()[0], world()[1], ["config", "staging", PROFILE], dry=True)
    case("config-profile", rp["rc"] == 2 and "REFUSED profile:" in rp["err"] and rp2["rc"] == 2 and "REFUSED env:" in rp2["err"], "config refuses a foreign profile and env")

    # deploy-dry-run
    sim, sdir, _ = with_config()
    r = go(sim, sdir, ["deploy", "dev", PROFILE], dry=True)
    cmds = [ln for ln in r["out"].splitlines() if ln.startswith("+ ")]
    case("deploy-dry-run", r["rc"] == 0 and r["out"].rstrip().endswith("AWS CALLS: 0") and not aws_calls(sim) and not swaps(sim)
         and any(re.match(r"\+ just _laya-crates-root-swap crates \S+deploy-config-dev\.toml \S+deploy-dev\.state cargo pmcp deploy --manifest-path crates "
                          r"--regenerate-stack --no-post-deploy-test --no-oauth --no-color$", ln) for ln in cmds)
         and any(ln.startswith("+ aws lambda get-function --function-name aprender-mcp-laya-train ") for ln in cmds)
         and not any(c[0] == "touch" for c in sim.calls),
         "the swap and the cargo pmcp line are printed, never run; the revoke inspection is printed; AWS CALLS: 0")
    sim, sdir = world()
    r = go(sim, sdir, ["deploy", "dev", PROFILE], dry=True)
    rl = go(sim, sdir, ["deploy", "dev", "augury-dev"], dry=True)
    sim2, sdir2, _ = with_config()
    r_na = go(sim2, sdir2, ["deploy", "dev", PROFILE])
    case("deploy-refusals", r["rc"] == 2 and "REFUSED deploy: deploy-config-dev.toml is missing" in r["err"] and rl["rc"] == 2 and "REFUSED profile:" in rl["err"]
         and r_na["rc"] == 2 and "REFUSED approval: a live deploy needs LAYA_TRAINER_APPROVED_DEPLOY=dev" in r_na["err"] and not sim2.calls,
         "no config, a foreign profile and an unapproved live deploy each refuse before any command")

    # the revoke cases (live, simulated world)
    def live_deploy(setup=None, log=GOOD_LOG, swap_rc=0):
        sim, sdir, _ = with_config()
        sim.swap_out = log
        sim.swap_rc = swap_rc
        if setup:
            setup(sim.iam)
        r = go(sim, sdir, ["deploy", "dev", PROFILE], env_extra=DEPLOY_OK)
        return sim, sdir, r

    def names(sim):
        out = []
        for c in sim.calls:
            if c[0] == "aws":
                out.append(c[2])
            elif c[:2] == ["just", "_laya-crates-root-swap"]:
                out.append("SWAP")
            elif c[0] == "touch":
                out.append("touch")
        return out

    sim, sdir, r = live_deploy(lambda st: st.update(policy=True))
    seq = names(sim)
    case("deploy-revoke-grant", r["rc"] == 0 and "GRANT REVOKED for redeploy" in r["out"]
         and seq == ["get-function", "get-role-policy", "delete-role-policy", "get-role-policy", "touch", "SWAP"] and sim.iam["policy"] is False,
         "found -> deleted -> NoSuchEntity on the re-read -> GRANT REVOKED for redeploy, all before the swap (%s)" % ",".join(seq))
    sim, sdir, r = live_deploy()
    seq = names(sim)
    case("deploy-revoke-absent", r["rc"] == 0 and seq == ["get-function", "get-role-policy", "touch", "SWAP"] and "GRANT REVOKED" not in r["out"]
         and "nothing to revoke" in r["out"], "NoSuchEntity -> proceed with no delete (%s)" % ",".join(seq))
    sim, sdir, r = live_deploy(lambda st: st.update(function=False))
    seq = names(sim)
    case("deploy-revoke-no-function", r["rc"] == 0 and seq == ["get-function", "touch", "SWAP"] and "nothing to revoke" in r["out"],
         "ResourceNotFoundException -> proceed (%s)" % ",".join(seq))
    REF = "REFUSED deploy: cannot confirm the grant is absent"
    sim, sdir, r = live_deploy(lambda st: st.update(inspect_err="An error occurred (AccessDenied) when calling the GetRolePolicy operation: no"))
    simf, sdirf, rf = live_deploy(lambda st: st.update(fn_err="An error occurred (AccessDenied) when calling the GetFunction operation: no"))
    simg, sdirg, rg = live_deploy(lambda st: st.update(inspect_err="Throttling"))
    case("deploy-revoke-inspect-error", r["rc"] == 2 and REF in r["err"] and "SWAP" not in names(sim) and "touch" not in names(sim)
         and not (sdir / "deploy-dev.json").exists() and rf["rc"] == 2 and REF in rf["err"] and "SWAP" not in names(simf)
         and rg["rc"] == 2 and REF in rg["err"] and "SWAP" not in names(simg),
         "AccessDenied on get-role-policy, AccessDenied on get-function and an unclassified reply each refuse: no cargo pmcp call, no state")
    sim, sdir, r = live_deploy(lambda st: st.update(policy=True, delete_err="An error occurred (AccessDenied) when calling the DeleteRolePolicy operation: no"))
    case("deploy-revoke-delete-error", r["rc"] == 2 and REF in r["err"] and "delete-role-policy answered AccessDenied" in r["err"] and "SWAP" not in names(sim)
         and sim.iam["policy"] is True,
         "a delete that fails refuses with the same line; no cargo pmcp call")
    sim, sdir, r = live_deploy(lambda st: st.update(policy=True, sticky=True))
    sim2, sdir2, r2 = live_deploy(lambda st: st.update(policy=True, reread_err="An error occurred (AccessDenied) when calling the GetRolePolicy operation"))
    case("deploy-revoke-reread", r["rc"] == 2 and REF in r["err"] and "still reads back" in r["err"] and "SWAP" not in names(sim)
         and r2["rc"] == 2 and REF in r2["err"] and "SWAP" not in names(sim2),
         "a delete that does not take, and a re-read that cannot answer NoSuchEntity, both refuse")

    # deploy-wrong-package
    bad_log = GOOD_LOG.replace("   Compiling aprender-mcp-laya-train-lambda v0.69.3 (/x)\n", "   Compiling aprender-mcp-setfit-lambda v0.69.3 (/x)\n")
    sim, sdir, r = live_deploy(log=bad_log)
    extra_log = GOOD_LOG.replace("   Compiling aprender-setfit-train-lambda", "   Compiling aprender-mcp-chronos-lambda")
    sim2, sdir2, r2 = live_deploy(log=extra_log)
    cached_log = GOOD_LOG.replace("   Compiling aprender-setfit-train-lambda v0.69.3 (/x)\n", "").replace("   Compiling aprender-mcp-laya-train-lambda v0.69.3 (/x)\n", "")
    sim3, sdir3, r3 = live_deploy(log=cached_log)
    case("deploy-wrong-package", r["rc"] == 2 and "REFUSED deploy: wrong package" in r["err"] and not (sdir / "deploy-dev.json").exists()
         and r3["rc"] == 2 and "REFUSED deploy: wrong package: the build log does not name aprender-mcp-laya-train-lambda" in r3["err"]
         and not (sdir3 / "deploy-dev.json").exists()
         and r2["rc"] == 2 and "REFUSED deploy: wrong package" in r2["err"] and "aprender-mcp-chronos-lambda" in r2["err"] and not (sdir2 / "deploy-dev.json").exists(),
         "a log that does not name the package, and one that also builds another -lambda crate, refuse and record nothing")

    # deploy-state
    sim, sdir, r = live_deploy()
    dj = sdir / "deploy-dev.json"
    rec = json.loads(dj.read_text()) if dj.exists() else {}
    bad_label = GOOD_LOG.replace("aprender-mcp-laya-train.us-east", "aprender-mcp-decide.us-east")
    sim2, sdir2, r2 = live_deploy(log=bad_label)
    off = GOOD_LOG.replace("us-east.true-mcp.com", "eu-west.augury.pmcp.run")
    sim3, sdir3, r3 = live_deploy(log=off)
    nolog = "   Compiling aprender-mcp-laya-train-lambda v0.69.3 (/x)\nno endpoint here\n"
    sim4, sdir4, r4 = live_deploy(log=nolog)
    fail_sim, fail_sdir, fr = live_deploy(swap_rc=1)
    case("deploy-state", r["rc"] == 0 and stat.S_IMODE(dj.stat().st_mode) == 0o600
         and rec == {"schema": "laya-trainer-deploy-v1", "env": "dev", "server_id": SERVER_ID, "package": PACKAGE,
                     "endpoint": "https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp", "deployed_at": "2026-10-29T12:34:56Z"}
         and "endpoint: https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp" in r["out"] and r["out"].rstrip().endswith("DEPLOY OK aprender-mcp-laya-train")
         and (sdir / "deploy-dev.log").exists() and stat.S_IMODE((sdir / "deploy-dev.log").stat().st_mode) == 0o600
         and r2["rc"] == 2 and "names server aprender-mcp-decide" in r2["err"] and not (sdir2 / "deploy-dev.json").exists()
         and r3["rc"] == 2 and "not under *.us-east.true-mcp.com" in r3["err"] and not (sdir3 / "deploy-dev.json").exists()
         and r4["rc"] == 2 and "no https://<server>.<host>/mcp endpoint" in r4["err"]
         and fr["rc"] == 1 and "DEPLOY FAILED rc=1" in fr["out"] and not (fail_sdir / "deploy-dev.json").exists(),
         "the endpoint parsed from the log, its host label and tenant checked, deploy-dev.json (mode 600) written; a wrong label, a foreign tenant, "
         "no endpoint and a failed deploy record nothing")

    # ---- plan 14-10: grant (the 401 observed in the same invocation, on the endpoint bound to the function)
    GRANT_OK = {"LAYA_TRAINER_APPROVED_GRANT": "dev"}
    GRANT_EP = "https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp"
    POLICY_DOC = json.dumps({"Version": "2012-10-17", "Statement": [{"Effect": "Allow", "Action": ["dynamodb:GetItem"], "Resource": "*"}]})

    def grant_world(endpoint=GRANT_EP, package=PACKAGE, server_id=SERVER_ID, with_state=True):
        sim, sdir = world()
        if with_state:
            (sdir / "deploy-dev.json").write_text(json.dumps({"schema": "laya-trainer-deploy-v1", "env": "dev", "server_id": server_id, "package": package,
                                                              "endpoint": endpoint, "deployed_at": "2026-10-29T12:34:56Z"}))
        outp = sdir / "outputs-dev.json"
        o = json.loads(outp.read_text())
        o["trainer"]["RequestLambdaPolicy"] = POLICY_DOC
        outp.write_text(json.dumps(o))
        return sim, sdir

    def graw(sim, sdir, argv=("grant", "dev", PROFILE), live=True, **kw):
        return go(sim, sdir, list(argv), dry=not live, env_extra=GRANT_OK if live else None, **kw)

    def gnames(sim):
        out = []
        for c in sim.calls:
            if c[0] == "curl":
                out.append("health")
            elif c[:2] == ["just", "_laya-auth-refusal-check"]:
                out.append("auth")
            elif c[:2] == ["just", "_laya-account-check"]:
                out.append("account")
            elif c[0] == "aws":
                out.append(c[2])
        return out

    AUTH_OPEN = "REFUSED auth-open: unauthenticated tools/list answered HTTP 200 after 1 poll(s) (0 s, wait 0 s); tools/call was not sent\n"
    sim, sdir = grant_world()
    sim.auth = {"rc": 1, "out": "", "err": AUTH_OPEN}
    r = graw(sim, sdir)
    sim2, sdir2 = grant_world()
    sim2.auth = {"rc": 1, "out": "", "err": "REFUSED auth-open: tools/call answered HTTP 200 without a bearer although tools/list answered 401\n"}
    r2 = graw(sim2, sdir2)
    case("grant-open-refused", r["rc"] == 2 and "REFUSED grant: OAuth not confirmed (tools/list answered 200)" in r["err"] and "put-role-policy" not in gnames(sim)
         and not aws_calls(sim) and r2["rc"] == 2 and "REFUSED grant: OAuth not confirmed (tools/call answered 200)" in r2["err"] and not aws_calls(sim2),
         "an open tools/list (200) or tools/call is refused with the answered code; no IAM call, no aws call at all")
    sim, sdir = grant_world()
    sim.auth = {"rc": 1, "out": "", "err": "REFUSED auth-check: transport (curl exit 7) on tools/list, poll 1\n"}
    r = graw(sim, sdir)
    sim2, sdir2 = grant_world()
    sim2.health["rc"] = 7
    r2 = graw(sim2, sdir2)
    case("grant-unreachable", r["rc"] == 2 and "REFUSED grant: endpoint unreachable (transport error; retry)" in r["err"] and not aws_calls(sim)
         and r2["rc"] == 2 and "REFUSED grant: endpoint unreachable (transport error; retry)" in r2["err"] and not aws_calls(sim2)
         and not any(c[:2] == ["just", "_laya-auth-refusal-check"] for c in sim2.calls),
         "a curl transport failure (on the auth check or on /health) is `endpoint unreachable ... retry`, never `OAuth not confirmed`; no IAM call")
    NB = "REFUSED grant: endpoint not bound to aprender-mcp-laya-train"
    variants = []
    variants.append(grant_world(with_state=False))
    variants.append(grant_world(endpoint="https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp"))
    variants.append(grant_world(package="aprender-mcp-decide-lambda"))
    variants.append(grant_world(endpoint="https://aprender-mcp-laya-train.eu-west.augury.pmcp.run/mcp"))
    sim_h, sdir_h = grant_world()
    sim_h.health["body"] = '{"status":"healthy","serverId":"aprender-mcp-decide-motivation"}'
    variants.append((sim_h, sdir_h))
    sim_c, sdir_c = grant_world()
    sim_c.health["code"] = "404"
    variants.append((sim_c, sdir_c))
    sim_j, sdir_j = grant_world()
    sim_j.health["body"] = "not json"
    variants.append((sim_j, sdir_j))
    res = [(v[0], graw(v[0], v[1])) for v in variants]
    sim_x, sdir_x = grant_world()
    rx = graw(sim_x, sdir_x, ["grant", "dev", PROFILE, GRANT_EP])
    ry = graw(sim_x, sdir_x, ["grant", "dev", PROFILE, "--url", GRANT_EP])
    case("grant-url-binding", all(rr["rc"] == 2 and NB in rr["err"] and "auth" not in gnames(sm) and not aws_calls(sm) for sm, rr in res)
         and rx["rc"] == 2 and "REFUSED usage: grant takes exactly ENV PROFILE: unknown argument" in rx["err"] and not sim_x.calls
         and ry["rc"] == 2 and "REFUSED usage: unknown option --url" in ry["err"],
         "no deploy state, a foreign host label, a foreign package, a foreign tenant, a /health naming another server, a 404 and a non-JSON /health each "
         "refuse `endpoint not bound` with no auth check and no IAM call; a URL argument and --url are refused as unknown")

    # grant-dry-run
    sim, sdir = grant_world()
    r = graw(sim, sdir, live=False)
    cmds = [ln for ln in r["out"].splitlines() if ln.startswith("+ ")]
    idx = lambda pre: next((i for i, ln in enumerate(cmds) if ln.startswith(pre)), -1)
    i_h, i_a, i_f, i_p = idx("+ curl "), idx("+ just _laya-auth-refusal-check " + GRANT_EP + " 0 train_status"), idx("+ aws lambda get-function --function-name aprender-mcp-laya-train "), idx("+ aws iam put-role-policy ")
    case("grant-dry-run", r["rc"] == 0 and r["out"].rstrip().endswith("AWS CALLS: 0") and not aws_calls(sim) and 0 <= i_h < i_a < i_f < i_p
         and "--policy-name aprender-laya-train-dev" in cmds[i_p] and not iam_calls(sim),
         "health, the train_status auth check, get-function for aprender-mcp-laya-train and put-role-policy aprender-laya-train-dev are printed in that order; AWS CALLS: 0")

    # grant-order: a live grant, the 401 immediately before the IAM call
    sim, sdir = grant_world()
    r = graw(sim, sdir)
    seq = gnames(sim)
    case("grant-order", r["rc"] == 0 and seq == ["health", "auth", "account", "get-function", "put-role-policy"] and sim.iam["put_doc"] == POLICY_DOC
         and r["out"].rstrip().endswith("GRANT OK role deploy-role") and "policy: aprender-laya-train-dev" in r["out"]
         and [c for c in sim.calls if c[:2] == ["just", "_laya-auth-refusal-check"]][0][2:] == [GRANT_EP, "0", "train_status"],
         "health, the 401 check on the bound endpoint, the account check, the role lookup, then put-role-policy (%s)" % ",".join(seq))
    sim, sdir = grant_world()
    sim.account_refusal = True
    r = graw(sim, sdir)
    sim_n, sdir_n = grant_world()
    r_n = go(sim_n, sdir_n, ["grant", "dev", PROFILE])
    sim_f, sdir_f = grant_world()
    sim_f.iam["function"] = False
    r_f = graw(sim_f, sdir_f)
    sim_p, sdir_p = grant_world()
    sim_p.iam["put_err"] = "An error occurred (AccessDenied) when calling the PutRolePolicy operation: no"
    r_p = graw(sim_p, sdir_p)
    sim_d, sdir_d = grant_world()
    o2 = json.loads((sdir_d / "outputs-dev.json").read_text())
    o2["trainer"]["RequestLambdaPolicy"] = "{}"
    (sdir_d / "outputs-dev.json").write_text(json.dumps(o2))
    r_d = graw(sim_d, sdir_d)
    case("grant-refusals", r["rc"] == 2 and "REFUSED target-account" in r["err"] and "get-function" not in gnames(sim)
         and r_n["rc"] == 2 and "REFUSED approval: a live grant needs LAYA_TRAINER_APPROVED_GRANT=dev" in r_n["err"] and not sim_n.calls
         and r_f["rc"] == 2 and "REFUSED grant: cannot read the role of function aprender-mcp-laya-train" in r_f["err"] and "put-role-policy" not in gnames(sim_f)
         and r_p["rc"] == 1 and "GRANT FAILED" in r_p["out"] and "GRANT OK" not in r_p["out"]
         and r_d["rc"] == 2 and "REFUSED state: outputs-dev.json RequestLambdaPolicy has no Statement" in r_d["err"] and not sim_d.calls,
         "a foreign account, an unapproved live grant, an absent function, a failing put and a placeholder policy never reach (or never complete) the IAM write")

    # ---- placeholder state
    sim, sdir = world(stateless=True)
    box = io.StringIO()
    c2 = Ctx(env={"DRY_RUN": "1"}, stdout=box, stderr=io.StringIO())
    rc1 = write_placeholder_state(c2, str(sdir), "dev")
    try:
        write_placeholder_state(Ctx(env={"DRY_RUN": "1"}, stdout=io.StringIO(), stderr=io.StringIO()), str(sdir), "dev")
        again = False
    except Refused:
        again = True
    sim2, sdir2 = world(stateless=True)
    rp = go(sim2, sdir2, ok, dry=True)
    case("placeholder-state", rc1 == 0 and again and sorted(p.name for p in sdir.iterdir()) == ["images-dev.json", "outputs-dev.json", "outputs-images-dev.json"]
         and rp["rc"] == 2 and "REFUSED state: outputs-images-dev.json is missing" in rp["err"],
         "placeholder state is written once, never over an existing file; absent state refuses even a dry run")

    shutil.rmtree(work, ignore_errors=True)
    print("%d ok, %d FAIL" % (n_ok[0], len(fails)))
    if fails:
        print("LAYA TRAINER OPS SELFTEST FAILED: " + ", ".join(fails))
        return 1
    print("LAYA TRAINER OPS SELFTEST OK")
    return 0


if __name__ == "__main__":
    sys.exit(entry(sys.argv[1:]))
