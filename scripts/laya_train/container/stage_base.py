#!/usr/bin/env python3
"""Stage Laya's pinned base snapshot as ONE deterministic tar, extract it safely, prove it loads offline (plan 14-06).

    stage_base.py build  [--cache DIR] --out FILE [--revision REV] [--stage-json FILE]
    stage_base.py check  --tar FILE            (run under the uv project: `uv run --project scripts/laya_train --frozen python ...`)
    stage_base.py --selftest                   (stdlib only; prints STAGE BASE SELFTEST OK)

The tar is the S3 `base` channel's one object (laya-base.tar). The base is NEVER baked into the image and never
downloaded at build time: the training container (run_step.py) verifies the channel tar's sha256 against
base-stage.json, extracts it with safe_extract() into HF_HOME/hub, and runs with HF_HUB_OFFLINE=1.

DECLARED RULES (the selftest asserts each):

    contents      from the HF cache dir models--convaiinnovations--laya: snapshots/<rev>/** MINUS typed-decisions/ (1.6 GB
                  against the 0.84 GB root checkpoint), the blobs those snapshot links resolve to, and trees/<rev>.json.
                  Unreferenced blobs are not staged.
    links         a snapshot symlink must point at a blob of THIS cache (`../blobs/<name>`) and its target must resolve
                  inside the cache dir or inside the hub-level blob store `<hub>/blobs/` (the xet-backed cache keeps big
                  blobs there: the root model.safetensors blob is a symlink into it). Anything else is refused. In the tar
                  every blob is a regular file and every snapshot entry a RELATIVE symlink.
    determinism   entries sorted bytewise by name, mtime 0, uid/gid 0, empty uname/gname, modes 0644/0755 (0777 for links),
                  GNU format; two builds of the same cache are byte-identical.
    extraction    refuses absolute names, `..` components, hardlinks, devices and symlinks (absolute or resolving outside
                  the destination), then re-checks every extracted path's realpath.
    identity      the staged model.safetensors must hash to the pinned digest (the contract's base.model_safetensors_sha256;
                  `check` asserts the two pins in this file equal the contract).
"""
import argparse
import hashlib
import json
import os
import posixpath
import sys
import tarfile
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = Path(__file__).resolve().parents[3]

REPO_ID = "convaiinnovations/laya"
REVISION = "55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851"
MODEL_SHA256 = "891102d372688fc2a094dac56a384bc537b87c63f21f9f3dac0be2b7cbc8d86c"
CACHE_DIR_NAME = "models--convaiinnovations--laya"
DEFAULT_CACHE = Path.home() / ".cache" / "huggingface" / "hub" / CACHE_DIR_NAME
TAR_NAME = "laya-base.tar"
EXCLUDED = ("typed-decisions/",)
SCHEMA = "laya-base-stage-v1"
BASE_FILES = ("rl_agent_config.json", "model.safetensors", "tokenizer/*", "encoder/*")   # as train.py Base.directory


class StageError(Exception):
    pass


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# ------------------------------------------------------------------------------------------ build

def _excluded(rel):
    return any((rel + "/").startswith(e) for e in EXCLUDED)


