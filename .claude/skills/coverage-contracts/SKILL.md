---
name: coverage-contracts
description: Coverage-improvement workflow for aprender — find untested code with `pmat query --coverage` (never parse coverage JSON by hand) and co-evolve contracts with every test added. Use when raising coverage, hunting uncovered functions, or writing tests for coverage.
---

# Coverage-Guided Search (pmat 3.0.0+)

**Use `pmat query --coverage` to find untested code. NEVER parse coverage JSON manually.**

```bash
# Find top uncovered functions (no query needed)
pmat query --coverage-gaps

# Find uncovered functions matching a semantic query
pmat query "error handling" --coverage --uncovered-only

# Use pre-existing coverage data (avoids re-running cargo llvm-cov)
pmat query --coverage-gaps --coverage-file /path/to/coverage.json

# Coverage auto-detection: runs `cargo llvm-cov report --json` automatically
# Prerequisite: run `cargo llvm-cov test --lib --no-report` first to generate data
```

**Workflow for coverage improvement (MUST co-evolve with contracts):**
1. `cargo llvm-cov test --lib --no-report` — generate coverage data
2. `pmat query --coverage-gaps --exclude-tests` — find top uncovered functions by impact
3. For EACH function being tested, ALSO:
   a. Add `#[contract]` annotation if missing
   b. Add/strengthen falsification conditions in the relevant contract YAML
   c. Eliminate placeholder preconditions
4. Write tests targeting those functions
5. `make coverage` — verify improvement
6. `pmat comply check` — verify contract density improved

**RULE: Coverage without contracts is REJECTED. Both must improve together.**
See monorepo spec Rule 7: Coverage + Contracts Co-Evolution.
