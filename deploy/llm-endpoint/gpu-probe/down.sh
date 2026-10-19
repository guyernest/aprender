#!/usr/bin/env bash
# Remove EVERYTHING up.sh created, found by the Project tag / fixed names, and prove each is gone.
# Usage: down.sh [profile]. Mutates AWS: run it yourself. Idempotent.
set -uo pipefail
PROFILE="${1:-ze-kasher-dev}"; REGION=us-east-1
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; IDS="$HERE/../.ids/gpu-probe"
AWS=(aws --profile "$PROFILE" --region "$REGION"); ROLE=aprender-gpu-probe
IIDS=$("${AWS[@]}" ec2 describe-instances --filters Name=tag:Project,Values=aprender-gpu-probe Name=instance-state-name,Values=pending,running,stopping,stopped --query 'Reservations[].Instances[].InstanceId' --output text)
if [ -n "$IIDS" ]; then
  # shellcheck disable=SC2086
  "${AWS[@]}" ec2 terminate-instances --instance-ids $IIDS >/dev/null
  # shellcheck disable=SC2086
  "${AWS[@]}" ec2 wait instance-terminated --instance-ids $IIDS
fi
SG=$("${AWS[@]}" ec2 describe-security-groups --filters Name=group-name,Values=$ROLE --query 'SecurityGroups[0].GroupId' --output text)
[ "$SG" = None ] || "${AWS[@]}" ec2 delete-security-group --group-id "$SG"
"${AWS[@]}" iam remove-role-from-instance-profile --instance-profile-name $ROLE --role-name $ROLE 2>/dev/null
"${AWS[@]}" iam delete-instance-profile --instance-profile-name $ROLE 2>/dev/null
"${AWS[@]}" iam delete-role-policy --role-name $ROLE --policy-name weights-read 2>/dev/null
"${AWS[@]}" iam detach-role-policy --role-name $ROLE --policy-arn arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore 2>/dev/null
"${AWS[@]}" iam delete-role --role-name $ROLE 2>/dev/null
rm -f "$IDS/instance.json" "$IDS/userdata.rendered"
echo "== verify gone"
"${AWS[@]}" ec2 describe-instances --filters Name=tag:Project,Values=aprender-gpu-probe Name=instance-state-name,Values=pending,running,stopping,stopped --query 'length(Reservations)' --output text
"${AWS[@]}" ec2 describe-security-groups --filters Name=group-name,Values=$ROLE --query 'length(SecurityGroups)' --output text
"${AWS[@]}" iam get-role --role-name $ROLE 2>&1 | grep -c NoSuchEntity
"${AWS[@]}" iam get-instance-profile --instance-profile-name $ROLE 2>&1 | grep -c NoSuchEntity
echo "(want 0, 0, 1, 1)"
