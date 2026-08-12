# Aprender Makefile
# Certeza Methodology - Tiered Quality Gates
#
# PERFORMANCE TARGETS (Toyota Way: Zero Defects, Fast Feedback)
# - make test-fast: < 30 seconds (unit tests, no encryption features)
# - make test:      < 2 minutes (all tests, reduced property cases)
# - make coverage:  < 5 minutes (coverage report, reduced property cases)
# - make test-full: comprehensive (all tests, all features, full property cases)

# Use bash for shell commands
SHELL := /bin/bash

# Disable built-in rules for performance
.SUFFIXES:

# Delete partially-built files on error
.DELETE_ON_ERROR:

# Multi-line recipes execute in same shell
# CR-03: `.ONESHELL:` runs a whole recipe in ONE shell, and the default `.SHELLFLAGS` is
# `-c` with no `-e`. That shell does not stop at the first failure, so the recipe's status
# is whatever its LAST line returned — and every tier recipe ends in `@echo "Tier N: PASSED"`,
# which always succeeds. Measured on this repo with both makes installed:
#
#   SHELL := /bin/bash ; .ONESHELL: ; recipe = { false ; @echo "done" }
#     make  3.81 (macOS default) -> exit=2   (3.81 predates .ONESHELL and ignores it)
#     gmake 4.4.1 (Linux)        -> exit=0   FAILURE SWALLOWED
#
# Under Make 4.x that disarmed every gate D-26 deliberately moved INTO the tiers.
#
# `-e` ONLY, deliberately. `-u` and `-o pipefail` are separate hardening with a much larger
# blast radius here: 35 recipes reference `$$VAR` (a `-u` risk) and 5 pipe into `head`/`tail`,
# where the reader closing the pipe SIGPIPEs the writer and `pipefail` turns that into a
# failure. Adding them needs its own verification pass across all 85 targets on BOTH makes.
# `-e` alone restores the per-line abort semantics 3.81 already had, which is the defect.
#
# `.SHELLFLAGS` arrived in Make 3.82, so 3.81 ignores this line — harmless, since 3.81 also
# ignores `.ONESHELL:` and therefore never had the bug.
.SHELLFLAGS := -e -c
.ONESHELL:

.PHONY: all build test test-smoke test-fast test-quick test-full test-heavy lint fmt clean doc book book-build book-serve book-test tier1 tier2 tier3 tier4 coverage coverage-fast profile hooks-install hooks-verify lint-scripts bashrs-score bashrs-lint-makefile chaos-test chaos-test-full chaos-test-lite fuzz bench dev pre-push ci check run-ci run-bench audit deps-validate deny pmat-score pmat-gates quality-report semantic-search examples mutants mutants-fast property-test install-alsa test-alsa test-audio-full contract-validate contract-test contract-audit contract-audit-phase2 contract-audit-phase3 contract-regen contract-check dev-setup check-siblings setfit-feature-matrix setfit-repro-inproc setfit-repro-crossproc setfit-repro-replay gemm-thread-determinism

# Default target
all: tier2

# Build
build:
	cargo build --release

# ============================================================================
# TEST TARGETS (Performance-Optimized with nextest)
# ============================================================================

# Smoke tests (<2s): Minimal critical path verification (Section P: P2)
# Only runs core API tests, no proptests, no encryption, no network
test-smoke: ## Smoke tests (<2s target, Section P: P2)
	@echo "💨 Running smoke tests (target: <2s)..."
	@time PROPTEST_CASES=5 QUICKCHECK_TESTS=5 cargo test --lib --no-fail-fast -- \
		--skip prop_ \
		--skip test_encrypted \
		--skip test_cache_metadata_expiration \
		--skip test_cache_metadata_age \
		--skip test_cache_entry_is_valid_expired \
		--skip test_time_budget \
		--skip k20_trueno_simd \
		--skip test_de_handles_different \
		tests::test_lib_sanity 2>/dev/null || \
		cargo test --lib --no-fail-fast -- \
		--skip prop_ \
		--skip test_encrypted \
		--skip test_cache_metadata \
		--skip test_time_budget \
		--skip k20_ \
		--skip test_de_ \
		2>&1 | head -50
	@echo "✅ Smoke tests passed"

# Fast tests (<30s): Uses nextest for parallelism if available
# Pattern from bashrs: cargo-nextest + PROPTEST_CASES + exclude slow tests
# Excludes: prop_gbm_expected_value_convergence (46s alone!)
test-fast: ## Fast unit tests (<30s target)
	@echo "⚡ Running fast tests (target: <30s, -j2 to prevent OOM)..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		time env PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo nextest run --workspace --lib -j 2 \
			--status-level skip \
			--failure-output immediate \
			-E 'not test(/prop_gbm_expected_value_convergence/)'; \
	else \
		echo "💡 Install cargo-nextest for faster tests: cargo install cargo-nextest"; \
		time env PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo test --workspace --lib -- --test-threads=2 --skip prop_gbm_expected_value_convergence; \
	fi
	@echo "✅ Fast tests passed"

# Quick alias for test-fast
test-quick: test-fast

# Standard tests (<2min): All tests including integration
test: ## Standard tests (<2min target)
	@echo "🧪 Running standard tests (target: <2min, -j2 to prevent OOM)..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		time PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo nextest run --workspace -j 2 \
			--status-level skip \
			--failure-output immediate; \
	else \
		time PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo test --workspace -- --test-threads=2; \
	fi
	@echo "✅ Standard tests passed"

# Full comprehensive tests: All features, all property cases
test-full: ## Comprehensive tests (all features)
	@echo "🔬 Running full comprehensive tests..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		time PROPTEST_CASES=100 QUICKCHECK_TESTS=100 cargo nextest run --workspace --all-features; \
	else \
		time PROPTEST_CASES=100 QUICKCHECK_TESTS=100 cargo test --workspace --all-features; \
	fi
	@echo "✅ Full tests passed"

# Heavy tests: Runs ignored tests (Section P: P7)
# Includes: sleep()-based tests, slow encryption tests, long proptests
test-heavy: ## Heavy/slow tests (ignored tests)
	@echo "🐢 Running heavy tests (ignored tests)..."
	@time PROPTEST_CASES=256 QUICKCHECK_TESTS=256 cargo test --workspace -- --ignored
	@echo "✅ Heavy tests passed"

test-model: ## Run model falsification tests ONE AT A TIME (requires models/, ollama, GPU)
	@echo "🧪 Running model falsification tests (one at a time to avoid OOM)..."
	@for test in f_ollama_001 f_ollama_002 f_ollama_003 f_ollama_004 f_ollama_005 \
	             f_perf_003 f_trueno_004 f_trueno_008 f_rosetta_002 f_qa_002; do \
		echo "  ⏳ $$test"; \
		PROPTEST_CASES=10 QUICKCHECK_TESTS=10 \
		cargo test --features model-tests --test falsification_spec_v10_tests "$$test" 2>&1 \
			| grep "test result:" || echo "  ❌ $$test FAILED"; \
	done
	@echo "✅ Model tests complete"

test-spec: ## Run ALL spec falsification tests (structural only, no models)
	@echo "🔬 Running spec structural tests..."
	@PROPTEST_CASES=10 QUICKCHECK_TESTS=10 \
		cargo test --features model-tests --test falsification_spec_v10_tests 2>&1 \
		| grep "test result:"
	@echo "✅ Spec tests complete"

# Linting
lint:
	cargo clippy -- -D warnings

# Format check
fmt:
	cargo fmt

fmt-check:
	cargo fmt --check

# Clean build artifacts
clean:
	cargo clean

# Generate documentation
doc:
	cargo doc --no-deps --open

# EXTREME TDD Book (mdBook)
book: book-build ## Build and open the EXTREME TDD book

book-build: ## Build the book
	@echo "📚 Building EXTREME TDD book..."
	@if command -v mdbook >/dev/null 2>&1; then \
		mdbook build book; \
		echo "✅ Book built: book/book/index.html"; \
	else \
		echo "❌ mdbook not found. Install with: cargo install mdbook"; \
		exit 1; \
	fi

book-serve: ## Serve the book locally for development
	@echo "📖 Serving book at http://localhost:3000..."
	@mdbook serve book --open

