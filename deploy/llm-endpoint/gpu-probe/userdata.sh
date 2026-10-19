#!/bin/bash
# Instance bootstrap for the GPU probe. up.sh prepends the pinned values (APR_URL, APR_TARBALL_SHA256,
# W08_URI, W08_SHA256, W2B_URI, W2B_SHA256, CAP_MINUTES) as KEY=value lines above this file.
# Runs as root from cloud-init. Every step is logged; any failed verification leaves a REFUSED line
# and a marker file so the probe refuses to start on an unverified binary or model.
set -uo pipefail
exec > >(tee -a /var/log/gpu-probe-setup.log) 2>&1
echo "[setup] start $(date -u +%FT%TZ)"
# Cost cap first: the instance powers off (and terminates, InstanceInitiatedShutdownBehavior) at the cap.
shutdown -h "+${CAP_MINUTES}" "gpu-probe cost cap" || echo "[setup] WARN: could not arm shutdown"
refuse() { echo "[setup] REFUSED $1: $2" >&2; touch /var/lib/gpu-probe-REFUSED; exit 2; }
mkdir -p /opt/apr /opt/models
cd /opt/apr || exit 1
curl -fsSL -o apr.tgz "$APR_URL" || refuse download "$APR_URL"
printf '%s  apr.tgz\n' "$APR_TARBALL_SHA256" | sha256sum -c - || refuse apr-sha "tarball sha256 mismatch"
tar -xzf apr.tgz
APR_BIN=$(find /opt/apr -type f -name apr | head -1)
[ -x "$APR_BIN" ] || refuse apr-bin "no apr binary in the tarball"
ln -sf "$APR_BIN" /usr/local/bin/apr
echo "[setup] apr: $(/usr/local/bin/apr --version 2>&1 | head -1)"
aws s3 cp "$W08_URI" /opt/models/w08.gguf --only-show-errors || refuse s3 "$W08_URI"
aws s3 cp "$W2B_URI" /opt/models/w2b.gguf --only-show-errors || refuse s3 "$W2B_URI"
printf '%s  /opt/models/w08.gguf\n%s  /opt/models/w2b.gguf\n' "$W08_SHA256" "$W2B_SHA256" | sha256sum -c - \
  || refuse model-sha "GGUF sha256 mismatch"
nvidia-smi --query-gpu=name,compute_cap,driver_version,memory.total --format=csv,noheader || refuse gpu "nvidia-smi failed"
touch /var/lib/gpu-probe-READY
echo "[setup] READY $(date -u +%FT%TZ)"
