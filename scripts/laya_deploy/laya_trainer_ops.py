#!/usr/bin/env python3
"""The owner-run AWS recipes of the Laya training server's first deployment (plan 14-09; D-01, D-02, D-03, D-07, D-13, D-16).

    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] image  WHICH ENV PROFILE [--list-files]
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] infra  ACTION ENV PROFILE
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] stage  base ENV PROFILE [--tar FILE]
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] stage  dataset DIR PREFIX ENV PROFILE [--expect motivation-v1]
    python3 -I scripts/laya_deploy/laya_trainer_ops.py [--state-dir DIR] probe  cuda|sigterm ENV PROFILE
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

stdlib only. Every external command goes through ONE Runner (aws, npx cdk, git, just); --selftest replaces it with a
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
FINALIZER_ASSET = "deploy-extensions/assets/laya-finalizer/bootstrap"
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
        child.update(env)
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
        self.state = None

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


def connect(ctx, profile):
    """The account gate: region from the target table, then `_laya-account-check` before any other AWS call."""
    root = str(ctx.repo_root)
    t = ctx.runner.run(["just", "_laya-target-check", profile, ""], local=True, cwd=root, quiet=True)
    if t.rc != 0:
        passthrough_refusal(t, "target")
    parts = t.out.split()
    if len(parts) < 2 or not REGION_RE.fullmatch(parts[1]):
        refuse("target", "the target check answered no region")
    region = parts[1]
    a = ctx.runner.run(["just", "_laya-account-check", profile, region], local=True, cwd=root)
    if a.rc != 0:
        passthrough_refusal(a, "target-account")
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
    if action in ("diff", "deploy"):
        digests = load_image_digests(ctx, env)
        if not ctx.file_exists(str(ctx.repo_root / FINALIZER_ASSET)):
            msg = "the finalizer asset %s is absent; run `just laya-trainer-synth %s` to build it" % (FINALIZER_ASSET, env)
            if ctx.dry:
                ctx.say("DRY RUN NOTE: a live run would be REFUSED infra: " + msg)
            else:
                refuse("infra", msg)
    region = connect(ctx, profile)
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


# ---- dispatch ------------------------------------------------------------------------------------------------

VALUE_OPTS = {"--state-dir": "state_dir", "--tar": "tar", "--expect": "expect", "--stage-json": "stage_json", "--env": "env"}
FLAG_OPTS = {"--list-files": "list_files"}
COMMANDS = {"image": cmd_image, "infra": cmd_infra}
SUBCOMMAND_OPTS = {"image": ("state_dir", "list_files"), "infra": ("state_dir",)}


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
            refuse("usage", "unknown subcommand %r (image, infra, stage, probe)" % cmd[:40])
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
        self.real_git = False
        self.build_statuses = ["IN_PROGRESS", "IN_PROGRESS", "SUCCEEDED"]
        self.build_polls = 0
        self.digest = "sha256:" + "d" * 64
        self.stacks = {}
        self.objects = {}
        self.copied = {}
        self.cdk_rc = {}
        self.extra = {}

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
        h = self.extra.get((svc, op))
        if h:
            return h(argv)
        return Result(97, "", "fake aws: unexpected call: %s" % shlex.join(argv))


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

    def world(dry=False, env=None, real_git=False, tree=None, stateless=False):
        d = fresh("w")
        sim = Sim(d)
        sim.real_git = real_git
        sim.tree = list(tree if tree is not None else DEFAULT_TREE)
        sdir = d / "state"
        sdir.mkdir(mode=0o700)
        if not stateless:
            for e in ENVS:
                for name, data in placeholder_state(e).items():
                    (sdir / name).write_text(json.dumps(data))
        return sim, sdir

    def go(sim, sdir, argv, dry=False, env_extra=None, file_exists=None, state_opt=True):
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
    for argv in (["image", "train"], ["image", "pack"], ["infra", "diff"], ["infra", "deploy"], ["infra", "outputs"], ["infra", "outputs-images"]):
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
