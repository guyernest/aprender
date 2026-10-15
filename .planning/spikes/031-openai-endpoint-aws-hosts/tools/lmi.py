#!/usr/bin/env python3
"""Lambda Managed Instances host (adapted from spike 023's driver).

    uv run --with boto3 python3 tools/lmi.py up 4B [--mem 32768] [--gib-per-vcpu 2.0]   # provider + image function + publish
    uv run --with boto3 python3 tools/lmi.py scale 1        # min=max=1 on the published version
    uv run --with boto3 python3 tools/lmi.py census         # the EC2 hosts the provider runs (hidden by default)
    uv run --with boto3 python3 tools/lmi.py version        # latest published version (for remote_bench --qualifier)
    uv run --with boto3 python3 tools/lmi.py down           # scale 0, delete function, delete provider

023 lessons applied: create the provider with its FINAL MaxVCpuCount (a failed first publish wedged a
provider for good); $LATEST is not invocable, only published versions; subnets in >= 2 AZs, never us-east-1c;
set EphemeralStorage for the S3-staged weights. Package type here is a container image — whether LMI
accepts it is itself a finding (023 used a zip).
"""
import json
import sys
import time

import boto3

IDS = json.load(open("results/aws-ids.json"))
S = boto3.Session(profile_name=IDS["profile"], region_name=IDS["region"])
lam, ec2 = S.client("lambda"), S.client("ec2")
CP = f"{IDS['name']}-cp"
FN = f"{IDS['name']}-lmi-4b"
SUBNETS = [s for s in IDS["subnets"].split(",") if s][:3]


