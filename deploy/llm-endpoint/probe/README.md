# llm-endpoint probe: the conformance harness for `apr serve`

This directory is the harness behind `contracts/llm-endpoint-openai-conformance-v1.yaml`
(Phase 9, D-07). pmcp.run talks to a model through two OpenAI-compatible clients (the platform's
durable-agent-lambda `openai_v1` and the SDK's pmcp-agent `OpenAiCompatSource`); the probe replays
the request shapes both send, and the Rust reader test
`cargo test -p aprender-serve --lib api::llm_endpoint_conformance_tests` checks every record the
probe writes. Nothing this phase claims about `apr serve` conformance is licensed except by a row
of the contract and a record the reader has accepted.

| File | What it is |
|------|------------|
| `probe_openai.py` | the conformance probe: stdlib only, run it as `python3 -I`; writes a labelled, profiled record |
| `auth_probe.sh` | the bearer-auth table: `--expect gated` or `--expect open` asserts the result instead of leaving it to the eye |
| `results/` | the records. Only `conformance-*.json` files are read by the reader |

## Running the probe

Start `apr serve` on a model (pin the binary: `. scripts/apr_bin.sh`, never a bare `apr`), then:

```bash
# Open server, the pinned release as measured (spike-029 verdicts, unchanged):
python3 -I deploy/llm-endpoint/probe/probe_openai.py \
    --base http://127.0.0.1:8029/v1 --model Qwen3.5-0.8B-Q4_K_M \
    --profile baseline --label release-local --pin v0.70.3 \
    --server-version "apr 0.70.3 (95f64ba14)" \
    --out deploy/llm-endpoint/probe/results/conformance-v0.70.3-release-local.json

# Gated server (the D-04 auth gate present); the key comes from the environment, not argv:
APR_PROBE_KEY=... python3 -I deploy/llm-endpoint/probe/probe_openai.py \
    --base http://127.0.0.1:8029/v1 --model Qwen3.5-0.8B-Q4_K_M \
    --profile fixed --label release-local --pin v0.71.4 \
    --server-version "apr 0.71.4 (abcdef123)" \
    --out deploy/llm-endpoint/probe/results/conformance-v0.71.4-release-local.json

# The same, on a release where the D-03 content fix and the D-05 alias are not yet tagged:
#   --unreleased "tool result -> final answer=PR #NNNN (merged upstream, untagged)" \
#   --unreleased "max_completion_tokens honoured=PR #NNNN (open)"

# Auth table (bash, not zsh):
APR_PROBE_KEY=... bash deploy/llm-endpoint/probe/auth_probe.sh --expect gated 8029
bash deploy/llm-endpoint/probe/auth_probe.sh --expect open 8029
```

Arguments the probe requires: `--base`, `--model`, `--out`, `--profile`, `--label`, `--pin` and
`--server-version`. `--label` is checked against the label set below before any request is sent, so
an unknown label is refused up front. `--known-limitations` is repeatable (a probe name contains
commas, so there is no list syntax) and defaults to the five names under "Known limitations".

Exit status: `0` on a written record; `2` when a `fixed` run contains a cross (the record is still
written, so the failure can be read). A probe that cannot turn red is theater: pointed at an open
server in the `fixed` profile, the `no Authorization header` row reads `✗` and the run exits `2`.

The `just llm-endpoint-probe base model label server_version` recipe the contract's row 007 names as
the live harness wraps these commands: it accepts only `release-local`, `interim` and
`deployed-https`, names the record per the contract below, and runs the reader test right after
writing it. See `deploy/llm-endpoint/README.md`, section "Probe and measure".

## The probe-record contract

Stated once here; the reader (`api::llm_endpoint_conformance_tests::check_record`), the probe and
every later writer agree with it verbatim.

- **Directory:** `deploy/llm-endpoint/probe/results/`, flat. No subdirectory is read.
- **Reader input glob:** `conformance-*.json` directly in that directory, non-recursive. The reader
  FAILS (never skips) when the glob matches zero files. Every other file in the directory
  (`auth-table-*.md`, `agent-run-*.json`, `platform-turn-evidence.json`, `UNEXPECTED-*.json`) shares
  the directory but is outside the glob and is never opened, so a malformed sibling cannot break a
  full-suite rerun.
- **File name:** `conformance-<pin>-<label>.json` by default
  (`conformance-v0.71.4-release-local.json`, `conformance-<tag>-deployed-https.json`). Three
  fixed-name exceptions: `conformance-v0.70.3-spike029-baseline.json` (the transcription of spike
  029, committed by plan 09-01), `conformance-interim-upstream-main-plus-fixes.json` (plan 09-05,
  label `interim`) and `conformance-<tag>-deployed-invoke.json` (plan 09-07, label `deployed`,
  written through `lambda:Invoke`). The reader never parses the file name: the header is the truth.
- **Header keys the reader REQUIRES:** `label`, `profile`, `pin`, `server_version`,
  `probes[]` (`{probe, verdict, status, ...}`) and `known_limitations[]`. Keys writers carry and the
  reader IGNORES: `model`, `base`, `generated_at`, `source`, `notes`, `transport`
  (`lambda-invoke` on plan 09-07's record; absent on HTTP records).
- **Label set:** `baseline | release-local | interim | deployed | deployed-https`.
  **Profile set:** `baseline | fixed`.
- **`server_version` rule:** `interim` is NOT a bare release string (it carries a non-release git
  sha: a `+`, a `-g<sha>` or the word `interim`); `release-local`, `deployed` and `deployed-https`
  start with `apr <tag digits>`; `baseline` is as transcribed from the spike.
- **`pin`:** the release tag (`vX.Y.Z`) for release builds; the fixed string `upstream-main+PRs`
  for `interim`. The reader ignores `pin`; the brief quotes it.
- **Probe-name rules.** `probes[].probe` names are unique in every profile (a duplicate could hide a
  `✗`). The `fixed` profile REQUIRES all eight of `tool result -> final answer`,
  `tool_choice=required`, `max_completion_tokens honoured`, `error envelope {error:{message,type}}`,
  `foreign model id`, `tool_choice=none`, `no Authorization header` and `cpu_part in /health`.
  `known_limitations[]` may hold only the five D-05 gaps (`tool_choice=required`,
  `error envelope {error:{message,type}}`, `foreign model id`, `tool_choice=none`,
  `cpu_part in /health`) plus, only with a matching `unreleased_at_pin` entry, `tool result -> final
  answer` and `max_completion_tokens honoured`. `no Authorization header` is NEVER a limitation: in
  the `fixed` profile it must read `✓`, so an open server cannot be recorded as fixed. In the `fixed`
  profile `cpu_part in /health` reads `✓` (the server reports it) or `KNOWN LIMITATION` (and then it
  is in `known_limitations`), never `✗` or absent; in the `baseline` profile it reads `ℹ`, so the
  baseline `✗` set stays the spike's four. The committed spike-029 transcription predates the probe
  and is exempt from the required-probe set. The baseline record is immutable: release-specific
  drift is a different profile or an `UNEXPECTED-*` file, never an edit.
- **`unreleased_at_pin`** (OPTIONAL header key; absent means empty): a list of `{probe, reason}` for
  fixes that are not in the pinned release (merged upstream but unreleased, or still a pending PR).
  In the `fixed` profile an entry's `probe` is one of the two unreleased-eligible probes
  (`tool result -> final answer`, `max_completion_tokens honoured`); its `reason` contains the words
  `unreleased at pin`; the named probe reads `KNOWN LIMITATION` and is listed in
  `known_limitations` (an entry beside a `✓` is a contradiction and is refused). Conversely an
  eligible probe that reads `KNOWN LIMITATION` with no entry is refused.

## Profiles

- **`baseline`**: the spike-029 verdicts, unchanged. The `✗` set is exactly the four spike gaps
  (`tool result -> final answer`, `tool_choice=required`, `max_completion_tokens honoured`,
  `error envelope {error:{message,type}}`). `foreign model id`, `no Authorization header` and
  `cpu_part in /health` read `ℹ`. Used for an open server: the release as measured.
- **`fixed`**: the D-04 auth gate is present (`no Authorization header` reads `✓`, because a request
  without the bearer is a 401) and no `✗` remains. It does NOT mean all three upstream fixes are
  released: D-05 says none of the other 029 fixes may gate the end-to-end turn, so the D-03 content
  fix and the D-05 alias may read `KNOWN LIMITATION` when the record says, through
  `unreleased_at_pin`, that they are unreleased at the pin. The records say which ones were missing.

## Known limitations

A `KNOWN LIMITATION` verdict means "the probe computed a cross, and the contract records the gap as
a limitation, not a fix". Each of the five is one that D-05 leaves open:

- **`tool_choice=required`**: honouring it needs constrained decoding. The server behaves as `auto`,
  and neither pmcp.run client sends the field.
- **`error envelope {error:{message,type}}`**: the server answers `{"error": "<string>"}`. That shape
  is contracted (routes.rs:321 and FALSIFY-AUTH-001), so this phase never reshapes it.
- **`foreign model id`**: `model` is echoed, not checked. Making it load-bearing would break
  `LLMModel.modelId` against a server whose `/v1/models` answers `default`; an opt-in strict mode is
  the upstream proposal.
- **`tool_choice=none`**: the tool-call text can leak into the content through the shared render
  path (`qwen35_chat_backend.rs:109`, `format_chat_messages_official_thinking`), which fixing needs
  upstream.
- **`cpu_part in /health`**: `cpu_part` is recorded by the entrypoint's `[entry]` log line, not by
  `/health`.

Two further probes may read `KNOWN LIMITATION` only through `--unreleased` and a matching
`unreleased_at_pin` entry: `tool result -> final answer` (the D-03 content fix) and
`max_completion_tokens honoured` (the D-05 alias). `--unreleased` is accepted only with
`--profile fixed`, only for those two probes, and refuses anything else (in particular
`no Authorization header`) at argument parsing.

## No key value is ever recorded

The probe sends the bearer key (from `--key` or `APR_PROBE_KEY`) and records request bodies only, so
no `Authorization` value reaches a record. As a second guard the probe refuses to write the record
if the key's value appears anywhere in it. No API key value, plaintext or hash, belongs in a
committed record, the contract or this file. `auth_probe.sh` never prints the key either.
