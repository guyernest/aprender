#!/bin/bash
# Remove every resource spike 031 created (all tagged spike=031, us-east-1). Idempotent.
#   bash tools/teardown.sh            # everything, including the weights bucket
#   KEEP_BUCKET=1 bash tools/teardown.sh
set -uo pipefail
P="${AWS_PROFILE_NAME:-ze-kasher-dev}"; R=us-east-1; N=llm-spike-031
A=$(aws sts get-caller-identity --profile "$P" --query Account --output text) || exit 1
B="$N-$A"
aws() { command aws --profile "$P" --region "$R" "$@"; }

# LMI first (it bills while up): scale every version to 0, delete the function, then the provider.
for fn in $(aws lambda list-functions --query "Functions[?starts_with(FunctionName, '$N-lmi')].FunctionName" --output text); do
  for v in $(aws lambda list-versions-by-function --function-name "$fn" --query "Versions[?Version!='\$LATEST'].Version" --output text); do
    aws lambda put-function-scaling-config --function-name "$fn" --qualifier "$v" --function-scaling-config MinExecutionEnvironments=0,MaxExecutionEnvironments=0 >/dev/null 2>&1 || true
  done
  aws lambda delete-function --function-name "$fn" && echo "deleted $fn"
done
for _ in $(seq 1 60); do
  if aws lambda delete-capacity-provider --capacity-provider-name "$N-cp" >/dev/null 2>&1; then echo "deleted capacity provider $N-cp"; break; fi
  aws lambda get-capacity-provider --capacity-provider-name "$N-cp" >/dev/null 2>&1 || break
  sleep 15
done

for fn in $(aws lambda list-functions --query "Functions[?starts_with(FunctionName, '$N-lambda')].FunctionName" --output text); do
  aws lambda delete-function --function-name "$fn" && echo "deleted $fn"
done
for t in $(aws ecs list-tasks --cluster "$N" --query 'taskArns' --output text 2>/dev/null); do aws ecs stop-task --cluster "$N" --task "$t" --reason teardown >/dev/null && echo "stopped $t"; done
for td in $(aws ecs list-task-definitions --family-prefix "$N" --query 'taskDefinitionArns' --output text 2>/dev/null); do aws ecs deregister-task-definition --task-definition "$td" >/dev/null && echo "deregistered $td"; done
aws ecs delete-cluster --cluster "$N" >/dev/null 2>&1 && echo "deleted cluster $N"
aws ecr delete-repository --repository-name "$N" --force >/dev/null 2>&1 && echo "deleted ecr $N"
if [ -z "${KEEP_BUCKET:-}" ]; then aws s3 rm "s3://$B" --recursive >/dev/null 2>&1; aws s3api delete-bucket --bucket "$B" 2>/dev/null && echo "deleted bucket $B"; fi
SG=$(aws ec2 describe-security-groups --filters Name=group-name,Values="$N" --query 'SecurityGroups[0].GroupId' --output text 2>/dev/null)
[ -n "$SG" ] && [ "$SG" != "None" ] && { for _ in $(seq 1 20); do aws ec2 delete-security-group --group-id "$SG" >/dev/null 2>&1 && { echo "deleted sg $SG"; break; }; sleep 15; done; }
for g in "/ecs/$N" "/aws/lambda/$N-lambda-0-8b" "/aws/lambda/$N-lambda-2b" "/aws/lambda/$N-lambda-4b" "/aws/lambda/$N-lmi-4b" "/aws/lambda/capacity-provider/$N-cp"; do
  aws logs delete-log-group --log-group-name "$g" >/dev/null 2>&1 && echo "deleted logs $g"
done
aws codebuild delete-project --name "$N-fetch-weights" >/dev/null 2>&1 && echo "deleted codebuild $N-fetch-weights"
aws logs delete-log-group --log-group-name "/aws/codebuild/$N-fetch-weights" >/dev/null 2>&1
for role in "$N-lambda" "$N-ecs-exec" "$N-ecs-task" "$N-lmi-operator" "$N-codebuild"; do
  for p in $(aws iam list-attached-role-policies --role-name "$role" --query 'AttachedPolicies[].PolicyArn' --output text 2>/dev/null); do aws iam detach-role-policy --role-name "$role" --policy-arn "$p"; done
  for p in $(aws iam list-role-policies --role-name "$role" --query 'PolicyNames' --output text 2>/dev/null); do aws iam delete-role-policy --role-name "$role" --policy-name "$p"; done
  aws iam delete-role --role-name "$role" >/dev/null 2>&1 && echo "deleted role $role"
done
echo "managed instances still visible (should be none):"
aws ec2 describe-instances --include-managed-resources --filters Name=instance-state-name,Values=pending,running Name=tag-key,Values=aws:lambda:capacity-provider \
  --query 'Reservations[].Instances[].[InstanceId,InstanceType,State.Name]' --output text 2>/dev/null || echo "(aws-cli lacks --include-managed-resources; use tools/lmi.py census)"
echo "TEARDOWN DONE"
