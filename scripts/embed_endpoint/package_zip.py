#!/usr/bin/env python3
"""Deterministic Lambda package ZIP and its package record (Phase 11, plan 11-01).

usage:
  package_zip.py build <asset_dir> <out.zip> --record <record.json>
  package_zip.py verify <zip> <record.json>
  package_zip.py selftest

`build` writes a byte-deterministic ZIP of the asset directory: entries sorted by
relative path, every timestamp 1980-01-01, `bootstrap` mode 0755 and every other
file 0644 in the external attributes, one fixed compression level, no directory
entries, and a refusal on any symlink. It then writes the package record
(schema embed-package-record-v1), computed by reading the ZIP back.

`verify` recomputes every record field from the ZIP and exits 1 naming the first
field that differs. `code_sha256_b64` is the base64 of the raw SHA-256 digest of the
ZIP file: the form in which Lambda reports CodeSha256, so the owner-run gate can
compare the live function against the bytes that were tested.

Standard library only. It imports no sibling module.
"""

import argparse
import base64
import hashlib
import json
import os
import shutil
import stat
import sys
import tempfile
import zipfile

SCHEMA = "embed-package-record-v1"
FIXED_TIME = (1980, 1, 1, 0, 0, 0)
COMPRESS_LEVEL = 6
BOOTSTRAP = "bootstrap"
APR_ENTRY = "model/full_model.apr"
RECORD_FIELDS = [
    "schema",
    "zip_file",
    "zip_sha256",
    "code_sha256_b64",
    "size_bytes",
    "bootstrap_sha256",
    "apr_sha256",
    "files",
]


class PackageError(Exception):
    """The asset cannot be packaged; the message names the cause."""


class VerifyError(Exception):
    """A record field does not match the ZIP; `field` names it."""

    def __init__(self, field):
        super().__init__(field)
        self.field = field


def sha256_hex(data):
    return hashlib.sha256(data).hexdigest()


def collect(asset_dir):
    """Sorted (relpath, abspath) for every regular file. Symlinks are refused."""
    found = []
    for root, dirs, files in os.walk(asset_dir):
        dirs.sort()
        for name in dirs:
            path = os.path.join(root, name)
            if os.path.islink(path):
                raise PackageError(f"refusing symlink directory: {os.path.relpath(path, asset_dir)}")
        for name in files:
            path = os.path.join(root, name)
            rel = os.path.relpath(path, asset_dir).replace(os.sep, "/")
            if os.path.islink(path):
                raise PackageError(f"refusing symlink: {rel}")
            if not os.path.isfile(path):
                raise PackageError(f"not a regular file: {rel}")
            found.append((rel, path))
    return sorted(found)


def write_zip(entries, out_path):
    with zipfile.ZipFile(out_path, "w") as archive:
        for rel, path in entries:
            with open(path, "rb") as handle:
                data = handle.read()
            info = zipfile.ZipInfo(rel, date_time=FIXED_TIME)
            info.create_system = 3  # Unix, so external_attr carries the mode
            mode = 0o755 if rel == BOOTSTRAP else 0o644
            info.external_attr = (stat.S_IFREG | mode) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data, compresslevel=COMPRESS_LEVEL)


def read_entries(zip_path):
    """(name, bytes, ZipInfo) for every entry, in archive order; CRC-checked by zipfile."""
    entries = []
    with zipfile.ZipFile(zip_path) as archive:
        for info in archive.infolist():
            if info.is_dir():
                raise PackageError(f"zip contains a directory entry: {info.filename}")
            entries.append((info.filename, archive.read(info.filename), info))
    return entries


def describe(zip_path):
    """The record fields, recomputed from the ZIP bytes alone."""
    with open(zip_path, "rb") as handle:
        raw = handle.read()
    digest = hashlib.sha256(raw).digest()
    entries = read_entries(zip_path)
    by_name = {name: data for name, data, _ in entries}  # archive order is kept
    if BOOTSTRAP not in by_name:
        raise PackageError(f"zip has no {BOOTSTRAP} entry")
    if APR_ENTRY not in by_name:
        raise PackageError(f"zip has no {APR_ENTRY} entry")
    return {
        "schema": SCHEMA,
        "zip_file": os.path.basename(zip_path),
        "zip_sha256": digest.hex(),
        "code_sha256_b64": base64.b64encode(digest).decode("ascii"),
        "size_bytes": len(raw),
        "bootstrap_sha256": sha256_hex(by_name[BOOTSTRAP]),
        "apr_sha256": sha256_hex(by_name[APR_ENTRY]),
        "files": [
            {"path": name, "size": len(data), "sha256": sha256_hex(data)}
            for name, data, _ in entries
        ],
    }


