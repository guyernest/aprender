# 15 Roll Runbook (plan 15-05): the pack image, then the stack

Purpose: put the amended pack bound (amendment A2, plan 15-04) into the image that runs on the authority CPU. The contract is baked
into the pack image (`/src/contracts` and a `pack_laya` compiled from the tree), so until the image is rebuilt and the pack task
definition repinned, Graviton still enforces A1. Under D-03, every command that touches the account is **owner-run**. Run each one in
your own shell with the `!` prefix from the repository root, after `assume ze-kasher-dev`. Claude runs no `aws`, `cdk`, `cargo pmcp`
or `pmcp` command. It reads the files these commands write and records the verdicts in `15-ROLL-EVIDENCE.json`. The orchestrator
records your replies there.

What Claude did offline before this runbook (Task 1, recorded under `pre_roll` and `expected` in the evidence):

- checked that no pack image context path has changed since plan 15-04's `image_head`;
- dry-ran `DRY_RUN=1 just laya-trainer-image pack dev ze-kasher-dev` (`AWS CALLS: 0`, names HEAD as the commit it would archive);
- synthesized the trainer stack with the CURRENT pack digest (`sha256:504084d0...`, the image that refused attempt 1) and kept the
  template as `models/laya-trainer/p15-template-pre.json` (gitignored), so plan 15-05 Task 3 can compare it with the post-roll
  template leaf by leaf.

The train image is unchanged (structure S-B), so only the pack image is rebuilt.

Execution context: the **main checkout** only. Every path below is gitignored state (`models/laya-trainer/`,
`deploy-extensions/assets/`) that a fresh worktree does not have.

```
+---------------------------------------------------------------------------------------------------------------+
| THREE FACTS TO KNOW BEFORE YOU START                                                                          |
|                                                                                                               |
| 1. The image is built from `git archive HEAD` over the context list in                                        |
|    deploy-extensions/container/pack-arm64/context-paths.txt plus the Dockerfile. The recipe REFUSES when a    |
|    listed path has uncommitted changes. What you build is what is committed, never what is on disk.           |
|                                                                                                               |
| 2. The diff will ALWAYS show the Finalizer function's code key (Code S3Key, the asset hash) changing. The     |
|    provenance sidecar bootstrap.build.json rides inside the deployed package and carries a UTC build time,    |
|    so every synth gives a new asset hash. That is expected. The finalizer is also affected for real: its      |
|    cargo-tree closure contains aprender-decide, which plan 15-02 edited.                                      |
|                                                                                                               |
| 3. Any commit to the finalizer's closure (crates/, Cargo.toml, Cargo.lock, rust-toolchain.toml) AFTER the     |
|    synth makes `infra diff|deploy` print                                                                      |
|      REFUSED infra: finalizer provenance: closure path changed since the build: <path>                        |
|    The cure is a fresh `just laya-trainer-synth dev <train digest> <pack digest>`, then the section again.    |
|    NEVER edit the sidecar by hand. A changed .planning/ file is outside the closure and never triggers this.  |
+---------------------------------------------------------------------------------------------------------------+
```

---

## Section A: rebuild the pack image (owner; CodeBuild minutes only, no GPU)

### A1. Build

```
! assume ze-kasher-dev && just laya-trainer-image pack dev ze-kasher-dev > models/laya-trainer/p15-image-pack.log 2>&1; tail -n 2 models/laya-trainer/p15-image-pack.log
```

- It zips the context from `git archive HEAD`, uploads it, starts the CodeBuild project, waits up to 90 min (you may background it)
  and records `{repo_uri, repo_name, digest, commit, build_id}` in `models/laya-trainer/images-dev.json`.
- **Expected last line:** `IMAGE OK pack sha256:<64 hex>`. The digest is NOT `sha256:504084d0c254af66e01b40f9f4679ba48fc09c4bf80386cc458a9388ae3090f3`.
- **STOP if:** the last line is `IMAGE FAILED pack <status>` or any `REFUSED ...` line (a dirty context path, a wrong profile). Paste it.
- **Then reply with the last line.** The orchestrator records it in `owner_replies`; an executor never records or relays it.

