# 14 Redeploy Runbook (plan 14-22): the finalizer, then the request function

Purpose: put the code of plans 14-17 and 14-18 into production in ze-kasher-dev. The finalizer gets WR-01, WR-02,
IN-01 and the Catch reduction of WR-03. The request function gets the request-side sinks of WR-03. Under D-01 and
D-02, every command that touches the account is **owner-run**. Run each one in your own shell with the `!` prefix
from the repository root. Claude runs no `aws`, `cdk`, `cargo pmcp` or `pmcp` command. It reads the files these
commands write and records the verdicts in `14-GAP-REDEPLOY-EVIDENCE.json`. The orchestrator records your replies
there.

What Claude did offline before this runbook (Task 1, recorded under `readiness` in the evidence):

- ran the offline suite;
- ran `just laya-trainer-synth dev` on a clean, committed closure (the sidecar commit and sha256 are in the evidence);
- dry-ran `DRY_RUN=1 just laya-trainer-infra diff|deploy dev ze-kasher-dev` (`AWS CALLS: 0`, provenance accepted);
- wrote and bashrs-gated the read-only helper `models/laya-trainer/redeploy-finalizer-check.sh`.

The images are unchanged, so no CodeBuild run is needed. No contract, CDK resource, decide crate, container image or
workflow changes in this plan (D-22: the deployment-unit register stays 11).

Execution context: the **main checkout** only. Every path below is gitignored state (`models/laya-trainer/`,
`deploy-extensions/assets/`) that a fresh worktree does not have.

---

## Section A: the finalizer (`aprender-laya-train-finalizer-dev`)

```
+---------------------------------------------------------------------------------------------------------------+
| TWO FACTS TO KNOW BEFORE YOU START                                                                            |
|                                                                                                               |
| 1. The diff will ALWAYS show the Finalizer function's code key (Code S3Key, the asset hash) changing.         |
|    deploy-extensions/bin/app.ts hands the whole directory deploy-extensions/assets/laya-finalizer to          |
|    Code.fromAsset. The provenance sidecar bootstrap.build.json therefore rides inside the deployed package    |
|    next to bootstrap and carries a UTC build time, so every synth gives a new asset hash. That code-key       |
|    change is the EXPECTED change. The sidecar holds the commit, the closure path names, the binary sha256,    |
|    the target and the time. It holds no secret. The hash helper hashes only the entry named bootstrap, so     |
|    the extra entry does not disturb it.                                                                       |
|    STOP on any IAM statement or IAM policy change, on any resource added ([+]) or removed ([-]), and on any   |
|    other changed property.                                                                                    |
|                                                                                                               |
| 2. If `infra diff` or `infra deploy` prints                                                                   |
|      REFUSED infra: finalizer provenance: closure path changed since the build: <path>                        |
|    then a commit touched the finalizer's source closure after the synth. The closure is 12 crates (the        |
|    output of cargo tree -p aprender-mcp-laya-train-lambda -e normal reduced to workspace paths: aprender-core,|
|    aprender-compute, aprender-decide, ...) plus Cargo.lock, Cargo.toml and rust-toolchain.toml. The cure is a |
|    fresh `just laya-trainer-synth dev`, then this section from step A1 again. NEVER edit the sidecar by hand. |
|    A changed .planning/ file is outside the closure and never triggers this.                                  |
+---------------------------------------------------------------------------------------------------------------+
```

Before A1: Claude confirms that the synth's sidecar commit equals HEAD, or that it differs from HEAD only in paths
outside the closure (`finalizer-provenance verify` says `FINALIZER PROVENANCE OK`). Do not run
`just laya-trainer-synth` between A1 and A2. A re-synth changes the asset, so the saved diff would no longer describe
the deploy. If you do re-synth, start again at A1.

### A1. Save the read-only change set (a CloudFormation change-set diff; creates nothing)

```
! assume ze-kasher-dev && just laya-trainer-infra diff dev ze-kasher-dev > models/laya-trainer/redeploy-finalizer-diff.txt 2>&1; tail -n 2 models/laya-trainer/redeploy-finalizer-diff.txt
```

- **Expected last line:** `INFRA OK diff`. Under `Resources`, the file's only change is
  `[~] AWS::Lambda::Function Finalizer ...`, whose only changed property is `Code` (`S3Key`, old asset hash to new).
  The same function's asset metadata naming the same new asset hash counts as part of that one change.
- **STOP if:**
  - the last line is a `REFUSED ...` or `INFRA FAILED diff ...` line (see fact 2 for the provenance refusal);
  - the file has an `IAM Statement Changes` or `IAM Policy Changes` section;
  - any `[+]` or `[-]` resource appears;
  - any resource other than the Finalizer function shows `[~]`;
  - the Finalizer shows a changed property other than its code.
