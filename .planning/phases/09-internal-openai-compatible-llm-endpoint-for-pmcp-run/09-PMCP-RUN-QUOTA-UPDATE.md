# Update for the pmcp.run dev team: the G/VT quota request is granted

DRAFT, NOT SENT. From Aprender, 2026-10-19. This closes one item in your change request of 2026-10-18 ("G/VT quota request for 8 vCPUs is open").

## What happened

The request with id `1c45fe6f5ca74408965416975c3de9dcAe0cfKRC` (your case reference) was raised on 2026-10-18 at 14:00 Pacific, asking for 8 vCPUs on "Running On-Demand G and VT instances" (quota code `L-DB2E81BA`) in us-east-1. The case (`179235720700760`) closed at 14:37 Pacific. We read the applied quota afterwards: it is **8 vCPUs**, the value asked for. This is the quota in the `ze-kasher-dev` account, the account the endpoint functions run in.

## What 8 vCPUs fits

| Instance | vCPU | GPU | Fits |
|----------|------|-----|------|
| `g5.xlarge` | 4 | A10G, 22.9 GiB | two at once, or one with room |
| `g5.2xlarge` | 8 | A10G, 22.9 GiB | one |
| `g4dn.xlarge` | 4 | T4, 16 GiB | two at once |
| `g5.12xlarge` and larger | 48+ | multi-GPU | no |

The quota counts vCPUs across all running G and VT instances in the account, so it is shared. Anything we or you launch there draws on the same 8.

## What we did with it

On 2026-10-19 we ran one private probe: a `g5.xlarge` (A10G, sm_86) with the released `apr` v0.71.4 x86_64 CUDA build and the same 0.8B and 2B GGUFs. It was a measurement host, not an endpoint: no inbound rule, server on 127.0.0.1, SSM only, self-terminating. It is torn down and checked gone. Evidence: `09-GPU-PROBE-EVIDENCE.json`.

- **The CUDA build starts and serves both sizes on an A10G.** The server reports `Backend: GPU (CUDA, NVIDIA A10G)` with all 24 layers resident, a process held 2.1 GiB (0.8B) and 4.5 GiB (2B) on the card, and a response carried `used_gpu: true`. The repo's earlier records covered only RTX 4090 and GB10.
- **Speed, one request:** 278 prompt tokens in 204 ms (about 1,360 tokens/s) and 96 decoded tokens in 646 ms (about 149 tokens/s), 0.8B. Our CPU measurements had prefill at the decode rate, 12 to 16 tokens/s. It is one request on one host, not an agent turn.
- **First request after start was slow:** 14.0 s on the 0.8B, 5.6 s on the 2B. We read this as one-time kernel compilation on a fresh host; we did not measure it separately. A GPU host that restarts often would pay it each time unless the cache is kept.
- **Tool calls:** the 2B returned a tool call; the 0.8B answered in prose with an invented weather report. One sample each at temperature 0, so it is a data point and no more.
- **Not tested:** authentication on this path (the CUDA routers still ignore the key, #5478, and we did not run the 401 proof), concurrency, a T4, and cost per agent turn.

**Our GPU position is unchanged:** no GPU-backed deployment of this endpoint until those routers are gated or the 401 proof is repeated on that path. The probe does not give you a written agreement that a proxy Lambda alone satisfies it. The SageMaker shim and the 4B on Fargate are also unchanged from our earlier reply.

## What we would like from you

1. **Tell us before you launch an EC2 GPU host in this account.** Our probe used 4 of the 8 vCPUs for under an hour. The quota is shared, so we would like the same notice from you.
2. **Say whether you want more from the GPU side:** a T4 (`g4dn`) number, the 12-message replay set on the A10G, or nothing further because you are targeting SageMaker `ml.g5`. The same script runs a T4; the replay set needs work on our side.