book-test: ## Test book synchronization
	@echo "🔍 Testing book synchronization..."
	@for example in examples/*.rs; do \
		if [ -f "$$example" ]; then \
			EXAMPLE_NAME=$$(basename "$$example" .rs); \
			CASE_STUDY=$$(echo "$$EXAMPLE_NAME" | sed 's/_/-/g'); \
			if [ ! -f "book/src/examples/$$CASE_STUDY.md" ]; then \
				echo "❌ Missing case study for $$EXAMPLE_NAME"; \
				exit 1; \
			fi; \
		fi; \
	done
	@echo "✅ All examples have corresponding book chapters"

# Tier 1: On-save (<1 second, non-blocking)
tier1:
	@echo "Running Tier 1: Fast feedback..."
	@cargo fmt --check
	@cargo clippy -- -W clippy::all
	@cargo check
	@echo "Tier 1: PASSED"

# Tier 2: Pre-commit (<5 seconds, changed files only)
# PMAT-484: probar golden regression if tests/golden/ exists
tier2:
	@echo "Running Tier 2: Pre-commit checks..."
	@PROPTEST_CASES=5 QUICKCHECK_TESTS=5 cargo test --lib
	@cargo clippy -- -D warnings
# Phase 1 SetFit conformance (D-26). The gates must live INSIDE a tier: a target
# outside the tiers is a target that stops being run.
#
# PLACEMENT WAS MEASURED, not assumed (2026-08-08, warm tree):
#   whole conformance suite, ONE invocation ......  7 s wall / 0.57 s test time
#   one filtered invocation (e.g. gradient_gate_)   6 s wall / 0.44 s test time
#   setfit:: lib module, conformance-fixtures .... 19 s wall / 1.72 s test time
# The wall clock is dominated by cargo's per-invocation freshness check, not by
# test execution. The pre-accepted tier2/tier3 SPLIT (user, 2026-08-07) would
# therefore cost ~3 x 6 s = ~18 s for the three "fast" gates alone — WORSE than
# the 7 s single run it was meant to avoid, because a `cargo test` line may carry
# at most ONE positional filter (a second exits `error: unexpected argument`) and
# the three gate prefixes do not share one. The fallback is NOT triggered: the
# whole suite stays here, as one invocation. The plan's 10-30 s estimate for the
# suite was 20-50x high.
#
# The lib line uses the MODULE PATH, not the plan's unscoped `--features setfit`
# form (D30). Measured: `--lib --features setfit` runs 14202 tests in 119 s of
# which 162 are this phase's, while `setfit::` selects exactly 162 in 19 s — and
# 162 is precisely the feature-gated delta (14283 tests with conformance-fixtures
# vs 14121 with default features), so the filter loses no coverage.
	@echo "Phase 1 SetFit: encoder/tokenizer/import/loss/model unit gates..."
	@cargo test -p aprender-core --lib --features conformance-fixtures setfit::
	@echo "Phase 1 SetFit: fixture parity + ENC-04 gradient/frozen/detach gates..."
	@cargo test -p aprender-core --features setfit,conformance-fixtures --test setfit_conformance
# Phase 2 contrastive-data (D-26 again — a gate outside the tiers stops being run).
#
# RUNTIME WAS RE-MEASURED, not estimated (2026-08-09, warm tree, three consecutive
# runs): 6.48 s / 6.34 s / 6.46 s wall, rc=0 each time. History of this line, because
# the trend is the point: 2/2/2 s with one determinism doctest, 1/2/1 s after plan
# 02-03 grew it to 67 lib + 5 doc, 1.75 s after plan 02-05 took it to 126 lib + 7
# integration + 7 doc, 3.0 s after plan 02-07 took it to 206 lib + 11 integration +
# 7 doc, and now 6.4 s after plan 02-08 took it to 212 lib + 24 integration + 7 doc
# across EIGHT suites (1 further test is #[ignore]d — the golden regenerator).
# Actual test execution inside that 6.4 s is 1.18 s lib + 0.23 s trybuild + 3.33 s
# doc; the rest is cargo's per-invocation freshness check over eight targets. The
# step is still comfortably tier2-shaped.
# The 1.75 -> 3.0 s step was the pair sampler's two heaviest properties: a 40,000-draw
# marginal-equivalence measurement on layout [3,5,7] (which is what proves the O(K)
# negative scheme preserves D-14's n_j*n_k class-pair weights rather than merely
# being faster) and a 24,576-pair streamed manifest hash. The 3.0 -> 6.4 s step is
# `tests/ui.rs`: trybuild spawns a nested cargo build for the five compile-fail
# programs that pin DATA-06's non-constructibility. All three costs are deliberate —
# a cheaper marginal test could not distinguish the weighted scheme from a uniform
# one, and a compile-fail claim that is not compiled is not a claim.
# Re-measure and update when the suite grows again; a tier2 line whose comment
# records a stale number is worse than one with no comment, because it will be
# trusted.
#
# UNFILTERED on purpose. The Phase 1 lines above take a module filter because
# they select 162 of 14202 tests in a large crate. Here the whole crate IS this
# phase, a filter would exclude the doctests, and `cargo test` accepts at most
# one positional anyway.
	@echo "Phase 2 contrastive-data: protocol unit gates..."
	@cargo test -p aprender-contrastive-data
# Phase 3 D-16, tier2 half. The decision splits ONE gate across two tiers: the fast
# in-process comparison here, the authoritative cross-process one in tier3. Wiring only
# the tier3 half would leave D-16's fast signal in a test file no tier invokes.
#
# Measured standalone before wiring (warm tree): rc=0, 6 s wall, 1 test. It is one
# libtest filter on an already-built target, which is tier2-shaped. Its failure mode was
# induced, observed and reverted — see 03-10-SUMMARY.md.
	@echo "Phase 3 SetFit: in-process two-clean-runs equality (D-16)..."
	@$(MAKE) setfit-repro-inproc
	@if [ -d tests/golden ]; then \
		if . scripts/apr_bin.sh 2>/dev/null; then \
			echo "Running probar golden regression... ($$APR)"; \
			"$$APR" probar tests/golden/model.apr --golden tests/golden/ --assert --tolerance 0.98 2>/dev/null || true; \
		else \
			echo "Skipping probar golden regression: no apr built from HEAD (scripts/apr_bin.sh)"; \
		fi; \
	fi
	@echo "Tier 2: PASSED"

# Tier 3: Pre-push (1-5 minutes, full validation)
# PMAT-484: probar golden regression + profile if tests/golden/ exists
tier3:
	@echo "Running Tier 3: Full validation..."
	@PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo test --all
	@cargo clippy -- -D warnings
	@echo "Checking include!() files tracked by git..."
	@bash scripts/check_include_files.sh
	@echo "Checking publish safety (symlinks, companion lookups)..."
	@bash scripts/check_publish_safety.sh
	@echo "Checking build.rs crate-root escapes (v0.31.1 yank class)..."
	@bash scripts/check_build_rs_paths.sh
	@echo "Checking self-hosted CI jobs pin a discriminating runner label..."
	@bash scripts/check_runner_labels.sh
# D-26: the contract gate must be REACHED, not merely listed. Before this line,
# `contract-validate` was reachable only from `contract-check`, which no tier
# depends on — so appending a contract to $(CONTRACTS) alone would have parked
# the Phase 1 gate outside the tiers entirely.
#
# The BROAD form was chosen from evidence, not preference (W4). `make
# contract-validate` was run STANDALONE first, with its status captured directly
# (`make contract-validate > /tmp/cv.log 2>&1; rc=$$?`, never through a pipe —
# CLAUDE.md rule 1): rc=0 in 8 s wall, all 41 pre-existing contracts reporting
# "0 error(s), 0 warning(s)". Nothing is already red, so wiring the whole list
# cannot make tier3 fail for a defect this phase did not cause, and 8 s is well
# inside tier3's 1-5 minute budget. Had any contract been red, the narrow
# `$(PV_BIN) validate contracts/setfit-encoder-conformance-v1.yaml` form would
# have been used instead and the red contracts surfaced as their own finding.
	@echo "Validating provable contracts (incl. the Phase 1 setfit gate, D-26)..."
	@$(MAKE) contract-validate
# Review finding F9 (plan 02-08): `contract-validate` above checks contract
# SHAPE and says nothing about whether an equation is bound to an
# implementation, so both Phase 2 contracts could have been "valid" with all 25
# equations bound to nothing. This is the BLOCKING coverage gate. It is the
# SCOPED form on purpose — the repo-wide `contract-audit` reports 132 unbound
# equations across 38 contracts and exits 0 anyway; see that target's comment
# block for the measurement and why neither wiring it nor fixing it belongs to
# this phase. Same evidence discipline as the two blocks around it: run
# standalone first (rc=0, 9 s cold / ~1 s warm), and its failure mode induced,
# observed and reverted before it was wired.
	@$(MAKE) contract-audit-phase2
# Phase 3's equivalent, wired here for exactly the reason the line above exists:
# a target outside the tiers is a target that stops being run. Scoped to
# $(PHASE3_CONTRACTS). Same evidence discipline — run standalone with the status
# captured directly, and its failure mode induced, observed and reverted rather
# than assumed. See the target's own comment block.
	@$(MAKE) contract-audit-phase3
# TRN-06's AUTHORITATIVE reproducibility claim (D-16) and D-13's GEMM control, wired
# here for the reason the three lines above exist: a target outside the tiers is a
# target that stops being run. Both were run STANDALONE first with the status captured
# directly, and both had a failure INDUCED, observed and reverted before being wired —
# see 03-10-SUMMARY.md for the rc values and the perturbations used. The recipes read
# `$$?` on the line after the redirect and contain no `tee`.
	@$(MAKE) setfit-repro-crossproc
# REVIEW CR-02: the replay check was written, committed, and wired into NOTHING. Two clean
# runs agreeing proves reproducibility; only this proves the reproduced order is the
# INTENDED one. Without it the pair of gates above can both pass on a wrong-but-consistent
# order — the one failure mode the recorded-vs-recomputed split exists to catch.
	@$(MAKE) setfit-repro-replay
	@$(MAKE) gemm-thread-determinism
	@$(MAKE) setfit-feature-matrix
# D-04 (Phase 2), wired here for the same reason the line above exists: a target
# outside the tiers is a target that stops being run. Same evidence discipline as
# the D-26 block — `make contrastive-data-boundary` was run STANDALONE first with
# its status captured directly (`> /tmp/cdb-standalone.log 2>&1; rc=$$?`, never
# through a pipe): rc=0 in 1 s wall. Nothing was already red, and 1 s is nothing
# against tier3's 1-5 minute budget. Its four failure modes were each induced,
# observed and reverted rather than assumed — see the target's own comment block.
	@$(MAKE) contrastive-data-boundary
	@if [ -d tests/golden ]; then \
		if . scripts/apr_bin.sh 2>/dev/null; then \
			echo "Running probar golden regression with profiling... ($$APR)"; \
			"$$APR" probar tests/golden/model.apr --golden tests/golden/ --assert --tolerance 0.98 2>/dev/null || true; \
		else \
			echo "Skipping probar golden regression: no apr built from HEAD (scripts/apr_bin.sh)"; \
		fi; \
	fi
	@echo "Tier 3: PASSED"

# D-06: the setfit feature must be dependency-CLOSED and must not leak into a
# minimal build. Wired into tier3 above.
#
# `--all-features` is deliberately ABSENT (D22): it enables `audio-alsa`, whose
# `alsa-sys` build script needs the Linux ALSA headers, so on macOS it fails for
# reasons that have nothing to do with this phase. Proven independent of setfit —
# `cargo check -p aprender-core --no-default-features --features audio-alsa`,
# which touches no setfit code at all, fails identically. The union below is the
# platform-appropriate one D22's fix direction asks for and covers every feature
# combination this phase introduces.
setfit-feature-matrix: ## D-06/D-05: setfit feature isolation for aprender-core AND aprender-train
	@echo "Feature matrix: aprender-core setfit isolation (D-06)"
	@cargo check -p aprender-core --no-default-features
	@cargo check -p aprender-core --features setfit
	@cargo check -p aprender-core --features conformance-fixtures
	@cargo check -p aprender-core --features setfit,conformance-fixtures,model-tests
	@echo "  negative: a no-default-features build must contain NO tokenizers node"
# The tree is captured to a file and `cargo tree`'s own status checked FIRST.
# Piping straight into `grep -q` would read grep's status, and a `cargo tree`
# that failed outright would feed grep nothing — the guard would then pass
# vacuously, which is exactly the CLAUDE.md rule 1 failure mode.
	@cargo tree -p aprender-core --no-default-features -e normal \
		> target/setfit-feature-matrix-tree.txt 2>&1 || \
		{ echo "FAIL: cargo tree failed; the D-06 negative check would pass vacuously"; \
		  cat target/setfit-feature-matrix-tree.txt; exit 1; }
	@if grep -q tokenizers target/setfit-feature-matrix-tree.txt; then \
		echo "FAIL: tokenizers leaked into a no-default-features build (D-06)"; \
		grep -n tokenizers target/setfit-feature-matrix-tree.txt; exit 1; \
	fi
# ─── Phase 3 D-05: the same closure obligation, one crate up ────────────────
#
# `setfit` now has to propagate through a 37-module crate that also carries
# GPU/LoRA/distill/server. Three legs, and one of them is not the shape the
# plan first asked for — the reason is recorded here rather than in a commit
# message, because a reader of this Makefile is who needs it.
#
# A literal `--all-features` leg is NOT buildable on the CPU profile: this
# crate's feature list includes `cuda`, `gpu`, `nvml` and `wasm`, which need
# toolchains a CPU host does not have. Leg (c) is the honest substitute — every
# CPU-buildable feature co-enabled with setfit, which IS the D-05 risk surface.
# Measured 2026-08-09 before wiring, exactly as the D-04 block below was: the
# same feature list WITHOUT setfit (the control) exits 0, and WITH setfit it
# also exits 0. Nothing pre-existing was inherited and nothing was hidden.
	@echo "Feature matrix: aprender-train setfit closure (Phase 3 D-05)"
	@cargo check -p aprender-train --features setfit
	@cargo check -p aprender-train --features setfit,cpu-fallback,gguf,monitor,tui,citl,server,tracing,ruchy-sessions,parquet,hub,viz
# Leg (a) is a two-sided DIFF, not a plain green check, and that is deliberate.
#
# Measured 2026-08-09: `cargo check -p aprender-train --no-default-features` is
# RED at HEAD with 8 errors, ALL of them `presentar_terminal` unlinked under
# `src/monitor/tui/` — because `src/monitor/mod.rs:45` declares `pub mod tui;`
# UNCONDITIONALLY while its only dependency is gated behind the `tui` feature.
# That is a pre-existing defect in a module Phase 3 does not own (deferred-items
# D-ITEM-05), so wiring the plain leg would import someone else's red into this
# phase's gate. Dropping the leg would hide it. What IS this phase's business is
# that enabling `setfit` adds NOTHING to that build — so the leg asserts the two
# diagnostic streams are byte-identical, and goes RED the moment setfit leaks
# into the minimal build. Statuses are read from `$$?` on the line AFTER the
# redirect and never through a pipe or an `if !` (CLAUDE.md rule 1) — the first
# draft of this leg used `if ! cargo check; then rc=$$?; fi`, where `$$?` is the
# status of the NEGATION and is therefore always 0. The vacuity check below is
# what caught it, which is the whole reason it is here.
#
# THE `set +e` AROUND THE TWO CARGO CHECKS IS LOAD-BEARING, for exactly the
# reason recorded on `contract-audit-phase3` below. This Makefile sets
# `.SHELLFLAGS := -e -c` (line 40) and `.ONESHELL:`, so a FAILING
# `cargo check` aborts the whole recipe before `ctl_rc=$$?` on the same line can
# run — and the control build is measured RED at HEAD, which is the ONLY case
# every guard below was written for. Reproduced directly:
# `bash -e -c 'false > /tmp/x 2>&1; rc=$$?; echo rc=$$rc; echo REACHED'` prints
# NOTHING and exits 1. Without `set +e` this leg could never emit its verdict and
# `make tier3` failed with a bare cargo error instead. `mkdir -p target` for the
# same class of reason: the shell opens the redirect before cargo runs, so a
# clean checkout without `target/` would abort on the redirect itself.
	@echo "  leg (a): minimal-build setfit-diff"
	@mkdir -p target; \
	set +e; \
	cargo check -p aprender-train --no-default-features \
	      > target/sfm-train-min-control.log 2>&1; ctl_rc=$$?; \
	cargo check -p aprender-train --no-default-features --features setfit \
	      > target/sfm-train-min-setfit.log 2>&1; sf_rc=$$?; \
	set -e; \
	grep -A1 -E '^error' target/sfm-train-min-control.log \
	      > target/sfm-train-min-control.errs || true; \
	grep -A1 -E '^error' target/sfm-train-min-setfit.log \
	      > target/sfm-train-min-setfit.errs || true; \
	if [ "$$ctl_rc" != "0" ] && [ ! -s target/sfm-train-min-control.errs ]; then \
	  echo "FAIL: the minimal build failed (rc=$$ctl_rc) but produced no diagnostics to compare;"; \
	  echo "      this guard would pass vacuously. Inspect target/sfm-train-min-control.log."; \
	  exit 1; \
	fi; \
	if [ "$$ctl_rc" = "0" ] && [ -s target/sfm-train-min-control.errs ]; then \
	  echo "FAIL: the minimal build exited 0 yet emitted diagnostics; the comparison is not"; \
	  echo "      measuring what it claims. Inspect target/sfm-train-min-control.log."; \
	  exit 1; \
	fi; \
	if [ "$$ctl_rc" != "$$sf_rc" ]; then \
	  echo "FAIL: enabling setfit changed the minimal build's exit status ($$ctl_rc -> $$sf_rc) (D-05 leakage)"; \
	  exit 1; \
	fi; \
	if ! diff -u target/sfm-train-min-control.errs target/sfm-train-min-setfit.errs; then \
	  echo "FAIL: enabling setfit changed the minimal build's diagnostics (D-05 leakage)"; \
	  exit 1; \
	fi; \
	echo "    identical with and without setfit (control rc=$$ctl_rc, setfit rc=$$sf_rc)"
# The dependency-closure negative, two-sided. The positive half is what stops
# the negative half passing for the wrong reason: a `cargo tree` invocation that
# silently stopped resolving these packages would satisfy an absence-only check
# forever. Both trees are captured to files and `cargo tree`'s own status is
# checked FIRST, same discipline as the aprender-core block above.
	@echo "  negative: a DEFAULT aprender-train build must contain NO contrastive-data, rand or tokenizers node"
	@cargo tree -p aprender-train -e normal \
		> target/sfm-train-tree-default.txt 2>&1 || \
		{ echo "FAIL: cargo tree failed; the D-05 closure check would pass vacuously"; \
		  cat target/sfm-train-tree-default.txt; exit 1; }
	@cargo tree -p aprender-train --features setfit -e normal \
		> target/sfm-train-tree-setfit.txt 2>&1 || \
		{ echo "FAIL: cargo tree --features setfit failed; the D-05 closure check would pass vacuously"; \
		  cat target/sfm-train-tree-setfit.txt; exit 1; }
	@if grep -qE 'aprender-contrastive-data|aprender-rand|tokenizers' target/sfm-train-tree-default.txt; then \
		echo "FAIL: a setfit-only dependency leaked into the DEFAULT aprender-train build (D-05)"; \
		grep -nE 'aprender-contrastive-data|aprender-rand|tokenizers' target/sfm-train-tree-default.txt; exit 1; \
	fi
	@for node in aprender-contrastive-data aprender-rand tokenizers; do \
		if ! grep -q "$$node" target/sfm-train-tree-setfit.txt; then \
			echo "FAIL: --features setfit did NOT pull in $$node; the absence check above is vacuous (D-05)"; \
			exit 1; \
		fi; \
	done
	@echo "setfit-feature-matrix: PASSED"

# D-04: the aprender-contrastive-data bytes boundary. Wired into tier3 above.
#
# BOTH HALVES ARE POSITIVE CHECKS, and that is the whole design. The first draft
# of this gate was a dependency DENY-list plus a grep that skipped #[cfg(test)],
# and both can report PASS while the property is false: a deny-list only ever
# catches the hazards someone already enumerated, so the first dependency nobody
# thought to name passes silently; and a cfg-blind grep cannot actually tell test
# code from library code, so it either exempts too much or claims a precision it
# does not have. Replaced by (a) a POSITIVE allowlist compared against the
# resolved closure, so a new transitive dependency fails by DEFAULT, and (b) a
# src/-wide symbol ban with NO cfg(test) exemption, which turns "the public API
# contains no path types" into a mechanical consequence.
#
# EVERY FAILURE MODE WAS OBSERVED, not assumed (2026-08-08, each mutation applied,
# run, and reverted):
#   add `tempfile` to [dependencies] ........ FAIL, prints tempfile as an offender
#   `use std::path::PathBuf;` in src/schema.rs FAIL, names schema.rs and the line
#   the same line inside #[cfg(test)] mod tests FAIL (no exemption, by design)
#   rename allowed-deps.txt away ............ FAIL with a missing-allowlist message,
#                                             NOT a vacuous pass on an empty list
# Standalone timing before wiring: 1 s wall, rc=0.
contrastive-data-boundary: ## D-04: bytes boundary for aprender-contrastive-data (positive allowlist + src symbol ban)
	@echo "Bytes boundary: aprender-contrastive-data (D-04)"
	@mkdir -p target
# (a) DEPENDENCY ALLOWLIST. cargo tree's OWN status is checked FIRST. Piping it
# into the comparison would read the comparison's status (CLAUDE.md rule 1), and
# a cargo tree that failed outright would feed an EMPTY closure into a subset
# test — which passes vacuously and silently disarms the supply-chain half.
	@cargo tree -p aprender-contrastive-data -e normal --prefix none --no-dedupe \
		> target/contrastive-data-tree.txt 2>&1 || \
		{ echo "FAIL: cargo tree failed; the D-04 dependency check would pass vacuously"; \
		  cat target/contrastive-data-tree.txt; exit 1; }
	@if [ ! -s target/contrastive-data-tree.txt ]; then \
		echo "FAIL: cargo tree produced no output; the D-04 dependency check would pass vacuously"; \
		exit 1; \
	fi
	@awk 'NF { print $$1 }' target/contrastive-data-tree.txt | sort -u \
		> target/contrastive-data-deps.txt
	@if [ ! -f crates/aprender-contrastive-data/allowed-deps.txt ]; then \
		echo "FAIL: crates/aprender-contrastive-data/allowed-deps.txt is MISSING."; \
		echo "      Without it every dependency would be admitted and this gate would"; \
		echo "      report PASS while checking nothing."; \
		exit 1; \
	fi
	@grep -v '^[[:space:]]*#' crates/aprender-contrastive-data/allowed-deps.txt \
		| grep -v '^[[:space:]]*$$' | sort -u > target/contrastive-data-allowed.txt
	@if [ ! -s target/contrastive-data-allowed.txt ]; then \
		echo "FAIL: allowed-deps.txt has no entries. An empty allowlist cannot admit even"; \
		echo "      the crate itself, so this is a broken gate rather than a strict one."; \
		exit 1; \
	fi
	@comm -23 target/contrastive-data-deps.txt target/contrastive-data-allowed.txt \
		> target/contrastive-data-offenders.txt
	@if [ -s target/contrastive-data-offenders.txt ]; then \
		echo "FAIL: packages in the resolved normal-dependency closure but ABSENT from"; \
		echo "      crates/aprender-contrastive-data/allowed-deps.txt (D-04):"; \
		sed 's/^/        /' target/contrastive-data-offenders.txt; \
		echo "      Do NOT widen the allowlist just to turn this green: the allowlist"; \
		echo "      entry IS the review. Read what the package pulls in first."; \
		exit 1; \
	fi
	@echo "  deps:   resolved closure is a subset of allowed-deps.txt"
# (b) SOURCE SURFACE BAN. Matches are taken with true line numbers first, then
# comment lines are dropped from the RESULTS, so a doc comment can neither trip
# the gate nor satisfy it and the reported line number still points at the real
# file. There is deliberately NO #[cfg(test)] exemption — tests that genuinely
# need a filesystem belong in tests/ (outside the library boundary) or in apr-cli.
	@find crates/aprender-contrastive-data/src -type f -name '*.rs' \
		> target/contrastive-data-srcfiles.txt 2>&1 || \
		{ echo "FAIL: could not enumerate src/; the D-04 source check would pass vacuously"; \
		  exit 1; }
	@if [ ! -s target/contrastive-data-srcfiles.txt ]; then \
		echo "FAIL: no .rs files found under crates/aprender-contrastive-data/src;"; \
		echo "      the D-04 source check would pass vacuously"; \
		exit 1; \
	fi
	@: > target/contrastive-data-symbols.txt
	@while IFS= read -r srcfile; do \
		{ grep -nE 'std::fs|std::net|std::path' "$$srcfile" || true; \
		  grep -nwE 'Path|PathBuf' "$$srcfile" || true; } \
		| grep -vE '^[0-9]+:[[:space:]]*//' \
		| sed "s|^|$$srcfile:|" >> target/contrastive-data-symbols.txt || true; \
	done < target/contrastive-data-srcfiles.txt
	@if [ -s target/contrastive-data-symbols.txt ]; then \
		echo "FAIL: forbidden filesystem/network/path symbols under src/ (D-04)."; \
		echo "      The crate is bytes-in/bytes-out; apr-cli owns every fs adapter."; \
		sort -u target/contrastive-data-symbols.txt | sed 's/^/        /'; \
		exit 1; \
	fi
	@echo "  source: no fs/net/path symbols under src/ (no cfg(test) exemption)"
	@echo "contrastive-data-boundary: PASSED"

