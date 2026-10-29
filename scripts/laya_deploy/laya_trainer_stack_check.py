#!/usr/bin/env python3
"""Assertions over the synthesized Laya training stacks (Phase 14, plan 14-08).

usage: laya_trainer_stack_check.py [TEMPLATE_TRAINER TEMPLATE_IMAGES]
       laya_trainer_stack_check.py --selftest [TEMPLATE_TRAINER TEMPLATE_IMAGES]

Reads the two CloudFormation templates `cdk synth` wrote and refuses anything the phase decided
against: every spend limit (instance, count, runtime, machine timeout, arm64 task size), every
image pin, the two exact SageMaker channels, the per-principal S3 rights, the D-19 lifecycle, the
abort and reconcile paths, the request policy that is an OUTPUT only, and the ECR rules. It makes
no AWS call and imports no network module.

Exit 0 and `TRAINER STACK CHECK OK ...` when every assertion holds; exit 1 with one
`REFUSED <assertion>: <detail>` line per failure otherwise.

`--selftest` proves each assertion can fail: it mutates a copy of the real templates once per case
and requires that exact assertion to refuse. It ends `STACK CHECK SELFTEST OK` only if every case
was refused for the intended reason and the unmutated templates pass.

Standard library only; run with `python3 -I`.
"""

import copy
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
OUT = os.path.join(ROOT, "deploy-extensions", "cdk.out")
DEFAULT_TRAINER = os.path.join(OUT, "laya-trainer", "aprender-laya-trainer-dev.template.json")
DEFAULT_IMAGES = os.path.join(OUT, "laya-trainer-images", "aprender-laya-trainer-images-dev.template.json")

DIGEST_RE = r"sha256:[0-9a-f]{64}"
BASE_PREFIX = "base/laya-55cf4c4e/"
TRAIN_ENV_KEYS = {
    "LAYA_STEP", "LAYA_TASK_ID", "LAYA_NAME", "LAYA_BUCKET", "LAYA_RUN_PREFIX",
    "LAYA_EPOCHS", "LAYA_EXPECTED_INPUTS", "LAYA_TRAIN_IMAGE_DIGEST",
}
PACK_ENV_KEYS = {"LAYA_TASK_ID", "LAYA_NAME", "LAYA_BUCKET", "LAYA_RUN_PREFIX", "LAYA_PACK_IMAGE_DIGEST"}
REQUIRED_OUTPUTS = {
    "BucketName", "TableName", "StateMachineArn", "FinalizerName", "SageMakerRoleArn", "TrainRepoUri",
    "PackTaskDefinitionArn", "ClusterArn", "SubnetIds", "SecurityGroupId", "FinalizerDlqUrl",
    "RequestLambdaPolicy",
}
FINALIZER_ENV_KEYS = {
    "APRENDER_LAYA_TRAIN_TABLE", "APRENDER_LAYA_TRAIN_BUCKET", "APRENDER_LAYA_TRAIN_STATE_MACHINE_ARN",
    "APRENDER_LAYA_TRAIN_CLUSTER_ARN", "APRENDER_LAYA_TRAIN_PACK_FAMILY",
}


# ----------------------------------------------------------------------------- template helpers

def flat(x):
    """Resolve CloudFormation intrinsics to one string; a reference becomes `<Ref:id>` / `<GetAtt:id.attr>`."""
    if isinstance(x, str):
        return x
    if isinstance(x, (int, float, bool)) or x is None:
        return json.dumps(x)
    if isinstance(x, dict) and len(x) == 1:
        (k, v), = x.items()
        if k == "Ref":
            return "<Ref:%s>" % v
        if k == "Fn::GetAtt":
            return "<GetAtt:%s>" % ".".join(v if isinstance(v, list) else str(v).split(".", 1))
        if k == "Fn::Join":
            return v[0].join(flat(p) for p in v[1])
        if k == "Fn::Select":
            return "<Select:%s>" % v[0]
        if k == "Fn::Sub":
            return flat(v if isinstance(v, str) else v[0])
    return json.dumps(x, sort_keys=True)


def resources(t, type_):
    return {k: v for k, v in t.get("Resources", {}).items() if v.get("Type") == type_}


def one(t, type_):
    found = resources(t, type_)
    return next(iter(found.items())) if len(found) == 1 else (None, None)


def asl_of(t):
    """The state machine definition as a dict, intrinsics flattened to placeholder strings."""
    _, m = one(t, "AWS::StepFunctions::StateMachine")
    if m is None:
        return None, None
    return m, json.loads(flat(m["Properties"]["DefinitionString"]))


def set_asl(t, asl):
    _, m = one(t, "AWS::StepFunctions::StateMachine")
    m["Properties"]["DefinitionString"] = json.dumps(asl)


def as_list(x):
    return x if isinstance(x, list) else [x]


def statements(doc):
    return as_list(doc.get("Statement", []))


def role_policy_statements(t, role_id):
    """Every statement attached to one role: inline AWS::IAM::Policy resources and the role's own Policies."""
    out = []
    for p in list(resources(t, "AWS::IAM::Policy").values()) + list(resources(t, "AWS::IAM::ManagedPolicy").values()):
        roles = [r.get("Ref") if isinstance(r, dict) else r for r in as_list(p["Properties"].get("Roles", []))]
        if role_id in roles:
            out.extend(statements(p["Properties"]["PolicyDocument"]))
    role = t["Resources"].get(role_id)
    if role:
        for inline in role["Properties"].get("Policies", []):
            out.extend(statements(inline["PolicyDocument"]))
    return out


def all_allow_statements(t):
    """(owner, statement) for every Allow statement in every IAM resource, bucket and queue policy."""
    out = []
    for k, v in t.get("Resources", {}).items():
        ty = v.get("Type")
        p = v.get("Properties", {})
        if ty in ("AWS::IAM::Policy", "AWS::IAM::ManagedPolicy", "AWS::S3::BucketPolicy", "AWS::SQS::QueuePolicy"):
            for st in statements(p["PolicyDocument"]):
                out.append((k, st))
        elif ty == "AWS::IAM::Role":
            for inline in p.get("Policies", []):
                for st in statements(inline["PolicyDocument"]):
                    out.append((k, st))
    return [(k, s) for k, s in out if s.get("Effect") == "Allow"]


def bucket_id(t):
    k, _ = one(t, "AWS::S3::Bucket")
    return k


def norm_resource(r, bucket):
    """A resource spelled relative to the run bucket: `<bucket>/runs/*` -> `runs/*`, the bucket itself -> ``."""
    s = flat(r)
    b = "<GetAtt:%s.Arn>" % bucket
    if s == b:
        return ""
    if s.startswith(b + "/"):
        return s[len(b) + 1:]
    return "other:" + s


