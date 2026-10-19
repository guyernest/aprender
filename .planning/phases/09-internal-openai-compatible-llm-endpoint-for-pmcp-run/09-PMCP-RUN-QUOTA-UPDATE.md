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

## What we have and have not done

- **Prepared, not launched:** a probe that starts one private `g5.xlarge` and runs the released `apr` v0.71.4 x86_64 CUDA build on the same 0.8B and 2B GGUFs, to measure the one thing nobody has: apr on an A10G (sm_86). No instance exists. It has no inbound rule, binds 127.0.0.1, is reached only through SSM, and powers itself off after 150 minutes. It is not an endpoint and has no URL.
- **Our GPU position is unchanged:** no GPU-backed deployment of this endpoint until the CUDA-path routers are gated (#5478, open) or the 401 proof is repeated on that path. A private measurement host does not change it, and it gives you no written agreement that a proxy Lambda alone satisfies it. That decision is still open and still yours and ours to make in writing.
- **The SageMaker shim and the 4B on Fargate** are unchanged from our earlier reply.

## What we would like from you

1. **Tell us before you launch an EC2 GPU host in this account.** The probe uses 4 of the 8 vCPUs while it runs (about 20 minutes, never more than 150). We will not launch it while you have a GPU instance running, and we would ask the same of you.
2. **Say whether the A10G measurement is useful to you.** If you would rather measure on your own host, or target SageMaker `ml.g5` and skip EC2, we will not run it.
3. **If you want a T4 (`g4dn`) number as well,** say so; the same script runs it.

## What you would get back

One sample per shape, one environment: whether the CUDA build starts and serves both sizes on sm_86, the start time with weights pulled from S3, a cold and a warm tool-call round trip, GPU memory used, and the server's own report of the device it used. It will not be an agent-turn cost, a concurrency figure, or anything about authentication on the CUDA path.
