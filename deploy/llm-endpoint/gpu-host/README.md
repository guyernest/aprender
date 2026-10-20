# GPU host: a reachable A10G behind a proxy Lambda (short-test scope)

STATUS: RUN ONCE on 2026-10-19/20 (us-east-1b); pmcp.run's 72-call test passed. Evidence: `09-GPU-HOST-EVIDENCE.json` in the phase 9 planning directory. Another launch needs the operator's explicit word.

Scope: the short GPU test pmcp.run asked for on 2026-10-19. It is **not** a standing exception to the
recorded GPU position (`deferred-items.md` D-09-03: no GPU-backed deployment until the CUDA routers are
gated, #5478, or the 401 proof is repeated on that path). The operator accepted a simple proxy as the gate
for this test only; the host is private and is torn down after.

## Shape

```
platform --lambda://aprender-llm-endpoint-dev-gpu-4b--> proxy Lambda (VPC, proxy SG, no Function URL)
                                                         --http :8080--> g5.xlarge (host SG: ingress 8080
                                                                          from the proxy SG only; no CIDR)
```

- `proxy/lambda_function.py`: forwards `POST /v1/chat/completions`, `GET /v1/models`, `GET /health`;
  everything else is a 404 that never reaches the host (so `/v1/gpu/*` is closed). Adds no auth, logs no
  bodies, no streaming. A **reference**, to be adopted or replaced by the platform.
- `userdata.sh`: verifies the CUDA tarball and the GGUF by sha256 (the model pin is read from the
  capability ladder, never typed), runs `apr serve run --backend cuda` under systemd bound to the private
  address, then sends one tiny request so the one-time kernel compilation (about 14 s on the probe host)
  is not paid by the first measured call.
- `up.sh` refuses unless the host group's only ingress is TCP 8080 from one security group, refuses a
  function or role name it did not create, and walks AZs on `InsufficientInstanceCapacity`.
- `check.sh` is the readiness check (one SSM command; prints READY / NOT READY / REFUSED).
- `down.sh` removes the Lambda, instance, both groups, both roles and the profile, then prints counts
  (want `0 0 0 1 1 1 1`). The instance also powers off and terminates itself after `CAP_MINUTES` (180).

## Run

```
! assume ze-kasher-dev && deploy/llm-endpoint/gpu-host/up.sh ze-kasher-dev
! assume ze-kasher-dev && deploy/llm-endpoint/gpu-host/check.sh ze-kasher-dev   # model pull is ~2.7 GB
# hand pmcp.run:  aprender-llm-endpoint-dev-gpu-4b  (lambda://aprender-llm-endpoint-dev-gpu-4b/v1/chat/completions)
! assume ze-kasher-dev && deploy/llm-endpoint/gpu-host/down.sh ze-kasher-dev
```

Cost: `g5.xlarge` is billed while it exists, idle or not (about $1/h; verify the current price before
quoting it to pmcp.run). The proxy Lambda bills per call.

## Launch time (measured once, 2026-10-20) and launching on demand

| Step | Time |
|------|------|
| `run-instances` accepted to OS boot | 15 s |
| First-boot cloud-init before user-data starts | 40 s (55 s from launch) |
| CUDA tarball + 4B GGUF (2.74 GB) pulled and sha256-checked | 12 s |
| `apr serve` start to model ready | 11 s |
| Warm-up request (one-time kernel compilation) | 13 s |
| **Host launch to READY** | **96 s** |
| Proxy Lambda create to Active (VPC network interface) | about 3 min 40 s |

One launch, one AZ, one sample. The proxy Lambda is the slower part, not the GPU host, and it costs
nothing while idle, so the on-demand shape is: leave the proxy deployed, launch only the host when a
test starts, terminate it when the test ends. A host that exists only during a test bills about $1/h
for its lifetime, not around the clock.

- **Terminate and relaunch (what `up.sh` does today):** about 96 s to READY. It needs one more step that
  `up.sh` does not do yet: a new instance has a new private address, so the proxy's `HOST_URL` must be
  updated (a configuration change on a function that already exists). Not built or tested.
- **Stop and start (not built, not measured):** the private address is kept, so the proxy needs no
  change, and the model and compiled-kernel cache stay on the volume. It would skip the first-boot and
  download steps, but it needs `InstanceInitiatedShutdownBehavior=stop`, the cost cap re-armed on every
  boot (user-data runs only once), and it leaves 120 GB of gp3 billing at about $0.013/h when stopped.
  Estimate only; measure before quoting it.

## Tested offline, and not

Tested: the proxy against a local stub host (6 tests; widening the allowlist fails the refusal test), the
4B pin (`00fe7986…`), the security-group refusal expression over 7 cases with real JMESPath, syntax of
every script.

Not tested: any of it in AWS. In particular the Lambda-in-VPC reaching the host, IAM propagation timing,
the systemd unit, and whether the 4B loads on sm_86 (the 0.8B and 2B did; the 4B has not run there).
The `bashrs lint` determinism findings on timestamps are accepted for ops scripts.

## The pattern this starts (to develop slowly)

| Stage | Now | Next |
|-------|-----|------|
| Deploy | `up.sh` / `down.sh`, one size | fold into the variant recipe once it settles |
| Update weights | re-run with a new `MODEL_ID` (pulled from Hugging Face, sha-checked) | stage in S3 like the CPU variants; swap and restart over SSM |
| Wire to the platform | proxy Lambda named for the allowlist | the platform owns the proxy and its IAM |
| Protect | proxy is the only gate; host SG admits one group | gate the CUDA routers upstream (#5478) and repeat the 401 proof on the GPU path |