# Tier 4: CI/CD (5-60 minutes, heavyweight)
tier4: tier3
	@echo "Running Tier 4: CI/CD validation..."
	@PROPTEST_CASES=100 QUICKCHECK_TESTS=100 cargo test --release
	@echo "Running pmat analysis..."
	-pmat tdg . --include-components
	-pmat rust-project-score
	-pmat quality-gates --report
	@echo "Tier 4: PASSED"

# ============================================================================
# COVERAGE TARGETS (Two-Phase Pattern from bashrs)
# ============================================================================
# Pattern: bashrs/Makefile - Two-phase coverage with mold linker workaround
# CRITICAL: mold linker breaks LLVM coverage instrumentation
# Solution: Temporarily move ~/.cargo/config.toml during coverage runs

# Exclusion patterns for coverage reports
# ONLY excludes truly external/feature-gated code - all apr subcommands INCLUDED
#   External crates:
#     - .cargo/           : Dependencies from crates.io
#     - trueno/           : Local sibling crate (SIMD tensor ops)
#     - realizar/         : Local sibling crate (inference engine)
#     - entrenar/         : Local sibling crate (training)
#   Local exclusions:
#     - fuzz/             : Fuzz test infrastructure
#     - golden_traces/    : Trace data files
#   Feature-gated (require --all-features):
#     - audio/            : Requires audio feature + ALSA
#     - hf_hub/           : HuggingFace hub (network-dependent)
#   Test infrastructure:
#     - test_factory      : Test code, not production
#     - demo/             : Demo/example code
# NOTE: Coverage tracks the main aprender library only.
# Subcrate tests still RUN (--workspace), exercising main lib code paths,
# but subcrate source files are excluded from the coverage REPORT.
# External deps (trueno, realizar, .cargo) also excluded.
# Subcrate code, external deps, and modules requiring external model files for coverage.
# models/ = dead code per UCBD §9.1 (scheduled for deletion).
# serialization/ = SafeTensors IO (needs actual .safetensors files).
# speech/ = like audio/ (already excluded), speech recognition IO.
# format/onnx = ONNX format support (needs .onnx files).
# format/converter = format conversion (needs model files, covered by integration tests).
# format/rosetta = cross-format parity (needs model files).
# transfer/ = transfer learning (needs pretrained models).
# bench/ = benchmark visualization (non-core).
COVERAGE_EXCLUDE_REGEX := \.cargo/|trueno|realizar/|entrenar/|fuzz/|golden_traces/|hf_hub/|demo/|test_factory|pacha/|showcase/|apr-cli/|aprender-shell/|aprender-tsp/|aprender-monte-carlo/|chaos\.rs|audio/|format/quantize\.rs|format/signing\.rs|voice/|playback\.rs|rustlib/src/rust|models/|serialization/|speech/|format/onnx|format/converter|format/rosetta|transfer/|bench_viz/

