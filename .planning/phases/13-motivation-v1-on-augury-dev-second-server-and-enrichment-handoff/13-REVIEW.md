---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
reviewed: 2026-10-28T00:00:00Z
depth: standard
files_reviewed: 8
files_reviewed_list:
  - contracts/census.json
  - contracts/contracts.nt
  - contracts/motivation-enrichment-v1.yaml
  - justfile
  - scripts/laya_deploy/m2m_token.py
  - scripts/laya_gates.tsv
  - scripts/laya_train/prepare_motivation.py
  - scripts/laya_train/select_motivation.py
findings:
  critical: 1
  warning: 10
  info: 7
  total: 18
status: issues_found
---

# Phase 13: Code Review Report

**Reviewed:** 2026-10-28
**Depth:** standard
**Files Reviewed:** 8
**Status:** issues_found

## Summary

Scope: the eight files listed above. For the justfile that meant the hunks in `git diff 4979e1c1a HEAD -- justfile`: the target, account, state-dir and endpoint helpers, the region parameter, the D-07 auth recipes, the re-exec in `laya-deploy` and `laya-deploy-verify`, and the three new selftests. `census.json` and `contracts.nt` agree with the new contract: n_files goes from 1851 to 1852, schema from 768 to 769, unanchored from 1843 to 1844, and the five triples carry id `motivation-enrichment-v1`, kind `schema` and the right file path. The merge tree now holds 1852 counted contract files, the same instrument `check_readme_claims.sh` uses.

The validators in `prepare_motivation.py` and `select_motivation.py` match the contract value for value: patterns, enums, caps, the 12 mapping cells and the 17 and 13 refusal names. The merge rules are applied in the declared order. No verification-discipline violation was found: no `$?` is read through a pipe in the new recipe code, and the new guard regexes have case tables.

The main defect is in the D-07 redeploy path. `laya-deploy` cannot tell that the augury server is OAuth-protected. Without `LAYA_M2M_FILE` it deploys and then throttles a healthy production server to zero. With the file set, it cannot see a `--no-oauth` redeploy that turns OAuth off. The other findings, in summary:

- The minted token is held far longer, and seen by far more processes, than the "one command" design claims.
- The in-repo guards can be bypassed on macOS by spelling a path with different case.
- The D-06 triple ignores credentials set in environment variables.
- The probe-argv WIRING guard cannot see the multi-line form that `laya-deploy` actually uses.
- One "offline" selftest reads the developer's real `~/.pmcp/config.toml`.
- The README contract count now trails by two.

## Critical Issues

### CR-01: An auth-on redeploy has no guard: it either contains a healthy server or silently reopens it (BLOCKER)

**File:** `justfile:2239-2283` (block 4b), `justfile:2347-2353` (`AUTH_ON` / `--no-oauth`), `justfile:2404-2418` (identity probe, then `contain`)

**Issue:** The augury server was published with `[auth] enabled = false`, because cargo-pmcp cannot render an auth descriptor for pmcp-run (13-HANDOFF.md:97). The owner then turned OAuth on out of band, so nothing in the config or the state dir tells `laya-deploy` that this server needs a bearer. Block 4b only adds a token when the operator remembers `LAYA_M2M_FILE`. Two failure paths follow:

1. **`LAYA_M2M_FILE` unset (an omission).** The recipe uploads, runs `cargo pmcp deploy`, then runs the identity probe with no bearer. tools/list answers 401 (RESEARCH P6), the output does not match the 503 retry pattern, and `contain "the identity probe failed..."` sets reserved concurrency 0 on the live, healthy function. The 4b comment names this outcome ("this recipe would CONTAIN a healthy function") but does nothing to prevent it.
2. **`LAYA_M2M_FILE` set.** The deploy always passes `--no-oauth`, because the config says auth is off, and the hand-off says that "may reset OAuth" (RESEARCH A3). If it does, the probe still succeeds, since an open server accepts a request that carries a bearer. The recipe prints `DEPLOYED`, and the partner-facing server is now unauthenticated. The only detector is `_laya-auth-refusal-check`, which the recipe never calls; the hand-off leaves it as a manual follow-up. That is the CLAUDE.md rule 5 failure: the guard is not on the surface where the decision is made.

