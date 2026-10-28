---
status: testing
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
source: [13-VERIFICATION.md]
started: 2026-10-28T09:01:33Z
updated: 2026-10-28T10:00:00Z
---

## Current Test

number: 2
name: Owner confirms the d15_amendment of contracts/motivation-enrichment-v1.yaml
expected: |
  The owner confirms, or asks for a different convention (which would be motivation-enrichment-v2); the contract's status line stops reading `pending`
awaiting: user response

## Tests

### 1. Owner decides the disposition of review finding CR-01 (an auth-on redeploy has no guard) before any redeploy of the augury motivation server: fix it (a persisted oauth-enabled marker that refuses a token-less laya-deploy, plus a post-deploy _laya-auth-refusal-check that fails loudly) or record it as deferred with the hand-off's manual procedure as the accepted control
expected: 13-REVIEW-DISPOSITION.md CR-01 moves from open to fixed or deferred with a reason; until then nobody runs laya-deploy against augury-dev
result: pass
reported: "defer"
resolution: "Deferred by the owner (2026-10-28). CR-01 is recorded as deferred in 13-REVIEW-DISPOSITION.md; the accepted control is the manual procedure in section 9 of 13-HANDOFF.md. No laya-deploy against augury-dev until a guard exists."

### 2. Owner confirms the d15_amendment of contracts/motivation-enrichment-v1.yaml (evidence level `none`, evidence describes the RECORDED function, refusal of other+clear / social_affiliative+none / conspiratorial+none) before the partner builds the annotation sheet
expected: The owner confirms, or asks for a different convention (which would be motivation-enrichment-v2); the contract's status line stops reading `pending`
result: [pending]

### 3. Owner decides whether the uncertainty band's inert margin clause (with three labels summing to 1, `p1 - p2 < 0.20` adds nothing beyond `p1 <= 0.75`) stays as declared
expected: Keep as is (the note and selftest already say so) or request new constants and a regenerated note
result: [pending]

## Summary

total: 3
passed: 1
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