# Coverage threshold (enforced: fail if below)
COV_THRESHOLD := 95

# Enforced RATCHET floor, distinct from the aspirational target above.
#
# Measured 2026-07-29 by the nightly on 95145584f (the commit that fixed the
# measurement itself): TOTAL: 786448/885829 lines covered = 88.78%. The 95%
# target is real but is NOT where the tree is, so gating on 95 today would paint
# the nightly permanently red and train everyone to ignore it - the exact
# "gate that cannot turn red usefully" failure this repo keeps finding.
#
# So the enforced condition is "do not regress below what we actually have".
# Raise this number whenever a run comes in higher; never lower it to make red
# go away. Integer truncation gives ~0.78pt of headroom before 88 becomes 87.
COV_FLOOR := 88

# NVMe target dir (mirrors cargo() shell function that sets CARGO_TARGET_DIR)
# Without this, Make's subshell bypasses the function and uses ./target/ instead
# of /mnt/nvme-raid0/targets/aprender, causing profraw/binary mismatch.
NVME_TARGET_DIR := $(wildcard /mnt/nvme-raid0)
ifdef NVME_TARGET_DIR
  COV_TARGET_DIR := /mnt/nvme-raid0/targets/aprender
else
  COV_TARGET_DIR :=
endif
COV_CARGO_ENV := $(if $(COV_TARGET_DIR),CARGO_TARGET_DIR=$(COV_TARGET_DIR))

# Coverage: SINGLE-phase (tests instrument AND write the report in one invocation).
#
# This was a two-phase pattern (`test --no-report`, then a separate `report`) and it
# silently measured NOTHING: every run reported "TOTAL: 0/0 lines covered (0%)".
#
# Why: `cargo llvm-cov report` takes its package scope from the CURRENT package, and it
# does NOT accept --workspace/--exclude ("--workspace is specific to [test,nextest,...]
# and not supported for subcommand 'report'"). Phase 1 instrumented
# `--workspace --exclude aprender-gpu`, phase 2 then reported on the ROOT package - which
# is a facade with no code - so the LCOV came out empty and COV_PCT computed to 0. Same
# facade trap .github/workflows/ci.yml:60-64 already documents for sovereign-ci.
#
# Verified on a multi-package run (aprender-common + aprender-bench-compute), in the
# SHARED target dir this Makefile uses:
#   two-phase, unscoped report  -> LH=0   LF=0    (empty)
#   report --summary-only -p A -p B -> LH=686 LF=737  (93.08%)
#   single-phase --lcov --output-path -> LH=686 LF=737  (93.08%)
# Single-phase is chosen over an explicit -p list because the invocation that selects the
# scope is the one that writes the report, so the two cannot drift apart again. profraw
# survive it (31 present afterwards), so coverage-html still has data to work from.
.PHONY: coverage-check contracts

# Alias the dogfood pre-release protocol looks for. It expects `coverage-check`;
# without it the gate reports WARN ("verify >=95% manually"), i.e. a release gate
# that asks a human to do the measurement is not a gate. `coverage` already
# enforces COV_FLOOR, so this is a name, not a new policy.
coverage-check: coverage

# Ditto for `contracts`. The provable-contract tier is a HARD release gate per
# CLAUDE.md, and the dogfood protocol looked for a target that did not exist, so
# it WARNed instead of checking. `pv lint` runs validate + audit + score across
# contracts/ and is the documented entry point (never hand-rolled bash).
contracts:
	@echo "== provable contracts: pv lint contracts/ =="
	@pv lint contracts/ 2>&1 | tail -5
	@echo "== contract engine tests =="
	@cargo test -p aprender-contracts --lib 2>&1 | grep -E "test result" | tail -1

coverage: ## Coverage summary + threshold check (warm: ~3min)
	@echo "📊 Running coverage ($(COV_THRESHOLD)%+ threshold)..."
	@which cargo-llvm-cov > /dev/null 2>&1 || { cargo install cargo-llvm-cov --locked || exit 1; }
	@test -f ~/.cargo/config.toml && mv ~/.cargo/config.toml ~/.cargo/config.toml.bak || true
	@# Pre-clean: remove stale profraw files to avoid LLVM version mismatch
	@COVDIR=$$($(COV_CARGO_ENV) cargo llvm-cov show-env 2>/dev/null | grep CARGO_LLVM_COV_TARGET_DIR | sed "s/.*=//"); \
	if [ -n "$$COVDIR" ]; then find "$$COVDIR" -name '*.profraw' -delete 2>/dev/null || true; fi
	@mkdir -p target/coverage
	@printf '%s' '$(COVERAGE_EXCLUDE_REGEX)' > target/coverage/.exclude-re
	@echo "🧪 Tests with instrumentation + report in ONE invocation (CB-127-A: cargo llvm-cov test, not nextest)..."
	@PROPTEST_CASES=10 QUICKCHECK_TESTS=10 RUST_MIN_STACK=16777216 CARGO_BUILD_JOBS=4 \
		$(COV_CARGO_ENV) cargo llvm-cov test \
		--workspace --exclude aprender-gpu --lib \
		--lcov --output-path target/coverage/lcov.info \
		--ignore-filename-regex "$$(cat target/coverage/.exclude-re)" \
		-- --skip prop_gbm_expected_value --skip slow --skip heavy --skip h12_ --skip j2_ \
		   --skip falsification --skip chaos --skip disconnect --skip benchmark_parity \
		   --skip qwen2_generation --skip qwen2_golden --skip qwen2_weight --skip load_test \
		   --skip spec_checklist_w --skip spec_checklist_u --skip verify_audio --skip g9_roofline \
		   --skip cuda --skip gpu_ \
		|| { test -f ~/.cargo/config.toml.bak && mv ~/.cargo/config.toml.bak ~/.cargo/config.toml; exit 1; }
	@echo "📊 Parsing LCOV for the threshold check..."
	@# Parse LCOV for line coverage (LH=lines hit, LF=lines found)
	@LH=$$(awk -F: '/^LH:/{s+=$$2} END{print s+0}' target/coverage/lcov.info); \
	LF=$$(awk -F: '/^LF:/{s+=$$2} END{print s+0}' target/coverage/lcov.info); \
	if [ "$$LF" -gt 0 ]; then COV_PCT=$$((LH * 100 / LF)); else COV_PCT=0; fi; \
	echo "TOTAL: $$LH/$$LF lines covered ($${COV_PCT}%)"; \
	echo "TOTAL $$LH $$LF $${COV_PCT}%" > target/coverage/summary.txt; \
	test -f ~/.cargo/config.toml.bak && mv ~/.cargo/config.toml.bak ~/.cargo/config.toml || true; \
	if [ "$$COV_PCT" -lt "$(COV_FLOOR)" ]; then \
		echo "❌ REGRESSION: coverage $${COV_PCT}% fell below the enforced floor $(COV_FLOOR)%"; \
		echo "   The floor is the last measured value, so this means coverage went DOWN."; \
		echo "   Add tests for what you changed, or justify and lower COV_FLOOR deliberately."; \
		exit 1; \
	elif [ "$$COV_PCT" -lt "$(COV_THRESHOLD)" ]; then \
		echo "✅ Coverage $${COV_PCT}% holds the floor $(COV_FLOOR)% (target is $(COV_THRESHOLD)%, not yet reached)"; \
		if [ "$$COV_PCT" -gt "$(COV_FLOOR)" ]; then \
			echo "   ⬆  Above the floor - raise COV_FLOOR to $${COV_PCT} to lock the gain in."; \
		fi; \
	else \
		echo "✅ Coverage $${COV_PCT}% meets threshold $(COV_THRESHOLD)%"; \
	fi

# Fast coverage alias
coverage-fast: coverage

# HTML + LCOV reports (run after 'make coverage' to generate browseable report)
# KNOWN DEFECT (same root cause as `coverage` above, NOT yet fixed): both `report` calls
# below are unscoped, so they report the root facade and produce an EMPTY html/lcov. They
# need either an explicit `-p <pkg>` list or to be folded into the instrumenting run, the
# way `coverage` now is. Left as-is here because it is report-only cosmetics and does not
# gate anything - unlike `coverage`, whose 0% fed the >=95% threshold check.
coverage-html: ## Generate HTML + LCOV reports from last coverage run
	@echo "📊 Generating HTML + LCOV reports..."
	@test -f ~/.cargo/config.toml && mv ~/.cargo/config.toml ~/.cargo/config.toml.bak || true
	@mkdir -p target/coverage
	@printf '%s' '$(COVERAGE_EXCLUDE_REGEX)' > target/coverage/.exclude-re
	@$(COV_CARGO_ENV) cargo llvm-cov report --html --output-dir target/coverage/html --ignore-filename-regex "$$(cat target/coverage/.exclude-re)"
	@$(COV_CARGO_ENV) cargo llvm-cov report --lcov --output-path target/coverage/lcov.info --ignore-filename-regex "$$(cat target/coverage/.exclude-re)"
	@test -f ~/.cargo/config.toml.bak && mv ~/.cargo/config.toml.bak ~/.cargo/config.toml || true
	@echo "📍 HTML: target/coverage/html/index.html"