- **Then reply `diff saved`.** Claude reads the file and answers **go** or **stop**. The file stays in the gitignored
  state directory. It may name ARNs, and none of them is copied into a committed file.

### A2. Deploy (only after Claude answered go)

```
! assume ze-kasher-dev && just laya-trainer-infra deploy dev ze-kasher-dev > models/laya-trainer/redeploy-finalizer-deploy.log 2>&1; tail -n 2 models/laya-trainer/redeploy-finalizer-deploy.log
```

- This re-runs the provenance check and `cdk diff`, then `cdk deploy --require-approval never` in the same invocation.
  The saved A1 diff is therefore the only stop, and it was read before this command ran. The stack update took about
  40 s last time (14-IAM-FIX-NOTE: 39.55 s).
- **Expected last line:** `INFRA OK deploy`.
- **STOP if:** the last line is `REFUSED infra: ...` (nothing was deployed) or `INFRA FAILED deploy cdk-deploy rc=<n>`
  (CloudFormation rolls back by itself). Paste that line to Claude. Do not retry blind.
- Claude also compares the diff this command printed into the log with the saved A1 diff. They must show the same
  change.

### A3. The hash check (read-only: one `lambda get-function` and a download)

```
! assume ze-kasher-dev && bash models/laya-trainer/redeploy-finalizer-check.sh
```

- What it does: one `aws lambda get-function` on `aprender-laya-train-finalizer-dev` (profile ze-kasher-dev,
  us-east-1). It downloads the code package into `models/laya-trainer/` (the presigned URL reaches curl on stdin and is
  never printed) and checks the download against the function's `CodeSha256`. It then hashes the entry `bootstrap`
  with `shasum -a 256`, copies the package's own `bootstrap.build.json` to
  `models/laya-trainer/redeploy-finalizer-deployed-sidecar.json`, and writes `models/laya-trainer/redeploy-finalizer-sha.txt`.
  It deletes the package, also on failure. It writes nothing to AWS.
- **Expected last line:** `FINALIZER REDEPLOY CHECK OK sha256=<64 hex>`, where the hex equals the sidecar's
  `bootstrap_sha256`. `deployed sidecar sha256` names the same hash.
- **STOP if:** the last line is `FINALIZER REDEPLOY CHECK FAILED ...`.
  - `deployed bootstrap X != sidecar Y` is a **failed redeploy**. Claude records `finalizer.equal: false` and reports
    it. Do not re-synth or re-deploy to make the numbers agree before Claude has recorded the mismatch.
  - `the downloaded package does not match the function's CodeSha256` means the download, not the deploy, is in
    doubt. Re-run A3 once.
  - an `aws` error (for example an expired session) means re-assume and re-run A3. A3 is read-only and safe to repeat.
- **Then reply `done`.** If a step failed, paste the last line of that step instead.

---

## Section B: the request function (`aprender-mcp-laya-train` on pmcp.run): only after the owner's decision

Section B runs only if the owner replied **redeploy-now** at the Task 3 decision. With **defer**, nothing here runs and
the open item is recorded instead. It names the request-side sinks still on old code in production:

- the dispatch error text (an AWS SDK message with ARNs when StartExecution is denied or throttled);
- the backend error text inside `train` and `train_status`;
- the task-store errors of the Tasks protocol.

**The cost (D-10, measured in 14-13).** A `--no-oauth` publish may reset OAuth. The recipe therefore deletes the inline
grant `aprender-laya-train-dev` FIRST, fail-closed (`GRANT REVOKED for redeploy`). From the publish until you re-enable
OAuth and the grant recipe sees the 401 itself, the endpoint is open but holds no spend right. 14-13 measured that
interval at 10114 s (about 2 h 49 min, an upper bound). It was harmless by construction: the role simulation answered
`implicitDeny` for `states:StartExecution`, `dynamodb:PutItem` and `s3:GetObject`. During the same interval
`train_status` stops answering.

### B0. Precondition: no run in flight (read-only)

```
! assume ze-kasher-dev && aws stepfunctions list-executions --profile ze-kasher-dev --region us-east-1 --status-filter RUNNING --query 'length(executions)' --state-machine-arn "$(python3 -I -c 'import json; print(json.load(open("models/laya-trainer/outputs-dev.json"))["trainer"]["StateMachineArn"])')"
```

- **Expected:** `0`. The single-flight lock is free, and no run needs `train_status` while the grant is absent.
- **STOP if:** it prints anything other than `0`. Wait for the run to finish, or defer.

### B1. Re-render the deploy config from the stack outputs (local file only)

