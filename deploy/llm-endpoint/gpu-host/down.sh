#!/usr/bin/env bash
# Remove everything up.sh created, found by the Project tag and fixed names, and prove each is gone.
# Usage: down.sh [profile]. Mutates AWS: run it yourself. Idempotent.
set -uo pipefail
PROFILE="${1:-ze-kasher-dev}"; REGION=us-east-1
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; IDS="$HERE/../.ids/gpu-host"
AWS=(aws --profile "$PROFILE" --region "$REGION"); HROLE=aprender-gpu-host
FN=$(python3 -I -c 'import json,sys;print(json.load(open(sys.argv[1]))["proxy_function"])' "$IDS/host.json" 2>/dev/null || echo "${FN:-aprender-llm-endpoint-dev-gpu-4b}")
"${AWS[@]}" lambda delete-function --function-name "$FN" 2>/dev/null
IIDS=$("${AWS[@]}" ec2 describe-instances --filters Name=tag:Project,Values=aprender-gpu-host Name=instance-state-name,Values=pending,running,stopping,stopped --query 'Reservations[].Instances[].InstanceId' --output text)
if [ -n "$IIDS" ]; then
  # shellcheck disable=SC2086
  "${AWS[@]}" ec2 terminate-instances --instance-ids $IIDS >/dev/null
  # shellcheck disable=SC2086
  "${AWS[@]}" ec2 wait instance-terminated --instance-ids $IIDS
fi
# The Lambda's VPC network interfaces take minutes to release; retry the group deletes.
for G in aprender-gpu-host aprender-gpu-proxy; do
  for _ in $(seq 1 20); do
    ID=$("${AWS[@]}" ec2 describe-security-groups --filters Name=group-name,Values=$G --query 'SecurityGroups[0].GroupId' --output text)
    [ "$ID" = None ] && break
    "${AWS[@]}" ec2 delete-security-group --group-id "$ID" 2>/dev/null && break
    sleep 15
  done
done
"${AWS[@]}" iam remove-role-from-instance-profile --instance-profile-name $HROLE --role-name $HROLE 2>/dev/null
"${AWS[@]}" iam delete-instance-profile --instance-profile-name $HROLE 2>/dev/null
"${AWS[@]}" iam detach-role-policy --role-name $HROLE --policy-arn arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore 2>/dev/null
"${AWS[@]}" iam delete-role --role-name $HROLE 2>/dev/null
"${AWS[@]}" iam detach-role-policy --role-name "$FN" --policy-arn arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole 2>/dev/null
"${AWS[@]}" iam delete-role --role-name "$FN" 2>/dev/null
rm -f "$IDS/host.json" "$IDS/userdata.rendered" "$IDS/proxy.zip"
echo "== verify gone (want 0 0 0 1 1 1 1)"
"${AWS[@]}" ec2 describe-instances --filters Name=tag:Project,Values=aprender-gpu-host Name=instance-state-name,Values=pending,running,stopping,stopped --query 'length(Reservations)' --output text
"${AWS[@]}" ec2 describe-security-groups --filters 'Name=group-name,Values=aprender-gpu-host,aprender-gpu-proxy' --query 'length(SecurityGroups)' --output text
"${AWS[@]}" ec2 describe-volumes --filters Name=tag:Project,Values=aprender-gpu-host --query 'length(Volumes)' --output text
"${AWS[@]}" lambda get-function --function-name "$FN" 2>&1 | grep -c ResourceNotFound
"${AWS[@]}" iam get-role --role-name $HROLE 2>&1 | grep -c NoSuchEntity
"${AWS[@]}" iam get-role --role-name "$FN" 2>&1 | grep -c NoSuchEntity
"${AWS[@]}" iam get-instance-profile --instance-profile-name $HROLE 2>&1 | grep -c NoSuchEntity