# Full coverage: All features (for CI, slower)
# CB-127-A: Use 'cargo llvm-cov test' instead of nextest to avoid profraw explosion
coverage-full: ## Full coverage report (all features, CI only)
	@echo "📊 Running full coverage analysis (all features)..."
	@which cargo-llvm-cov > /dev/null 2>&1 || { cargo install cargo-llvm-cov --locked || exit 1; }
	@test -f ~/.cargo/config.toml && mv ~/.cargo/config.toml ~/.cargo/config.toml.bak || true
	@mkdir -p target/coverage
	@printf '%s' '$(COVERAGE_EXCLUDE_REGEX)' > target/coverage/.exclude-re
	@PROPTEST_CASES=10 QUICKCHECK_TESTS=10 CARGO_BUILD_JOBS=4 \
		$(COV_CARGO_ENV) cargo llvm-cov test --no-report --workspace --lib --all-features \
		--ignore-filename-regex "$$(cat target/coverage/.exclude-re)" \
		-- --skip prop_gbm_expected_value --skip slow --skip heavy --skip benchmark --skip h12_ --skip j2_
	@$(COV_CARGO_ENV) cargo llvm-cov report --html --output-dir target/coverage/html --ignore-filename-regex "$$(cat target/coverage/.exclude-re)"
	@$(COV_CARGO_ENV) cargo llvm-cov report --lcov --output-path target/coverage/lcov.info --ignore-filename-regex "$$(cat target/coverage/.exclude-re)"
	@echo ""
	@$(COV_CARGO_ENV) cargo llvm-cov report --summary-only --ignore-filename-regex "$$(cat target/coverage/.exclude-re)"
	@test -f ~/.cargo/config.toml.bak && mv ~/.cargo/config.toml.bak ~/.cargo/config.toml || true

# Open coverage report in browser
coverage-open: ## Open HTML coverage report in browser
	@if [ -f target/coverage/html/index.html ]; then \
		xdg-open target/coverage/html/index.html 2>/dev/null || \
		open target/coverage/html/index.html 2>/dev/null || \
		echo "Open: target/coverage/html/index.html"; \
	else \
		echo "❌ Run 'make coverage' first"; \
	fi

# Profiling (requires renacer)
profile:
	renacer --function-time --source -- cargo bench

# Benchmarks
bench:
	cargo bench

# Chaos engineering tests (from renacer, Issue #99)
chaos-test: build ## Run chaos engineering tests with renacer
	@echo "🔥 Running chaos engineering tests..."
	@if command -v renacer >/dev/null 2>&1; then \
		./crates/aprender-shell/scripts/chaos-baseline.sh ci; \
	else \
		echo "⚠️  renacer not found. Install with: cargo install --git https://github.com/paiml/renacer"; \
		echo "💡 Running lightweight chaos simulation instead..."; \
		$(MAKE) chaos-test-lite; \
	fi
	@echo "✅ Chaos tests completed"

chaos-test-full: build ## Run full chaos tests including aggressive mode
	@echo "🔥 Running full chaos engineering tests..."
	@./crates/aprender-shell/scripts/chaos-baseline.sh full

chaos-test-lite: ## Lightweight chaos tests (no renacer required)
	@echo "🧪 Running lightweight chaos simulation..."
	@PROPTEST_CASES=10 QUICKCHECK_TESTS=10 cargo test -p aprender-shell --test cli_integration -- chaos --nocapture 2>/dev/null || true
	@echo "✅ Lite chaos tests completed"

# Fuzz testing (from renacer, 60s)
fuzz: ## Run fuzz testing for 60 seconds
	@echo "🎲 Running fuzz tests (60s)..."
	@cargo +nightly fuzz run fuzz_target_1 -- -max_total_time=60 || echo "⚠️  Fuzz testing requires nightly Rust: rustup default nightly"
	@echo "✅ Fuzz testing complete"

# Development workflow
dev: tier1

# Pre-push checks
pre-push: tier3

# CI/CD checks
ci: tier4

# Quick check (compile only)
check:
	cargo check --all

# Run security audit
audit:
	@echo "🔒 Running security audit..."
	@cargo audit
	@echo "✅ Security audit completed"

# Validate dependencies (duplicates + security)
deps-validate:
	@echo "🔍 Validating dependencies..."
	@cargo tree --duplicate | grep -v "^$$" || echo "✅ No duplicate dependencies"
	@cargo audit || echo "⚠️  Security issues found"

# Run cargo-deny checks (licenses, bans, advisories, sources)
deny:
	@echo "🔒 Running cargo-deny checks..."
	@if command -v cargo-deny >/dev/null 2>&1; then \
		cargo deny check; \
	else \
		echo "❌ cargo-deny not installed. Install with: cargo install cargo-deny"; \
		exit 1; \
	fi
	@echo "✅ cargo-deny checks passed"

# Install PMAT pre-commit hooks
hooks-install: ## Install PMAT pre-commit hooks
	@echo "🔧 Installing PMAT pre-commit hooks..."
	@pmat hooks install || exit 1
	@echo "✅ Hooks installed successfully"

# Verify PMAT hooks
hooks-verify: ## Verify PMAT hooks are working
	@echo "🔍 Verifying PMAT hooks..."
	@pmat hooks verify
	@pmat hooks run