**Fix:** Persist a non-secret marker when OAuth is enabled (for example `$SD/oauth-enabled-$SERVER`, written by the 13-08 enablement step). In `laya-deploy`, refuse before any AWS write when the marker is present and no token will be minted. After a successful identity probe, run the refusal check and fail loudly if the server is open:
```bash
OAUTH_MARK="$SD/oauth-enabled-$SERVER"
if [ -e "$OAUTH_MARK" ] && [ -z "${LAYA_M2M_FILE:-}" ] && [ "${LAYA_M2M_MINTED:-}" != "1" ]; then
    refuse probe-auth "$SERVER has OAuth enabled ($OAUTH_MARK); set LAYA_M2M_FILE or the identity probe will contain a healthy function"
fi
# ... after the identity probe succeeded:
if [ -e "$OAUTH_MARK" ]; then
    just _laya-auth-refusal-check "$ENDPOINT" 0 \
        || { echo "AUTH REGRESSION: $ENDPOINT answers unauthenticated tools/list after a --no-oauth redeploy; re-enable OAuth now" >&2; exit 1; }
fi
```

## Warnings

### WR-01: The token is minted at the start of the run and is visible to the whole build and deploy toolchain (WARNING)

**File:** `justfile:2280` (`laya-deploy` re-exec), `justfile:4131` (`laya-deploy-verify` re-exec), `scripts/laya_deploy/m2m_token.py:30-31`

**Issue:** Both recipes re-exec themselves, from the top, under the helper. `APRENDER_DECIDE_PROBE_TOKEN` is therefore in the environment of `laya-verify`, `cargo pmcp deploy` (every `build.rs` and proc-macro in the dependency graph, plus cargo-pmcp itself), `cargo run --example probe` and `cargo build`. The helper's "ACCEPTED RISK" covers same-user processes while one command runs. In practice the token is handed to all third-party build code for up to about 40 minutes. The token also has to outlast eligibility, upload, build, deploy, a 900 s edge-health wait and the settle loop, so the 2400 s floor exists only because the mint happens early. A token that expires mid-run contains a healthy function (the RISK the comment documents).

**Fix:** Mint around the probe invocations only, the way `laya-probe-auth` already does. Each probe call becomes `python3 -I scripts/laya_deploy/m2m_token.py --file "$LAYA_M2M_FILE" --expect-host "$TH" -- "$PROBE" --url ...` (or the `cargo run ... --example probe --` form). The re-exec of the whole recipe can then go, the token never reaches cargo or cargo-pmcp, and the default floor can drop back to 900 s.

### WR-02: The in-repo guards can be bypassed by a case-variant path on macOS (WARNING)

**File:** `scripts/laya_train/prepare_motivation.py:360-366` (`_under_repo`), used by `read_enrichment` (line 383) and by `select_motivation.py:252` and `:1100` (`--records`, `--out`)

**Issue:** `Path.resolve()` follows symlinks but does not fold case, and APFS (this platform) is case-insensitive. I checked this. `/Users/guy/development/machine-learning/APRENDER/contracts/motivation-enrichment-v1.yaml` exists, yet `resolve().relative_to(REPO)` raises, so `_under_repo` returns False. One mistyped letter therefore gets past `select-out-in-repo`, which writes the post_id selection into the checkout. It also gets past `select-in-repo` and `enrichment-in-repo`. These three guards are the "never copied here" promise.

**Fix:** Compare filesystem identity, not strings. Walk the resolved path's ancestors and test `os.path.samefile(ancestor, REPO)`, or compare `(st_dev, st_ino)` of each ancestor with REPO's. For `--out`, apply the walk to the nearest existing ancestor. Add a case-variant row to both selftests' refusal tables.

### WR-03: The D-06 triple ignores credentials set in environment variables (WARNING)

