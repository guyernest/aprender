#!/usr/bin/env bash
# Launch ONE reachable GPU host (g5.xlarge, A10G) serving a model over HTTP to a proxy Lambda in the
# same VPC, and create the proxy. Mutates AWS (IAM, two security groups, an instance, a Lambda):
# run it yourself. Usage: up.sh [profile]
#
#   MODEL_ID    default qwen35-4b-q4km  (the capability-ladder rung; the GGUF sha256 is read from the ladder)
#   VARIANT     default gpu-4b          (proxy function: aprender-llm-endpoint-dev-<VARIANT>)
#   CAP_MINUTES default 180             (the host powers off and terminates itself)
#   PROXY_SG    optional: use the platform's own proxy security group instead of creating one
#
# Not managed by `just llm-endpoint-*` (those recipes know the CPU Lambda variants only); teardown is
# gpu-host/down.sh. Writes deploy/llm-endpoint/.ids/gpu-host/host.json.
set -euo pipefail
PROFILE="${1:-ze-kasher-dev}"; REGION=us-east-1
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; ROOT="$HERE/../../.."
IDS="$HERE/../.ids/gpu-host"; mkdir -p "$IDS"
AWS=(aws --profile "$PROFILE" --region "$REGION")
[ ! -e "$IDS/host.json" ] || { echo "REFUSED state: $IDS/host.json exists, run down.sh first" >&2; exit 2; }
MODEL_ID="${MODEL_ID:-qwen35-4b-q4km}"; VARIANT="${VARIANT:-gpu-4b}"; CAP_MINUTES="${CAP_MINUTES:-180}"; PORT=8080
case "$MODEL_ID" in qwen35-4b-q4km) MODEL_FILE=Qwen3.5-4B-Q4_K_M.gguf; HF_REPO=unsloth/Qwen3.5-4B-GGUF ;;
  *) echo "REFUSED model: $MODEL_ID has no Hugging Face source wired here (only qwen35-4b-q4km)" >&2; exit 2 ;; esac
FN="aprender-llm-endpoint-dev-$VARIANT"
printf '%s' "$VARIANT" | grep -Eq '^[a-z0-9]([a-z0-9-]{0,12}[a-z0-9])?$' || { echo "REFUSED variant: $VARIANT" >&2; exit 2; }
case "$VARIANT" in codebuild|*-codebuild) echo "REFUSED variant: $VARIANT collides with a CodeBuild role name" >&2; exit 2 ;; esac
MODEL_SHA256=$(python3 -I "$HERE/../ladder_pin.py" "$ROOT/contracts/model-capability-ladder-v1.yaml" "$MODEL_ID")
APR_URL=https://github.com/paiml/aprender/releases/download/v0.71.4/apr-v0.71.4-x86_64-unknown-linux-gnu-cuda.tar.gz
APR_TARBALL_SHA256=03015a46b4c1a9c27e8070f9ed7293bfbc06e8c4ba3c54427d41caf2d26704c8
MODEL_URL="https://huggingface.co/$HF_REPO/resolve/main/$MODEL_FILE"
HROLE=aprender-gpu-host; HSG=aprender-gpu-host; PSG_NAME=aprender-gpu-proxy; PROLE="$FN"
TAG='{Key=Project,Value=aprender-gpu-host}'
if "${AWS[@]}" lambda get-function --function-name "$FN" >/dev/null 2>&1 || "${AWS[@]}" iam get-role --role-name "$FN" >/dev/null 2>&1; then
  echo "REFUSED name: function or role $FN already exists (not created by this recipe); pick another VARIANT, or run down.sh if it is a leftover of this recipe" >&2; exit 2
fi
VPC=$("${AWS[@]}" ec2 describe-vpcs --filters Name=is-default,Values=true --query 'Vpcs[0].VpcId' --output text)
sg_id() { "${AWS[@]}" ec2 describe-security-groups --filters Name=group-name,Values="$1" Name=vpc-id,Values="$VPC" --query 'SecurityGroups[0].GroupId' --output text; }
# Host role: SSM core only (no S3: the model comes from Hugging Face, verified by sha256).
if ! "${AWS[@]}" iam get-role --role-name $HROLE >/dev/null 2>&1; then
  "${AWS[@]}" iam create-role --role-name $HROLE --tags Key=Project,Value=aprender-gpu-host --assume-role-policy-document '{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"Service":"ec2.amazonaws.com"},"Action":"sts:AssumeRole"}]}' >/dev/null
  "${AWS[@]}" iam attach-role-policy --role-name $HROLE --policy-arn arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore
  "${AWS[@]}" iam create-instance-profile --instance-profile-name $HROLE --tags Key=Project,Value=aprender-gpu-host >/dev/null
  "${AWS[@]}" iam add-role-to-instance-profile --instance-profile-name $HROLE --role-name $HROLE
  sleep 15
fi
# Security groups. The proxy's group has no ingress. The host's group admits TCP $PORT from the proxy's group and nothing else.
PSG="${PROXY_SG:-$(sg_id $PSG_NAME)}"
if [ "$PSG" = None ]; then
  PSG=$("${AWS[@]}" ec2 create-security-group --group-name $PSG_NAME --vpc-id "$VPC" --description "aprender gpu proxy: no ingress" --tag-specifications "ResourceType=security-group,Tags=[$TAG]" --query GroupId --output text)