```
! just laya-trainer-config dev ze-kasher-dev
```

- **Expected last line:** `CONFIG OK deploy-config-dev.toml`. The file is mode 600 under `models/laya-trainer/`. It
  has no `[[iam.statements]]` and `[auth] enabled = false`.
- **STOP if:** `REFUSED config: ...`.

### B2. Publish (deletes the grant first, then `cargo pmcp deploy ... --no-oauth`)

```
! assume ze-kasher-dev && LAYA_TRAINER_APPROVED_DEPLOY=dev just laya-trainer-deploy dev ze-kasher-dev > models/laya-trainer/redeploy-request-deploy.log 2>&1; tail -n 4 models/laya-trainer/redeploy-request-deploy.log
```

- **Expected:** `GRANT REVOKED for redeploy` (or the absent-policy line) early in the log. The tail shows
  `endpoint: https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp`, then `recorded deploy-dev.json; ...`, and the
  last line is `DEPLOY OK aprender-mcp-laya-train`. The OAuth window starts here.
- **STOP if:**
  - `REFUSED deploy: cannot confirm the grant is absent`: nothing was published, and the grant state is unknown, so
    paste the line;
  - `REFUSED deploy: wrong package`;
  - any other `REFUSED` or a `cargo pmcp` failure. 14-13's descriptor-parse class created nothing and restored the
    crates root.

### B3. Re-enable OAuth (the pmcp.run admin UI; the one step only you can do)

In the ze-kasher-dev pmcp.run admin UI, open `aprender-mcp-laya-train`, confirm OAuth is enabled (enable it if the
publish reset it), and confirm the M2M client is still present. If the client was lost, create it as in 14-13 Task 2:
client-credentials, `client_secret_post`, scopes covering the host's `/mcp`. Write
`{"client_id", "client_secret", "token_url"}` to a mode-600 file OUTSIDE this checkout, and reply with its path. Claude
writes the path, never the contents, to the gitignored `models/laya-trainer/m2m-file-path`.

Then observe the 401 (curl only, no aws):

```
! LAYA_AUTH_POLL_SLEEP=30 just _laya-auth-refusal-check https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp 600 train_status
```

- **Expected:** a line starting `auth-refusal ok: tools/list 401, tools/call 401`.
- **STOP if:** it times out without a 401. OAuth is not on. Do NOT run B4, and paste the line.

### B4. Grant (sees the 401 itself, in its own invocation, immediately before the IAM call)

```
! assume ze-kasher-dev && LAYA_TRAINER_APPROVED_GRANT=dev just laya-trainer-grant dev ze-kasher-dev
```

- **Expected last line:** `GRANT OK role <role name>`, after `policy: aprender-laya-train-dev`.
- **STOP if:**
  - `REFUSED grant: OAuth not confirmed (tools/list answered <code>)`: OAuth is off, so go back to B3;
  - `REFUSED grant: endpoint not bound to aprender-mcp-laya-train`;
  - `REFUSED grant: endpoint unreachable (transport error; retry)`: retry once.
- The window closes here. The grant recipe saw the 401 in its own invocation, which is the D-10 ordering guarantee.

### B5. Read-only confirmation through the authenticated client

```
! LAYA_M2M_FILE="$(cat models/laya-trainer/m2m-file-path)" SSL_CERT_FILE=/etc/ssl/cert.pem just laya-trainer-call ze-kasher-dev https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp list-tools
```

- **Expected:** one JSON line with `"ok": true` naming exactly `train`, `train_status` and `validate_dataset`. The mint
  line ends `(auth form)`. The token is never printed.
- **STOP if:** `REFUSED call: ...`, an `ok: false` line, or a different tool set.
- **Then paste the last line of each of B0 to B5.** The orchestrator records them in `owner_replies`, and Claude fills
  `request_function.steps`.

---

## What gets recorded (by Claude and the orchestrator, never by you)

| Field in `14-GAP-REDEPLOY-EVIDENCE.json` | Source |
|---|---|
| `readiness` | Task 1: offline suite results, synth sidecar commit and sha256, dry-run lines, bashrs-gate line |
| `finalizer.diff_verdict` | Claude's read of `models/laya-trainer/redeploy-finalizer-diff.txt` (go or stop, and what changed) |
| `finalizer.deployed_sha256` / `sidecar_sha256` / `equal` | `models/laya-trainer/redeploy-finalizer-sha.txt` against the sidecar |
| `request_function.decision` / `steps` | the owner's Task 3 reply, then the B0 to B5 last lines |
| `owner_replies` | written by the orchestrator only (an executor never records or relays an approval) |

No account id, ARN, presigned URL, token or M2M material goes into a committed file. `.env.m2m` is never read.
