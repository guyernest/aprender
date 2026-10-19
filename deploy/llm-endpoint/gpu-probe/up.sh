#!/usr/bin/env bash
# Launch ONE private GPU probe instance (g5.xlarge, A10G) for the aprender CUDA build. NOT an endpoint:
# no Function URL, no load balancer, no ingress rule at all, server bound to 127.0.0.1, reached only
# through SSM. Mutates AWS (IAM role + profile, security group, one instance): run it yourself.
# Usage: up.sh [profile]. Writes deploy/llm-endpoint/.ids/gpu-probe/instance.json.
set -euo pipefail
PROFILE="${1:-ze-kasher-dev}"; REGION=us-east-1
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IDS="$HERE/../.ids/gpu-probe"; mkdir -p "$IDS"
AWS=(aws --profile "$PROFILE" --region "$REGION")
[ ! -e "$IDS/instance.json" ] || { echo "REFUSED state: $IDS/instance.json exists, run down.sh first" >&2; exit 2; }
ACCT=$("${AWS[@]}" sts get-caller-identity --query Account --output text)
# Pins: the release's own sha256 (fetched and checked in by the author), and the ladder GGUF shas.
APR_URL=https://github.com/paiml/aprender/releases/download/v0.71.4/apr-v0.71.4-x86_64-unknown-linux-gnu-cuda.tar.gz
APR_TARBALL_SHA256=03015a46b4c1a9c27e8070f9ed7293bfbc06e8c4ba3c54427d41caf2d26704c8
W08_SHA256=bd258782e35f7f458f8aced1adc053e6e92e89bc735ba3be89d38a06121dc517
W2B_SHA256=aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223
W08_URI="s3://aprender-llm-endpoint-weights-$ACCT-dev/Qwen3.5-0.8B-Q4_K_M.gguf"
W2B_URI="s3://aprender-llm-endpoint-weights-$ACCT-dev-2b/Qwen3.5-2B-Q4_K_M.gguf"
CAP_MINUTES="${CAP_MINUTES:-150}"
TYPE="${GPU_TYPE:-g5.xlarge}"
ROLE=aprender-gpu-probe; SG_NAME=aprender-gpu-probe
TAGS_RES='{Key=Project,Value=aprender-gpu-probe}'
# Both objects must exist before anything is created.
for U in "$W08_URI" "$W2B_URI"; do
  B=${U#s3://}; B=${B%%/*}; K=${U#s3://*/}
  "${AWS[@]}" s3api head-object --bucket "$B" --key "$K" >/dev/null || { echo "REFUSED weights: $U absent" >&2; exit 2; }
done
# IAM: SSM core + read on the two weights buckets, nothing else.
if ! "${AWS[@]}" iam get-role --role-name "$ROLE" >/dev/null 2>&1; then
  "${AWS[@]}" iam create-role --role-name "$ROLE" --tags Key=Project,Value=aprender-gpu-probe \
    --assume-role-policy-document '{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"Service":"ec2.amazonaws.com"},"Action":"sts:AssumeRole"}]}' >/dev/null
  "${AWS[@]}" iam attach-role-policy --role-name "$ROLE" --policy-arn arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore
  "${AWS[@]}" iam put-role-policy --role-name "$ROLE" --policy-name weights-read --policy-document "{\"Version\":\"2012-10-17\",\"Statement\":[{\"Effect\":\"Allow\",\"Action\":[\"s3:GetObject\"],\"Resource\":[\"arn:aws:s3:::aprender-llm-endpoint-weights-$ACCT-dev/*\",\"arn:aws:s3:::aprender-llm-endpoint-weights-$ACCT-dev-2b/*\"]}]}"
  "${AWS[@]}" iam create-instance-profile --instance-profile-name "$ROLE" --tags Key=Project,Value=aprender-gpu-probe >/dev/null
  "${AWS[@]}" iam add-role-to-instance-profile --instance-profile-name "$ROLE" --role-name "$ROLE"
  sleep 15   # IAM propagation before run-instances can use the profile
fi
# Security group: created with NO ingress rule. (A new SG has only the default allow-all egress.)
VPC=$("${AWS[@]}" ec2 describe-vpcs --filters Name=is-default,Values=true --query 'Vpcs[0].VpcId' --output text)
SG=$("${AWS[@]}" ec2 describe-security-groups --filters Name=group-name,Values=$SG_NAME Name=vpc-id,Values="$VPC" --query 'SecurityGroups[0].GroupId' --output text)
if [ "$SG" = None ]; then
  SG=$("${AWS[@]}" ec2 create-security-group --group-name $SG_NAME --vpc-id "$VPC" --description "aprender gpu probe: no ingress" \
    --tag-specifications "ResourceType=security-group,Tags=[$TAGS_RES]" --query GroupId --output text)
fi
[ "$("${AWS[@]}" ec2 describe-security-groups --group-ids "$SG" --query 'length(SecurityGroups[0].IpPermissions)' --output text)" = 0 ] \
  || { echo "REFUSED sg: $SG has ingress rules" >&2; exit 2; }
SUBNET=$("${AWS[@]}" ec2 describe-subnets --filters Name=default-for-az,Values=true Name=availability-zone,Values=us-east-1a --query 'Subnets[0].SubnetId' --output text)
case "$TYPE" in g5.xlarge|g4dn.xlarge) ;; *) echo "REFUSED type: $TYPE not in the 8-vCPU quota plan (g5.xlarge, g4dn.xlarge)" >&2; exit 2 ;; esac
AMI=$("${AWS[@]}" ssm get-parameter --name /aws/service/deeplearning/ami/x86_64/base-oss-nvidia-driver-gpu-amazon-linux-2023/latest/ami-id --query Parameter.Value --output text)
{ printf '#!/bin/bash\n'; for V in APR_URL APR_TARBALL_SHA256 W08_URI W08_SHA256 W2B_URI W2B_SHA256 CAP_MINUTES; do printf '%s=%q\n' "$V" "${!V}"; done
  tail -n +2 "$HERE/userdata.sh"; } > "$IDS/userdata.rendered"
IID=$("${AWS[@]}" ec2 run-instances --image-id "$AMI" --instance-type "$TYPE" --subnet-id "$SUBNET" --security-group-ids "$SG" \
  --iam-instance-profile Name=$ROLE --user-data "file://$IDS/userdata.rendered" \
  --metadata-options HttpTokens=required,HttpEndpoint=enabled,HttpPutResponseHopLimit=1 \
  --instance-initiated-shutdown-behavior terminate \
  --block-device-mappings 'DeviceName=/dev/xvda,Ebs={VolumeSize=120,VolumeType=gp3,Encrypted=true,DeleteOnTermination=true}' \
  --tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=aprender-gpu-probe},$TAGS_RES]" "ResourceType=volume,Tags=[$TAGS_RES]" \
  --query 'Instances[0].InstanceId' --output text)
printf '{"instance_id":"%s","type":"%s","ami":"%s","sg":"%s","subnet":"%s","cap_minutes":%s,"launched":"%s"}\n' \
  "$IID" "$TYPE" "$AMI" "$SG" "$SUBNET" "$CAP_MINUTES" "$(date -u +%FT%TZ)" | tee "$IDS/instance.json"
echo "next: wait ~5 min for setup, then: deploy/llm-endpoint/gpu-probe/probe.sh $PROFILE ; finally down.sh"