def collect(cache, revision):
    """{archive name: ("dir",) | ("file", Path) | ("link", relative target)} for the staged subtree."""
    cache = Path(cache)
    snap = cache / "snapshots" / revision
    tree = cache / "trees" / (revision + ".json")
    if not snap.is_dir():
        raise StageError("no snapshot %s under %s" % (revision, cache))
    if not tree.is_file():
        raise StageError("no trees/%s.json under %s" % (revision, cache))
    prefix = cache.name
    roots = [os.path.realpath(cache)]
    store = cache.parent / "blobs"
    if store.is_dir():
        roots.append(os.path.realpath(store))

    def inside(p):
        return any(p == r or p.startswith(r + os.sep) for r in roots)

    members = {}
    members["%s/trees/%s.json" % (prefix, revision)] = ("file", tree)
    for src in sorted(snap.rglob("*")):
        rel = src.relative_to(snap).as_posix()
        if _excluded(rel):
            continue
        arc = "%s/snapshots/%s/%s" % (prefix, revision, rel)
        if src.is_symlink():
            real = os.path.realpath(src)
            if not inside(real) or not os.path.isfile(real):
                raise StageError("snapshot link %s resolves outside the cache (%s)" % (rel, real))
            lex = os.path.normpath(os.path.join(os.path.dirname(src), os.readlink(src)))
            name = os.path.relpath(lex, cache / "blobs")
            if name.startswith("..") or os.sep in name:
                raise StageError("snapshot link %s does not point at a blob of this cache" % rel)
            members["%s/blobs/%s" % (prefix, name)] = ("file", Path(real))
            members[arc] = ("link", posixpath.relpath("%s/blobs/%s" % (prefix, name), posixpath.dirname(arc)))
        elif src.is_dir():
            members[arc] = ("dir",)
        elif src.is_file():
            members[arc] = ("file", src)
        else:
            raise StageError("%s is neither a file, a directory nor a link" % rel)
    for arc in list(members):
        parent = posixpath.dirname(arc)
        while parent:
            members.setdefault(parent, ("dir",))
            parent = posixpath.dirname(parent)
    return members


def write_tar(members, out):
    with tarfile.open(out, "w", format=tarfile.GNU_FORMAT) as tf:
        for arc in sorted(members, key=lambda s: s.encode("utf-8")):
            kind = members[arc]
            ti = tarfile.TarInfo(arc)
            ti.mtime, ti.uid, ti.gid, ti.uname, ti.gname = 0, 0, 0, "", ""
            if kind[0] == "dir":
                ti.type, ti.mode = tarfile.DIRTYPE, 0o755
                tf.addfile(ti)
            elif kind[0] == "link":
                ti.type, ti.mode, ti.linkname = tarfile.SYMTYPE, 0o777, kind[1]
                tf.addfile(ti)
            else:
                real = kind[1]
                ti.size = os.stat(real).st_size
                ti.mode = 0o755 if os.stat(real).st_mode & 0o111 else 0o644
                with open(real, "rb") as f:
                    tf.addfile(ti, f)


def build(cache, out, revision=REVISION, expected_model_sha256=MODEL_SHA256):
    """Build the deterministic tar; returns {sha256, bytes, entries, model_safetensors_sha256}."""
    members = collect(cache, revision)
    model_arc = "%s/snapshots/%s/model.safetensors" % (Path(cache).name, revision)
    if model_arc not in members:
        raise StageError("the snapshot has no root model.safetensors")
    kind = members[model_arc]
    if kind[0] == "link":
        kind = members[posixpath.normpath(posixpath.join(posixpath.dirname(model_arc), kind[1]))]
    model_sha = sha256_file(kind[1])
    if expected_model_sha256 is not None and model_sha != expected_model_sha256:
        raise StageError("model.safetensors sha256 %s != the pinned %s" % (model_sha, expected_model_sha256))
    write_tar(members, out)
    return {"sha256": sha256_file(out), "bytes": os.stat(out).st_size, "entries": len(members),
            "model_safetensors_sha256": model_sha}


# ------------------------------------------------------------------------------------------ extract

def _check_member(m):
    name = m.name
    if not name or "\0" in name or name.startswith("/") or (len(name) > 1 and name[1] == ":"):
        raise StageError("unsafe member name %r" % name)
    if any(part == ".." for part in name.split("/")):
        raise StageError("member %r has a '..' component" % name)
    if m.isdir() or m.isreg():
        return
    if m.issym():
        target = m.linkname
        if not target or "\0" in target or target.startswith("/"):
            raise StageError("symlink %r has an absolute or empty target" % name)
        resolved = posixpath.normpath(posixpath.join(posixpath.dirname(name.rstrip("/")), target))
        if resolved == ".." or resolved.startswith("../") or resolved.startswith("/"):
            raise StageError("symlink %r resolves outside the extraction root" % name)
        return
    raise StageError("member %r is a hardlink, device or other special entry" % name)