fi
HSGID=$(sg_id $HSG)
if [ "$HSGID" = None ]; then
  HSGID=$("${AWS[@]}" ec2 create-security-group --group-name $HSG --vpc-id "$VPC" --description "aprender gpu host: ingress from the proxy group only" --tag-specifications "ResourceType=security-group,Tags=[$TAG]" --query GroupId --output text)
  "${AWS[@]}" ec2 authorize-security-group-ingress --group-id "$HSGID" --ip-permissions "IpProtocol=tcp,FromPort=$PORT,ToPort=$PORT,UserIdGroupPairs=[{GroupId=$PSG}]" >/dev/null
fi
# Refuse unless the host group's ONLY ingress is TCP $PORT from one security group and no CIDR.
[ "$("${AWS[@]}" ec2 describe-security-groups --group-ids "$HSGID" --query "length(SecurityGroups[0].IpPermissions[?FromPort==\`$PORT\` && length(UserIdGroupPairs)==\`1\` && length(IpRanges)==\`0\` && length(Ipv6Ranges)==\`0\`]) == length(SecurityGroups[0].IpPermissions)" --output text)" = True ] \
  || { echo "REFUSED sg: $HSGID has ingress other than $PORT from one security group" >&2; exit 2; }
AMI=$("${AWS[@]}" ssm get-parameter --name /aws/service/deeplearning/ami/x86_64/base-oss-nvidia-driver-gpu-amazon-linux-2023/latest/ami-id --query Parameter.Value --output text)
{ printf '#!/bin/bash\n'; for V in APR_URL APR_TARBALL_SHA256 MODEL_URL MODEL_SHA256 PORT CAP_MINUTES; do printf '%s=%q\n' "$V" "${!V}"; done
  tail -n +2 "$HERE/userdata.sh"; } > "$IDS/userdata.rendered"
IID=""
for AZ in ${GPU_AZS:-us-east-1b us-east-1c us-east-1d us-east-1f us-east-1a}; do
  SUBNET=$("${AWS[@]}" ec2 describe-subnets --filters Name=default-for-az,Values=true Name=availability-zone,Values="$AZ" --query 'Subnets[0].SubnetId' --output text)
  [ "$SUBNET" != None ] || continue
  if OUT=$("${AWS[@]}" ec2 run-instances --image-id "$AMI" --instance-type g5.xlarge --subnet-id "$SUBNET" --security-group-ids "$HSGID" \
    --iam-instance-profile Name=$HROLE --user-data "file://$IDS/userdata.rendered" \
    --metadata-options HttpTokens=required,HttpEndpoint=enabled,HttpPutResponseHopLimit=1 --instance-initiated-shutdown-behavior terminate \
    --block-device-mappings 'DeviceName=/dev/xvda,Ebs={VolumeSize=120,VolumeType=gp3,Encrypted=true,DeleteOnTermination=true}' \
    --tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=aprender-gpu-host},$TAG]" "ResourceType=volume,Tags=[$TAG]" \
    --query 'Instances[0].[InstanceId,PrivateIpAddress]' --output text 2>&1); then
    IID=${OUT%%[[:space:]]*}; HIP=${OUT##*[[:space:]]}; echo "launched in $AZ"; break
  fi
  case "$OUT" in *InsufficientInstanceCapacity*) echo "no g5.xlarge capacity in $AZ, trying the next" >&2 ;; *) echo "$OUT" >&2; exit 1 ;; esac
done
[ -n "$IID" ] || { echo "REFUSED capacity: no AZ had g5.xlarge; role/groups remain, run down.sh to remove them" >&2; exit 3; }
# Proxy Lambda: in the host's subnet, attached to the proxy security group, no Function URL.
if ! "${AWS[@]}" iam get-role --role-name "$PROLE" >/dev/null 2>&1; then
  "${AWS[@]}" iam create-role --role-name "$PROLE" --tags Key=Project,Value=aprender-gpu-host --assume-role-policy-document "file://$HERE/../iam/lambda-trust.json" >/dev/null
  "${AWS[@]}" iam attach-role-policy --role-name "$PROLE" --policy-arn arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole
  sleep 15
fi
ACCT=$("${AWS[@]}" sts get-caller-identity --query Account --output text)
( cd "$HERE/proxy" && python3 -I -c 'import zipfile;z=zipfile.ZipFile("'"$IDS"'/proxy.zip","w",zipfile.ZIP_DEFLATED);z.write("lambda_function.py");z.close()' )
"${AWS[@]}" lambda create-function --function-name "$FN" --runtime python3.12 --architectures arm64 --handler lambda_function.handler \
  --role "arn:aws:iam::$ACCT:role/$PROLE" --zip-file "fileb://$IDS/proxy.zip" --timeout 300 --memory-size 256 \
  --vpc-config "SubnetIds=$SUBNET,SecurityGroupIds=$PSG" --environment "Variables={HOST_URL=http://$HIP:$PORT}" \
  --tags Project=aprender-gpu-host --query '[FunctionName,State]' --output text
printf '{"instance_id":"%s","private_ip":"%s","port":%s,"model_id":"%s","proxy_function":"%s","proxy_sg":"%s","host_sg":"%s","subnet":"%s","ami":"%s","cap_minutes":%s,"launched":"%s"}\n' \
  "$IID" "$HIP" "$PORT" "$MODEL_ID" "$FN" "$PSG" "$HSGID" "$SUBNET" "$AMI" "$CAP_MINUTES" "$(date -u +%FT%TZ)" | tee "$IDS/host.json"
echo "next: wait for setup (model pull ~2.7 GB), then: gpu-host/check.sh $PROFILE ; invoke $FN ; finally down.sh"