def s3_grants(statements_, bucket):
    """{action: {tails}}; ListBucket tails are `[prefix,...]` of the s3:prefix condition (or `<unconditioned>`)."""
    grants = {}
    for st in statements_:
        if st.get("Effect") != "Allow":
            continue
        acts = [a for a in as_list(st.get("Action", [])) if isinstance(a, str) and a.startswith("s3:")]
        for a in acts:
            for r in as_list(st.get("Resource", [])):
                tail = norm_resource(r, bucket)
                if a == "s3:ListBucket":
                    cond = st.get("Condition", {})
                    pre = cond.get("StringLikeIfExists", {}).get("s3:prefix")
                    # Plain StringLike fails on GetObject/HeadObject (no s3:prefix key), turning a missing
                    # key's 404 into a 403; any operator but StringLikeIfExists is its own, rejected tail.
                    if pre and not cond.get("StringLike"):
                        tail = "[%s]" % ",".join(sorted(as_list(pre)))
                    elif cond.get("StringLike", {}).get("s3:prefix"):
                        tail = "<StringLike:%s>" % ",".join(sorted(as_list(cond["StringLike"]["s3:prefix"])))
                    else:
                        tail = "<unconditioned>"
                grants.setdefault(a, set()).add(tail)
    return grants


def role_ids(t):
    """Logical ids of the principals this stack defines, found by what they are bound to."""
    ids = {}
    for k, v in resources(t, "AWS::IAM::Role").items():
        svc = flat(v["Properties"]["AssumeRolePolicyDocument"])
        if "sagemaker.amazonaws.com" in svc:
            ids["sagemaker"] = k
    _, td = one(t, "AWS::ECS::TaskDefinition")
    if td:
        ids["fargate"] = td["Properties"]["TaskRoleArn"]["Fn::GetAtt"][0]
        ids["fargate_exec"] = td["Properties"]["ExecutionRoleArn"]["Fn::GetAtt"][0]
    _, fn = one(t, "AWS::Lambda::Function")
    if fn:
        ids["finalizer"] = fn["Properties"]["Role"]["Fn::GetAtt"][0]
    _, m = one(t, "AWS::StepFunctions::StateMachine")
    if m:
        ids["machine"] = m["Properties"]["RoleArn"]["Fn::GetAtt"][0]
    return ids


def without_prefix(d):
    """Names of a Parameters dict without the `.$` marker."""
    return {k[:-2] if k.endswith(".$") else k for k in d}


# ----------------------------------------------------------------------------- assertions

class Report:
    def __init__(self):
        self.refused = []
        self.count = 0

    def check(self, name, ok, detail=""):
        self.count += 1
        if not ok:
            self.refused.append((name, detail))


def check_wildcards(t, rep, label):
    for owner, st in all_allow_statements(t):
        for a in [x for x in as_list(st.get("Action", [])) if isinstance(x, str)]:
            rep.check("iam-action-star", a != "*", "%s %s: Action \"*\"" % (label, owner))
            rep.check("iam-service-star", a not in ("s3:*", "iam:*", "sagemaker:*"), "%s %s: %s" % (label, owner, a))
        if "iam:PassRole" in as_list(st.get("Action", [])):
            res = as_list(st.get("Resource", []))
            ok = bool(res) and all(isinstance(r, dict) and "Fn::GetAtt" in r and r["Fn::GetAtt"][1] == "Arn" for r in res)
            rep.check("iam-passrole", ok, "%s %s: iam:PassRole on %s, not a specific role ARN" % (label, owner, [flat(r) for r in res]))


def check_external_grants(t, rep, label):
    in_template = set(resources(t, "AWS::IAM::Role"))
    for k, p in list(resources(t, "AWS::IAM::Policy").items()) + list(resources(t, "AWS::IAM::ManagedPolicy").items()):
        for r in as_list(p["Properties"].get("Roles", [])):
            ref = r.get("Ref") if isinstance(r, dict) else None
            rep.check("no-external-grant", ref in in_template, "%s %s attaches to %r, a role this stack does not own" % (label, k, r))


def check_bucket(t, rep):
    bid, b = one(t, "AWS::S3::Bucket")
    rep.check("bucket-present", b is not None, "the trainer template does not hold exactly one bucket")
    if b is None:
        return None
    rep.check("bucket-retain", b.get("DeletionPolicy") == "Retain" and b.get("UpdateReplacePolicy") == "Retain",
              "DeletionPolicy %r, UpdateReplacePolicy %r (must both be Retain in every env)" % (b.get("DeletionPolicy"), b.get("UpdateReplacePolicy")))
    pab = b["Properties"].get("PublicAccessBlockConfiguration", {})
    rep.check("bucket-public-access", all(pab.get(k) is True for k in ("BlockPublicAcls", "BlockPublicPolicy", "IgnorePublicAcls", "RestrictPublicBuckets")),
              "PublicAccessBlockConfiguration is %r" % pab)
    tls = any(
        st.get("Effect") == "Deny" and st.get("Condition", {}).get("Bool", {}).get("aws:SecureTransport") == "false"
        for p in resources(t, "AWS::S3::BucketPolicy").values() for st in statements(p["Properties"]["PolicyDocument"])
    )
    rep.check("bucket-tls", tls, "no bucket policy denies aws:SecureTransport=false")
    rep.check("bucket-encryption", "BucketEncryption" in b["Properties"], "no server-side encryption")
    rules = b["Properties"].get("LifecycleConfiguration", {}).get("Rules", [])

    def view(rule):
        f = rule.get("Filter")
        prefix, tags = rule.get("Prefix"), {x["Key"]: x["Value"] for x in rule.get("TagFilters", [])}
        if isinstance(f, dict):
            a = f.get("And", f)
            prefix = a.get("Prefix", prefix)
            tags = {x["Key"]: x["Value"] for x in a.get("Tags", [])} or ({f["Tag"]["Key"]: f["Tag"]["Value"]} if "Tag" in f else tags)
        return prefix, tags

    runs = [r for r in rules if view(r)[0] == "runs/"]
    rep.check("lifecycle-runs-rule", len(runs) == 1, "expected exactly one rule over runs/, found %d" % len(runs))
    for r in runs:
        rep.check("lifecycle-runs-tag", view(r)[1] == {"laya-retention": "expire"},
                  "the runs/ rule is filtered on %r, not on the laya-retention=expire tag" % (view(r)[1],))
        rep.check("lifecycle-days", r.get("ExpirationInDays") == 30, "the runs/ rule expires after %r days, not 30" % r.get("ExpirationInDays"))
    sm = [r for r in rules if view(r)[0] == "sagemaker-output/"]
    rep.check("lifecycle-sagemaker-output", len(sm) == 1 and sm[0].get("ExpirationInDays") == 30,
              "expected one sagemaker-output/ rule at 30 days, found %r" % [(view(r), r.get("ExpirationInDays")) for r in sm])
    rep.check("lifecycle-datasets", not any((view(r)[0] or "").startswith("datasets/") for r in rules), "a lifecycle rule targets datasets/")
    rep.check("lifecycle-base", not any((view(r)[0] or "").startswith("base/") for r in rules), "a lifecycle rule targets base/")
    rep.check("lifecycle-abort-multipart",
              any(r.get("AbortIncompleteMultipartUpload", {}).get("DaysAfterInitiation") == 7 for r in rules),
              "no abort-incomplete-multipart rule (7 days)")
    unscoped = [r.get("Id") for r in rules if not view(r)[0] and ("ExpirationInDays" in r or "ExpirationDate" in r)]
    rep.check("lifecycle-unscoped", not unscoped, "rule(s) %s expire objects with no prefix filter (would reach datasets/ and base/)" % unscoped)
    rep.check("lifecycle-rule-set", len(rules) == 3, "expected exactly the three D-19 rules, found %d" % len(rules))
    return bid