**File:** `justfile:1759-1763` (`_laya-target-check` `agree`), `justfile:1800-1826` (`_laya-account-check`)

**Issue:** The check covers `PMCP_TARGET`, `AWS_PROFILE`, `AWS_REGION` and `AWS_DEFAULT_REGION`. It does not cover `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`, `AWS_CONFIG_FILE` or `AWS_SHARED_CREDENTIALS_FILE`. The account check runs `aws sts get-caller-identity --profile "$1"`. botocore treats an explicit `--profile` as overriding environment credentials, so the check reports the profile's account. A consumer that resolves through the exported `AWS_PROFILE` instead uses environment credentials first: in the AWS SDK default chain, environment credentials beat the profile. With stale static keys in the shell, the account check passes for 783960389328 while such a consumer acts on another account. I did not verify whether cargo-pmcp reads AWS credentials for the pmcp-run target. Every `aws` call in the recipes passes `--profile` and is safe.

**Fix:** In `_laya-target-check`, refuse when any of those variables is set and non-empty (`REFUSED target: AWS_ACCESS_KEY_ID is set; ...`). Alternatively, `unset` them next to the existing `unset PMCP_API_URL AWS_DEFAULT_REGION` in every wired recipe. Either way, add must-refuse rows to the TARGET case table.

### WR-04: The probe-argv WIRING guard cannot see multi-line probe invocations (WARNING)

**File:** `justfile:4851-4856` (`probe_lines` in `_laya-auth-selftest`), with the probe calls at `justfile:2407-2408` and `justfile:4184-4185`