def safe_extract(tar_path, dest):
    """Extract `tar_path` into `dest` or raise StageError. Nothing is written when any member is unsafe."""
    dest = Path(dest)
    with tarfile.open(tar_path, "r:") as tf:
        members = tf.getmembers()
        for m in members:
            _check_member(m)
        dest.mkdir(parents=True, exist_ok=True)
        if hasattr(tarfile, "data_filter"):
            tf.extractall(dest, members=members, filter="data")
        else:
            tf.extractall(dest, members=members)
    root = os.path.realpath(dest)
    for p in sorted(dest.rglob("*")):
        real = os.path.realpath(p)
        if real != root and not real.startswith(root + os.sep):
            raise StageError("%s resolves outside %s after extraction" % (p, dest))
    return len(members)


# ------------------------------------------------------------------------------------------ check (uv project)

def check(tar_path):
    work = Path(tempfile.mkdtemp(prefix="laya-base-check-"))
    hf_home = work / "hf"
    safe_extract(tar_path, hf_home / "hub")
    for key in ("HF_HUB_CACHE", "HUGGINGFACE_HUB_CACHE", "TRANSFORMERS_CACHE"):
        os.environ.pop(key, None)
    os.environ.update({"HF_HOME": str(hf_home), "HF_HUB_OFFLINE": "1", "TRANSFORMERS_OFFLINE": "1"})
    sys.path.insert(0, str(ROOT / "scripts" / "laya_train"))
    import contract
    b = contract.base()
    if (b["repo"], b["revision"], b["model_safetensors_sha256"]) != (REPO_ID, REVISION, MODEL_SHA256):
        raise StageError("this file's pins differ from the contract's base block")
    from huggingface_hub import snapshot_download
    snap = Path(snapshot_download(REPO_ID, revision=REVISION, allow_patterns=list(BASE_FILES), local_files_only=True))
    got = sha256_file(snap / "model.safetensors")
    if got != MODEL_SHA256:
        raise StageError("the extracted model.safetensors hashes to %s" % got)
    from laya import Agent
    Agent(REPO_ID, device="cpu", expected_sha256={"model.safetensors": MODEL_SHA256}, revision=REVISION)
    print("OFFLINE LOAD OK model.safetensors sha256=%s" % got)
    return 0


# ------------------------------------------------------------------------------------------ selftest

def _synthetic_cache(root, with_escape=False):
    """A tiny HF-style cache: one blob stored in a hub-level store (like xet), a typed-decisions dir that must be
    excluded, an unreferenced blob, a tokenizer blob shared by two links, and trees/<rev>.json."""
    hub = Path(root) / "hub"
    cache = hub / CACHE_DIR_NAME
    rev = "r" * 40
    (hub / "blobs" / "zz").mkdir(parents=True)
    (hub / "blobs" / "zz" / "zzstore").write_bytes(b"MODEL-BYTES" * 100)
    for d in ("blobs", "trees", "snapshots/%s/encoder" % rev, "snapshots/%s/tokenizer" % rev,
              "snapshots/%s/typed-decisions/encoder" % rev):
        (cache / d).mkdir(parents=True)
    (cache / "blobs" / "mmm").symlink_to("../../blobs/zz/zzstore")           # the weights: into the hub store
    (cache / "blobs" / "aaa").write_bytes(b'{"cfg":1}')
    (cache / "blobs" / "ccc").write_bytes(b'{"enc":1}')
    (cache / "blobs" / "ddd").write_bytes(b"TYPED-DECISIONS-ONLY")
    (cache / "blobs" / "eee").write_bytes(b"UNREFERENCED")
    s = cache / "snapshots" / rev
    (s / "model.safetensors").symlink_to("../../blobs/mmm")
    (s / "rl_agent_config.json").symlink_to("../../blobs/aaa")
    (s / "encoder" / "config.json").symlink_to("../../../blobs/ccc")
    (s / "tokenizer" / "tokenizer.json").symlink_to("../../../blobs/aaa")
    (s / "typed-decisions" / "model.safetensors").symlink_to("../../../blobs/ddd")
    (s / "typed-decisions" / "encoder" / "config.json").symlink_to("../../../../blobs/ddd")
    (cache / "trees" / (rev + ".json")).write_text('{"files":{}}')
    if with_escape:
        outside = Path(root) / "outside.txt"
        outside.write_text("SECRET")
        (s / "escape.txt").symlink_to("../../../../outside.txt")
    return cache, rev