def check_machine(t, rep):
    m, asl = asl_of(t)
    rep.check("machine-present", m is not None, "the trainer template does not hold exactly one state machine")
    if m is None:
        return
    bid = bucket_id(t)
    bucket = "<Ref:%s>" % bid
    env = re.sub(r"^aprender-laya-train-", "", m["Properties"].get("StateMachineName", ""))
    rep.check("machine-name", m["Properties"].get("StateMachineName") in ("aprender-laya-train-dev", "aprender-laya-train-prod"),
              "StateMachineName %r" % m["Properties"].get("StateMachineName"))
    rep.check("machine-type", m["Properties"].get("StateMachineType") == "STANDARD", "not a STANDARD machine (.sync needs it)")
    rep.check("machine-timeout", asl.get("TimeoutSeconds") == 36000, "TimeoutSeconds is %r, not 36000" % asl.get("TimeoutSeconds"))
    states = asl.get("States", {})
    rep.check("machine-shape",
              set(states) == {"Train", "PackVerify", "Finalize", "CatchFinalize"} and asl.get("StartAt") == "Train"
              and states.get("Train", {}).get("Next") == "PackVerify" and states.get("PackVerify", {}).get("Next") == "Finalize"
              and states.get("Finalize", {}).get("End") is True and states.get("CatchFinalize", {}).get("End") is True,
              "states %s, StartAt %r" % (sorted(states), asl.get("StartAt")))
    train, pack = states.get("Train", {}), states.get("PackVerify", {})
    tp, pp = train.get("Parameters", {}), pack.get("Parameters", {})
    rep.check("train-resource", flat(train.get("Resource", "")).endswith(":states:::sagemaker:createTrainingJob.sync"),
              "Train Resource is %r" % train.get("Resource"))
    rep.check("train-runtime", tp.get("StoppingCondition", {}).get("MaxRuntimeInSeconds") == 21600,
              "StoppingCondition is %r (MaxRuntimeInSeconds must be the literal 21600)" % tp.get("StoppingCondition"))
    rc = tp.get("ResourceConfig", {})
    rep.check("train-instance", rc.get("InstanceType") == "ml.g5.xlarge" and rc.get("InstanceCount") == 1 and set(rc) == {"InstanceType", "InstanceCount", "VolumeSizeInGB"},
              "ResourceConfig is %r (instance and count must be the literals ml.g5.xlarge and 1, never read from the input)" % rc)
    image = tp.get("AlgorithmSpecification", {}).get("TrainingImage", "")
    m_image = re.search(r"/aprender-laya-train-%s@(%s)$" % (re.escape(env), DIGEST_RE), image) if env else None
    rep.check("train-image-pinned", bool(m_image) and "TrainingImage" in tp.get("AlgorithmSpecification", {}),
              "TrainingImage %r is not <train repo>@sha256:<64 hex>" % image)
    train_digest = m_image.group(1) if m_image else None
    rep.check("train-job-name", tp.get("TrainingJobName.$") == "States.Format('laya-{}', $.task_id)",
              "TrainingJobName is %r" % {k: v for k, v in tp.items() if k.startswith("TrainingJobName")})
    chans = tp.get("InputDataConfig", [])
    rep.check("train-channels-exact", sorted(c.get("ChannelName") for c in chans) == ["base", "data"],
              "channels are %r (exactly data and base)" % [c.get("ChannelName") for c in chans])
    by = {c.get("ChannelName"): c.get("DataSource", {}).get("S3DataSource", {}) for c in chans}
    rep.check("train-channel-data-type", by.get("data", {}).get("S3DataType") == "S3Prefix" and by.get("base", {}).get("S3DataType") == "S3Prefix",
              "S3DataType is data=%r base=%r (both must be S3Prefix)" % (by.get("data", {}).get("S3DataType"), by.get("base", {}).get("S3DataType")))
    rep.check("train-channel-data-uri",
              by.get("data", {}).get("S3Uri.$") == "States.Format('s3://%s/{}', $.dataset_prefix)" % bucket and "S3Uri" not in by.get("data", {}),
              "data channel is %r (must be States.Format over $.dataset_prefix on the run bucket, never a literal or a caller URI)" % by.get("data"))
    rep.check("train-channel-base", by.get("base", {}).get("S3Uri") == "s3://%s/%s" % (bucket, BASE_PREFIX) and "S3Uri.$" not in by.get("base", {}),
              "base channel is %r (must be exactly s3://<bucket>/%s)" % (by.get("base"), BASE_PREFIX))
    rep.check("train-output", tp.get("OutputDataConfig", {}).get("S3OutputPath") == "s3://%s/sagemaker-output/" % bucket,
              "OutputDataConfig is %r" % tp.get("OutputDataConfig"))
    tenv = tp.get("Environment", {})
    rep.check("train-env-expected-inputs", tenv.get("LAYA_EXPECTED_INPUTS.$") == "$.inputs_sha256",
              "LAYA_EXPECTED_INPUTS is %r (must be bound to $.inputs_sha256)" % {k: v for k, v in tenv.items() if "EXPECTED" in k})
    rep.check("train-env-keys", without_prefix(tenv) == TRAIN_ENV_KEYS, "Environment keys are %s" % sorted(without_prefix(tenv)))
    rep.check("train-env-bindings",
              tenv.get("LAYA_STEP") == "train" and tenv.get("LAYA_TASK_ID.$") == "$.task_id" and tenv.get("LAYA_NAME.$") == "$.name"
              and tenv.get("LAYA_RUN_PREFIX.$") == "$.run_prefix" and tenv.get("LAYA_EPOCHS.$") == "$.epochs"
              and tenv.get("LAYA_BUCKET") == bucket,
              "Environment is %r" % tenv)
    rep.check("train-digest-literal", bool(train_digest) and tenv.get("LAYA_TRAIN_IMAGE_DIGEST") == train_digest,
              "LAYA_TRAIN_IMAGE_DIGEST is %r, the TrainingImage digest is %r (a literal equal to it, never from the input)" % (tenv.get("LAYA_TRAIN_IMAGE_DIGEST"), train_digest))
    for st_name, st in (("train", train), ("pack", pack)):
        c = st.get("Catch", [])
        rep.check("%s-catch" % st_name,
                  len(c) == 1 and c[0].get("ErrorEquals") == ["States.ALL"] and c[0].get("Next") == "CatchFinalize" and c[0].get("ResultPath") == "$.error",
                  "%s Catch is %r (must send States.ALL to CatchFinalize with ResultPath $.error)" % (st_name, c))
    rep.check("pack-resource", flat(pack.get("Resource", "")).endswith(":states:::ecs:runTask.sync"), "PackVerify Resource is %r" % pack.get("Resource"))
    rep.check("pack-launch", pp.get("LaunchType") == "FARGATE" and pp.get("NetworkConfiguration", {}).get("AwsvpcConfiguration", {}).get("AssignPublicIp") == "ENABLED"
              and pack.get("TimeoutSeconds") == 10800,
              "PackVerify launch is %r, TimeoutSeconds %r" % (pp.get("LaunchType"), pack.get("TimeoutSeconds")))
    ov = pp.get("Overrides", {}).get("ContainerOverrides", [{}])
    penv = {e["Name"]: e for e in ov[0].get("Environment", [])} if len(ov) == 1 else {}
    rep.check("pack-overrides",
              set(penv) == PACK_ENV_KEYS and penv.get("LAYA_TASK_ID", {}).get("Value.$") == "$.task_id" and penv.get("LAYA_NAME", {}).get("Value.$") == "$.name"
              and penv.get("LAYA_RUN_PREFIX", {}).get("Value.$") == "$.run_prefix" and penv.get("LAYA_BUCKET", {}).get("Value") == bucket,
              "container override environment is %s" % sorted(penv))
    _, td = one(t, "AWS::ECS::TaskDefinition")
    pack_image = flat(td["Properties"]["ContainerDefinitions"][0]["Image"]) if td else ""
    m_pack = re.search(r"/aprender-laya-pack-%s@(%s)$" % (re.escape(env), DIGEST_RE), pack_image) if env else None
    rep.check("pack-image-pinned", bool(m_pack), "pack task image %r is not <pack repo>@sha256:<64 hex>" % pack_image)
    rep.check("pack-digest-literal", bool(m_pack) and penv.get("LAYA_PACK_IMAGE_DIGEST", {}).get("Value") == (m_pack.group(1) if m_pack else None),
              "LAYA_PACK_IMAGE_DIGEST is %r, the task definition's image digest is %r" % (penv.get("LAYA_PACK_IMAGE_DIGEST", {}).get("Value"), m_pack.group(1) if m_pack else None))
    if td:
        p = td["Properties"]
        rep.check("pack-arch", p.get("RuntimePlatform", {}).get("CpuArchitecture") == "ARM64", "RuntimePlatform is %r (must be ARM64)" % p.get("RuntimePlatform"))
        rep.check("pack-size", p.get("Cpu") == "8192" and p.get("Memory") == "16384", "task size is %r cpu / %r MiB (8 vCPU / 16 GiB)" % (p.get("Cpu"), p.get("Memory")))
        rep.check("pack-command", p["ContainerDefinitions"][0].get("Command") == ["pack"], "container command is %r" % p["ContainerDefinitions"][0].get("Command"))
    fin, cat = states.get("Finalize", {}), states.get("CatchFinalize", {})
    for st_name, st, extra in (("finalize", fin, False), ("catch-finalize", cat, True)):
        pl = st.get("Parameters", {}).get("Payload", {})
        rep.check("%s-start-time" % st_name, pl.get("start_time.$") == "$$.Execution.StartTime", "%s payload start_time is %r" % (st_name, pl.get("start_time.$")))
        rep.check("%s-payload" % st_name,
                  pl.get("source") == "state-machine" and pl.get("execution_arn.$") == "$$.Execution.Id" and pl.get("input.$") == "$$.Execution.Input"
                  and (pl.get("error.$") == "$.error") == extra,
                  "%s payload is %r" % (st_name, pl))