**Issue:** `probe_lines` checks one rendered line at a time, keeping only lines that contain `"$PROBE"` or `--example probe`. The `laya-deploy` identity probe is `cargo run ... --example probe -- \` with its arguments (`--url "$ENDPOINT" --apr ...`) on the next line. The cold sample in `laya-deploy-verify` is also split with a trailing `\`. A `--bearer "$X"` on a continuation line, the form these recipes actually use, is never inspected. The guard's case table only plants single-line forms, so it passes on exactly the form it cannot see (CLAUDE.md rules 5 and 7).

**Fix:** Join backslash-continued lines before scanning (`re.sub(r"\\\n\s*", " ", "\n".join(body))`, then split). Add a must-match row to the case table with the bearer on a continuation line: `[f'    {pv} --url "$U" \\', '        --bearer "$X"']`.

### WR-05: The "offline" auth selftest reads the developer's real pmcp config (WARNING)

**File:** `justfile:4757` (`live()` in `_laya-auth-selftest`)

**Issue:** The RE-EXEC LIVE cases run with `env -u DRY_RUN -u LAYA_PMCP_CONFIG`, because `LAYA_PMCP_CONFIG` is refused without `DRY_RUN=1`. Every `_laya-target-check` in those children therefore reads `$HOME/.pmcp/config.toml`. The cases pass only on a machine whose real config has a correctly shaped `[targets.augury-dev]`. On CI, on a fresh machine, or after the owner edits that config, `AUTH SELFTEST OK` fails, and with it rows `auth-selftest` and `probe-auth` and `laya-deploy-selftest`. This contradicts the selftest family's own claim that "no case reads the real config".

**Fix:** Keep the fixture in force for these cases. Add a flag honoured only under `DRY_RUN=1` that forces the live re-exec branch (for example `LAYA_M2M_REEXEC_FORCE=1`, the same pattern as `LAYA_ACCOUNT_CHECK_FORCE`), and run the cases with `DRY_RUN=1 LAYA_PMCP_CONFIG=<fixture>`.

### WR-06: Under `DRY_RUN=1`, `laya-probe-auth` still mints and probes a real endpoint, without the account check (WARNING)

**File:** `justfile:4283-4324`

**Issue:** Under `DRY_RUN=1`, `_laya-account-check` makes no sts call. `_laya-endpoint-check` accepts both loopback and real tenant URLs, and the recipe then really mints a token from `LAYA_M2M_FILE` and runs the probe. A `DRY_RUN=1` left in the shell, for example by a selftest, turns this into a live authenticated call with the D-06 account binding switched off. Everywhere else, `DRY_RUN=1` means no network.

**Fix:** Under `DRY_RUN=1`, refuse any URL that is not loopback:
```bash
if [ "${DRY_RUN:-0}" = "1" ] && [ "$G" != "loopback" ]; then refuse "DRY_RUN=1 probes only the loopback stub; unset DRY_RUN for a live probe"; fi
```

### WR-07: The enrichment manifest drops the header's provenance (the model shown and the guide version) (WARNING)

**File:** `scripts/laya_train/prepare_motivation.py:432-439` (validated), `:497-499` (`counts`), `:558` (`sources`)

**Issue:** `artifact_sha256` ("the model whose output the expert saw") and `guide` (the guide designation) are validated and then discarded. Neither reaches the manifest or stdout. The guide is not frozen and a retrain is expected after the freeze (D-01), and D-16 says the hand-off of any retrain on `prediction_shown` data must disclose the anchoring. Once several rounds exist, the manifest cannot say which guide draft the labels follow or which model's predictions anchored them.

**Fix:** Record both, per source. Neither is post text: the guide field is the expert's version string, capped at 200 characters.
```python
return {"id": sid, "sha256": sha, "annotation_mode": mode, "artifact_sha256": art, "guide": guide, ...}
# manifest sources: dict({"id": ..., "sha256": ..., "shown_artifact_sha256": s["artifact_sha256"], "guide": s["guide"]}, **s["counts"])
```
If the guide string is judged to be partner content, record `sha256(guide)` instead.

### WR-08: The ingest's `--out` is not limited to the gitignored data tree (WARNING)

**File:** `scripts/laya_train/prepare_motivation.py:1350-1356`

**Issue:** With `--enrichment`, the only output refusal is `--out` resolving to `data/decide/motivation-v1`. An enriched `train.jsonl` holds the expert's annotated partner posts, and nothing stops it being written into a tracked directory (`--out contracts/x`, `--out crates/...`). The docstring says output "lands under the root-anchored, gitignored /data/", and the contract's `enrichment-out` reads "or otherwise refused", but nothing else is refused. `select_motivation.py` refuses an in-repo `--out`; the ingest has no matching check.

**Fix:** Refuse `enrichment-out` unless `--out` resolves outside the repository or under `REPO / "data"` (using the identity comparison from WR-02). Optionally also confirm `git check-ignore -q <out>/train.jsonl`.

### WR-09: An OS error prints a traceback that includes the partner file's path (WARNING)

**File:** `scripts/laya_train/prepare_motivation.py:387-391` and `:1376-1381`; `scripts/laya_train/select_motivation.py:255` and `:1114`

**Issue:** `open(path, "rb")` and `path.read_bytes()` can raise `PermissionError`, `IsADirectoryError` (if the path changes after `is_file()`) or `OSError`. Both `main()` functions catch only `DataError` (and `PrepareFailed`), so Python prints a traceback whose message contains the full path, e.g. `PermissionError: [Errno 13] Permission denied: '/.../<file name>'`. Both scripts promise that "a file name is never printed (it could carry a post fragment)". A mode-000 file or an unreadable directory component is enough to trigger it.

**Fix:** Wrap the reads, and give the `main()` functions a catch-all that never quotes the path:
```python
try:
    with open(path, "rb") as f:
        raw = f.read(cap + 1)
except OSError as e:
    raise DataError("enrichment-missing", "%s: the --enrichment file cannot be read (%s)" % (sid, errno.errorcode.get(e.errno, "error")))
