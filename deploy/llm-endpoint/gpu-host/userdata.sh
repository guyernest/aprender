#!/bin/bash
# Bootstrap for the reachable GPU host. up.sh prepends the pinned values (APR_URL, APR_TARBALL_SHA256,
# MODEL_URL, MODEL_SHA256, PORT, CAP_MINUTES) as KEY=value lines above this file. Runs as root.
# Any failed verification leaves /var/lib/gpu-host-REFUSED and the server is never started.
set -uo pipefail
exec > >(tee -a /var/log/gpu-host-setup.log) 2>&1
echo "[setup] start $(date -u +%FT%TZ)"
shutdown -h "+${CAP_MINUTES}" "gpu-host cost cap" || echo "[setup] WARN: could not arm shutdown"
refuse() { echo "[setup] REFUSED $1: $2" >&2; touch /var/lib/gpu-host-REFUSED; exit 2; }
mkdir -p /opt/apr /opt/models; cd /opt/apr || exit 1
curl -fsSL -o apr.tgz "$APR_URL" || refuse download "$APR_URL"
printf '%s  apr.tgz\n' "$APR_TARBALL_SHA256" | sha256sum -c - || refuse apr-sha "tarball sha256 mismatch"
tar -xzf apr.tgz
APR_BIN=$(find /opt/apr -type f -name apr | head -1)
[ -x "$APR_BIN" ] || refuse apr-bin "no apr binary in the tarball"
ln -sf "$APR_BIN" /usr/local/bin/apr
curl -fsSL --retry 5 --retry-delay 5 -o /opt/models/model.gguf "$MODEL_URL" || refuse download "$MODEL_URL"
printf '%s  /opt/models/model.gguf\n' "$MODEL_SHA256" | sha256sum -c - || refuse model-sha "GGUF sha256 mismatch"
nvidia-smi --query-gpu=name,compute_cap --format=csv,noheader || refuse gpu "nvidia-smi failed"
TOKEN=$(curl -fsS -X PUT -H 'X-aws-ec2-metadata-token-ttl-seconds: 60' http://169.254.169.254/latest/api/token) || refuse imds "no token"
IP=$(curl -fsS -H "X-aws-ec2-metadata-token: $TOKEN" http://169.254.169.254/latest/meta-data/local-ipv4) || refuse imds "no private ip"
# Bind the private address only. The CUDA-path routers ignore APR_API_KEY_HASH (#5478); the security
# group (ingress from the proxy's group only) is what restricts callers.
cat > /etc/systemd/system/apr-serve.service <<UNIT
[Unit]
Description=apr serve (CUDA)
After=network-online.target
[Service]
ExecStart=/usr/local/bin/apr serve run /opt/models/model.gguf --host $IP --port $PORT --backend cuda
Restart=on-failure
RestartSec=5
[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload && systemctl enable --now apr-serve || refuse systemd "could not start apr-serve"
for _ in $(seq 1 240); do curl -s -o /dev/null "http://$IP:$PORT/health" && break; sleep 1; done
curl -s -o /dev/null "http://$IP:$PORT/health" || refuse health "server did not answer /health in 240 s"
# Warm-up: the first request after a fresh start paid ~14 s of one-time kernel compilation on the probe
# host. Pay it here so the first measured call is not an outlier. Not a measurement.
W0=$(date +%s.%N)
curl -s -o /dev/null -H 'content-type: application/json' -d '{"model":"m","max_tokens":4,"messages":[{"role":"user","content":"hi"}]}' "http://$IP:$PORT/v1/chat/completions"
echo "[setup] warmup_s=$(awk -v a="$(date +%s.%N)" -v b="$W0" 'BEGIN{printf "%.1f", a-b}')"
touch /var/lib/gpu-host-READY
echo "[setup] READY $IP:$PORT $(date -u +%FT%TZ)"