def check_roles_and_policies(t, rep):
    bid = bucket_id(t)
    ids = role_ids(t)
    if not bid or not all(k in ids for k in ("sagemaker", "fargate", "finalizer")):
        rep.check("roles-present", False, "could not identify the SageMaker, Fargate task and finalizer roles: %r" % ids)
        return
    tag_dir = "runs/*"
    expected = {
        "sagemaker": {
            "s3:GetObject": {"datasets/*", BASE_PREFIX + "*"},
            "s3:ListBucket": {"[%s]" % ",".join(sorted(["datasets/*", BASE_PREFIX + "*"]))},
            "s3:PutObject": {"runs/*", "sagemaker-output/*"},
            "s3:PutObjectTagging": {"runs/*"},
            "s3:AbortMultipartUpload": {"sagemaker-output/*"},
            "s3:ListMultipartUploadParts": {"sagemaker-output/*"},
        },
        "fargate": {
            "s3:GetObject": {"runs/*", BASE_PREFIX + "*"},
            "s3:PutObject": {"runs/*"},
            "s3:PutObjectTagging": {"runs/*"},
        },
        "finalizer": {
            "s3:GetObject": {"runs/*"}, "s3:PutObject": {"runs/*"}, "s3:PutObjectTagging": {"runs/*"},
            "s3:GetObjectTagging": {"runs/*"}, "s3:ListBucket": {"[runs/*]"},
        },
    }
    for who, name in (("sagemaker", "sagemaker"), ("fargate", "fargate"), ("finalizer", "finalizer")):
        g = s3_grants(role_policy_statements(t, ids[who]), bid)
        if who in ("sagemaker", "fargate"):
            rep.check("%s-put-tagging" % name, tag_dir in g.get("s3:PutObjectTagging", set()),
                      "the %s role has no s3:PutObjectTagging on runs/* (every run_step upload is tagged: AccessDenied otherwise); grants %s" % (who, {k: sorted(v) for k, v in g.items()}))
        rep.check("%s-s3-scope" % name, g == expected[who],
                  "the %s role's S3 grants are %s, expected %s" % (who, {k: sorted(v) for k, v in sorted(g.items())}, {k: sorted(v) for k, v in sorted(expected[who].items())}))
    fin = {flat(r): s for s in role_policy_statements(t, ids["finalizer"]) for r in as_list(s.get("Resource", []))}
    acts = {a for s in role_policy_statements(t, ids["finalizer"]) for a in as_list(s.get("Action", []))}
    need = {"states:DescribeExecution", "states:StopExecution", "states:ListExecutions", "sagemaker:DescribeTrainingJob", "sagemaker:StopTrainingJob",
            "ecs:ListTasks", "ecs:DescribeTasks", "ecs:StopTask", "dynamodb:GetItem", "dynamodb:PutItem", "dynamodb:UpdateItem", "dynamodb:DeleteItem", "dynamodb:Query"}
    rep.check("finalizer-actions", need <= acts, "the finalizer role lacks %s" % sorted(need - acts))
    ecs_st = [s for s in role_policy_statements(t, ids["finalizer"]) if any(str(a).startswith("ecs:") for a in as_list(s.get("Action", [])))]
    rep.check("finalizer-ecs-cluster", bool(ecs_st) and all("ecs:cluster" in json.dumps(s.get("Condition", {})) for s in ecs_st), "the finalizer's ECS rights are not conditioned on the cluster")
    sm = [s for s in role_policy_statements(t, ids["finalizer"]) if any(str(a).startswith("sagemaker:") for a in as_list(s.get("Action", [])))]
    rep.check("finalizer-sagemaker-scope", bool(sm) and all(flat(r).endswith(":training-job/laya-*") for s in sm for r in as_list(s.get("Resource", []))),
              "the finalizer's SageMaker rights are not limited to training-job/laya-*")
    del fin


