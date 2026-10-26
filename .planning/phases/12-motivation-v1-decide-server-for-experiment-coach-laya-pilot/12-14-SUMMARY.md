---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 14
subsystem: ml-decision-classifier
tags: [kev-4b, qwen3.5, gguf, batched-prefill, parity, leakage, cost-projection, owner-checkpoint]

requires:
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: "12-01 shared motivation-v1 split and task evidence (guide docx sha256); 12-13 live Laya timing, after which the main target/ was free to clean"
provides:
  - "Kev-4B Rust path (PEFT merge fp32 -> f32 GGUF -> upstream Qwen35Model batched prefill + pointer head) proven against a torch fp32 oracle"
  - "spike 033 driver with parity and identity-bound resumable dump modes"
  - "395 motivation rows encoded from the shared split (ids only) with the guide prompt (Sections 4.1-4.4, 5, 6; 4.5 omitted)"
  - "12-KEV-ARM-EVIDENCE.json: disk, GGUF sha256, parity, prompt, leakage, one timed train row, projections (outcome prepared)"
  - "12-KEV-OWNER-REPLY.json: owner chose option full, verbatim, bound to the evidence hash"
affects: [12-15]

actuals:
  tokens: 25488
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "identity-bound resumable scoring: dump writes an identity header and refuses a resume whose header differs"
    - "exporter writes the merged backbone state dict directly as safetensors shards (one resident 4B model)"
    - "prove the mechanism on public rows and one synthetic long row before spending hours"

key-files:
  created:
    - .planning/spikes/033-motivation-kev-4b-arm/README.md
    - .planning/spikes/033-motivation-kev-4b-arm/.gitignore
    - .planning/spikes/033-motivation-kev-4b-arm/Cargo.toml
    - .planning/spikes/033-motivation-kev-4b-arm/Cargo.lock
    - .planning/spikes/033-motivation-kev-4b-arm/src/main.rs
    - .planning/spikes/033-motivation-kev-4b-arm/tools/export_kev4b.py
    - .planning/spikes/033-motivation-kev-4b-arm/tools/extract_guide.py
    - .planning/spikes/033-motivation-kev-4b-arm/tools/encode_rows.py
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-KEV-ARM-EVIDENCE.json
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-KEV-OWNER-REPLY.json
  modified:
    - .planning/spikes/MANIFEST.md

key-decisions:
  - "Owner chose option full (100 eval + all 295 train rows, bias and temperature fitted on train): projected 18.8 h CPU, peak RSS about 40 GB; compute over 1 hour authorised for plan 12-15"
  - "Kev-4B stays a reference arm (D-06): Laya is served regardless"

requirements-completed: [D-07, D-11, D-24, D-06]

coverage:
  - id: D1
    description: "Kev-4B Rust path proven against the torch fp32 oracle: 9 short rows max |dp| 2.8e-6 and one 6,808-token synthetic row 8.9e-7, every argmax equal, tolerance 1e-4"
    requirement: "D-07"
    verification:
      - kind: other
        ref: "bash plan-14 Task 1 <automated> verify (4B RUST PATH OK parity 2.8014183044433594e-06 argmax 9 / 9 | long measured 6808 8.940696716308594e-07)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Guide prompt (4.1-4.4, 5, 6 without 4.5) 6,812 Qwen tokens, 395 rows encoded from the shared split with ids only, leakage 0 and 0, one train row timed, hours projected for full, subset and zero-shot"
    requirement: "D-11"
    verification:
      - kind: other
        ref: "bash plan-14 Task 2 <automated> verify (KEV PROBE OK warm 155.882335083 s/row; full 18.8 h; subset 7.6 h; NO POST TEXT in the spike or the evidence)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Owner budget choice recorded verbatim and bound to the evidence sha256"
    requirement: "D-24"
    verification:
      - kind: other
        ref: "bash plan-14 Task 3 <automated> verify (KEV CHOICE RECORDED full) plus a verbatim string equality check"
        status: pass
    human_judgment: false