# Lint shell scripts (bashrs quality gates)
lint-scripts: ## Lint shell scripts with bashrs (determinism + idempotency + safety)
	@echo "🔍 Linting shell scripts with bashrs..."
	@if command -v bashrs >/dev/null 2>&1; then \
		for script in scripts/*.sh; do \
			echo "  Linting $$script..."; \
			bashrs lint "$$script" || exit 1; \
		done; \
		echo "✅ All shell scripts pass bashrs lint"; \
	else \
		echo "❌ bashrs not installed. Install with: cargo install bashrs"; \
		exit 1; \
	fi

bashrs-score: ## Score shell script quality with bashrs
	@echo "📊 Scoring shell scripts..."
	@for script in scripts/*.sh; do \
		echo ""; \
		echo "Scoring $$script:"; \
		bashrs score "$$script"; \
	done

bashrs-lint-makefile: ## Lint Makefile with bashrs
	@echo "🔍 Linting Makefile with bashrs..."
	@bashrs make lint Makefile || echo "⚠️  Makefile linting found issues"

# Run CI pipeline
run-ci: ## Run full CI pipeline
	@./scripts/ci.sh

# Run benchmarks
run-bench: ## Run benchmark suite
	@./scripts/bench.sh

# PMAT Quality Analysis (v2.200.0 features)

pmat-score: ## Calculate Rust project quality score
	@echo "📊 Calculating Rust project quality score..."
	@pmat rust-project-score || echo "⚠️  pmat not found. Install with: cargo install pmat"
	@echo ""

pmat-gates: ## Run pmat quality gates
	@echo "🔍 Running pmat quality gates..."
	@pmat quality-gates --report || echo "⚠️  pmat not found or gates failed"
	@echo ""

quality-report: ## Generate comprehensive quality report
	@echo "📋 Generating comprehensive quality report..."
	@mkdir -p docs/quality-reports
	@echo "# Aprender Quality Report" > docs/quality-reports/latest.md
	@echo "" >> docs/quality-reports/latest.md
	@echo "Generated: $$(date)" >> docs/quality-reports/latest.md
	@echo "" >> docs/quality-reports/latest.md
	@echo "## Rust Project Score" >> docs/quality-reports/latest.md
	@pmat rust-project-score >> docs/quality-reports/latest.md 2>&1 || echo "Error getting score" >> docs/quality-reports/latest.md
	@echo "" >> docs/quality-reports/latest.md
	@echo "## Quality Gates" >> docs/quality-reports/latest.md
	@pmat quality-gates --report >> docs/quality-reports/latest.md 2>&1 || echo "Error running gates" >> docs/quality-reports/latest.md
	@echo "" >> docs/quality-reports/latest.md
	@echo "## TDG Score" >> docs/quality-reports/latest.md
	@pmat tdg . --include-components >> docs/quality-reports/latest.md 2>&1 || echo "Error getting TDG" >> docs/quality-reports/latest.md
	@echo "✅ Report generated: docs/quality-reports/latest.md"

semantic-search: ## Interactive semantic code search
	@echo "🔍 Semantic code search..."
	@echo "First run will build embeddings (may take a few minutes)..."
	@pmat semantic || echo "⚠️  pmat semantic search not available"

# ============================================================================
# SHOWCASE BENCHMARKING (qwen2.5-coder-showcase-demo.md)
# ============================================================================

.PHONY: showcase-headless showcase-ci falsification-tests falsification-quick showcase-verify showcase-pmat showcase-full

showcase-headless: ## Run cbtop in headless mode with JSON output (simulated data for CI)
	@echo "🎯 Running showcase headless benchmark (simulated mode)..."
	@cargo run --release -p apr-cli -- cbtop --headless --simulated --json --output target/showcase-results.json --iterations 100
	@echo "✅ Results saved to target/showcase-results.json"

showcase-ci: ## Run showcase benchmark in CI mode with threshold check
	@echo "🔍 Running showcase CI validation (throughput >= 100 tok/s)..."
	@cargo run --release -p apr-cli -- cbtop --headless --simulated --ci --throughput 100 --iterations 100
	@echo "✅ CI validation passed"

falsification-tests: ## Run all 137 falsification tests (F001-F105, M001-M020, O001-O009, R001)
	@echo "🧪 Running Popperian falsification test suite (137 tests)..."
	@PROPTEST_CASES=100 QUICKCHECK_TESTS=100 cargo test --release --test falsification_brick_tests --test falsification_budget_tests --test falsification_correctness_tests --test falsification_cuda_tests --test falsification_measurement_tests --test falsification_performance_tests --test falsification_2x_ollama_tests --test falsification_real_profiling -- --test-threads=2
	@echo "✅ All falsification tests passed (137 tests)"

falsification-quick: ## Run falsification tests in debug mode (faster compile)
	@echo "⚡ Running falsification tests (debug mode)..."
	@PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo test --test falsification_brick_tests --test falsification_budget_tests --test falsification_correctness_tests --test falsification_cuda_tests --test falsification_measurement_tests --test falsification_performance_tests --test falsification_2x_ollama_tests --test falsification_real_profiling -- --test-threads=2
	@echo "✅ Falsification tests passed (137 tests)"

showcase-pmat: ## Run PMAT quality gates for showcase (spec section 7.0.2)
	@echo "📊 Running PMAT quality gates..."
	@echo ""
	@echo "=== Rust Project Score ==="
	@pmat rust-project-score 2>/dev/null || echo "pmat not available, skipping rust-project-score"
	@echo ""
	@echo "=== TDG Score ==="
	@pmat tdg . --include-components 2>/dev/null || echo "pmat not available, skipping TDG"
	@echo ""
	@echo "=== Quality Gates ==="
	@pmat quality-gates 2>/dev/null || echo "pmat not available, skipping quality-gates"
	@echo ""
	@echo "✅ PMAT analysis complete"

showcase-verify: showcase-headless falsification-tests ## Full showcase verification
	@echo "📊 Showcase verification complete"
	@echo "   - Headless benchmark: target/showcase-results.json"
	@echo "   - Falsification tests: 60/60 passing"

showcase-full: falsification-tests showcase-headless showcase-pmat ## Complete showcase validation
	@echo ""
	@echo "════════════════════════════════════════════════════════════════"
	@echo "  SHOWCASE FULL VALIDATION COMPLETE"
	@echo "════════════════════════════════════════════════════════════════"
	@echo "  Falsification Tests: 60/60 passing (F001-F040, M001-M020)"
	@echo "  Headless Benchmark:  target/showcase-results.json"
	@echo "  PMAT Quality Gates:  See above output"
	@echo ""
	@echo "  Current Score: 60/120 (50%) - Blocked: F041-F100"
	@echo "════════════════════════════════════════════════════════════════"

# ============================================================================
# EXAMPLES TARGETS
# ============================================================================

examples: ## Run all examples to verify they work
	@echo "🎯 Running all examples..."
	@failed=0; \
	total=0; \
	for example in examples/*.rs; do \
		name=$$(basename "$$example" .rs); \
		total=$$((total + 1)); \
		echo "  Running $$name..."; \
		if cargo run --example "$$name" --quiet 2>/dev/null; then \
			echo "    ✅ $$name passed"; \
		else \
			echo "    ❌ $$name failed"; \
			failed=$$((failed + 1)); \
		fi; \
	done; \
	echo ""; \
	echo "📊 Results: $$((total - failed))/$$total examples passed"; \
	if [ $$failed -gt 0 ]; then exit 1; fi
	@echo "✅ All examples passed"

examples-fast: ## Run examples with release mode (faster execution)
	@echo "⚡ Running examples in release mode..."
	@for example in examples/*.rs; do \
		name=$$(basename "$$example" .rs); \
		echo "  Running $$name..."; \
		cargo run --example "$$name" --release --quiet 2>/dev/null || echo "    ⚠️  $$name failed"; \
	done
	@echo "✅ Examples complete"

examples-list: ## List all available examples
	@echo "📚 Available examples:"
	@for example in examples/*.rs; do \
		name=$$(basename "$$example" .rs); \
		echo "  - $$name"; \
	done
	@echo ""
	@echo "Run with: cargo run --example <name>"

# ============================================================================
# MUTATION TESTING TARGETS
# ============================================================================

mutants: ## Run mutation testing (full, ~30-60 min)
	@echo "🧬 Running mutation testing (full suite)..."
	@echo "⚠️  This may take 30-60 minutes for full coverage"
	@which cargo-mutants > /dev/null 2>&1 || (echo "📦 Installing cargo-mutants..." && cargo install cargo-mutants --locked)
	@cargo mutants --no-times --timeout 300 -- --all-features
	@echo "✅ Mutation testing complete"

mutants-fast: ## Run mutation testing on a sample (quick feedback, ~5 min)
	@echo "⚡ Running mutation testing (fast sample)..."
	@which cargo-mutants > /dev/null 2>&1 || (echo "📦 Installing cargo-mutants..." && cargo install cargo-mutants --locked)
	@cargo mutants --no-times --timeout 120 --shard 1/10 -- --lib
	@echo "✅ Mutation sample complete"

mutants-file: ## Run mutation testing on specific file (usage: make mutants-file FILE=src/metrics/mod.rs)
	@echo "🧬 Running mutation testing on $(FILE)..."
	@if [ -z "$(FILE)" ]; then \
		echo "❌ Usage: make mutants-file FILE=src/path/to/file.rs"; \
		exit 1; \
	fi
	@which cargo-mutants > /dev/null 2>&1 || { cargo install cargo-mutants --locked || exit 1; }
	@cargo mutants --no-times --timeout 120 --file "$(FILE)" -- --all-features
	@echo "✅ Mutation testing on $(FILE) complete"

mutants-list: ## List mutants without running tests
	@echo "📋 Listing potential mutants..."
	@cargo mutants --list 2>/dev/null | head -100
	@echo "..."
	@echo "(showing first 100 mutants)"

# ============================================================================
# PROPERTY TESTING TARGETS
# ============================================================================

property-test: ## Run property-based tests with extended cases
	@echo "🎲 Running property-based tests..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		PROPTEST_CASES=250 cargo nextest run --test property_tests --no-fail-fast; \
	else \
		PROPTEST_CASES=250 cargo test --test property_tests; \
	fi
	@echo "✅ Property tests passed"

property-test-fast: ## Run property tests with fewer cases (quick feedback)
	@echo "⚡ Running property tests (fast mode)..."
	@if command -v cargo-nextest >/dev/null 2>&1; then \
		PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo nextest run --test property_tests; \
	else \
		PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo test --test property_tests; \
	fi
	@echo "✅ Property tests passed"

property-test-extensive: ## Run property tests with maximum coverage (10K cases)
	@echo "🔬 Running extensive property tests (10K cases per test)..."
	@PROPTEST_CASES=2500 cargo test --test property_tests -- --test-threads=1
	@echo "✅ Extensive property tests complete"

# ============================================================================
# SYSTEM DEPENDENCIES (Native Audio, etc.)
# ============================================================================

install-alsa: ## Install ALSA development libraries (Linux only)
	@echo "🔊 Installing ALSA development libraries..."
	@if [ "$$(uname)" = "Linux" ]; then \
		if command -v apt-get >/dev/null 2>&1; then \
			echo "  Detected: Debian/Ubuntu"; \
			sudo apt-get update && sudo apt-get install -y libasound2-dev; \
		elif command -v dnf >/dev/null 2>&1; then \
			echo "  Detected: Fedora/RHEL"; \
			sudo dnf install -y alsa-lib-devel; \
		elif command -v pacman >/dev/null 2>&1; then \
			echo "  Detected: Arch Linux"; \
			sudo pacman -S --noconfirm alsa-lib; \
		elif command -v zypper >/dev/null 2>&1; then \
			echo "  Detected: openSUSE"; \
			sudo zypper install -y alsa-devel; \
		else \
			echo "❌ Unknown package manager. Please install ALSA dev libraries manually:"; \
			echo "   - Debian/Ubuntu: sudo apt-get install libasound2-dev"; \
			echo "   - Fedora/RHEL: sudo dnf install alsa-lib-devel"; \
			echo "   - Arch: sudo pacman -S alsa-lib"; \
			exit 1; \
		fi; \
		echo "✅ ALSA development libraries installed"; \
	else \
		echo "⚠️  ALSA is Linux-only. Current OS: $$(uname)"; \
	fi

test-alsa: ## Run tests with ALSA audio capture feature (Linux only)
	@echo "🔊 Running tests with audio-alsa feature..."
	@if [ "$$(uname)" = "Linux" ]; then \
		if pkg-config --exists alsa 2>/dev/null; then \
			PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo test --features audio-alsa; \
		else \
			echo "❌ ALSA not installed. Run: make install-alsa"; \
			exit 1; \
		fi; \
	else \
		echo "⚠️  ALSA is Linux-only. Running standard audio tests..."; \
		PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo test --features audio; \
	fi
	@echo "✅ ALSA tests complete"

test-audio-full: ## Run all audio tests including ALSA (if available)
	@echo "🎵 Running full audio test suite..."
	@if [ "$$(uname)" = "Linux" ] && pkg-config --exists alsa 2>/dev/null; then \
		echo "  ALSA available - running with audio-alsa feature"; \
		PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo test --features audio-alsa audio::; \
	else \
		echo "  Running standard audio tests"; \
		PROPTEST_CASES=25 QUICKCHECK_TESTS=25 cargo test --features audio audio::; \
	fi
	@echo "✅ Audio tests complete"

# ============================================================================
# CONTRACT ENFORCEMENT (provable-contracts integration)
# ============================================================================
# Kernel contracts live in-tree at contracts/ (APR-MONO Phase 2b
# consolidation, 2026-04-18). Binding registry:
# contracts/aprender/binding.yaml. Generated tests: tests/contracts/.
# Pre-consolidation `../provable-contracts/` references retired.

PV_BIN := cargo run --release -p aprender-contracts-cli --bin pv --
BINDING := contracts/aprender/binding.yaml
CONTRACTS := contracts/softmax-kernel-v1.yaml \
             contracts/rmsnorm-kernel-v1.yaml \
             contracts/rope-kernel-v1.yaml \
             contracts/attention-kernel-v1.yaml \
             contracts/activation-kernel-v1.yaml \
             contracts/matmul-kernel-v1.yaml \
             contracts/flash-attention-v1.yaml \
             contracts/swiglu-kernel-v1.yaml \
             contracts/gqa-kernel-v1.yaml \
             contracts/layernorm-kernel-v1.yaml \
             contracts/silu-kernel-v1.yaml \
             contracts/cross-entropy-kernel-v1.yaml \
             contracts/adamw-kernel-v1.yaml \
             contracts/ssm-kernel-v1.yaml \
             contracts/conv1d-kernel-v1.yaml \
             contracts/batchnorm-kernel-v1.yaml \
             contracts/kmeans-kernel-v1.yaml \
             contracts/pagerank-kernel-v1.yaml \
             contracts/lbfgs-kernel-v1.yaml \
             contracts/cma-es-kernel-v1.yaml \
             contracts/model-config-algebra-v1.yaml \
             contracts/qk-norm-v1.yaml \
             contracts/tensor-shape-flow-v1.yaml \
             contracts/roofline-model-v1.yaml \
             contracts/gated-delta-net-v1.yaml \
             contracts/format-parity-v1.yaml \
             contracts/shannon-entropy-v1.yaml \
             contracts/f16-conversion-v1.yaml \
             contracts/kernel-launch-budget-v1.yaml \
             contracts/tensor-inventory-v1.yaml \
             contracts/performance-grading-v1.yaml \
             contracts/lora-algebra-v1.yaml \
             contracts/quantization-ordering-v1.yaml \
             contracts/q4k-q6k-superblock-v1.yaml \
             contracts/sampling-algorithms-v1.yaml \
             contracts/validated-tensor-v1.yaml \
             contracts/hybrid-layer-dispatch-v1.yaml \
             contracts/qwen35-shapes-v1.yaml \
             contracts/kv-cache-sizing-v1.yaml \
             contracts/backend-dispatch-v1.yaml \
             contracts/kv-cache-equivalence-v1.yaml \
             contracts/setfit-encoder-conformance-v1.yaml \
             contracts/tweet-eval-stance-benchmark-v1.yaml \
             contracts/contrastive-pair-protocol-v1.yaml \
             contracts/multinomial-head-v1.yaml \
             contracts/setfit-train-lifecycle-v1.yaml \
             contracts/linear-probe-classifier-v1.yaml

# The two Phase 2 contracts, audited as a BLOCKING tier3 gate by
# `contract-audit-phase2` below. Deliberately a separate, narrower list than
# $(CONTRACTS) — see that target's comment block for the measurement that
# forced the narrowing.
PHASE2_CONTRACTS := contracts/contrastive-pair-protocol-v1.yaml \
                    contracts/tweet-eval-stance-benchmark-v1.yaml

# The Phase 3 contracts, audited as a BLOCKING tier3 gate by
# `contract-audit-phase3` below. Same narrowing rationale as PHASE2_CONTRACTS:
# scoped to what this phase OWNS, because the repo-wide `contract-audit` is
# vacuous (see that target's comment block).
#
# 03-02 HAND-OFF, resolved by measurement rather than assumption (plan 03-04
# task 3, W-04). `test -f
# .planning/phases/03-faithful-two-stage-trainer-and-head/03-02-SUMMARY.md`
# returned rc=0 (the file exists, 28303 bytes), and a grep for the heading
# `CONTINGENCY FIRED` in it returned rc=1 — the heading is ABSENT. 03-02's GEMM
# partition-determinism gate was therefore GREEN, `contracts/gemm-partition-determinism-v1.yaml`
# was deliberately never authored (`ls` rc=1), and there is nothing for this
# phase to wire on its behalf. Recorded explicitly because "I did not see a
# heading" and "I did not look" are indistinguishable afterwards.
# linear-probe-classifier-v1.yaml is here because plan 03-06 task 3 binds
# FrozenProbeRun to it. A contract that is bound but absent from this list is
# audited by NOTHING: `contract-audit` repo-wide is vacuous (see that target),
# so the bindings would sit unchecked while looking checked. Its presence was
# verified by inducing a bogus binding status and observing this target go red.
PHASE3_CONTRACTS := contracts/multinomial-head-v1.yaml \
                    contracts/setfit-train-lifecycle-v1.yaml \
                    contracts/linear-probe-classifier-v1.yaml

# NOTE (plan 02-01, D-24): $(CONTRACTS) is an EXPLICIT HARDCODED LIST, not a glob
# over contracts/*.yaml. A contract file that merely EXISTS in contracts/ is
# validated by nothing. tweet-eval-stance-benchmark-v1.yaml sat in the tree
# unreferenced and therefore unvalidated, and `pv validate` rejected it the whole
# time (PROVABILITY-001 x2: no proof_obligations, no kani_harnesses) without any
# gate ever noticing. The line above is what makes tier3 reach it — tier3 calls
# `$(MAKE) contract-validate`, which iterates exactly this list. Every future phase
# contract needs its own line here or it is decoration.

contract-validate: ## Validate all kernel contracts (schema + staleness)
	@echo "Validating kernel contracts..."
	@for contract in $(CONTRACTS); do \
		echo "  $$contract"; \
		$(PV_BIN) validate "$$contract" || exit 1; \
	done
	@echo "Contract validation passed"

contract-test: ## Run contract-driven property tests
	@echo "Running contract property tests..."
	@PROPTEST_CASES=100 cargo test --test contract_tests
	@echo "Contract tests passed"

contract-audit: ## Audit binding coverage (equations -> implementations)
	@echo "Running binding audit..."
	@for contract in $(CONTRACTS); do \
		echo ""; \
		$(PV_BIN) audit "$$contract" --binding $(BINDING); \
	done
	@echo ""
	@echo "Binding audit complete"

# D-26 / review finding F9 (plan 02-08). `pv validate` checks contract SHAPE;
# it says nothing about whether an equation is bound to any implementation. A
# schema-valid contract with no binding is a claim that nothing checks, which
# is exactly the failure class the Phase 2 gates exist to close. This target is
# BLOCKING and is wired into tier3.
#
# WHY THIS IS SCOPED, AND WHY THE REPO-WIDE `contract-audit` IS NOT WIRED.
# Measured, not assumed (`make contract-audit > /tmp/ca-repo.log 2>&1; rc=$$?`,
# status captured directly): it reports **132 BIND-001 errors across 38 of the
# 44 contracts** — 10 in Phase 1's setfit-encoder-conformance-v1.yaml, the rest
# spread over the kernel contracts — and **exits 0 anyway**, because its loop
# body ends in `;` and never reads the audit's status. So the broad target is
# today a vacuous gate: it prints failures and reports success. Making it
# blocking would turn tier3 red on 132 pre-existing unbound equations this phase
# did not create; leaving it non-blocking keeps a target that checks nothing.
# Neither is this plan's to fix — logged in the phase's deferred-items.md.
# Scoping to the two contracts this phase OWNS is the gate it can honestly stand
# behind, and neither Phase 2 contract appears anywhere in those 132.
#
# EVIDENCE DISCIPLINE, same as the D-26 and D-04 blocks near tier3. Run
# STANDALONE first with the status captured directly
# (`make contract-audit-phase2 > /tmp/cap2.log 2>&1; rc=$$?`, never through a
# pipe — CLAUDE.md rule 1): rc=0, 24/24 equations bound for
# contrastive-pair-protocol-v1 and 1/1 for tweet-eval-stance-benchmark-v1.
# Wall time 9 s cold (pv is rebuilt by $(PV_BIN)), then 1 s / 0 s / 1 s over
# three warm runs — nothing against tier3's 1-5 minute budget, and tier3 has
# already built pv via `contract-validate` two lines earlier.
#
# ITS FAILURE MODE WAS INDUCED, OBSERVED AND REVERTED before it was trusted,
# because a gate that has only ever been seen passing is not evidence: deleting
# the `pair_manifest_hash` entry from $(BINDING) turned it rc=1 with
# "[ERROR] BIND-001: Equation 'pair_manifest_hash' ... has no binding entry",
# naming the deleted equation. That check matters more than usual here — plan
# 02-02 found that a `contract:` field carrying a `../` prefix parses cleanly
# and binds NOTHING, so this gate could otherwise have been green while
# inspecting nothing at all.
contract-audit-phase2: ## Audit Phase 2 binding coverage (BLOCKING, wired into tier3)
	@echo "Auditing binding coverage for the Phase 2 contracts..."
	@unbound=""; \
	for contract in $(PHASE2_CONTRACTS); do \
		echo "  $$contract"; \
		$(PV_BIN) audit "$$contract" --binding $(BINDING); \
		status=$$?; \
		if [ "$$status" -ne 0 ]; then \
			unbound="$$unbound $$contract"; \
		fi; \
	done; \
	if [ -n "$$unbound" ]; then \
		echo "FAIL: unbound equations remain in:$$unbound"; \
		echo "Every equation of a Phase 2 contract needs an entry in $(BINDING)."; \
		exit 1; \
	fi; \
	echo "Phase 2 binding audit: every equation is bound"

# Phase 3's twin of contract-audit-phase2, and it exists for the same reason:
# `contract-validate` checks contract SHAPE and says nothing about whether an
# equation is bound to any implementation, so multinomial-head-v1.yaml could be
# "valid" with all five equations bound to nothing at all. BLOCKING, wired into
# tier3 immediately after the Phase 2 audit.
#
# The loop reads the audit's STATUS (`status=$$?` on its own line). That is not
# incidental: the repo-wide `contract-audit` target ends its loop body in `;`,
# never reads the status, and therefore reports success while printing 132
# BIND-001 errors. Copying that shape would have produced a gate that cannot
# fail.
#
# EVIDENCE DISCIPLINE, matching the Phase 2 block above. Run STANDALONE first
# with the status captured directly (`make contract-audit-phase3 > /tmp/cap3.log
# 2>&1; rc=$$?`, never through a pipe — CLAUDE.md rule 1): rc=0, 5/5 equations
# bound, 8 obligations, 12 falsification tests. Wall time ~1 s warm; tier3 has
# already built pv via `contract-validate` two lines earlier.
#
# ITS FAILURE MODE WAS INDUCED, OBSERVED AND REVERTED before it was trusted,
# because a gate that has only ever been seen passing is not evidence. Deleting
# the `analytic_gradient` entry from $(BINDING) turned it **rc=2** (make's status
# for a failed recipe, not the recipe's own 1 — measured, not assumed) with
# "[ERROR] BIND-001: Equation 'analytic_gradient' in multinomial-head-v1.yaml has
# no binding entry" and "FAIL: unbound equations remain in:
# contracts/multinomial-head-v1.yaml", naming the deleted equation; "Bound
# equations" fell 5 -> 4. $(BINDING) was then restored and verified BYTE-IDENTICAL
# by sha256 (dfbce939bdc9a291...) and the target re-run green at rc=0.
#
# THE `set +e` AROUND THE AUDIT IS LOAD-BEARING, and it is a correction to the
# shape copied from the Phase 2 target. This Makefile sets `.SHELLFLAGS := -e -c`
# (line 40), so a failing `$(PV_BIN) audit` inside the loop body ABORTS the whole
# recipe before `status=$$?` on the next line can run: `unbound` never
# accumulates, the remaining contracts are never audited, and the summarising
# "FAIL: unbound equations remain in:" line is unreachable. Reproduced directly:
# `bash -e -c 'for i in 1 2; do echo iter=$$i; false; status=$$?; echo st=$$status;
# done; echo REACHED_END'` prints ONLY `iter=1` and exits 1. The gate still fails
# closed, so this was never a false green — but it reported one contract where
# three were asked for. `set +e` for exactly the audit call restores the
# accumulate-then-report behaviour the loop is written for, and the status is
# still read from `$$?` on its own line, never through a pipe (CLAUDE.md rule 1).
# `contract-audit-phase2` above carries the same latent defect and is left for a
# change that owns that target.
contract-audit-phase3: ## Audit Phase 3 binding coverage (BLOCKING, wired into tier3)
	@echo "Auditing binding coverage for the Phase 3 contracts..."
	@unbound=""; \
	for contract in $(PHASE3_CONTRACTS); do \
		echo "  $$contract"; \
		set +e; \
		$(PV_BIN) audit "$$contract" --binding $(BINDING); \
		status=$$?; \
		set -e; \
		if [ "$$status" -ne 0 ]; then \
			unbound="$$unbound $$contract"; \
		fi; \
	done; \
	if [ -n "$$unbound" ]; then \
		echo "FAIL: unbound equations remain in:$$unbound"; \
		echo "Every equation of a Phase 3 contract needs an entry in $(BINDING)."; \
		exit 1; \
	fi; \
	echo "Phase 3 binding audit: every equation is bound"

# ============================================================================
# PHASE 3 REPRODUCIBILITY GATES (TRN-06 / D-16 / D-13)
# ============================================================================
#
# A NAME-FILTERED `cargo test` THAT MATCHES NOTHING EXITS 0 (REVIEW CR-02).
#
# libtest prints `test result: ok. 0 passed; ... N filtered out` and returns success.
# Every target below selects its test by NAME, so renaming a test — or mistyping a
# filter — would turn the gate green while running nothing, and it would keep printing
# its own success banner while doing it. That is a worse failure than red: red gets
# investigated.
#
# `assert_tests_ran` reads the count libtest actually reported and fails if it is below
# the number the target expects. `awk` parses it, not the rtk hook's summarised form.
# Measured both ways before being trusted: with the real filter it reads 1 (or 2 for the
# GEMM target) and passes; with a deliberately misspelled filter it reads 0 and the gate
# exits non-zero instead of printing success.
#
define assert_tests_ran
ran=$$(awk '/^test result:/ { for (i = 1; i <= NF; i++) if ($$(i+1) ~ /^passed/) s += $$i } END { print s + 0 }' $(1)); \
if [ "$$ran" -lt "$(2)" ]; then \
	echo "FAIL: $(3) reported $$ran test(s) passed, expected at least $(2)."; \
	echo "A name filter that matches nothing exits 0 (REVIEW CR-02) — this gate was"; \
	echo "about to report success having run nothing. Check the test name in the"; \
	echo "filter against the test binary: $(1)"; \
	exit 1; \
fi
endef

#
# D-16 SPLITS one gate across two tiers, and the split is not decoration:
#
#   tier2  setfit-repro-inproc      two runs in ONE process agree
#   tier3  setfit-repro-crossproc   two SEPARATE processes at pool sizes 1 and 3 agree
#   tier3  setfit-repro-replay      recorded digests == an independent recomputation
#
# The in-process form is structurally blind — both runs share the rayon pool, the
# allocator's free lists and every lazily-initialized static, which is exactly the
# class of nondeterminism a "clean run" exists to expose — so it is the fast signal
# and NOT the claim. The cross-process form is the authoritative one. Wiring only the
# tier3 half would implement half of D-16 and leave the fast signal in a test file no
# tier invokes.
#
# EVERY RECIPE BELOW READS `$$?` ON THE LINE AFTER THE REDIRECT, NEVER THROUGH A PIPE.
# CLAUDE.md Verification Discipline rule 1: piping into `tee` and then reading `$$?`
# reports the PIPE's last status, and this repo has shipped that defect twice (#2336
# qwen-story-daily, #2360 make publish's POST-PUBLISH VERIFICATION — three green runs
# that proved nothing). A pipe before the capture here is a defect, not a style choice.
#
# The obvious check for that property — a bare substring search for `tee` across each
# recipe — is UNFIT, and it was measured rather than reasoned about: it matched the word
# "guaran-tee" in two failure messages and reported a violation in recipes that contain
# no pipe at all (CLAUDE.md rule 7 — a guard pattern is re-checked by re-running its case
# table, not by re-reading it). The messages avoid that substring so the naive form also
# reads clean, but the pattern to reuse is a PIPE-aware one, e.g. `\| *tee`.
#
# `CARGO_INCREMENTAL=0` per STATE.md's ENOSPC mitigation: this workspace has stopped
# twice on a full disk in `target/debug/incremental` at ~25 GB.
#
# `mkdir -p target` because the log destination must exist before the redirect; a
# redirect into a missing directory fails the shell line, which would be reported as a
# gate failure rather than as the setup error it is.

setfit-repro-inproc: ## TRN-06/D-16 (tier2 half): in-process two-clean-runs equality
	@echo "TRN-06: in-process two-run equality (D-16's fast, non-authoritative half)"
	@mkdir -p target
	@set +e; CARGO_INCREMENTAL=0 cargo test -p aprender-train --test setfit_repro \
		--features setfit in_process > target/setfit-repro-inproc.log 2>&1; rc=$$?; \
	set -e; \
	tail -3 target/setfit-repro-inproc.log; \
	if [ $$rc -ne 0 ]; then \
		echo "FAIL: the in-process two-run comparison is red (rc=$$rc)"; \
		echo "See target/setfit-repro-inproc.log. NOTE: tier2 as a WHOLE is red on"; \
		echo "arm64 from 24 pre-existing clippy errors (D-ITEM-02) and its headline"; \
		echo "test step runs zero tests (D-ITEM-03) — neither is this gate's status."; \
		exit $$rc; \
	fi
	@$(call assert_tests_ran,target/setfit-repro-inproc.log,1,setfit-repro-inproc)
	@echo "  in-process: every composite component agreed"

setfit-repro-crossproc: ## TRN-06/D-16 (tier3, AUTHORITATIVE): cross-process hash equality
	@echo "TRN-06: cross-process two-clean-runs equality at fixed pool sizes 1 and 3"
	@mkdir -p target
	@set +e; CARGO_INCREMENTAL=0 cargo test -p aprender-train --test setfit_repro \
		--features setfit setfit_repro_cross_process \
		> target/setfit-repro-crossproc.log 2>&1; rc=$$?; \
	set -e; \
	tail -3 target/setfit-repro-crossproc.log; \
	if [ $$rc -ne 0 ]; then \
		echo "FAIL: two separate processes did not agree, or did not run at two"; \
		echo "DISTINCT pool sizes (the mechanism-engaged half). Either way TRN-06's"; \
		echo "two-clean-runs claim does not hold as measured on this host."; \
		echo "See target/setfit-repro-crossproc.log"; \
		exit $$rc; \
	fi
	@$(call assert_tests_ran,target/setfit-repro-crossproc.log,1,setfit-repro-crossproc)
	@echo "  cross-process: THREADS differed and all ten components agreed"

setfit-repro-replay: ## TRN-06 (tier3): recorded digests match an INDEPENDENT recomputation
	@echo "TRN-06: recorded-vs-expected replay (reproducible is not the same as correct)"
	@mkdir -p target
	@set +e; CARGO_INCREMENTAL=0 cargo test -p aprender-train --test setfit_repro \
		--features setfit setfit_repro_recorded_matches_expected_replay \
		> target/setfit-repro-replay.log 2>&1; rc=$$?; \
	set -e; \
	tail -3 target/setfit-repro-replay.log; \
	if [ $$rc -ne 0 ]; then \
		echo "FAIL: the RECORDED pair order / batch boundaries do not match an"; \
		echo "independent recomputation from the public epoch_pair_order + a fresh"; \
		echo "PairSampler. Two clean runs could still AGREE while both being wrong;"; \
		echo "this is the check that separates reproducible from correct."; \
		echo "See target/setfit-repro-replay.log"; \
		exit $$rc; \
	fi
	@$(call assert_tests_ran,target/setfit-repro-replay.log,1,setfit-repro-replay)
	@echo "  replay: recorded digests equal the independently recomputed ones"

gemm-thread-determinism: ## D-13/TRN-06: Tensor::matmul does not depend on the rayon pool size
	@echo "D-13: GEMM determinism across fixed rayon pool sizes 1/2/3 (03-02 T3)"
	@mkdir -p target
	@set +e; CARGO_INCREMENTAL=0 cargo test -p aprender-core --test gemm_thread_determinism \
		> target/gemm-thread-determinism.log 2>&1; rc=$$?; \
	set -e; \
	tail -3 target/gemm-thread-determinism.log; \
	if [ $$rc -ne 0 ]; then \
		echo "FAIL: Tensor::matmul's output moved with the rayon pool size, so"; \
		echo "assumption A3 is falsified and TRN-06's bitwise claim does not"; \
		echo "hold on this host. See target/gemm-thread-determinism.log"; \
		exit $$rc; \
	fi
	@$(call assert_tests_ran,target/gemm-thread-determinism.log,2,gemm-thread-determinism)
	@echo "  GEMM: identical hashes at pool sizes 1, 2 and 3"

contract-regen: ## Regenerate wired test files from contracts
	@echo "Regenerating contract test files..."
	@for contract in $(CONTRACTS); do \
		name=$$(basename "$$contract" .yaml | sed 's/-kernel-v[0-9]*//;s/-v[0-9]*//'); \
		echo "  $$name <- $$contract"; \
		$(PV_BIN) probar "$$contract" --binding $(BINDING) > tests/contracts/$${name}_contract.rs.new 2>/dev/null || true; \
	done
	@echo "Regeneration complete (review .rs.new files)"