def check_request_policy(t, rep):
    out = t.get("Outputs", {}).get("RequestLambdaPolicy")
    rep.check("request-policy-output", out is not None, "no RequestLambdaPolicy output")
    if out is None:
        return
    try:
        doc = json.loads(flat(out["Value"]))
    except ValueError as e:
        rep.check("request-policy-json", False, "the output is not JSON: %s" % e)
        return
    sts = statements(doc)
    acts = {a for s in sts for a in as_list(s.get("Action", []))}
    rep.check("request-no-put", not any(a.startswith(("s3:Put", "s3:Delete", "s3:*")) for a in acts), "the request policy grants %s" % sorted(a for a in acts if a.startswith("s3:")))
    lb = [s for s in sts if "s3:ListBucket" in as_list(s.get("Action", []))]
    rep.check("request-list-condition", len(lb) == 1 and list(lb[0].get("Condition", {})) == ["StringLikeIfExists"]
              and lb[0]["Condition"]["StringLikeIfExists"].get("s3:prefix") == ["datasets/*"],
              "ListBucket statements are %r (must be conditioned on StringLikeIfExists s3:prefix datasets/*; plain StringLike turns a missing key's 404 into 403)" % lb)
    dx = [s for s in sts if "states:DescribeExecution" in as_list(s.get("Action", []))]
    rep.check("request-describe-execution", bool(dx) and all(":execution:aprender-laya-train-" in flat(r) for s in dx for r in as_list(s.get("Resource", []))),
              "no states:DescribeExecution on the machine's executions (14-04's dispatch confirm)")
    rep.check("request-start-execution", "states:StartExecution" in acts and "states:StopExecution" in acts, "the request policy lacks StartExecution or StopExecution")
    rep.check("request-dynamodb", {"dynamodb:GetItem", "dynamodb:PutItem", "dynamodb:UpdateItem", "dynamodb:DeleteItem", "dynamodb:Query"} <= acts, "the request policy lacks a DynamoDB item action")
    g = [s for s in sts if "s3:GetObject" in as_list(s.get("Action", []))]
    rep.check("request-get-scope", len(g) == 1 and sorted(flat(r).split("/", 1)[1] for r in as_list(g[0].get("Resource", []))) == ["datasets/*", "runs/*"],
              "s3:GetObject is granted on %r (must be datasets/* and runs/* only)" % [flat(r) for s in g for r in as_list(s.get("Resource", []))])
    rep.check("request-policy-scope", acts <= {
        "dynamodb:GetItem", "dynamodb:PutItem", "dynamodb:UpdateItem", "dynamodb:DeleteItem", "dynamodb:Query", "states:StartExecution",
        "states:DescribeExecution", "states:StopExecution", "s3:GetObject", "s3:ListBucket"}, "the request policy carries an unexpected action: %s" % sorted(acts))


def check_events(t, rep):
    rules = list(resources(t, "AWS::Events::Rule").values())
    status = [r for r in rules if "EventPattern" in r["Properties"]]
    rep.check("status-rule", len(status) == 1, "expected one status-change rule, found %d" % len(status))
    for r in status:
        p = r["Properties"]["EventPattern"]
        rep.check("status-rule-statuses",
                  p.get("source") == ["aws.states"] and p.get("detail-type") == ["Step Functions Execution Status Change"]
                  and sorted(p.get("detail", {}).get("status", [])) == ["ABORTED", "FAILED", "TIMED_OUT"] and bool(p.get("detail", {}).get("stateMachineArn")),
                  "the status rule pattern is %r (FAILED, TIMED_OUT and ABORTED for this machine)" % p)
        rep.check("status-rule-machine", all(":stateMachine:aprender-laya-train-" in flat(a) for a in p.get("detail", {}).get("stateMachineArn", [])), "the status rule is not scoped to this machine")
    sched = [r for r in rules if "ScheduleExpression" in r["Properties"]]
    ok = [r for r in sched if r["Properties"]["ScheduleExpression"] == "rate(10 minutes)" and r["Properties"].get("State", "ENABLED") == "ENABLED"
          and any(json.loads(x.get("Input", "null")) == {"source": "reconcile"} for x in r["Properties"].get("Targets", []))]
    rep.check("reconcile-rule", len(ok) == 1, "no rate(10 minutes) rule sending {\"source\":\"reconcile\"} to the finalizer")
    for r in rules:
        for x in r["Properties"].get("Targets", []):
            rep.check("target-dlq", bool(x.get("DeadLetterConfig", {}).get("Arn")), "an EventBridge target has no deadLetter queue: %r" % x.get("Id"))
            rep.check("target-retry", x.get("RetryPolicy", {}).get("MaximumRetryAttempts", 0) >= 1, "an EventBridge target has no retryAttempts")
    _, q = one(t, "AWS::SQS::Queue")
    rep.check("dlq-retention", q is not None and q["Properties"].get("MessageRetentionPeriod") == 1209600, "the DLQ does not keep messages 14 days")
    alarms = [a for a in resources(t, "AWS::CloudWatch::Alarm").values() if "FinalizerErrors" in str(a["Properties"].get("AlarmName", ""))]
    rep.check("finalizer-alarm", len(alarms) == 1 and alarms[0]["Properties"].get("MetricName") == "Errors" and alarms[0]["Properties"].get("Namespace") == "AWS/Lambda"
              and alarms[0]["Properties"].get("Threshold") == 1 and alarms[0]["Properties"].get("ComparisonOperator") == "GreaterThanOrEqualToThreshold",
              "no FinalizerErrors alarm on the Lambda Errors metric (>= 1)")