def _tar_with(path, entries):
    """entries: list of (name, kind, payload); kind file|dir|sym|hard."""
    import io
    with tarfile.open(path, "w", format=tarfile.GNU_FORMAT) as tf:
        for name, kind, payload in entries:
            ti = tarfile.TarInfo(name)
            if kind == "dir":
                ti.type, ti.mode = tarfile.DIRTYPE, 0o755
                tf.addfile(ti)
            elif kind in ("sym", "hard"):
                ti.type, ti.linkname = (tarfile.SYMTYPE if kind == "sym" else tarfile.LNKTYPE), payload
                tf.addfile(ti)
            else:
                ti.size, ti.mode = len(payload), 0o644
                tf.addfile(ti, io.BytesIO(payload))


def selftest():
    failures = []

    def case(name, ok, detail=""):
        print("  %-4s %-72s %s" % ("ok" if ok else "FAIL", name, detail))
        if not ok:
            failures.append(name)

    def refuses(fn):
        try:
            fn()
            return False
        except StageError:
            return True

    with tempfile.TemporaryDirectory(prefix="stage-base-selftest-") as tmp:
        tmp = Path(tmp)
        cache, rev = _synthetic_cache(tmp / "c1")
        print("build:")
        a, b = tmp / "a.tar", tmp / "b.tar"
        ra = build(cache, a, rev, expected_model_sha256=hashlib.sha256(b"MODEL-BYTES" * 100).hexdigest())
        for p in cache.rglob("*"):                                  # sources touched between the builds
            if p.is_file() and not p.is_symlink():
                os.utime(p, (123456789, 123456789))
        rb = build(cache, b, rev, expected_model_sha256=None)
        case("two builds are byte-identical (sources re-touched in between)", a.read_bytes() == b.read_bytes()
             and ra["sha256"] == rb["sha256"], ra["sha256"][:12])
        with tarfile.open(a) as tf:
            ms = tf.getmembers()
        names = [m.name.rstrip("/") for m in ms]
        case("typed-decisions/ and its blob, and the unreferenced blob, are excluded",
             not any("typed-decisions" in n for n in names) and not any(n.endswith("/ddd") or n.endswith("/eee") for n in names), str(len(names)))
        case("the root files, encoder, tokenizer, the used blobs and trees/<rev>.json are staged",
             {"%s/snapshots/%s/model.safetensors" % (CACHE_DIR_NAME, rev), "%s/blobs/mmm" % CACHE_DIR_NAME,
              "%s/blobs/aaa" % CACHE_DIR_NAME, "%s/blobs/ccc" % CACHE_DIR_NAME,
              "%s/trees/%s.json" % (CACHE_DIR_NAME, rev)} <= set(names))
        case("entries are sorted bytewise, mtime 0, uid/gid 0, empty uname/gname",
             names == sorted(names, key=lambda s: s.encode()) and all(m.mtime == 0 and m.uid == 0 and m.gid == 0
             and m.uname == "" and m.gname == "" for m in ms))
        case("every snapshot entry is a RELATIVE symlink to a regular blob; blobs are regular files (the store link is resolved)",
             all(m.issym() and not m.linkname.startswith("/") for m in ms if "/snapshots/" in m.name and not m.isdir())
             and all(m.isreg() for m in ms if "/blobs/" in m.name and not m.isdir()))
        case("modes are normalized (0644 files, 0755 dirs) and the tar is GNU format",
             all(m.mode == 0o644 for m in ms if m.isreg()) and all(m.mode == 0o755 for m in ms if m.isdir())
             and open(a, "rb").read(512)[257:265] == b"ustar  \0")
        case("a pinned model digest that does not match is refused", refuses(lambda: build(cache, tmp / "x.tar", rev, "0" * 64)))
        esc_cache, _ = _synthetic_cache(tmp / "c2", with_escape=True)
        case("a snapshot link resolving outside the cache and the hub store is refused",
             refuses(lambda: build(esc_cache, tmp / "y.tar", rev, None)))

        print("extract:")
        dest = tmp / "dest"
        n = safe_extract(a, dest)
        snap = dest / CACHE_DIR_NAME / "snapshots" / rev
        case("the tar extracts and the snapshot links read the staged bytes",
             n == len(ms) and (snap / "model.safetensors").read_bytes() == b"MODEL-BYTES" * 100
             and (snap / "tokenizer" / "tokenizer.json").read_bytes() == b'{"cfg":1}'
             and (snap / "model.safetensors").is_symlink())
        case("trees/<rev>.json and blobs/ exist beside the snapshot (what the offline loader needs)",
             (dest / CACHE_DIR_NAME / "trees" / (rev + ".json")).is_file() and (dest / CACHE_DIR_NAME / "blobs").is_dir())

        def bad(entries, label):
            t = tmp / ("bad-%s.tar" % label)
            _tar_with(t, entries)
            d = tmp / ("dest-%s" % label)
            r = refuses(lambda: safe_extract(t, d))
            return r and not (d.exists() and any(d.rglob("*"))) and not (tmp / "evil").exists()
        case("an absolute member name is refused, nothing written", bad([("/abs/evil", "file", b"x")], "abs"))
        case("a `..` member is refused, nothing written", bad([("ok/", "dir", None), ("../evil", "file", b"x")], "dots"))
        case("a nested `..` member is refused", bad([("a/../../evil", "file", b"x")], "nested"))
        case("a hardlink is refused", bad([("a", "file", b"x"), ("b", "hard", "a")], "hard"))
        case("an absolute symlink is refused", bad([("l", "sym", "/etc/passwd")], "abslink"))
        case("a symlink resolving outside the destination is refused", bad([("d/l", "sym", "../../evil")], "escape"))
        case("a symlink that stays inside the destination is accepted",
             not refuses(lambda: safe_extract(_mk(tmp / "ok.tar", [("d/", "dir", None), ("d/f", "file", b"1"), ("d/l", "sym", "f")]), tmp / "dest-ok")))

    if failures:
        print("STAGE BASE SELFTEST FAILED: %s" % ", ".join(failures))
        return 1
    print("STAGE BASE SELFTEST OK")
    return 0