plan_head_before: f25babd5accbb0f9b91f5de31b7edd3a31140632
plan_head_after: bf227dabcf0b950e9528fdbf256df593955e7a44

duration: multi-session (task commits 2026-10-26 15:45 to 16:03 PT; Task 3 spans the owner checkpoint)
completed: 2026-10-26
status: complete
---

# Phase 12 Plan 14: Kev-4B arm preparation and the owner's budget choice Summary

**Kev-4B runs in Rust from the in-place spike/016-upstream-sync branch and matches the torch fp32 oracle to 2.8e-6 (short rows) and 8.9e-7 (a 6,808-token row); one real train row costs 155.9 s warm, so the owner chose the full arm (395 rows, about 18.8 h, peak RSS about 40 GB) for plan 12-15.**

## Performance

- **Duration:** multi-session; task commits 15:45 to 16:03 PT on 2026-10-26, Task 3 waited on the owner
- **Tasks:** 3 (tracer, auto, blocking-human decision)
- **Files:** 11 changed across the three task commits (10 created, MANIFEST.md modified)

## Accomplishments

- **Disk (D-24):** `cargo clean` on the main checkout freed 19.7 GiB (28.6 to 48.0 GiB free). After the GGUF sha256 was recorded, only the merged fp32 HF directory (15.7 GiB) was removed because free disk was below 20 GiB; free was then 27.9 GiB. Nothing else was deleted (spike 017's models intact).
- **Branch used in place (D-07):** `spike/016-upstream-sync` at HEAD `32103ba83ce4165598ae31ddde89b5c9856b43f3`, dirty file Cargo.lock only (pre-existing); not landed, not modified.
- **GGUF:** `kev-4b-merged-f32.gguf`, 16,833,972,608 bytes, sha256 `549e40ca10ec1ba17b32b40cacace3c9e4dcef960234812f36984279f0ab0c22`.
- **Parity (T-12-47):** max |dp| 2.8e-6 over 9 short rows (six public stance rows plus the 3-question ticket) and 8.9e-7 on one seeded synthetic 6,808-token row; every argmax equal; tolerance 1e-4.
- **Prompt:** Sections 4.1-4.4, 5 and 6 without 4.5: 6,812 Qwen tokens (max row 7,011, under Kev's 8,192 limit), guide sha256 `837d915af312d65a706070ed5dd1a506e5e1d83f27e9eb52a9a18d663227174b`; about 6.6x Kev's 1,024-token training branch (outside its training distribution, recorded as the regime note). Options `social_affiliative`, `conspiratorial`, `other`.
- **Leakage (D-11):** eval posts in the guide 0, fit rows that are eval rows 0 (nfc-trim-ws-v1); no D-11 paragraph removal was needed.
- **Timed row:** one median-length TRAIN row (6,892 tokens, 14 threads): first touch 166.3 s, warm 155.9 s, peak RSS 12.97 GB measured; projected Rust peak RSS 40.4 GB (2.4 x the GGUF bytes) on a 48 GB host.
- **Projections (rows x warm_s x 1.1 / 3600):** full 395 rows 18.8 h; subset 160 rows 7.6 h; zero-shot 100 rows 4.8 h.
- **Owner choice (Task 3):** `full`, recorded verbatim in `12-KEV-OWNER-REPLY.json` with `evidence_sha256` 7304b8797a2a05f8c902d3594ff39c0dab10d2ba6b57216679904a8aa99cdd52 (the evidence file is unchanged since commit 9c66d7ac5). The owner's reply, verbatim: "Let's use the full option, to evaluate the performance of the kev option and compare it with the already trusted laya option. We want to develop the intuition and tools to create deciders for different use cases over time, and the motivation classifier is the second example after the antisemite posts classifiers. We can run the classifier in a batch mode, which can make the inference cost-effective even with the larger memory requirement, and even the GPU options that we saw can speed things up compared to the CPU option."

## Task Commits

1. **Task 1 (tracer): the 4B Rust path, disk, GGUF and parity** - `125843d36` (feat)
2. **Task 2: guide prompt, 395 encoded rows, leakage, timed row, projections** - `9c66d7ac5` (feat)
3. **Task 3: owner budget reply recorded** - `bf227dabc` (feat)

**Plan metadata:** the SUMMARY commit that follows this file (docs).

## Files Created/Modified

- `.planning/spikes/033-motivation-kev-4b-arm/` - driver crate (`parity`, identity-bound `dump`), `export_kev4b.py`, `extract_guide.py`, `encode_rows.py`, README, .gitignore, regenerated Cargo.lock
- `.planning/spikes/MANIFEST.md` - row 033 (verdict pending 12-15)
- `.planning/phases/12-.../12-KEV-ARM-EVIDENCE.json` - preparation and probe evidence, outcome `prepared`
- `.planning/phases/12-.../12-KEV-OWNER-REPLY.json` - schema `laya-kev-owner-reply-v1`, option `full`

## Decisions Made

- Option `full` is chosen and the over-1-hour compute is authorised for plan 12-15 by the owner's reply (CLAUDE.md check-in satisfied).
- The owner's remarks about batch mode and GPU are context, not an option change. They are recorded verbatim in the reply file. Plan 12-15 may consider them only inside its declared mechanism; no GPU path was picked here.

## Deviations from Plan

None in the plan's rules sense (no Rule 1-4 fixes in this continuation). Execution notes carried from Tasks 1 and 2, recorded for the verifier:

- The base checkpoint is multimodal, so the key-set equality was asserted on the text image: the written backbone equals the base's 426 text keys (297 visual and 15 mtp keys are not part of the Kev backbone; tie_word_embeddings true, so no lm_head). `merged_tensor_keys_equal_base` is true on that basis.
- The §4 lead-in text is kept, giving 6,812 tokens (inside the [6,500, 7,200] band).
- The base snapshot was read directly from the cached HF snapshot.
- The tokenizer is Kev's.
- Scratch paths are under the session scratchpad, not /tmp.
- The evidence outcome label is `prepared` (not a blocked class).

**Total deviations:** 0 auto-fixed. **Impact on plan:** none.

## Issues Encountered

- `gsd_run` state and roadmap tracking fails on `04-VERIFICATION.md` (`gaps_acknowledged` is outside the tool's closed set). It was not edited. STATE.md, ROADMAP.md and REQUIREMENTS.md updates are left to the orchestrator, as instructed.

## Known Stubs

None.

## Threat Flags

None. No new network endpoint, auth path or trust-boundary file access beyond the plan's threat model.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plan 12-15 may run the full Kev arm (395 rows, about 18.8 h CPU, peak RSS about 40 GB). It must use the dump mode's identity header (gguf, head, rows, driver binary, worktree commit, threads) so resume is bound to inputs.
- Free disk is 27.9 GiB after cleanup; host memory 48 GB (51,539,607,552 bytes) against a projected 40.4 GB peak, so little headroom for other workloads during the run.
- Kev remains a reference arm (D-06); a clearly better Kev is a finding, and switching methods is a later phase.

## Self-Check: PASSED

- All ten created files and the modified MANIFEST row exist on disk.
- Commits `125843d36`, `9c66d7ac5`, `bf227dabc` are ancestors of HEAD; `git rev-list --count f25babd5a..HEAD` was 3 at write time (the measured `commits:`).
- Plan verifies re-run at close-out: Task 1 (4B RUST PATH OK), Task 2 (KEV PROBE OK, NO POST TEXT), Task 3 (KEV CHOICE RECORDED full, verbatim equal) all exit 0.

---
*Phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot*
*Completed: 2026-10-26*
