# Phase 2 — Deferred Items

Out-of-scope discoveries surfaced during execution. Logged, not fixed (executor Scope
Boundary: only issues directly caused by the current task's changes are auto-fixed).

## D-ITEM-01 — Both CB-510 packaging guards are VACUOUS on macOS/BSD

**Found:** plan 02-02, Task 1 (2026-08-08). **Pre-existing**, not caused by this phase.

`scripts/check_include_files.sh:18` and `scripts/check_package_includes.sh:25` both use
GNU PCRE grep:

```sh
included=$(echo "$content" | grep -oP 'include!\(\s*"([^"]+)"\s*\)' | sed ... || true)
```

BSD `grep` (the default on Darwin) has no `-P`. It exits 2 with `grep: invalid option -- P`,
the `|| true` swallows the failure, `$included` is empty, and both scripts print

```
OK: All 0 include!() files are tracked by git
OK: All 0 include!() files are included in cargo package
```

and exit **0**. The true count, measured with GNU grep (`ggrep`, present at
`/opt/homebrew/bin/ggrep`) over `crates/` and `src/`, is **1768** `include!("...")`
occurrences. So on every macOS developer machine these two guards report PASS while
inspecting nothing — the exact "a guard that passes vacuously" failure class CLAUDE.md
Verification Discipline rules 1 and 5 describe, and the guards in question are the ones
that exist to prevent the CB-510 publish break.

CI runs on Linux, where `grep -P` works, so the guards are presumably real there. That
makes this a *local* false-green rather than a shipped hole — but a developer who runs
`make tier3` before pushing is being told something untrue.

**Why not fixed here:** the scripts are untouched by this plan, the fix is a repo-wide
shell-portability change (`grep -oE` with a POSIX-ERE rewrite, or a `grep -P`-capability
probe that hard-fails instead of degrading), and it needs its own bashrs lint pass plus a
must-match/must-not-match case table per CLAUDE.md rule 7. Worth its own PMAT ticket.

**Compensating measurement taken for plan 02-02:** `aprender-contrastive-data` contains
**zero** `include!()` macros (verified directly), and all 16 of its new files are visible
to git and to `cargo package` (verified via `git ls-files --others --exclude-standard`,
`git check-ignore` on each file, and the 19-entry `.crate` listing). The CB-510 property
this plan needed is therefore established by direct evidence, not by the vacuous guards.