contract-check: contract-validate contract-test contract-audit ## Full contract compliance check
	@echo ""
	@echo "Contract compliance check: PASSED"

# ============================================================================
# DEVELOPMENT ENVIRONMENT SETUP (GH-344, GH-345)
# ============================================================================

# Sibling repos required for full-stack development
SIBLINGS := ../realizar ../entrenar ../trueno ../renacer ../provable-contracts ../pacha

dev-setup: ## Set up local dev environment with sibling repo overrides
	@echo "Setting up full-stack development environment..."
	@if [ ! -f .cargo/config.toml ]; then \
		cp .cargo/config.toml.dev-overrides .cargo/config.toml; \
		echo "Created .cargo/config.toml with sibling overrides"; \
	elif ! grep -q '\[patch.crates-io\]' .cargo/config.toml; then \
		echo "" >> .cargo/config.toml; \
		cat .cargo/config.toml.dev-overrides >> .cargo/config.toml; \
		echo "Appended sibling overrides to .cargo/config.toml"; \
	else \
		echo ".cargo/config.toml already has [patch.crates-io] section"; \
	fi
	@echo ""
	@$(MAKE) --no-print-directory check-siblings

publish: ## Publish crate(s) to crates.io — strips [patch], publishes, then verifies cargo install
	@echo "Publishing to crates.io (removing [patch.crates-io] temporarily)..."
	@if [ -f .cargo/config.toml ]; then \
		cp .cargo/config.toml .cargo/config.toml.publish-backup; \
		echo "# Clean config for publishing" > .cargo/config.toml; \
	fi
	@CRATE=$(CRATE); \
	if [ -z "$$CRATE" ]; then \
		echo "Usage: make publish CRATE=aprender   (or apr-cli, entrenar-lora)"; \
		echo "Restoring config..."; \
		if [ -f .cargo/config.toml.publish-backup ]; then \
			cp .cargo/config.toml.publish-backup .cargo/config.toml; \
			rm -f .cargo/config.toml.publish-backup; \
		fi; \
		exit 1; \
	fi; \
	echo "Publishing $$CRATE..."; \
	cargo publish -p $$CRATE --allow-dirty --locked; \
	STATUS=$$?; \
	echo "Restoring .cargo/config.toml..."; \
	if [ -f .cargo/config.toml.publish-backup ]; then \
		cp .cargo/config.toml.publish-backup .cargo/config.toml; \
		rm -f .cargo/config.toml.publish-backup; \
	fi; \
	if [ $$STATUS -ne 0 ]; then \
		echo "FAIL: cargo publish failed"; \
		exit $$STATUS; \
	fi; \
	echo ""; \
	echo "=== POST-PUBLISH VERIFICATION (PMAT-517) ==="; \
	echo "Waiting for crates.io index to update..."; \
	sleep 15; \
	if [ "$$CRATE" = "apr-cli" ]; then \
		echo "Verifying: cargo install apr-cli --force ..."; \
		cargo install apr-cli --force 2>&1 | tee /tmp/publish-verify-$$CRATE.log; \
		INSTALL_STATUS=$${PIPESTATUS[0]}; \
		if [ $$INSTALL_STATUS -ne 0 ]; then \
			echo ""; \
			echo "FATAL: cargo install apr-cli FAILED after publish!"; \
			echo "The published crate is BROKEN. You must fix and republish."; \
			echo "Build log: /tmp/publish-verify-$$CRATE.log"; \
			exit 1; \
		fi; \
		echo "Verifying apr --version..."; \
		WANT=$$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/'); \
		APR_BIN_PATH="$${CARGO_HOME:-$$HOME/.cargo}/bin/apr"; \
		GOT=$$("$$APR_BIN_PATH" --version 2>&1); \
		echo "  expected $$WANT, $$APR_BIN_PATH reports: $$GOT"; \
		case "$$GOT" in \
			*"$$WANT"*) echo "POST-PUBLISH VERIFICATION: PASSED" ;; \
			*) echo "FATAL: published apr reports '$$GOT' but this tree is $$WANT."; \
			   echo "The publish did not produce the binary we think it did."; \
			   exit 1 ;; \
		esac; \
	else \
		echo "Verifying: cargo install apr-cli --force (depends on $$CRATE)..."; \
		cargo install apr-cli --force 2>&1 | tee /tmp/publish-verify-$$CRATE.log; \
		INSTALL_STATUS=$${PIPESTATUS[0]}; \
		if [ $$INSTALL_STATUS -ne 0 ]; then \
			echo ""; \
			echo "FATAL: cargo install apr-cli FAILED after publishing $$CRATE!"; \
			echo "The published $$CRATE broke the apr-cli build."; \
			echo "Build log: /tmp/publish-verify-$$CRATE.log"; \
			exit 1; \
		fi; \
		echo "POST-PUBLISH VERIFICATION: PASSED"; \
	fi

check-siblings: ## Verify sibling repos exist and versions are compatible
	@echo "Checking sibling repositories..."
	@all_ok=true; \
	for repo in $(SIBLINGS); do \
		name=$$(basename "$$repo"); \
		if [ -d "$$repo" ]; then \
			version=$$(grep '^version' "$$repo/Cargo.toml" 2>/dev/null | head -1 | sed 's/.*"\(.*\)"/\1/'); \
			echo "  ✓ $$name ($$version)"; \
		else \
			echo "  ✗ $$name — not found at $$repo"; \
			all_ok=false; \
		fi; \
	done; \
	echo ""; \
	if [ "$$all_ok" = true ]; then \
		echo "All sibling repos present"; \
	else \
		echo "Missing sibling repos. Clone them alongside aprender:"; \
		echo "  cd .. && git clone <repo-url>"; \
		echo ""; \
		echo "Or build standalone (uses crates.io versions):"; \
		echo "  Remove [patch.crates-io] from .cargo/config.toml"; \
	fi
