#!/usr/bin/env python3
"""Upload the ladder-pinned GGUFs to the spike bucket once, gently (the laptop uplink is ~3.7 MB/s and
the default s3 cp dropped connections in 021): 16 MB parts, 2 concurrent.

    uv run --with boto3 python3 tools/upload_weights.py --profile ze-kasher-dev --bucket llm-spike-031-<acct> models/*.gguf
"""
import argparse
import hashlib
import os
import sys
import time

import boto3
from boto3.s3.transfer import TransferConfig


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--profile", required=True)
    ap.add_argument("--bucket", required=True)
    ap.add_argument("--region", default="us-east-1")
    ap.add_argument("files", nargs="+")
    a = ap.parse_args()
    s3 = boto3.Session(profile_name=a.profile, region_name=a.region).client("s3")
    cfg = TransferConfig(multipart_chunksize=16 * 1024 * 1024, max_concurrency=2, multipart_threshold=16 * 1024 * 1024)
    for path in a.files:
        key = os.path.basename(path)
        real = os.path.realpath(path)
        size = os.path.getsize(real)
        try:
            head = s3.head_object(Bucket=a.bucket, Key=key)
            if head["ContentLength"] == size:
                print(f"skip {key}: already in s3 ({size} bytes)", flush=True)
                continue
        except s3.exceptions.ClientError:
            pass
        h = hashlib.sha256()
        with open(real, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 24), b""):
                h.update(chunk)
        sha = h.hexdigest()
        t0 = time.time()
        done = [0]

        def cb(n, _done=done, _t0=t0, _size=size):
            _done[0] += n
            if _done[0] % (256 * 1024 * 1024) < n:
                el = time.time() - _t0
                print(f"  {key}: {_done[0] / 1e6:.0f}/{_size / 1e6:.0f} MB {(_done[0] / 1e6) / max(el, 1e-9):.1f} MB/s", file=sys.stderr, flush=True)

        s3.upload_file(real, a.bucket, key, Config=cfg, Callback=cb, ExtraArgs={"Metadata": {"sha256": sha}, "Tagging": "spike=031"})
        el = time.time() - t0
        print(f"uploaded {key}: {size} bytes sha256={sha[:16]}… in {el:.0f}s ({size / 1e6 / el:.1f} MB/s)", flush=True)
    print("UPLOAD DONE", flush=True)


if __name__ == "__main__":
    main()
