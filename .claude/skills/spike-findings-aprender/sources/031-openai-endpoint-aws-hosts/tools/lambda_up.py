#!/usr/bin/env python3
"""Default-Lambda host for the spike image.

    uv run --with boto3 python3 tools/lambda_up.py create 0.8B [--mem 10240] [--threads 6]
    uv run --with boto3 python3 tools/lambda_up.py logs 0.8B      # [entry] timeline + REPORT lines of the latest stream
    uv run --with boto3 python3 tools/lambda_up.py delete 0.8B

Function name: llm-spike-031-lambda-<model lowercased>. Container image from ECR, arm64, 10 GB ephemeral
storage (/tmp holds the GGUF), 900 s timeout, RAYON_NUM_THREADS = 6 (what 10,240 MB buys).
"""
import json
import sys
import time

import boto3

IDS = json.load(open("results/aws-ids.json"))
S = boto3.Session(profile_name=IDS["profile"], region_name=IDS["region"])
lam = S.client("lambda")
logs = S.client("logs")


def fn_name(model):
    # Lambda function names allow [a-zA-Z0-9-_] only: "0.8B" -> "0-8b"
    return f"{IDS['name']}-lambda-{model.lower().replace('.', '-')}"


def create(model, mem=10240, threads=6, tag="cpu-v0.70.3"):
    fn = fn_name(model)
    env = {"MODEL_S3_URI": f"s3://{IDS['bucket']}/Qwen3.5-{model}-Q4_K_M.gguf", "RAYON_NUM_THREADS": str(threads), "PLATFORM": "lambda"}
    t0 = time.time()
    try:
        lam.create_function(FunctionName=fn, PackageType="Image", Code={"ImageUri": f"{IDS['ecr']}:{tag}"},
                            Role=IDS["roles"]["lambda"], Architectures=["arm64"], MemorySize=mem, Timeout=900,
                            EphemeralStorage={"Size": 10240}, Environment={"Variables": env}, Tags={"spike": "031"})
    except lam.exceptions.ResourceConflictException:
        lam.update_function_configuration(FunctionName=fn, MemorySize=mem, Timeout=900, Environment={"Variables": env})
        lam.get_waiter("function_updated_v2").wait(FunctionName=fn)
        lam.update_function_code(FunctionName=fn, ImageUri=f"{IDS['ecr']}:{tag}")
    lam.get_waiter("function_active_v2").wait(FunctionName=fn)
    lam.get_waiter("function_updated_v2").wait(FunctionName=fn)
    c = lam.get_function_configuration(FunctionName=fn)
    print(json.dumps({"function": fn, "state": c["State"], "mem": c["MemorySize"], "arch": c["Architectures"], "s": round(time.time() - t0, 1)}))


def show_logs(model, n_streams=2):
    g = f"/aws/lambda/{fn_name(model)}"
    streams = logs.describe_log_streams(logGroupName=g, orderBy="LastEventTime", descending=True, limit=n_streams)["logStreams"]
    for st in reversed(streams):
        print(f"--- {st['logStreamName']}")
        ev = logs.get_log_events(logGroupName=g, logStreamName=st["logStreamName"], startFromHead=True, limit=400)["events"]
        for e in ev:
            m = e["message"].rstrip()
            if m.startswith("[entry]") or m.startswith("REPORT") or "Model ready" in m or "rror" in m or "arning" in m or "listening" in m.lower():
                print(time.strftime("%H:%M:%S", time.gmtime(e["timestamp"] / 1000)), m[:220])


def delete(model):
    try:
        lam.delete_function(FunctionName=fn_name(model))
        print("deleted", fn_name(model))
    except lam.exceptions.ResourceNotFoundException:
        print("absent", fn_name(model))


if __name__ == "__main__":
    cmd, model = sys.argv[1], sys.argv[2]
    kw = {}
    for a in sys.argv[3:]:
        k, v = a.lstrip("-").split("=") if "=" in a else (a.lstrip("-"), None)
        if k == "mem":
            kw["mem"] = int(v)
        if k == "threads":
            kw["threads"] = int(v)
    {"create": lambda: create(model, **kw), "logs": lambda: show_logs(model), "delete": lambda: delete(model)}[cmd]()