def _mk(path, entries):
    _tar_with(path, entries)
    return path


# ------------------------------------------------------------------------------------------ cli

def main(argv):
    if argv[1:] == ["--selftest"]:
        return selftest()
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--cache", default=str(DEFAULT_CACHE))
    b.add_argument("--out", required=True)
    b.add_argument("--revision", default=REVISION)
    b.add_argument("--stage-json", default="")
    c = sub.add_parser("check")
    c.add_argument("--tar", required=True)
    a = ap.parse_args(argv[1:])
    try:
        if a.cmd == "build":
            r = build(a.cache, a.out, a.revision)
            print("STAGED %s sha256=%s bytes=%d" % (a.out, r["sha256"], r["bytes"]))
            if a.stage_json:
                pin = {"schema": SCHEMA, "repo": REPO_ID, "revision": a.revision, "tar_name": TAR_NAME,
                       "tar_sha256": r["sha256"], "tar_bytes": r["bytes"],
                       "model_safetensors_sha256": r["model_safetensors_sha256"], "excluded": list(EXCLUDED),
                       "entries": r["entries"]}
                Path(a.stage_json).write_text(json.dumps(pin, indent=2) + "\n", encoding="utf-8")
                print("PIN WRITTEN %s" % a.stage_json)
            return 0
        return check(a.tar)
    except StageError as e:
        print("STAGE BASE FAILED: %s" % e, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