def build(asset_dir, out_zip, record_path):
    entries = collect(asset_dir)
    if BOOTSTRAP not in [rel for rel, _ in entries]:
        raise PackageError(f"no {BOOTSTRAP} file under {asset_dir}")
    write_zip(entries, out_zip)
    record = describe(out_zip)
    with open(record_path, "w", encoding="utf-8") as handle:
        handle.write(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record


def verify(zip_path, record_path):
    with open(record_path, encoding="utf-8") as handle:
        record = json.load(handle)
    with open(zip_path, "rb") as handle:
        raw = handle.read()
    # The ZIP hash is checked first, before any parse, so a flipped byte is named
    # as zip_sha256 rather than as whatever the parser trips over.
    if record.get("zip_sha256") != hashlib.sha256(raw).hexdigest():
        raise VerifyError("zip_sha256")
    try:
        expected = describe(zip_path)
    except PackageError as error:
        raise VerifyError("bootstrap_sha256") from error
    for field in RECORD_FIELDS:
        if record.get(field) != expected[field]:
            raise VerifyError(field)
    return expected


def selftest():
    cases = 0

    def expect(condition, name):
        nonlocal cases
        if not condition:
            print(f"PACKAGE ZIP SELFTEST FAILED: {name}", file=sys.stderr)
            sys.exit(1)
        cases += 1

    def sha_file(path):
        with open(path, "rb") as handle:
            return sha256_hex(handle.read())

    tmp = tempfile.mkdtemp(prefix="embed-package-selftest-")
    try:
        asset = os.path.join(tmp, "asset")
        os.makedirs(os.path.join(asset, "model", "1_Pooling"))
        with open(os.path.join(asset, BOOTSTRAP), "wb") as handle:
            handle.write(b"\x7fELF-selftest-bootstrap\n" * 64)
        with open(os.path.join(asset, APR_ENTRY), "wb") as handle:
            handle.write(bytes(range(256)) * 32)
        with open(os.path.join(asset, "model", "tokenizer.json"), "wb") as handle:
            handle.write(b'{"selftest": true}\n')
        with open(os.path.join(asset, "model", "1_Pooling", "config.json"), "wb") as handle:
            handle.write(b'{"pooling": "mean"}\n')

        z1, r1 = os.path.join(tmp, "a.zip"), os.path.join(tmp, "a.json")
        # The second build lands in another directory under the same basename: the
        # record names the zip by basename, so only the bytes may differ between runs.
        os.makedirs(os.path.join(tmp, "second"))
        z2, r2 = os.path.join(tmp, "second", "a.zip"), os.path.join(tmp, "second", "a.json")
        build(asset, z1, r1)
        build(asset, z2, r2)
        expect(sha_file(z1) == sha_file(z2), "two builds of one asset are byte-identical")
        expect(sha_file(r1) == sha_file(r2), "two records of one asset are byte-identical")

        names = [name for name, _, _ in read_entries(z1)]
        expect(names == sorted(names), "entries are sorted by relative path")
        infos = [info for _, _, info in read_entries(z1)]
        expect(all(info.date_time == FIXED_TIME for info in infos), "every timestamp is 1980-01-01")
        modes = {name: (info.external_attr >> 16) & 0o777 for name, _, info in read_entries(z1)}
        expect(modes[BOOTSTRAP] == 0o755, "bootstrap carries the executable bit after a round trip")
        expect(all(m == 0o644 for n, m in modes.items() if n != BOOTSTRAP), "other files are 0644")
        with zipfile.ZipFile(z1) as archive:
            expect(not any(i.is_dir() for i in archive.infolist()), "no directory entries")

        verify(z1, r1)  # raises on a mismatch, so a clean return is the pass

        with open(z1, "rb") as handle:
            data = bytearray(handle.read())
        data[len(data) // 2] ^= 0x01
        with open(z1, "wb") as handle:
            handle.write(bytes(data))
        try:
            verify(z1, r1)
            expect(False, "verify must fail on a flipped byte")
        except VerifyError as error:
            expect(error.field == "zip_sha256", f"a flipped byte is named zip_sha256 (got {error.field})")
        build(asset, z1, r1)  # restore the good ZIP for the record-edit case

        with open(r1, encoding="utf-8") as handle:
            bad = json.load(handle)
        bad["apr_sha256"] = "0" * 64
        bad_record = os.path.join(tmp, "bad.json")
        with open(bad_record, "w", encoding="utf-8") as handle:
            json.dump(bad, handle)
        try:
            verify(z1, bad_record)
            expect(False, "verify must fail on an edited record field")
        except VerifyError as error:
            expect(error.field == "apr_sha256", f"an edited record field is named (got {error.field})")

        symlinked = os.path.join(tmp, "asset-with-link")
        shutil.copytree(asset, symlinked, symlinks=True)
        os.symlink(os.path.join(asset, BOOTSTRAP), os.path.join(symlinked, "link-to-bootstrap"))
        try:
            build(symlinked, os.path.join(tmp, "c.zip"), os.path.join(tmp, "c.json"))
            expect(False, "build must refuse a symlink")
        except PackageError:
            expect(True, "build refuses a symlink")

        missing = os.path.join(tmp, "asset-no-bootstrap")
        shutil.copytree(asset, missing)
        os.remove(os.path.join(missing, BOOTSTRAP))
        try:
            build(missing, os.path.join(tmp, "d.zip"), os.path.join(tmp, "d.json"))
            expect(False, "build must refuse an asset with no bootstrap")
        except PackageError:
            expect(True, "build refuses an asset with no bootstrap")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print(f"PACKAGE ZIP SELFTEST OK {cases} cases")
    return 0


def main(argv):
    parser = argparse.ArgumentParser(description="deterministic Lambda package ZIP and record")
    sub = parser.add_subparsers(dest="command", required=True)
    b = sub.add_parser("build")
    b.add_argument("asset_dir")
    b.add_argument("out_zip")
    b.add_argument("--record", required=True)
    v = sub.add_parser("verify")
    v.add_argument("zip")
    v.add_argument("record")
    sub.add_parser("selftest")
    args = parser.parse_args(argv[1:])
    try:
        if args.command == "build":
            record = build(args.asset_dir, args.out_zip, args.record)
            print(
                f"PACKAGE BUILT {record['zip_file']} {record['size_bytes']} bytes "
                f"code_sha256_b64={record['code_sha256_b64']}"
            )
            return 0
        if args.command == "verify":
            verify(args.zip, args.record)
            print("PACKAGE VERIFY OK")
            return 0
        return selftest()
    except VerifyError as error:
        print(f"PACKAGE VERIFY FAILED: field {error.field} differs from the ZIP", file=sys.stderr)
        return 1
    except PackageError as error:
        print(f"PACKAGE ERROR: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
