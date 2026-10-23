#!/usr/bin/env python3
"""Ingress scanner for the Phase 11 embed stacks (R4, plan 11-01).

usage:
  ingress_scan.py <template.json>             scan; exit 0 clean, 1 with findings
  ingress_scan.py --selftest <template.json>  planted-violation case table

Three checks, in this order, on every entry of Resources:

  1. ALLOWLIST FIRST. The resource type must be in the closed set the stacks are
     expected to produce. Any other type is a violation, including one nobody
     anticipated (a denylist alone is blind to a type nobody thought of).
  2. DENYLIST. Known ingress types get a named message: Lambda::Url,
     Lambda::Permission, ApiGateway*, ApiGatewayV2*, ElasticLoadBalancing*,
     ElasticLoadBalancingV2*, CloudFront* and VpcLattice*.
  3. PRINCIPAL SCAN. Any Principal or NotPrincipal key, at any depth, whose value
     contains the character '*' anywhere (string, list or nested object) is a
     violation, whatever the resource type.

HONEST LIMIT: the allowlist was DERIVED from the templates these stacks synthesize,
so on a first run it is not an independent check. The independent guards are the
denylist and the principal scan, which do not depend on what the templates contain.
The selftest proves the scanner can fail; it does not prove the allowlist is right.

Standard library only. It imports no sibling module.
"""

import copy
import json
import sys

ALLOWED_TYPES = {
    "AWS::Lambda::Function",
    "AWS::Lambda::EventInvokeConfig",
    "AWS::IAM::Role",
    "AWS::Logs::LogGroup",
    "AWS::SSM::Parameter",
    "AWS::CDK::Metadata",
}
DENY_EXACT = {"AWS::Lambda::Url", "AWS::Lambda::Permission"}
DENY_PREFIXES = (
    "AWS::ApiGateway",  # also covers AWS::ApiGatewayV2::*
    "AWS::ElasticLoadBalancing",  # also covers AWS::ElasticLoadBalancingV2::*
    "AWS::CloudFront",
    "AWS::VpcLattice",
)
PRINCIPAL_KEYS = ("Principal", "NotPrincipal")


def contains_star(value):
    if isinstance(value, str):
        return "*" in value
    if isinstance(value, list):
        return any(contains_star(item) for item in value)
    if isinstance(value, dict):
        return any(contains_star(item) for item in value.values())
    return False


def wildcard_principal_paths(node, path=""):
    """Paths of every Principal/NotPrincipal key whose value contains '*', at any depth."""
    if isinstance(node, dict):
        for key, value in node.items():
            here = f"{path}/{key}"
            if key in PRINCIPAL_KEYS and contains_star(value):
                yield here
            yield from wildcard_principal_paths(value, here)
    elif isinstance(node, list):
        for index, item in enumerate(node):
            yield from wildcard_principal_paths(item, f"{path}[{index}]")


def scan(template):
    """List of (logical id, reason) for every violation in the template's Resources."""
    findings = []
    resources = template.get("Resources", {})
    for logical_id in sorted(resources):
        resource = resources[logical_id]
        rtype = resource.get("Type", "")
        if rtype in DENY_EXACT or rtype.startswith(DENY_PREFIXES):
            findings.append((logical_id, f"denylisted ingress type {rtype}"))
        elif rtype not in ALLOWED_TYPES:
            findings.append((logical_id, f"type {rtype} is not in the closed allowlist"))
        for path in wildcard_principal_paths(resource):
            findings.append((logical_id, f"wildcard principal at {path}"))
    return findings


def selftest(template_path):
    with open(template_path, encoding="utf-8") as handle:
        base = json.load(handle)

    def plant(logical_id, resource):
        planted = copy.deepcopy(base)
        planted["Resources"][logical_id] = resource
        return planted

    must_fail = [
        ("added Lambda::Url", plant("PlantUrl", {
            "Type": "AWS::Lambda::Url",
            "Properties": {"TargetFunctionArn": "EXAMPLE", "AuthType": "NONE"},
        })),
        ("added Lambda::Permission with Principal *", plant("PlantPermStar", {
            "Type": "AWS::Lambda::Permission",
            "Properties": {"Action": "lambda:InvokeFunction", "FunctionName": "EXAMPLE", "Principal": "*"},
        })),
        ("added Lambda::Permission with a named principal", plant("PlantPermNamed", {
            "Type": "AWS::Lambda::Permission",
            "Properties": {"Action": "lambda:InvokeFunction", "FunctionName": "EXAMPLE",
                           "Principal": "arn:aws:iam::EXAMPLE:role/caller"},
        })),
        ("added Lambda::Permission with the API Gateway service principal", plant("PlantPermApigw", {
            "Type": "AWS::Lambda::Permission",
            "Properties": {"Action": "lambda:InvokeFunction", "FunctionName": "EXAMPLE",
                           "Principal": "apigateway.amazonaws.com"},
        })),
        ("added ApiGatewayV2::Api", plant("PlantApiV2", {
            "Type": "AWS::ApiGatewayV2::Api",
            "Properties": {"Name": "EXAMPLE", "ProtocolType": "HTTP"},
        })),
        ("added ElasticLoadBalancingV2::LoadBalancer", plant("PlantLb", {
            "Type": "AWS::ElasticLoadBalancingV2::LoadBalancer",
            "Properties": {"Name": "EXAMPLE", "Scheme": "internet-facing"},
        })),
        ("IAM policy with Principal AWS * three levels deep", plant("PlantNestedStar", {
            "Type": "AWS::IAM::Role",
            "Properties": {"Policy": {"Doc": {"Statement": [{"Effect": "Allow", "Principal": {"AWS": "*"}}]}}},
        })),
        ("unknown type AWS::Foo::Bar", plant("PlantUnknown", {
            "Type": "AWS::Foo::Bar",
            "Properties": {},
        })),
    ]
    must_pass = [
        ("the clean template", base),
        ("role trust principal lambda.amazonaws.com", plant("PlantTrustRole", {
            "Type": "AWS::IAM::Role",
            "Properties": {"AssumeRolePolicyDocument": {"Statement": [
                {"Effect": "Allow", "Principal": {"Service": "lambda.amazonaws.com"}, "Action": "sts:AssumeRole"},
            ]}},
        })),
    ]

    cases = 0
    for name, planted in must_fail:
        if not scan(planted):
            print(f"INGRESS SELFTEST FAILED: {name} was NOT refused", file=sys.stderr)
            return 1
        cases += 1
    for name, planted in must_pass:
        findings = scan(planted)
        if findings:
            print(f"INGRESS SELFTEST FAILED: {name} was refused: {findings}", file=sys.stderr)
            return 1
        cases += 1
    print(f"INGRESS SELFTEST OK {cases} must-fail {len(must_fail)} must-pass {len(must_pass)}")
    return 0


def main(argv):
    if len(argv) == 3 and argv[1] == "--selftest":
        return selftest(argv[2])
    if len(argv) != 2:
        print("usage: ingress_scan.py [--selftest] <template.json>", file=sys.stderr)
        return 2
    with open(argv[1], encoding="utf-8") as handle:
        template = json.load(handle)
    findings = scan(template)
    for logical_id, reason in findings:
        print(f"INGRESS VIOLATION {logical_id} {reason}")
    if findings:
        return 1
    print(f"INGRESS SCAN CLEAN {len(template.get('Resources', {}))} resources")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
