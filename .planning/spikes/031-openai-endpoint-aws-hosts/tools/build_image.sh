#!/bin/bash
# Build and push the one image: extract the sha256-verified release binary from bin/ into deploy/
# (deploy/apr is gitignored — 57 MB), then buildx for linux/arm64 and push to the spike's ECR repo.
#   bash tools/build_image.sh            # tag cpu-v0.70.3
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
P="${AWS_PROFILE_NAME:-ze-kasher-dev}"; R=us-east-1
ECR=$(python3 -c "import json;print(json.load(open('results/aws-ids.json'))['ecr'])") || exit 1
TAR=bin/apr-v0.70.3-aarch64-unknown-linux-gnu-cpu.tar.gz
(cd bin && shasum -a 256 -c apr-v0.70.3-aarch64-unknown-linux-gnu-cpu.tar.gz.sha256) || exit 1
tar xzf "$TAR" -C deploy --strip-components=1 apr-v0.70.3-aarch64-unknown-linux-gnu-cpu/apr || exit 1
aws ecr get-login-password --profile "$P" --region "$R" | docker login --username AWS --password-stdin "${ECR%%/*}" || exit 1
docker buildx build --platform linux/arm64 --provenance=false -t "$ECR:cpu-v0.70.3" --push deploy
