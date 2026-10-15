#!/usr/bin/env python3
"""ECS Fargate host (the ECS/EKS proxy): register the task definition, RunTask from zero, record the
ECS lifecycle timestamps -> port open -> first token, run the remote bench over HTTP (streamed), then stop.

    uv run --with boto3 python3 tools/fargate_run.py 4B --vcpu 16 --mem-gb 32 [--threads 16] [--keep]

Prices (us-east-1 arm64 list, 2026-09): $0.03238 per vCPU-hour + $0.00356 per GB-hour.
"""
import argparse
import json
import socket
import subprocess
import sys
import time

import boto3

IDS = json.load(open("results/aws-ids.json"))
S = boto3.Session(profile_name=IDS["profile"], region_name=IDS["region"])
ecs, ec2, logs = S.client("ecs"), S.client("ec2"), S.client("logs")
PRICE_VCPU_H, PRICE_GB_H = 0.03238, 0.00356


def rel(ts, t0):
    return round(ts.timestamp() - t0, 2) if ts else None


def register(model, vcpu, mem_gb, threads, tag):
    fam = f"{IDS['name']}-fargate-{model.lower()}"
    td = ecs.register_task_definition(
        family=fam, networkMode="awsvpc", requiresCompatibilities=["FARGATE"], cpu=str(vcpu * 1024), memory=str(mem_gb * 1024),
        runtimePlatform={"cpuArchitecture": "ARM64", "operatingSystemFamily": "LINUX"},
        executionRoleArn=IDS["roles"]["ecs_exec"], taskRoleArn=IDS["roles"]["ecs_task"],
        ephemeralStorage={"sizeInGiB": 30},
        containerDefinitions=[{
            "name": "apr", "image": f"{IDS['ecr']}:{tag}", "essential": True,
            "portMappings": [{"containerPort": 8080, "protocol": "tcp"}],
            "environment": [{"name": "PLATFORM", "value": "fargate"}, {"name": "RAYON_NUM_THREADS", "value": str(threads)},
                            {"name": "MODEL_S3_URI", "value": f"s3://{IDS['bucket']}/Qwen3.5-{model}-Q4_K_M.gguf"},
                            {"name": "MODEL_LOCAL", "value": "/tmp/model.gguf"}],
            "logConfiguration": {"logDriver": "awslogs", "options": {"awslogs-group": f"/ecs/{IDS['name']}", "awslogs-region": IDS["region"],
                                                                    "awslogs-stream-prefix": f"fargate-{model.lower()}"}}}],
        tags=[{"key": "spike", "value": "031"}])
    return td["taskDefinition"]["taskDefinitionArn"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("model")
    ap.add_argument("--vcpu", type=int, default=16)
    ap.add_argument("--mem-gb", type=int, default=32)
    ap.add_argument("--threads", type=int, default=None)
    ap.add_argument("--tag", default="cpu-v0.70.3")
    ap.add_argument("--keep", action="store_true", help="leave the task running (remember to stop it)")
    ap.add_argument("--conc", type=int, default=2)
    a = ap.parse_args()
    threads = a.threads or a.vcpu
    label = f"fargate-{a.model.lower()}-{a.vcpu}vcpu"
    tdarn = register(a.model, a.vcpu, a.mem_gb, threads, a.tag)
    print(f"[fargate] taskdef {tdarn}", file=sys.stderr, flush=True)
    t0 = time.time()
    run = ecs.run_task(cluster=IDS["name"], taskDefinition=tdarn, launchType="FARGATE", count=1,
                       networkConfiguration={"awsvpcConfiguration": {"subnets": IDS["subnets"].split(","), "securityGroups": [IDS["sg"]], "assignPublicIp": "ENABLED"}},
                       tags=[{"key": "spike", "value": "031"}])
    if run.get("failures"):
        print(json.dumps(run["failures"]), file=sys.stderr)
        sys.exit(1)
    arn = run["tasks"][0]["taskArn"]
    ip = None
    rec = {"label": label, "taskdef": tdarn, "task": arn.split("/")[-1], "vcpu": a.vcpu, "mem_gb": a.mem_gb, "threads": threads}
    try:
        while True:
            task = ecs.describe_tasks(cluster=IDS["name"], tasks=[arn])["tasks"][0]
            if task["lastStatus"] == "STOPPED":
                rec["stopped"] = task.get("stoppedReason")
                rec["containers"] = [c.get("reason") for c in task["containers"]]
                print(json.dumps(rec, indent=1))
                sys.exit(2)
            if not ip:
                eni = next((d["value"] for att in task.get("attachments", []) for d in att["details"] if d["name"] == "networkInterfaceId"), None)
                if eni:
                    assoc = ec2.describe_network_interfaces(NetworkInterfaceIds=[eni])["NetworkInterfaces"][0].get("Association")
                    ip = assoc and assoc.get("PublicIp")
            if ip and task["lastStatus"] == "RUNNING":
                try:
                    socket.create_connection((ip, 8080), timeout=1).close()
                    break
                except OSError:
                    pass
            time.sleep(1)
        port_open = round(time.time() - t0, 2)
        task = ecs.describe_tasks(cluster=IDS["name"], tasks=[arn])["tasks"][0]
        rec["az"] = task.get("availabilityZone")
        rec["timeline_s"] = {"run_task": 0.0, "pull_started": rel(task.get("pullStartedAt"), t0), "pull_stopped": rel(task.get("pullStoppedAt"), t0),
                             "container_started": rel(task.get("startedAt"), t0), "port_open_model_ready": port_open}
        print(f"[fargate] ip={ip} timeline={rec['timeline_s']}", file=sys.stderr, flush=True)
        out = f"results/bench-{label}.json"
        cmd = [sys.executable, "tools/remote_bench.py", "http", f"http://{ip}:8080/v1", "--model", a.model, "--label", label, "--out", out,
               "--conc", str(a.conc), "--long"]
        subprocess.run(cmd, check=False)
        rec["bench"] = out
        # the server's own timeline from CloudWatch
        try:
            g = f"/ecs/{IDS['name']}"
            st = logs.describe_log_streams(logGroupName=g, logStreamNamePrefix=f"fargate-{a.model.lower()}/apr/{arn.split('/')[-1]}")["logStreams"]
            if st:
                ev = logs.get_log_events(logGroupName=g, logStreamName=st[0]["logStreamName"], startFromHead=True, limit=200)["events"]
                rec["server_log"] = [e["message"][:200] for e in ev if e["message"].startswith("[entry]") or "Model ready" in e["message"] or "rror" in e["message"]]
        except Exception as e:  # noqa: BLE001
            rec["server_log_error"] = str(e)[:200]
        task = ecs.describe_tasks(cluster=IDS["name"], tasks=[arn])["tasks"][0]
        hours = (time.time() - t0) / 3600
        rec["cost_usd_for_session"] = round(hours * (a.vcpu * PRICE_VCPU_H + a.mem_gb * PRICE_GB_H), 4)
        rec["price_per_hour_usd"] = round(a.vcpu * PRICE_VCPU_H + a.mem_gb * PRICE_GB_H, 4)
        rec["ip"] = ip
        json.dump(rec, open(f"results/fargate-{label}.json", "w"), indent=1)
        print(json.dumps({k: v for k, v in rec.items() if k != "server_log"}, indent=1))
    finally:
        if not a.keep:
            ecs.stop_task(cluster=IDS["name"], task=arn, reason="spike 031 measurement done")
            print("[fargate] task stopped", file=sys.stderr)


if __name__ == "__main__":
    main()
