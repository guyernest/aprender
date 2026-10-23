---
phase: 10
phase_name: "antisemitism-v1 Decide Server for experiment-coach"
project: "Aprender Native SetFit Classification"
generated: "2026-10-23"
status: partial
partial_reason: "Extracted mid-phase at the owner's request: 8 of 12 plans have a SUMMARY (10-01..10-05, 10-07, 10-08, 10-11). 10-12 awaits the owner's retirement step, 10-10 and 10-09 have not run, 10-06 is deferred. Re-run after the phase closes; a re-run overwrites this file."
counts:
  decisions: 9
  lessons: 9
  patterns: 6
  surprises: 7
missing_artifacts:
  - "10-VERIFICATION.md (phase verifier has not run)"
  - "10-UAT.md (not created)"
  - "10-06-SUMMARY.md (plan deferred)"
  - "10-09-SUMMARY.md (not executed)"
  - "10-10-SUMMARY.md (not executed)"
  - "10-12-SUMMARY.md (plan awaiting the owner's retirement step; its evidence file 10-RENAME-DEPLOY-EVIDENCE.json was used instead)"
---

# Phase 10 Learnings: antisemitism-v1 Decide Server for experiment-coach

Every item is taken from a phase artifact and names it. Learnings that exist only in the working session
(for example the coding assistant's permission checks refusing deploy-class commands) are not in any artifact
and are therefore not extracted here.

## Decisions

### The served door stays at 8 texts; the owner picked the p90 task text
The owner chose `door-8 p90` (2026-10-21): the 86-token-prefix draft, five median posts or four p95 posts per call,
and no contract, runtime, ledger or README change for the door. A fuller guide was treated as a hypothesis about
accuracy, measurable only by the gate run.

**Rationale:** the fuller draft (193-token prefix) would have forced the door down to 4 texts, a MAJOR contract bump and edits across the compiled constant, mirror test, four ledger rows and three documents. The owner preferred the draft that keeps the published contract unchanged.
**Source:** 10-01-SUMMARY.md, 10-11-SUMMARY.md, 10-TASK-EVIDENCE.json

---

### The task text paraphrases the IHRA examples; the 12-paragraph guideline was not fetched
The approved text paraphrases the IHRA working definition's examples with the labelling rules of arXiv 2304.14599. That paper cites the 12-paragraph guideline as a separate document (arXiv 1910.01214, not fetched). The owner did not ask for a redraft.

**Rationale:** the paper's fetched bytes (357,202, sha256 recorded) do not reproduce the guideline; the record states this instead of implying the text is verbatim.
**Source:** 10-01-SUMMARY.md, scripts/laya_train/prepare_goldstandard.py docstring

---

### Epochs were fixed by a written rule from a measured probe: E = 4
One timed epoch on `mps:0` measured 1.4695 s per step. The rule gives E = 6 only at 1.0 s or less, so the declared run used E = 4.

**Rationale:** the rule is declared before the probe, so the epoch count is not tuned after seeing results. At E = 4 early stopping can only restore the best epoch (earliest stop is epoch 4 = E), and the record says so.
**Source:** 10-03-SUMMARY.md, 10-PROBE-EVIDENCE.json

---

### One declared gate run, no retune; a fail would stop at the owner
`just laya-train` ran exactly once for 3 seeds. A non-pass would have stopped at the owner's checkpoint with nothing retuned and at most one budgeted re-run, counted as two declared runs.

**Rationale:** keeps the gate honest: a refusal is a result, not a prompt to tune.
**Source:** 10-04-SUMMARY.md (Task 3 skipped on the record), 10-REVIEWS.md

---

### The SetFit comparison is held open as blocked, not forced or faked
After measuring that `apr eval` cannot evaluate the cell on a 48 GB host, the owner chose option C (2026-10-22): record the comparison as blocked, hold SC-4 open, do not run the cell, change `apr eval`, or use the cloud.

**Rationale:** a fix to `predict_rows` needs a contract first under `aprender-train`'s CLAUDE.md, and a different chunk size shifts batched probabilities by about 1e-6. The evidence adds a `blocked` branch with `failure_class: implementation` instead of mislabelling it a domain refusal.
**Source:** 10-05-SUMMARY.md, 10-SETFIT-CELL-EVIDENCE.json

---

### Plan 10-06 is deferred and removed from the dependencies of 10-10 and 10-09
The owner deferred 10-06 (the `--dump-probabilities` addition to `apr eval`) because it needs the SetFit cell that never ran. 10-10 does not read its output, so its dependency was dropped. 10-09 does read the SetFit evidence, so it must carry a blocked-SetFit branch.

**Rationale:** avoids committing code that cannot be exercised against anything real, on the same `apr eval` path that has the memory defect.
**Source:** STATE.md (Last activity), ROADMAP.md (10-06 DEFERRED note), 10-09-PLAN.md, 10-10-PLAN.md

---

### Replace the stance demo in place, auth off (D-13)
The owner decided `replace-auth-off` (2026-10-22): the new artifact replaced the stance demo under `aprender-mcp-decide`, with no bearer token.

**Rationale:** keeps the existing posture and avoids a token to distribute; the open endpoint's cost exposure (each cold call buys about 25 s of 10 GB compute) was accepted knowingly.
**Source:** 10-07-SUMMARY.md, 10-08-SUMMARY.md, 10-LIVE-DEPLOY-EVIDENCE.json

---

### Rename the server to `aprender-mcp-decide-antisemite` (D-19) and let the owner retire the old one (D-20)
Because several classifiers from different datasets must run in parallel under their own server ids, the owner required the server, its Lambda and its endpoint to carry the dataset name. The owner deletes the old server from the pmcp.run admin UI; no executor tears anything down.

**Rationale:** a server id named for the mechanism cannot hold a second model.
**Source:** 10-12-PLAN.md (must_haves D-19, D-20), 10-RENAME-DEPLOY-EVIDENCE.json

---

### A shared model-agnostic Lambda library plus one thin crate per served model
`aprender-mcp-decide-lambda` became a library carrying `bootstrap::run`. Each served model gets a thin `<server>-lambda` crate (Cargo.toml, a shim main.rs, a README). The server id is a required argument of the six AWS-touching recipes. The FALSIFY-MONO-011 register was swapped 9 to 9, not grown, and the owner approved that reading.

**Rationale:** cargo-pmcp resolves `<server>-lambda` first, so each id needs its own crate. Rejected: renaming the crate in place (blocks a second model), keeping both bins (grows the register and leaves the old id resolvable), a symlinked crate dir, a `[[bin]]` path outside the package, and a per-model deploy root.
**Source:** 10-RENAME-DEPLOY-EVIDENCE.json (design), 10-12-PLAN.md, crates/aprender-mcp-decide-lambda/README.md

---

## Lessons

### Never assert `tokens_planned == 8 x P`
`tokens_planned` is the request total (800), not 8 times the prefix (688 here). Assertions must use `min_row_tokens` (86) and `texts` instead.

**Context:** cross-AI review caught the wrong check in 10-04, 10-08 and 10-10 before execution, and the 10-02 probe work showed it false on the stance artifact too (800 against 57).
**Source:** 10-REVIEWS.md (concern 2), 10-02-SUMMARY.md, 10-04-SUMMARY.md

---

### A plan's verify must have a branch for the honest failure it can hit
The OOM branch of 10-03 could not pass its own verify, 10-05's verify accepted only `completed` and `refused (domain-refusal)`, and 10-12's token-margin assertion could not hold for an owner-run redeploy. Each time the executor added the narrowest honest branch and disclosed it rather than faking a pass.

**Context:** a gate that cannot evaluate an outcome forces either a false pass or a stalled plan.
**Source:** 10-REVIEWS.md (concern 6), 10-05-SUMMARY.md, 10-RENAME-DEPLOY-EVIDENCE.json

---

### Check the plan's file and path assumptions in a rehearsal before the long run
The plan named the Hugging Face MiniLM snapshot as `--model-dir`, but it holds only `model.safetensors` and `apr setfit train` needs `full_model.apr`. An s8 rehearsal found this, and the memory blow-up in `apr eval`, in minutes instead of after a 30-minute cell.
**Context:** the fix was the pinned `~/.cache/aprender/minilm-l6-v2-1110a243` checkout, whose manifest records the same safetensors sha256.
**Source:** 10-05-SUMMARY.md

---

### `apr eval` memory grows with the eval set
Measured with `/usr/bin/time -l` on s8 artifacts: 61 rows 9.4 GB, 241 rows 40.3 GB footprint, 852 rows killed at 90 GB on a 48 GB host. Memory appears to accumulate across the 256-row batches. The test split has 2,129 rows.

**Context:** training is unaffected (the Laya gate does not use `apr eval`); only the SetFit comparison is blocked. Root cause is unconfirmed.
**Source:** 10-05-SUMMARY.md, 10-SETFIT-CELL-EVIDENCE.json

---

### A pass in-distribution is a narrow claim, and the shipped model is effectively one epoch
The gate certifies margin and calibration on a held-out, group-disjoint 20 % of the same dataset, not robustness to a shifted population. Training loss reached 0.0000 by epoch 3 in every seed and two seeds restored epoch 1, so the shipped model is in effect a one-epoch fine-tune. Seed 17 hit the temperature clamp (T = 5.0) yet still passed.

**Context:** left unacted as a recipe fact for the owner, not a reason to doubt the gate result.
**Source:** 10-04-SUMMARY.md

---

### The 18-minute verify budget was an assumption; real runs took about 21 minutes
Pack and verify each took about 21 minutes (09:08:48Z to 09:30:01Z for the verify) against the plan's assumed 18, which RESEARCH A2 had estimated linearly. The summary says each of the five downstream verify runs that plan 10-10's budget assumes should be priced at about 21 minutes. The 10-08 upload step, which runs the verify then the transfer, took 1,473 s in total.

**Context:** the recipes re-run the verify before every upload and deploy, so the per-step cost multiplies across a phase.
**Source:** 10-04-SUMMARY.md, 10-08-SUMMARY.md

---

### The pmcp.run login window decides how deploy work is split
The token lasts about an hour and is needed only from `cargo pmcp deploy`, about 22 minutes into `laya-deploy`. The first deploy attempt was held back because 32 minutes could not cover the re-verify, build and deploy; the rename deploy was split into upload first, deploy on a fresh login.

**Context:** the executor never refreshes the token; the owner logs in right before the step that needs it.
**Source:** 10-08-SUMMARY.md, 10-RENAME-DEPLOY-EVIDENCE.json

---

### The recipe can contain a brand-new server id before the edge has registered it (F-10-12-1)
After `cargo pmcp deploy` created the new function, the recipe's single edge `GET /health` returned 404 and the recipe set reserved concurrency 0. The same GET returned 200 about 8 minutes later. A bounded retry on the 404 before containing is the suggested fix and is not implemented.

**Context:** the earlier 10-08 update of an existing id never hit this; only a brand-new id does.
**Source:** 10-RENAME-DEPLOY-EVIDENCE.json (findings F-10-12-1)

---

### A redeploy cleared the sticky pmcp.run error state; waiting did not (F-10-12-2)
After un-containment, every MCP POST still returned 503 "Server is in error state" with `/health` at 200, for at least 4 minutes. Re-running only the deploy step (rc 0, 00:51 to 00:53Z) cleared it, and the identity probe then passed first try. This matches D-ITEM-08-17-D from Phase 8.

**Context:** whether longer waiting would also have cleared it is unknown.
**Source:** 10-RENAME-DEPLOY-EVIDENCE.json (F-10-12-2), 08 deferred-items.md (D-ITEM-08-17-D)

---

## Patterns

### Declare a data-split rule in the script's docstring before the run, and pin the source
`prepare_goldstandard.py` states its grouping, conflict and split rules first, pins the CSV's sha256 and row counts so a changed file fails loudly, writes once and is byte-identical on re-run, and prints counts and sha256s only.

**When to use:** any new dataset for a Laya classifier; copy the script and change the pins, the label map and the task text.
**Source:** 10-01-SUMMARY.md

---

### Prove a "no text leaves the machine" promise with a planted sentinel
The selftest plants a sentinel in synthetic rows and asserts it is absent from captured stdout and stderr on the success path and on each malformed-input path, with a control proving both streams are captured.

**When to use:** any script that handles data that must not be logged.
**Source:** 10-01-SUMMARY.md

---

### Make the served door a parameter of a measurement, so gates assert facts of the prefix only
`--measure` reports the largest door a task admits and refuses only a real cut or an explicitly passed `--max-texts` that overruns; gate rows assert facts of P, never the relation to the live contract constant.

**When to use:** a gate row that must keep passing if a later plan lowers a contract constant.
**Source:** 10-01-SUMMARY.md

---

### A pure adapter with I/O only at the edge, hashes computed by the crate
`laya_dir_to_canonical` is a bytes-in, bytes-out `adapt` function with I/O only in `run()`; every hash comes from the crate's own doors, and criteria are read through an ordered-map visitor so the label index is document order, not lexical.

**When to use:** converting between two data layouts that each have a validating loader.
**Source:** 10-05-SUMMARY.md

---

### Read exit status from a file for long detached runs, and prove the device from a trace line
Hour-long runs ran detached with a log and an rc file; the device was read back from the `DEVICE seed=13 requested=mps used=mps:0` line, not taken from the request. Host load was sampled during the probe, and recorded as unmeasured when it was not.

**When to use:** any run longer than the shell's foreground limit, and any claim about which device was used.
**Source:** 10-03-SUMMARY.md, 10-04-SUMMARY.md

---

### Read-only access checks before a live step, never from the owner's silence
Access was established by `aws sts get-caller-identity` and by reading only the `expires_at` key of the credentials file, recorded as `access_confirmed` true or false with the failing check named, and the executor never refreshed credentials.

**When to use:** before any step that writes to the owner's AWS or pmcp.run account.
**Source:** 10-07-SUMMARY.md, 10-08-SUMMARY.md

---

## Surprises

### The plan's own test contradicted itself
10-02's `run_count_refusal_fails_when_the_server_accepts` asked for a limit one higher than the server's, which sends max + 2 texts and is refused, so the server could not be shown accepting. The executor used `max_texts - 1` to preserve the intent.

**Impact:** a plan-text bug found only by writing the test; no behaviour change.
**Source:** 10-02-SUMMARY.md

---

### The gate run finished faster than the probe projected, while pack and verify ran slower
The three-seed run took 4.67 h wall (`total_seconds` 16822.3, training plus calibration plus scoring) against the probe's 5.13 h projection (4.13 h training plus the 1.0 h scoring tail). Pack and verify each took about 21 minutes against the assumed 18.

**Impact:** contention did not visibly cost time in the run, but host load was not sampled during it, so contention is unmeasured, not ruled out; the verify-time overrun came from the loaded host.
**Source:** 10-04-SUMMARY.md, 10-03-SUMMARY.md, 10-GATE-RUN-EVIDENCE.json

---

### The seed with the best margin was not the shipped seed
Seed 17 had the highest macro-F1 (0.8054) and margin, but seed 23 shipped because the contract's rule selects the median ECE. Seed ECE ranged from 0.0117 to 0.0456 (standard deviation 0.0187).

**Impact:** shipped macro-F1 is 0.7956, not the best seen; the selection rule is about calibration, not accuracy.
**Source:** 10-04-SUMMARY.md, 10-GATE-RUN-EVIDENCE.json

---

### Phase 8's resolver proof was stale against the installed tool
The committed proof was for cargo-pmcp 0.24.3 while 0.28.1 was installed, so `laya-deploy` would have refused. Re-proving moved the SDK commit and the builder revision.

**Impact:** found offline in 10-07 instead of after an hour of upload.
**Source:** 10-07-SUMMARY.md, 10-LIVE-DEPLOY-EVIDENCE.json

---

### cargo-pmcp 0.28.1 can create a brand-new server id
The plan listed first creation as unverified because the repo only showed updates of an existing id; the owner-run `laya-deploy` created the function (F-10-12-3).

**Impact:** removes a risk for the next team deploying a new id; the two edge problems above still apply.
**Source:** 10-RENAME-DEPLOY-EVIDENCE.json (F-10-12-3)

---

### The deployment-unit register and the duplicate-bin allowlist guard a rename
FALSIFY-MONO-011 is a shrink-only register at 9 that needs a recorded owner decision to grow, and `duplicate_bin_names_allowlist.txt` must equal the observed `bootstrap` claimants exactly. A rename touches both, which is why the design swapped an entry (9 to 9) instead of adding one.

**Impact:** a second model's thin crate is growth and needs its own recorded decision, not a same-PR edit.
**Source:** 10-RENAME-DEPLOY-EVIDENCE.json (design), crates/aprender-mcp-decide-lambda/README.md

---

### A cold request has only about 4.5 to 5 s of slack to the 30 s gateway cap
Cold samples ran 22.8 to 25.5 s across both deploys (15.3 s of it model load: about 9 s S3 download, 3.5 s sha256, 2.8 s build), worst slack 4.5 s; warm calls took about 1.1 s. A longer prefix, bigger artifact or more texts per call can push a cold request over the cap.

**Impact:** nothing keeps the Lambda warm today, so the first call after idle pays the load again; keep-warm or provisioned concurrency is unpriced.
**Source:** 10-08-SUMMARY.md, 10-RENAME-DEPLOY-EVIDENCE.json