def check_misc(t, rep):
    _, fn = one(t, "AWS::Lambda::Function")
    if fn:
        p = fn["Properties"]
        rep.check("finalizer-lambda", p.get("Architectures") == ["arm64"] and p.get("Runtime") == "provided.al2023" and p.get("MemorySize") == 512 and p.get("Timeout") == 300 and p.get("Handler") == "bootstrap",
                  "finalizer is %r %r %r MB %r s" % (p.get("Architectures"), p.get("Runtime"), p.get("MemorySize"), p.get("Timeout")))
        envs = p.get("Environment", {}).get("Variables", {})
        rep.check("finalizer-env", FINALIZER_ENV_KEYS <= set(envs), "finalizer env lacks %s" % sorted(FINALIZER_ENV_KEYS - set(envs)))
        arn = flat(envs.get("APRENDER_LAYA_TRAIN_STATE_MACHINE_ARN", ""))
        rep.check("finalizer-machine-arn", ":stateMachine:aprender-laya-train-" in arn and "<GetAtt:" not in arn and "<Ref:Machine" not in arn,
                  "the finalizer's machine ARN is %r (derive it from the fixed name: a reference is a circular dependency)" % arn)
    _, tb = one(t, "AWS::DynamoDB::Table")
    if tb:
        p = tb["Properties"]
        rep.check("table", [(k["AttributeName"], k["KeyType"]) for k in p["KeySchema"]] == [("owner_id", "HASH"), ("task_id", "RANGE")]
                  and p.get("BillingMode") == "PAY_PER_REQUEST" and p.get("TimeToLiveSpecification", {}).get("AttributeName") == "expires_at",
                  "the task table is not the SetFit key schema with TTL expires_at on demand")
    rep.check("network-no-nat", not resources(t, "AWS::EC2::NatGateway"), "the stack creates a NAT gateway")
    _, sg = one(t, "AWS::EC2::SecurityGroup")
    rep.check("network-no-ingress", sg is not None and not sg["Properties"].get("SecurityGroupIngress"), "the pack security group has ingress rules")
    rep.check("outputs", REQUIRED_OUTPUTS <= set(t.get("Outputs", {})), "outputs lack %s" % sorted(REQUIRED_OUTPUTS - set(t.get("Outputs", {}))))


def check_trainer(t):
    rep = Report()
    check_bucket(t, rep)
    check_machine(t, rep)
    check_roles_and_policies(t, rep)
    check_request_policy(t, rep)
    check_events(t, rep)
    check_misc(t, rep)
    check_wildcards(t, rep, "trainer")
    check_external_grants(t, rep, "trainer")
    return rep


def check_images(t):
    rep = Report()
    repos = resources(t, "AWS::ECR::Repository")
    rep.check("ecr-two-repos", len(repos) == 2, "expected two ECR repositories, found %d" % len(repos))
    for k, r in repos.items():
        p = r["Properties"]
        rep.check("ecr-immutable", p.get("ImageTagMutability") == "IMMUTABLE", "%s tag mutability is %r" % (k, p.get("ImageTagMutability")))
        rep.check("ecr-scan", p.get("ImageScanningConfiguration", {}).get("ScanOnPush") is True, "%s does not scan on push" % k)
        rep.check("ecr-retain", r.get("DeletionPolicy") == "Retain", "%s is not retained on stack deletion" % k)
        try:
            rules = json.loads(flat(p["LifecyclePolicy"]["LifecyclePolicyText"]))["rules"]
        except (KeyError, ValueError):
            rules = None
        rep.check("ecr-lifecycle", rules is not None and len(rules) == 1, "%s has no single-rule lifecycle policy" % k)
        for rule in rules or []:
            sel = rule.get("selection", {})
            rep.check("ecr-lifecycle-tagged",
                      sel.get("tagStatus") == "untagged" and sel.get("countType") == "sinceImagePushed" and sel.get("countNumber") == 14 and sel.get("countUnit") == "days"
                      and rule.get("action", {}).get("type") == "expire",
                      "%s lifecycle selection is %r (untagged images only, by age; a count rule on tagged images can delete a deployed digest)" % (k, sel))
    projects = resources(t, "AWS::CodeBuild::Project")
    rep.check("codebuild-two-projects", len(projects) == 2, "expected two CodeBuild projects, found %d" % len(projects))
    kinds = sorted((p["Properties"]["Environment"]["Type"], p["Properties"]["Environment"].get("PrivilegedMode")) for p in projects.values())
    rep.check("codebuild-architectures", kinds == [("ARM_CONTAINER", True), ("LINUX_CONTAINER", True)], "project environments are %r (LINUX_CONTAINER and ARM_CONTAINER, both privileged)" % kinds)
    for k, p in projects.items():
        spec = p["Properties"]["Source"].get("BuildSpec", "")
        rep.check("codebuild-digest-line", "IMAGE DIGEST" in spec and "REPO_URI" in spec, "%s buildspec does not print IMAGE DIGEST" % k)
        rep.check("codebuild-compute", p["Properties"]["Environment"].get("ComputeType") in ("BUILD_GENERAL1_LARGE", "BUILD_GENERAL1_LARGE"), "%s is not LARGE" % k)
    _, b = one(t, "AWS::S3::Bucket")
    if b:
        pab = b["Properties"].get("PublicAccessBlockConfiguration", {})
        rep.check("context-bucket-public-access", all(pab.get(x) is True for x in ("BlockPublicAcls", "BlockPublicPolicy", "IgnorePublicAcls", "RestrictPublicBuckets")), "the build-context bucket allows public access")
        rules = b["Properties"].get("LifecycleConfiguration", {}).get("Rules", [])
        rep.check("context-bucket-expiry", any(r.get("ExpirationInDays") == 7 for r in rules), "the build-context bucket does not expire contexts after 7 days")
    check_wildcards(t, rep, "images")
    check_external_grants(t, rep, "images")
    return rep


# ----------------------------------------------------------------------------- selftest

def asl_edit(fn):
    def apply(t):
        _, asl = asl_of(t)
        fn(asl)
        set_asl(t, asl)
    return apply


def edit_policy(role_key, fn):
    def apply(t):
        ids = role_ids(t)
        for st in role_policy_statements(t, ids[role_key]):
            fn(st)
    return apply


def drop_action(st, action):
    acts = as_list(st.get("Action", []))
    if action in acts:
        st["Action"] = [a for a in acts if a != action]


def swap_list_operator(st, new_op):
    """Rewrite a ListBucket statement's Condition operator (None drops the condition altogether)."""
    if "s3:ListBucket" in as_list(st.get("Action", [])) and "Condition" in st:
        cond = st.pop("Condition")
        if new_op:
            st["Condition"] = {new_op: cond["StringLikeIfExists"]}


def add_statement(role_key, statement):
    def apply(t):
        ids = role_ids(t)
        for p in resources(t, "AWS::IAM::Policy").values():
            if ids[role_key] in [r.get("Ref") for r in as_list(p["Properties"].get("Roles", []))]:
                p["Properties"]["PolicyDocument"]["Statement"].append(statement)
                return
        raise AssertionError("no inline policy for %s" % role_key)
    return apply


def edit_request_policy(fn):
    def apply(t):
        out = t["Outputs"]["RequestLambdaPolicy"]
        doc = json.loads(flat(out["Value"]))
        fn(doc)
        out["Value"] = json.dumps(doc)
    return apply


def edit_bucket_rules(fn):
    def apply(t):
        fn(one(t, "AWS::S3::Bucket")[1]["Properties"]["LifecycleConfiguration"]["Rules"])
    return apply