```

### WR-10: The new contract leaves the README contract count two behind (WARNING)

**File:** `README.md:55` and `README.md:335` (`CONTRACT_COUNT` block 1850), against `contracts/census.json` n_files 1852

**Issue:** `scripts/check_readme_claims.sh` (FALSIFY-README-002) treats the `CONTRACT_COUNT` block as an equality with the merge tree ("a generated number cannot legitimately lag"). The base was already one behind (README 1850, tree 1851), and this phase's contract makes it 1852. README.md is outside the reviewed file list, but this phase's contract is what widened the gap.

**Fix:** Run `make readme-sync` (after `make contracts`) and commit the regenerated block.

## Info

### IN-01: `LAYA_M2M_MINTED` is a self-asserted flag that switches off the pre-set-token refusal (WARNING, low)

**File:** `justfile:2251-2253`, `justfile:4108-4111`
**Issue:** `LAYA_M2M_MINTED=1 APRENDER_DECIDE_PROBE_TOKEN=<pasted> just laya-deploy ...` skips both the refusal and the mint. A token from anywhere is then used without the file, host and expiry checks that the comment says the refusal protects.
**Fix:** Have the helper put a value in `LAYA_M2M_MINTED` that the child can check against its own token (for example the first 16 hex characters of the token's sha256). Alternatively, rewrite the comment so it describes accident protection only.

### IN-02: The new re-exec line pastes `{{run}}`, `{{data}}` and `{{base}}` into shell text (WARNING, low)

**File:** `justfile:2280`
**Issue:** The pattern plan 13-01 removed from `laya-deploy-config`, because a quote in a value breaks the recipe, is reintroduced in new code. `laya-deploy` is not `[positional-arguments]`.
**Fix:** Mark `laya-deploy` `[positional-arguments]`, read `$2`, `$3` and `$4`, and re-exec with `"$@"`.

### IN-03: Helper rule naming and port leniency (WARNING, low)

**File:** `scripts/laya_deploy/m2m_token.py:112-122`, `:157-170`
**Issue:** An oversize client file is refused as `m2m-file-type`. An https `token_url` with any explicit port (for example `https://host:8443/oauth2/token`) is accepted.
**Fix:** Give the size check its own rule, or say "size" in the message. Reject an explicit port other than 443 unless the URL is loopback.

### IN-04: Record files are read with no size cap (WARNING, low)

**File:** `scripts/laya_train/select_motivation.py:255`
**Issue:** `path.read_bytes()` has no limit, while the enrichment ingest caps file bytes and rows. The contract declares no caps for `motivation-record-v1`.
**Fix:** Declare a `caps.file_bytes` value for the record format and mirror it here.

### IN-05: The contract's `deployment.this_round` triple is declared but not enforced (WARNING, low)

**File:** `contracts/motivation-enrichment-v1.yaml` (`formats.motivation-record-v1.fields.deployment.this_round`), `scripts/laya_train/select_motivation.py:227-234`
**Issue:** The selector checks only the shape of each deployment value. Records scored by a ze-kasher-dev deployment of the same artifact are accepted as this round's.
**Fix:** Either enforce `pmcp_target`, `aws_account` and `region` against `this_round` (with an override flag), or mark the block informational in the contract.

### IN-06: "Outside the checkout" is not enforced for `LAYA_M2M_FILE` (WARNING, low)

**File:** `justfile:4291`, `scripts/laya_deploy/m2m_token.py:93-145`
**Issue:** The recipe comments require the client file to live outside the checkout, but neither the helper nor the recipes check it. A secret file in a tracked directory would pass every other check.
**Fix:** In `read_client_file`, refuse a path whose resolved location is inside the repository root (using the identity comparison from WR-02).

### IN-07: The default region conflicts with the second profile, and the snapshot covers only one extra profile (WARNING, low)

**File:** `justfile` (`laya-deploy-config ... region="us-east-1"`, `laya-weights-bucket ... region="us-east-1"`, `_laya-state-snapshot`)
**Issue:** With `profile=augury-dev` and the default region, both recipes are refused, so the default is wrong for every non-default profile. `_laya-state-snapshot` covers only `models/decide/augury-dev`, so a third profile's state would be invisible to the snapshot comparison.
**Fix:** Default `region=""` (meaning the table's region). In the snapshot, walk every directory under `models/decide` whose name passes the profile ERE.

---

_Reviewed: 2026-10-28_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