def log(rec):
    rec["ts"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    print(json.dumps(rec, default=str), flush=True)
    with open("results/lmi.jsonl", "a") as f:
        f.write(json.dumps(rec, default=str) + "\n")


def wait(pred, what, t0, every=5, limit=1800):
    while time.time() - t0 < limit:
        ok, info = pred()
        if ok:
            return info
        time.sleep(every)
    raise TimeoutError(what)


def census():
    res = ec2.describe_instances(IncludeManagedResources=True,
                                 Filters=[{"Name": "vpc-id", "Values": [IDS["vpc"]]}, {"Name": "operator.managed", "Values": ["true"]},
                                          {"Name": "instance-state-name", "Values": ["pending", "running", "stopping", "shutting-down"]}])
    out = []
    for r in res["Reservations"]:
        for i in r["Instances"]:
            tags = {t["Key"]: t["Value"] for t in i.get("Tags", [])}
            out.append({"id": i["InstanceId"], "type": i["InstanceType"], "az": i["Placement"]["AvailabilityZone"], "state": i["State"]["Name"],
                        "launched": i["LaunchTime"].isoformat(), "cp": tags.get("aws:lambda:capacity-provider")})
    return out


def up(model="4B", mem=32768, gib_per_vcpu=2.0, tag="cpu-v0.70.3", max_vcpu=400):
    global FN
    FN = f"{IDS['name']}-lmi-{model.lower()}"
    t0 = time.time()
    try:
        lam.create_capacity_provider(
            CapacityProviderName=CP,
            VpcConfig={"SubnetIds": SUBNETS, "SecurityGroupIds": [IDS["sg"]]},
            PermissionsConfig={"CapacityProviderOperatorRoleArn": IDS["roles"]["lmi_operator"]},
            InstanceRequirements={"Architectures": ["arm64"]},
            CapacityProviderScalingConfig={"ScalingMode": "Auto", "MaxVCpuCount": max_vcpu},
            Tags={"spike": "031"})
    except lam.exceptions.ResourceConflictException:
        pass
    wait(lambda: (lam.get_capacity_provider(CapacityProviderName=CP)["CapacityProvider"]["State"] == "Active", None), "cp active", t0)
    cp = lam.get_capacity_provider(CapacityProviderName=CP)["CapacityProvider"]
    log({"step": "capacity provider Active", "s": round(time.time() - t0, 1), "cp": cp.get("CapacityProviderArn"), "instances": census()})
    threads = int(mem / 1024 / gib_per_vcpu)
    t1 = time.time()
    env = {"MODEL_S3_URI": f"s3://{IDS['bucket']}/Qwen3.5-{model}-Q4_K_M.gguf", "RAYON_NUM_THREADS": str(threads), "PLATFORM": "lmi"}
    try:
        lam.create_function(FunctionName=FN, PackageType="Image", Code={"ImageUri": f"{IDS['ecr']}:{tag}"}, Role=IDS["roles"]["lambda"],
                            Architectures=["arm64"], MemorySize=mem, Timeout=900, EphemeralStorage={"Size": 10240},
                            Environment={"Variables": env},
                            CapacityProviderConfig={"LambdaManagedInstancesCapacityProviderConfig": {
                                "CapacityProviderArn": cp["CapacityProviderArn"], "PerExecutionEnvironmentMaxConcurrency": 1,
                                "ExecutionEnvironmentMemoryGiBPerVCpu": gib_per_vcpu}},
                            Tags={"spike": "031"})
    except lam.exceptions.ResourceConflictException:
        pass
    except Exception as e:  # noqa: BLE001 — the finding if images are refused on LMI
        log({"step": "create_function FAILED", "error": str(e)[:600]})
        raise
    lam.get_waiter("function_active_v2").wait(FunctionName=FN)
    log({"step": "function created", "s": round(time.time() - t1, 1), "threads": threads})
    t2 = time.time()
    v = lam.publish_version(FunctionName=FN)["Version"]

    def active():
        c = lam.get_function_configuration(FunctionName=FN, Qualifier=v)
        return c["State"] in ("Active", "Failed"), c
    c = wait(active, "version active", t2, every=10)
    log({"step": f"version {v} {c['State']}", "s": round(time.time() - t2, 1), "state_reason": c.get("StateReason"),
         "scaling": lam.get_function_scaling_config(FunctionName=FN, Qualifier=v).get("FunctionScalingConfig") if c["State"] == "Active" else None,
         "instances": census()})


def publish(model="4B"):
    """Publish a version of an already-created function (the `up` flow after a waiter timeout)."""
    global FN
    FN = f"{IDS['name']}-lmi-{model.lower()}"
    t0 = time.time()
    c0 = lam.get_function_configuration(FunctionName=FN)
    log({"step": "pre-publish state", "state": c0["State"], "reason": c0.get("StateReason"), "last_update": c0.get("LastUpdateStatus")})
    v = lam.publish_version(FunctionName=FN)["Version"]

    def active():
        c = lam.get_function_configuration(FunctionName=FN, Qualifier=v)
        return c["State"] in ("Active", "Failed"), c
    c = wait(active, "version active", t0, every=10)
    log({"step": f"version {v} {c['State']}", "s": round(time.time() - t0, 1), "state_reason": c.get("StateReason"),
         "scaling": lam.get_function_scaling_config(FunctionName=FN, Qualifier=v).get("FunctionScalingConfig") if c["State"] == "Active" else None,
         "instances": census()})


def latest_version():
    return max(int(x["Version"]) for x in lam.list_versions_by_function(FunctionName=FN)["Versions"] if x["Version"] != "$LATEST")


def scale(n):
    v = latest_version()
    t0 = time.time()
    lam.put_function_scaling_config(FunctionName=FN, Qualifier=str(v),
                                    FunctionScalingConfig={"MinExecutionEnvironments": n, "MaxExecutionEnvironments": max(n, 1) if n else 0})
    time.sleep(5)
    log({"step": f"scale min=max={n} on v{v}", "s": round(time.time() - t0, 1),
         "applied": lam.get_function_scaling_config(FunctionName=FN, Qualifier=str(v)).get("FunctionScalingConfig"), "instances": census()})


def down():
    try:
        for x in lam.list_versions_by_function(FunctionName=FN)["Versions"]:
            if x["Version"] != "$LATEST":
                try:
                    lam.put_function_scaling_config(FunctionName=FN, Qualifier=x["Version"],
                                                    FunctionScalingConfig={"MinExecutionEnvironments": 0, "MaxExecutionEnvironments": 0})
                except Exception:  # noqa: BLE001
                    pass
        lam.delete_function(FunctionName=FN)
    except lam.exceptions.ResourceNotFoundException:
        pass
    t0 = time.time()
    while True:
        try:
            lam.delete_capacity_provider(CapacityProviderName=CP)
            break
        except lam.exceptions.ResourceNotFoundException:
            break
        except Exception as e:  # noqa: BLE001 — versions still detaching
            if time.time() - t0 > 900:
                raise
            time.sleep(15)
    log({"step": "down requested", "s": round(time.time() - t0, 1), "instances": census()})


if __name__ == "__main__":
    cmd = sys.argv[1]
    args = sys.argv[2:]
    if cmd == "up":
        model = args[0] if args and not args[0].startswith("--") else "4B"
        kw = {}
        for a in args:
            if a.startswith("--mem="):
                kw["mem"] = int(a.split("=")[1])
            if a.startswith("--gib-per-vcpu="):
                kw["gib_per_vcpu"] = float(a.split("=")[1])
        up(model, **kw)
    elif cmd == "publish":
        publish(args[0] if args else "4B")
    elif cmd == "scale":
        scale(int(args[0]))
    elif cmd == "census":
        log({"step": "census", "instances": census()})
    elif cmd == "version":
        print(latest_version())
    elif cmd == "down":
        down()