def set_prop(type_, path, value):
    def apply(t):
        _, r = one(t, type_)
        node = r
        for p in path[:-1]:
            node = node[p]
        node[path[-1]] = value
    return apply


def set_first(type_, path, value):
    """Like set_prop, for a template that holds several resources of the type: the first one is mutated."""
    def apply(t):
        node = next(iter(resources(t, type_).values()))
        for p in path[:-1]:
            node = node[p]
        node[path[-1]] = value
    return apply


def edit_ecr_rules(fn):
    def apply(t):
        for r in resources(t, "AWS::ECR::Repository").values():
            doc = json.loads(flat(r["Properties"]["LifecyclePolicy"]["LifecyclePolicyText"]))
            fn(doc["rules"])
            r["Properties"]["LifecyclePolicy"]["LifecyclePolicyText"] = json.dumps(doc)
            return
    return apply


def drop_first_dlq(t):
    for r in resources(t, "AWS::Events::Rule").values():
        r["Properties"]["Targets"][0].pop("DeadLetterConfig", None)
        return


def drop_reconcile(t):
    for k, r in list(resources(t, "AWS::Events::Rule").items()):
        if "ScheduleExpression" in r["Properties"]:
            del t["Resources"][k]


def drop_aborted(t):
    for r in resources(t, "AWS::Events::Rule").values():
        if "EventPattern" in r["Properties"]:
            r["Properties"]["EventPattern"]["detail"]["status"].remove("ABORTED")


def drop_pack_catch(asl):
    del asl["States"]["PackVerify"]["Catch"]


def third_channel(asl):
    asl["States"]["Train"]["Parameters"]["InputDataConfig"].append(
        {"ChannelName": "extra", "DataSource": {"S3DataSource": {"S3Uri": "s3://x/y/", "S3DataType": "S3Prefix"}}})


def channel(asl, name):
    return next(c for c in asl["States"]["Train"]["Parameters"]["InputDataConfig"] if c["ChannelName"] == name)["DataSource"]["S3DataSource"]


def data_literal(asl):
    ds = channel(asl, "data")
    del ds["S3Uri.$"]
    ds["S3Uri"] = "s3://some-bucket/datasets/fixed/"


def data_manifest(asl):
    channel(asl, "data")["S3DataType"] = "ManifestFile"


def base_other(asl):
    channel(asl, "base")["S3Uri"] = "s3://x/base/laya-other/"


def tparams(asl):
    return asl["States"]["Train"]["Parameters"]


def instance_from_input(asl):
    rc = tparams(asl)["ResourceConfig"]
    del rc["InstanceType"]
    rc["InstanceType.$"] = "$.instance_type"


def add_wildcard_action(t):
    add_statement("finalizer", {"Effect": "Allow", "Action": "*", "Resource": "*"})(t)


def widen_finalizer(t):
    """Add datasets/* to the finalizer's object-read statement."""
    bid = bucket_id(t)
    extra = {"Fn::Join": ["", [{"Fn::GetAtt": [bid, "Arn"]}, "/datasets/*"]]}
    for st in role_policy_statements(t, role_ids(t)["finalizer"]):
        if "s3:GetObject" in as_list(st.get("Action", [])):
            st["Resource"] = as_list(st["Resource"]) + [extra]


