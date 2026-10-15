#!/bin/bash
# Create every shared resource spike 031 needs (idempotent, all tagged spike=031, us-east-1):
# S3 bucket for weights, ECR repo, IAM roles (lambda, ecs-exec, ecs-task, lmi-operator), security
# group (8080 from this client IP only), log groups, ECS cluster. Prints the ids tools/*.py read.
set -uo pipefail
P="${AWS_PROFILE_NAME:-ze-kasher-dev}"; R=us-east-1
A=$(aws sts get-caller-identity --profile "$P" --query Account --output text) || exit 1
N=llm-spike-031; B="$N-$A"
echo "account=$A bucket=$B"
TAGS='[{"Key":"spike","Value":"031"}]'

aws s3api head-bucket --bucket "$B" --profile "$P" 2>/dev/null || aws s3api create-bucket --bucket "$B" --profile "$P" --region "$R" >/dev/null
aws s3api put-bucket-tagging --bucket "$B" --profile "$P" --tagging "{\"TagSet\":$TAGS}" >/dev/null
aws ecr describe-repositories --repository-names "$N" --profile "$P" --region "$R" >/dev/null 2>&1 || \
  aws ecr create-repository --repository-name "$N" --profile "$P" --region "$R" --tags "Key=spike,Value=031" >/dev/null

mkrole() { # name trust-service [managed-policy-arn...]
  local name="$1" svc="$2"; shift 2
  aws iam get-role --role-name "$name" --profile "$P" >/dev/null 2>&1 || \
    aws iam create-role --role-name "$name" --profile "$P" --tags "Key=spike,Value=031" \
      --assume-role-policy-document "{\"Version\":\"2012-10-17\",\"Statement\":[{\"Effect\":\"Allow\",\"Principal\":{\"Service\":\"$svc\"},\"Action\":\"sts:AssumeRole\"}]}" >/dev/null
  for pol in "$@"; do aws iam attach-role-policy --role-name "$name" --policy-arn "$pol" --profile "$P"; done
}
S3READ="{\"Version\":\"2012-10-17\",\"Statement\":[{\"Effect\":\"Allow\",\"Action\":[\"s3:GetObject\"],\"Resource\":\"arn:aws:s3:::$B/*\"},{\"Effect\":\"Allow\",\"Action\":[\"s3:ListBucket\"],\"Resource\":\"arn:aws:s3:::$B\"}]}"
mkrole "$N-lambda" lambda.amazonaws.com arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole arn:aws:iam::aws:policy/service-role/AWSLambdaVPCAccessExecutionRole
aws iam put-role-policy --role-name "$N-lambda" --policy-name s3read --policy-document "$S3READ" --profile "$P"
mkrole "$N-ecs-exec" ecs-tasks.amazonaws.com arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy
mkrole "$N-ecs-task" ecs-tasks.amazonaws.com
aws iam put-role-policy --role-name "$N-ecs-task" --policy-name s3read --policy-document "$S3READ" --profile "$P"
mkrole "$N-lmi-operator" lambda.amazonaws.com arn:aws:iam::aws:policy/AWSLambdaManagedEC2ResourceOperator

VPC=$(aws ec2 describe-vpcs --profile "$P" --region "$R" --filters Name=isDefault,Values=true --query 'Vpcs[0].VpcId' --output text)
MYIP=$(curl -s -m 10 https://checkip.amazonaws.com | tr -d '\n')
SG=$(aws ec2 describe-security-groups --profile "$P" --region "$R" --filters Name=group-name,Values="$N" Name=vpc-id,Values="$VPC" --query 'SecurityGroups[0].GroupId' --output text 2>/dev/null)
if [ -z "$SG" ] || [ "$SG" = "None" ]; then
  SG=$(aws ec2 create-security-group --profile "$P" --region "$R" --group-name "$N" --description "spike 031 llm endpoint" --vpc-id "$VPC" \
        --tag-specifications "ResourceType=security-group,Tags=[{Key=spike,Value=031}]" --query GroupId --output text)
fi
aws ec2 authorize-security-group-ingress --profile "$P" --region "$R" --group-id "$SG" --protocol tcp --port 8080 --cidr "$MYIP/32" >/dev/null 2>&1 || true
# LMI environments and Fargate tasks talk to S3 + ECR over the VPC's internet path; egress is open by default.

for g in "/ecs/$N" "/aws/lambda/$N-lambda-0-8b" "/aws/lambda/$N-lambda-2b" "/aws/lambda/$N-lmi-4b"; do
  aws logs create-log-group --log-group-name "$g" --profile "$P" --region "$R" 2>/dev/null || true
  aws logs put-retention-policy --log-group-name "$g" --retention-in-days 7 --profile "$P" --region "$R" 2>/dev/null || true
done
aws ecs describe-clusters --clusters "$N" --profile "$P" --region "$R" --query 'clusters[?status==`ACTIVE`].clusterName' --output text | grep -q "$N" || \
  aws ecs create-cluster --cluster-name "$N" --profile "$P" --region "$R" --tags key=spike,value=031 >/dev/null

SUBNETS=$(aws ec2 describe-subnets --profile "$P" --region "$R" --filters Name=vpc-id,Values="$VPC" Name=default-for-az,Values=true \
          --query 'Subnets[?AvailabilityZone!=`us-east-1c` && AvailabilityZone!=`us-east-1e`].SubnetId' --output text | tr '\t' ',')
cat > results/aws-ids.json <<EOF
{"account":"$A","region":"$R","profile":"$P","name":"$N","bucket":"$B","ecr":"$A.dkr.ecr.$R.amazonaws.com/$N","vpc":"$VPC","sg":"$SG","my_ip":"$MYIP","subnets":"$SUBNETS",
 "roles":{"lambda":"arn:aws:iam::$A:role/$N-lambda","ecs_exec":"arn:aws:iam::$A:role/$N-ecs-exec","ecs_task":"arn:aws:iam::$A:role/$N-ecs-task","lmi_operator":"arn:aws:iam::$A:role/$N-lmi-operator"}}
EOF
cat results/aws-ids.json
echo "AWS_UP DONE"