Claude then checks `images-dev.json`: `pack.digest` equals the reply and differs from the pre-roll digest, and `pack.commit` satisfies the
one identity rule: it is `expected.image_head` or a descendant (`git merge-base --is-ancestor`), AND
`git diff --quiet <image_head> <pack.commit>` holds over `expected.context_paths`. Committing this plan's runbook and evidence advances
HEAD but touches no context path, so it never breaks the rule.

---

## Section B: what Claude does after A (offline, no credentials)

1. Verify the identity rule above.
2. `just laya-trainer-synth dev <train digest> <new pack digest>` (cross-builds the finalizer, writes the sidecar, synthesizes both
   stacks, ends `TRAINER SYNTH OK env=dev`).
3. `python3 -I scripts/laya_deploy/laya_trainer_ops.py finalizer-provenance verify deploy-extensions/assets/laya-finalizer` ends
   `FINALIZER PROVENANCE OK`.
4. `DRY_RUN=1 just laya-trainer-infra deploy dev ze-kasher-dev` ends `AWS CALLS: 0` with no `REFUSED` line.
5. Compare `p15-template-pre.json` with the new trainer template value by value and record `expected_change_set` (every differing leaf
   with sha256 of the old and new value and, for an image reference, the old and new digest). Expected: no resource added or removed;
   inside the pack TaskDefinition's `ContainerDefinitions` the only differing leaf is the pack container's Image; the Finalizer function
   changes only under `Properties.Code`. This is a PREDICTION from two current-source synths. The deployed stack is checked only by
   Section C.

---

## Section C: the stack diff, deploy and outputs (owner; after Section B)

Do not run `just laya-trainer-synth` between C1 and C2: a re-synth changes the asset, so the saved diff would no longer describe the
deploy. If you do re-synth, start again at C1.

### C1. Save the read-only change set

```
! assume ze-kasher-dev && just laya-trainer-infra diff dev ze-kasher-dev > models/laya-trainer/p15-infra-diff.txt 2>&1; tail -n 2 models/laya-trainer/p15-infra-diff.txt
```

- **Expected last line:** `INFRA OK diff`. The file shows only what Section B predicted (the pack container Image and the Finalizer code).
- **STOP if:** the last line is `REFUSED ...` (fact 3 for the provenance refusal) or `INFRA FAILED diff ...`; an `IAM Statement Changes` or
  `IAM Policy Changes` section appears; any `[+]` or `[-]` resource appears; any change falls outside the predicted leaves; or the live
  "from" digest of the pack image is not the pre-roll digest (drift since the last deploy).
- **Then reply `diff saved`.** Claude reads the file against the prediction leaf by leaf and answers **go** or **stop**.

### C2. Deploy (only after Claude answered go)

```
! assume ze-kasher-dev && just laya-trainer-infra deploy dev ze-kasher-dev > models/laya-trainer/p15-infra-deploy.log 2>&1; tail -n 2 models/laya-trainer/p15-infra-deploy.log
```

- Re-runs the provenance check and `cdk diff`, then `cdk deploy --require-approval never` in the same invocation.
- **Expected last line:** `INFRA OK deploy`.
- **STOP if:** `REFUSED infra: ...` (nothing deployed) or `INFRA FAILED deploy cdk-deploy rc=<n>` (CloudFormation rolls back). Paste the line.

### C3. Outputs

```
! assume ze-kasher-dev && just laya-trainer-infra outputs dev ze-kasher-dev
```

- **Expected last line:** `INFRA OK outputs`; `outputs-dev.json` then names a new pack task definition revision.
- **STOP if:** `REFUSED ...` or `INFRA FAILED outputs ...`.

---

## Section D: the re-pack (plan 15-06)

D0. The offline script regeneration: filled in by plan 15-06.

D1. The re-pack run: filled in by plan 15-06. It must finish before about 2026-11-28 (`sources.motivation.expires_about`).

---

## What gets recorded (by Claude and the orchestrator, never by you)

| Field in `15-ROLL-EVIDENCE.json` | Source |
|---|---|
| `pre_roll`, `expected`, `owner_commands` | Task 1 (offline) |
| `owner_replies` | written by the orchestrator only (an executor never records or relays an approval) |
| `image`, `synth`, `dry_run_deploy`, `expected_change_set`, `post_template_sha256` | Task 3 (offline) |

No account id, ARN, repository URI, presigned URL or token goes into a committed file. `.env.m2m` is never read.