CASES = [
    # (name, mutate, trainer?, expected assertion)
    ("runtime 21601", asl_edit(lambda a: tparams(a)["StoppingCondition"].update(MaxRuntimeInSeconds=21601)), True, "train-runtime"),
    ("instance ml.g5.2xlarge", asl_edit(lambda a: tparams(a)["ResourceConfig"].update(InstanceType="ml.g5.2xlarge")), True, "train-instance"),
    ("instance read from $.input", asl_edit(instance_from_input), True, "train-instance"),
    ("an image without @sha256", asl_edit(lambda a: tparams(a)["AlgorithmSpecification"].update(
        TrainingImage="123456789012.dkr.ecr.us-east-1.amazonaws.com/aprender-laya-train-dev:latest")), True, "train-image-pinned"),
    ("a third channel", asl_edit(third_channel), True, "train-channels-exact"),
    ("a missing Catch on PackVerify", asl_edit(drop_pack_catch), True, "pack-catch"),
    ("an X86_64 task definition", set_prop("AWS::ECS::TaskDefinition", ["Properties", "RuntimePlatform", "CpuArchitecture"], "X86_64"), True, "pack-arch"),
    ("a LAYA_TRAIN_IMAGE_DIGEST env value differing from the TrainingImage digest",
     asl_edit(lambda a: tparams(a)["Environment"].update(LAYA_TRAIN_IMAGE_DIGEST="sha256:" + "1" * 64)), True, "train-digest-literal"),
    ("a runs/ lifecycle rule without the expire tag", edit_bucket_rules(lambda rules: [r.pop("TagFilters", None) for r in rules if r.get("Prefix") == "runs/"]), True, "lifecycle-runs-tag"),
    ("a lifecycle rule on datasets/", edit_bucket_rules(lambda rules: rules.append({"Id": "x", "Prefix": "datasets/", "ExpirationInDays": 30, "Status": "Enabled"})), True, "lifecycle-datasets"),
    ("a lifecycle rule on base/", edit_bucket_rules(lambda rules: rules.append({"Id": "y", "Prefix": "base/", "ExpirationInDays": 30, "Status": "Enabled"})), True, "lifecycle-base"),
    ("the expire rule at 31 days", edit_bucket_rules(lambda rules: [r.update(ExpirationInDays=31) for r in rules if r.get("Prefix") == "runs/"]), True, "lifecycle-days"),
    ("bucket DeletionPolicy Delete", set_prop("AWS::S3::Bucket", ["DeletionPolicy"], "Delete"), True, "bucket-retain"),
    ("public access block off", set_prop("AWS::S3::Bucket", ["Properties", "PublicAccessBlockConfiguration", "BlockPublicPolicy"], False), True, "bucket-public-access"),
    ("an Action \"*\"", add_wildcard_action, True, "iam-action-star"),
    ("an s3:* action", add_statement("finalizer", {"Effect": "Allow", "Action": "s3:*", "Resource": "*"}), True, "iam-service-star"),
    ("an iam:PassRole on \"*\"", add_statement("finalizer", {"Effect": "Allow", "Action": "iam:PassRole", "Resource": "*"}), True, "iam-passrole"),
    ("the request policy granting s3:PutObject", edit_request_policy(lambda d: [s["Action"].append("s3:PutObject") for s in d["Statement"] if "s3:GetObject" in s.get("Action", [])]), True, "request-no-put"),
    ("ListBucket without the datasets/* condition", edit_request_policy(lambda d: [s.pop("Condition", None) for s in d["Statement"] if "s3:ListBucket" in s.get("Action", [])]), True, "request-list-condition"),
    ("ListBucket with the old StringLike operator (request policy)", edit_request_policy(lambda d: [s["Condition"].update(StringLike=s["Condition"].pop("StringLikeIfExists")) for s in d["Statement"] if "s3:ListBucket" in s.get("Action", [])]), True, "request-list-condition"),
    ("ListBucket with the old StringLike operator (finalizer)", edit_policy("finalizer", lambda s: swap_list_operator(s, "StringLike")), True, "finalizer-s3-scope"),
    ("ListBucket with the old StringLike operator (SageMaker)", edit_policy("sagemaker", lambda s: swap_list_operator(s, "StringLike")), True, "sagemaker-s3-scope"),
    ("ListBucket without any prefix restriction (finalizer)", edit_policy("finalizer", lambda s: swap_list_operator(s, None)), True, "finalizer-s3-scope"),
    ("ListBucket without any prefix restriction (SageMaker)", edit_policy("sagemaker", lambda s: swap_list_operator(s, None)), True, "sagemaker-s3-scope"),
    ("the EventBridge rule without ABORTED", drop_aborted, True, "status-rule-statuses"),
    ("ECR tag mutability MUTABLE", set_first("AWS::ECR::Repository", ["Properties", "ImageTagMutability"], "MUTABLE"), False, "ecr-immutable"),
    # the ten cases that answer the plan's review dispositions
    ("the SageMaker role without s3:PutObjectTagging on runs/*", edit_policy("sagemaker", lambda s: drop_action(s, "s3:PutObjectTagging")), True, "sagemaker-put-tagging"),
    ("the Fargate task role without s3:PutObjectTagging on runs/*", edit_policy("fargate", lambda s: drop_action(s, "s3:PutObjectTagging")), True, "fargate-put-tagging"),
    ("the data channel S3Uri a literal instead of States.Format over $.dataset_prefix", asl_edit(data_literal), True, "train-channel-data-uri"),
    ("the data channel S3DataType ManifestFile instead of S3Prefix", asl_edit(data_manifest), True, "train-channel-data-type"),
    ("the base channel at another prefix", asl_edit(base_other), True, "train-channel-base"),
    ("the Train environment without LAYA_EXPECTED_INPUTS", asl_edit(lambda a: tparams(a)["Environment"].pop("LAYA_EXPECTED_INPUTS.$")), True, "train-env-expected-inputs"),
    ("the reconcile schedule rule removed", drop_reconcile, True, "reconcile-rule"),
    ("an EventBridge target without a deadLetter queue", drop_first_dlq, True, "target-dlq"),
    ("an ECR lifecycle rule expiring tagged images by count", edit_ecr_rules(lambda rules: rules.__setitem__(0, {
        "rulePriority": 1, "description": "keep 20", "selection": {"tagStatus": "any", "countType": "imageCountMoreThan", "countNumber": 20}, "action": {"type": "expire"}})), False, "ecr-lifecycle-tagged"),
    ("the request policy without states:DescribeExecution", edit_request_policy(lambda d: [drop_action(s, "states:DescribeExecution") for s in d["Statement"]]), True, "request-describe-execution"),
    # beyond the plan's 31: the other limits the truths name
    ("the PackVerify LAYA_PACK_IMAGE_DIGEST differing from the task definition's digest",
     asl_edit(lambda a: a["States"]["PackVerify"]["Parameters"]["Overrides"]["ContainerOverrides"][0]["Environment"].__setitem__(4, {"Name": "LAYA_PACK_IMAGE_DIGEST", "Value": "sha256:" + "2" * 64})), True, "pack-digest-literal"),
    ("the Finalize payload without start_time", asl_edit(lambda a: a["States"]["Finalize"]["Parameters"]["Payload"].pop("start_time.$")), True, "finalize-start-time"),
    ("the machine timeout 36001", asl_edit(lambda a: a.update(TimeoutSeconds=36001)), True, "machine-timeout"),
    ("the finalizer's S3 right widened to datasets/*", widen_finalizer, True, "finalizer-s3-scope"),
    ("a policy attached to a role outside the stack", lambda t: next(iter(resources(t, "AWS::IAM::Policy").values()))["Properties"].__setitem__("Roles", ["pmcp-request-role"]), True, "no-external-grant"),
]

def selftest(trainer_path, images_path):
    for p in (trainer_path, images_path):
        if not os.path.isfile(p):
            print("REFUSED selftest: no synthesized template at %s (run: just laya-trainer-synth dev)" % p, file=sys.stderr)
            return 2
    trainer = json.load(open(trainer_path))
    images = json.load(open(images_path))
    fails = 0
    base_t, base_i = check_trainer(trainer), check_images(images)
    ok = not base_t.refused and not base_i.refused
    print("%s base templates pass every assertion (%d trainer, %d images)" % ("  ok" if ok else "  FAIL", base_t.count, base_i.count))
    for name, detail in base_t.refused + base_i.refused:
        print("       REFUSED %s: %s" % (name, detail))
    fails += 0 if ok else 1
    for name, mutate, on_trainer, want in CASES:
        tpl = copy.deepcopy(trainer if on_trainer else images)
        try:
            mutate(tpl)
        except Exception as e:  # a mutation that cannot be applied proves nothing
            print("  FAIL %s: the mutation could not be applied: %r" % (name, e))
            fails += 1
            continue
        rep = (check_trainer if on_trainer else check_images)(tpl)
        names = {n for n, _ in rep.refused}
        if want in names:
            print("  ok %s -> REFUSED %s" % (name, want))
        else:
            print("  FAIL %s: expected REFUSED %s, got %s" % (name, want, sorted(names) or "no refusal"))
            fails += 1
    print("%d mutation cases, %d failure(s)" % (len(CASES), fails))
    if fails:
        print("STACK CHECK SELFTEST FAILED")
        return 1
    print("STACK CHECK SELFTEST OK")
    return 0


def main(argv):
    args = argv[1:]
    do_selftest = False
    if args and args[0] == "--selftest":
        do_selftest = True
        args = args[1:]
    if len(args) not in (0, 2):
        print("usage: laya_trainer_stack_check.py [--selftest] [TEMPLATE_TRAINER TEMPLATE_IMAGES]", file=sys.stderr)
        return 2
    trainer_path, images_path = (args if args else (DEFAULT_TRAINER, DEFAULT_IMAGES))
    if do_selftest:
        return selftest(trainer_path, images_path)
    for p in (trainer_path, images_path):
        if not os.path.isfile(p):
            print("REFUSED template: %s does not exist (run: just laya-trainer-synth dev)" % p, file=sys.stderr)
            return 2
    rt = check_trainer(json.load(open(trainer_path)))
    ri = check_images(json.load(open(images_path)))
    for name, detail in rt.refused:
        print("REFUSED %s: %s" % (name, detail))
    for name, detail in ri.refused:
        print("REFUSED %s: %s" % (name, detail))
    if rt.refused or ri.refused:
        print("TRAINER STACK CHECK FAILED: %d refusal(s)" % (len(rt.refused) + len(ri.refused)))
        return 1
    print("TRAINER STACK CHECK OK trainer=%d images=%d assertions" % (rt.count, ri.count))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
