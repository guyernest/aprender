---
phase: 04
phase_name: apr-artifact-and-production-parity
reviewed: 2026-08-15T00:00:00Z
depth: standard
diff_base: 87b780eefabb4dbb6f5e64efe2f5b6698d058e73
files_reviewed: 35
files_reviewed_list:
  - crates/apr-cli/src/commands/builder.rs
  - crates/apr-cli/src/commands/construction.rs
  - crates/apr-cli/src/commands/data_contrastive.rs
  - crates/apr-cli/src/commands/eval/mod.rs
  - crates/apr-cli/src/commands/eval/setfit.rs
  - crates/apr-cli/src/commands/inspect.rs
  - crates/apr-cli/src/commands/inspect_output_json.rs
  - crates/apr-cli/src/commands/inspect_setfit.rs
  - crates/apr-cli/src/commands/mod.rs
  - crates/apr-cli/src/commands/predict.rs
  - crates/apr-cli/src/commands/serve/handlers.rs
  - crates/apr-cli/src/commands/setfit_train.rs
  - crates/apr-cli/src/dispatch_analysis.rs
  - crates/apr-cli/src/extended_commands.rs
  - crates/apr-cli/src/lib.rs
  - crates/apr-cli/src/lib_dispatch_coverage.rs
  - crates/apr-cli/src/lib_parse_rosetta_02.rs
  - crates/apr-cli/src/lib_verbose_inheritance_parse.rs
  - crates/apr-cli/src/setfit_commands.rs
  - crates/apr-cli/src/setfit_io.rs
  - crates/apr-cli/src/setfit_tag.rs
  - crates/aprender-serve/src/api/mod.rs
  - crates/aprender-serve/src/api/mod_app_state_gpu.rs
  - crates/aprender-serve/src/api/mod_app_state_new.rs
  - crates/aprender-serve/src/api/router.rs
  - crates/aprender-serve/src/api/setfit_handlers.rs
  - crates/aprender-serve/src/api/types.rs
  - crates/aprender-train/src/train/setfit/apr_codec.rs
  - crates/aprender-train/src/train/setfit/apr_evaluate.rs
  - crates/aprender-train/src/train/setfit/apr_reload.rs
  - crates/aprender-train/src/train/setfit/credential.rs
  - crates/aprender-train/src/train/setfit/evaluate.rs
  - crates/aprender-train/src/train/setfit/lock.rs
  - crates/aprender-train/src/train/setfit/mod.rs
  - crates/aprender-train/src/train/setfit/verify.rs
findings:
  critical: 2
  warning: 6
  info: 5
  total: 13
status: findings
---

# Phase 04: Code Review Report

**Reviewed:** 2026-08-15
**Depth:** standard (per-file, Rust-specific, with cross-file tracing into `aprender-core`)
**Files Reviewed:** 35
**Status:** findings

## Summary

The security spine — `verify.rs`, `credential.rs`, `lock.rs`, `apr_reload.rs`, `setfit_io.rs` — holds up
under adversarial reading. Four of the five specific concerns raised in the brief are **refuted** (see
"Specific concerns: verdicts" below); the fifth is confirmed as bounded and safe. No `unwrap()`, no SATD,
no `unsafe`, no tensor indexing in scope.

The defects are all in the **consumer surfaces**, and they cluster in one shape: the *validation* path
and the *test* path of `apr eval` were written by different hands and the test path is missing gates the
validation path has. The two Critical findings are (1) `apr eval --split test` computing its accuracy by
comparing an artifact-label index against a dataset-label index with no label-map gate — the exact
"confidently wrong number rather than an error" the sibling path refuses by name — and (2) the new
`POST /v1/classify` surface being served without the `AuthGate` layer that the CLI's own APR router
applies, so `APR_API_KEY` silently does not protect it and not even the boot warning fires.

Two write paths (`atomic_write`, `write_lock`) share a clobber window that their doc comments claim to
have closed, and `write_lock` additionally uses a predictable, symlink-following temp file where its
in-repo precedent (`setfit_train::temp_path` + `create_new`) does not.

---

## Specific concerns: verdicts

| # | Concern | Verdict |
|---|---------|---------|
| 1 | Bounded read reports declared vs streamed length; large read before refusal | **REFUTED.** `artifact.rs:1926-1934` refuses on `declared_length` *before the reader is touched*, and `artifact.rs:1941` clamps the pre-allocation to `min(declared, cap)` so a lying length cannot over-reserve. `setfit_io.rs:71-89` stats first and passes `Some(metadata.len())`. Neighbouring readers (`predict.rs:236-271`, `eval/setfit.rs:623-660`, `setfit_tag.rs:111-133`, `inspect.rs:548-563`) all apply the same declared-then-stream shape. See IN-04 for the one that does not. |
| 2 | `into_artifact_bytes` ordering hazard / unnecessary 1.8 MB clones | **REFUTED.** Exactly one call site (`setfit_train.rs:654`), and it reads all three report values at 646-648 *before* consuming. No clone of the buffer exists anywhere; `verify.rs:698` takes only `bytes.len()` before the move at 732. |
| 3 | Retained bytes reachable via `Debug`/`Display`/serde | **REFUTED.** `RetainedArtifactBytes` (`mod.rs:404-410`) has a hand-written `Debug`; the field is `pub(crate)` with no accessor; `ArtifactVerifiedEvidence` and `SetFitRun` derive only `Debug` — no `Serialize`/`Display` anywhere in `train/setfit/`. See IN-05 for a scope caveat. |
| 4 | Credential comparisons constant-shape / total; seal unbreakable | **PARTIALLY CONFIRMED.** The seal is real: `mod sealed` in `credential.rs:64-67` is module-private, so a sibling module cannot even name `Sealed`, and the trybuild `.stderr` pins rustc's refusal. All comparisons are total `String`/`[u8;32]` equality with no short-circuit that admits a partial match. **But only 1 of the credential's 3 values is ever re-checked** — see IN-02. |
| 5 | HTTP: body-limit vs batch-bound; path leaks; readiness lying about verification | **CONFIRMED SAFE for the three named sub-concerns.** `router.rs:132-141` attaches `DefaultBodyLimit::max(1 MiB)` to `/v1/classify` only; batch bound is checked after parse (`setfit_handlers.rs:158-167`) and again inside core — neither subsumes the other, correctly. Error bodies are core's typed `Display` strings plus a fixed transport sentence; no path, no `Debug` of internal state reaches the wire. Readiness cannot lie: `classifier_verified` is `Some(true)` only when `setfit_model` is populated, and that slot's type `VerifiedSetFitModel` has no constructor outside `load_setfit_apr`. **A separate, larger HTTP problem was found** — see CR-02. |
| 6 | `AprCodec::deserialize` reachable out-of-crate without probe replay | **CONFIRMED AND BOUNDED.** `AprCodec` *is* reachable (`entrenar::train::setfit::apr_codec::AprCodec`, which `setfit_train.rs:57` imports), and so is `SetFitCodec` (`pub mod verify`). So an out-of-crate caller can obtain a `SetFitBundle` with no probe replay. The bound holds: `SetFitBundle`'s twenty fields are all `pub(crate)` (`bundle.rs:463-502`), `from_run_parts` is `pub(crate)`, `LifecycleState` and `SetFitCredential` are both sealed, and `AppState::with_setfit_model` takes `VerifiedSetFitModel` — so **no route turns a raw bundle into anything trusted**. Worth restating in the contract, not fixing in code. |

---

## Critical Issues

### CR-01: `apr eval --split test` compares artifact label indices against dataset label indices with no label-map gate

**File:** `crates/apr-cli/src/commands/eval/setfit.rs:490-505`

**Issue:**
`evaluate_test` builds `labels` from the **artifact's head** (`credential.model().ordered_labels()`, line
490) and scores against `row.label`, which is the **dataset's** declared label index:

```rust
let labels = credential.model().ordered_labels().to_vec();
...
let predicted = labels.iter().position(|label| label == result.label());
if predicted == Some(row.label) { correct += 1; }
```

Nothing on this path checks that the two label maps are the same list in the same order. The sibling
validation evaluator refuses exactly this, by name, at
`crates/aprender-train/src/train/setfit/apr_evaluate.rs:219-229`:

> *"Index i of one is not index i of the other, so every prediction would be compared against a different
> class's truth and the metric would be a confidently wrong number rather than an error"* —
> `AprEvaluateError::LabelMapMismatch`

None of the three upstream doors closes the gap. `reload_verified_run_from_apr` gates the *corpus*, the
*access ledger* and the *selection draw* (`apr_reload.rs:339-372`) — the head's label ordering is not one
of the three. `mint_test_token` compares only the artifact hash (`lock.rs:604-610`). `grant` compares the
artifact hash and the dataset fingerprint (`lock.rs:801-814`). A permuted head label row is invisible to
all of them.

**Reachability, stated honestly.** In the happy path the labels agree because both derive from the same
prepared dataset, and a run that goes validation-then-test would have hit `LabelMapMismatch` at
validation time. It bites when the test invocation does *not* follow a validation invocation in the same
tree — which is the documented workflow (`eval/setfit.rs:11-20`: two invocations with a durable file
between them), and `lock.rs:419-429` explicitly states a hand-written, internally consistent lock file
passes its own integrity check. It also bites on a version-skewed artifact whose head rows were written in
a different order. The output is a plausible accuracy in `[0,1]`, reported as the phase's headline test
number, with no error.

**Fix:** apply the sibling's gate at the top of `evaluate_test`, before any classify runs:

```rust
fn evaluate_test(
    credential: &ReloadedSetFitCredential,
    dataset: &PreparedDataset<Canonical>,   // add this parameter
    split: &Split<Test>,
) -> Result<TestMeasurement> {
    let labels = credential.model().ordered_labels().to_vec();
    let dataset_labels = dataset.label_names().to_vec();
    if labels != dataset_labels {
        return Err(CliError::ValidationFailed(format!(
            "the artifact's head indexes by {labels:?} and the dataset declares {dataset_labels:?}; \
             index i of one is not index i of the other, so every prediction would be scored \
             against a different class's truth"
        )));
    }
    ...
```

`run_test` already holds `dataset` (line 398), so the call site at line 423 becomes
`evaluate_test(credential, dataset, grant.test())?`.

---

### CR-02: the new `POST /v1/classify` server is mounted without the `AuthGate`, so `APR_API_KEY` does not protect it and no warning is emitted

**File:** `crates/apr-cli/src/commands/serve/handlers.rs:1502-1503`

**Issue:**
`start_setfit_server` builds and serves its router with no auth layer:

```rust
let state = AppState::default().with_setfit_model(Arc::new(model));
let app = create_router_with_config(state, RouterConfig::default());   // <- no auth::layer
...
axum::serve(listener, app)
```

`realizar::api::create_router_with_config` (`crates/aprender-serve/src/api/router.rs:57-169`) attaches
CORS (`CorsLayer::permissive()`, line 167) and the JSON-rejection sanitiser — but no authentication. The
CLI's own APR-CPU path does the opposite: `handlers.rs:1146` calls
`build_apr_cpu_router(state, super::auth::AuthGate::from_env())` and `handlers.rs:1332` applies
`super::auth::layer(auth_gate, router)`.

Consequence: an operator who sets `APR_API_KEY` (or `APR_API_KEY_HASH`) and runs
`apr serve classifier.apr` gets a **fully unauthenticated** `/v1/classify` plus `/health*`, `/metrics`,
`/v1/predict`, `/api/chat` and every other route `create_router_with_config` mounts — with
`CorsLayer::permissive()` in front of it, so any web origin can reach it. Worse than the bypass itself:
because `AuthGate::from_env()` is never *called* on this path, the "no APR_API_KEY … HTTP routes are
unauthenticated" warning at `auth.rs:70-72` never prints. The operator has no signal at all.

The `run_cpu_server` path (`serve/server.rs`, `realizar::api::create_router`) shares this gap, so the
condition is not unique to Phase 4 — but this is a **new** route added by this phase, on the surface a
classifier deployment is most likely to expose, and the fix is one line using a function that is already
generic over the router's state type (`auth::layer<S>`, `auth.rs:176-184`, works for `Router<()>`).

**Fix:**

```rust
let state = AppState::default().with_setfit_model(Arc::new(model));
let app = super::auth::layer(
    super::auth::AuthGate::from_env(),
    create_router_with_config(state, RouterConfig::default()),
);
```

The `setfit_serve_startup_reads_bounded_loads_through_the_one_door_and_builds_the_real_router` source
scan (`handlers.rs:1694-1732`) should gain a fifth needle asserting the `auth::layer` call site exists,
so the gate cannot be removed silently.

---

## Warnings

### WR-01: `atomic_write` and `write_lock` both clobber a file that appears in the check→rename window, and both doc comments deny it

**File:** `crates/apr-cli/src/commands/setfit_train.rs:176-207`; `crates/apr-cli/src/commands/eval/setfit.rs:584-618`

**Issue:**
`atomic_write` calls `refuse_existing_output(target, force)` (line 177), then writes and `sync_all`s the
temp file (`fill_and_sync`, lines 144-160), then `fs::rename(&temp, target)` (line 183). `fs::rename` on
Unix **replaces the destination unconditionally**. Anything that creates `target` during the temp write —
which for a ~90 MB artifact is a `write_all` plus an `fsync`, i.e. seconds — is silently destroyed even
though `--force` was not passed.

`refuse_existing_output`'s doc (lines 191-194) claims the opposite:

> *"the write-time check is what makes the guarantee true for a file that appeared while the run was going"*

It narrows the window; it does not close it. `write_lock` (`eval/setfit.rs:585-618`) has the identical
shape — `destination.exists() && !force` at 585, unconditional `fs::rename` at 618 — and the same
data-loss consequence, on a file the module itself calls "a COMMITMENT".

**Fix:** either close the window or stop claiming it is closed. To close it, take the destination as an
exclusive-create sentinel before the temp write and rename over your own file:

```rust
if !force {
    // O_CREAT|O_EXCL on the TARGET: the kernel adjudicates the race, not a stat.
    fs::OpenOptions::new().write(true).create_new(true).open(target)
        .map_err(|e| if e.kind() == std::io::ErrorKind::AlreadyExists {
            CliError::ValidationFailed(format!(
                "Refusing to replace existing file {} (pass --force to replace it)",
                target.display()))
        } else { CliError::Io(e) })?;
}
// ... temp write, then rename over the placeholder we now own
```

If that is judged too invasive, amend both doc comments to state that the guarantee is best-effort and
that `fs::rename` replaces unconditionally.

### WR-02: `write_lock`'s temp file has a predictable name, follows symlinks, and is not exclusive-create

**File:** `crates/apr-cli/src/commands/eval/setfit.rs:597-613`

**Issue:**

```rust
let temp = parent.join(format!(".{}.tmp", destination.file_name()...));
...
let mut file = fs::OpenOptions::new()
    .write(true)
    .create(true)
    .truncate(true)      // <- not create_new
    .open(&temp)
```

Three consequences, in increasing severity:

1. **Concurrent runs corrupt each other.** Two `apr eval --lock-out <same path>` processes derive the
   *same* temp name and both open it with `truncate(true)`. Their writes interleave; whichever renames
   last installs a lock file assembled from two records. `SelectionLock::from_canonical_bytes` would
   almost certainly reject it, but the corruption is silent at write time.
2. **A leftover temp from a crashed run is silently reused** rather than diagnosed.
3. **`create(true)` follows symlinks.** If `parent` is group- or world-writable (a shared scratch or
   results directory), an attacker who pre-creates `.selection-lock.json.tmp` as a symlink to any file
   the operator can write causes that file to be truncated and overwritten with the lock bytes. `O_EXCL`
   would refuse the existing path — including a dangling symlink — which is exactly why
   `setfit_train.rs:148-151` uses `create_new(true)`.

The in-repo precedent already solved all three: `setfit_train::temp_path` (lines 132-141) embeds
`std::process::id()` and a per-call `AtomicU64` ordinal, and `fill_and_sync` opens with `create_new(true)`
and documents why. This module did not adopt it.

**Fix:** reuse the precedent verbatim.

```rust
static LOCK_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
let ordinal = LOCK_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
let temp = parent.join(format!(".{stem}.tmp.{}.{ordinal}", std::process::id()));
let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
```

Better still: promote `setfit_train::atomic_write` to a shared helper so there is one implementation, per
the phase's own OPS-03 discipline.

### WR-03: `config_hash_of` silently collapses to `sha256("")` on serialization failure

**File:** `crates/apr-cli/src/commands/eval/setfit.rs:540-544`

**Issue:**

```rust
let bytes = serde_json::to_vec(requested).unwrap_or_default();
hex_of(&Sha256::digest(&bytes))
```

`unwrap_or_default()` on the *bytes* means a failed serialize yields an empty slice, and the config hash
becomes the constant `e3b0c442…b855` for **every** artifact that hits it. Every candidate in a sweep would
then be labelled with the same `config_hash` in the committed lock and in the `--json` report, and
`SelectionLock::from_candidates` does not refuse duplicate `config_hash` (only duplicate
`artifact_hash`, `lock.rs:269-280`) — so the lock commits and nothing notices.

The value is a *label*, not a gate (`lock.rs:83-86`), so this is not an authorization defect. It is a
silent fail-open on an identity value in a module whose entire thesis is that identity values are never
defaulted, and `serde_json::to_vec` over a `serde_json::Value` is effectively infallible anyway — so the
`unwrap_or_default` buys nothing and costs a correctness claim.

**Fix:**

```rust
fn config_hash_of(credential: &ReloadedSetFitCredential) -> Result<String> {
    let requested = &credential.model().doc_view().requested_config;
    let bytes = serde_json::to_vec(requested).map_err(|error| {
        CliError::Aprender(format!(
            "the artifact's requested_config sub-document did not serialize: {error}"
        ))
    })?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}
```

### WR-04: an arity mismatch is reported as `SelectionLabelOutOfRange`, which renders a diagnosis of something that did not happen

**File:** `crates/aprender-train/src/train/setfit/evaluate.rs:378-386`

**Issue:**

```rust
if truth.len() != predicted.len() {
    return Err(SetFitTrainError::SelectionLabelOutOfRange {
        label: predicted.len(),
        classes: truth.len(),
    });
}
```

That variant's `Display` (`mod.rs:1414-1418`) renders:

> `selection names class 480 but the dataset declares 512 classes (contract setfit-train-lifecycle-v1, requirement TRN-01)`

An operator who hits this is told the *selection's label map* is out of range and is sent to check
`--selection` against `--data`. Nothing about the selection is wrong; the classifier returned a different
number of results than there were rows. This is the exact failure mode `AprReloadError`'s own header calls
out (`apr_reload.rs:94-97`): *"a WRONG one, which is not an opaque diagnosis"*. It is also `PartialEq`-
matchable, so a caller can successfully match on a diagnosis nothing produced.

The comment concedes the arm is unreachable from both current callers — which is precisely why it is cheap
to fix and why nobody will notice the wrong message until it fires.

**Fix:** add a variant to the `#[non_exhaustive]` enum. `SetFitTrainError` is already non-exhaustive
(`mod.rs:1125`), so this is not a breaking change:

```rust
/// The prediction vector and the truth vector had different lengths.
PredictionArityMismatch { truth_rows: usize, predicted_rows: usize },
```

### WR-05: `apr inspect` shows no SetFit section for a tagged artifact whose `setfit` document is missing, while `apr predict` and `apr serve` route that same file to the SetFit path

**File:** `crates/apr-cli/src/commands/inspect.rs:575-581`, `crates/apr-cli/src/commands/inspect_setfit.rs:42-43`

**Issue:**
`inspect.rs` populates `setfit_doc` only when the tag *and* the custom key are both present:

```rust
let setfit_doc = if meta.model_type == crate::setfit_tag::SETFIT_MODEL_TYPE {
    meta.custom.get(crate::setfit_tag::SETFIT_CUSTOM_KEY).cloned()   // -> None if absent
} else { None };
```

and `build_setfit_inspection` early-returns on `let doc = doc?;` (line 43), so the entire APR-05 section
vanishes.

`setfit_tag::read_setfit_tag` is deliberately the opposite (`setfit_tag.rs:47-52, 144-146`):

> *"An artifact tagged `setfit` whose custom key is missing is still routed to the SetFit path — where the
> loader refuses it by name — rather than silently falling back to 'plain APR', which would report a
> corrupt classifier as a healthy generic model."*

So `apr predict` (`predict.rs:114-120`) routes it to `load_setfit_apr` and fails, `apr serve`
(`handlers.rs:766-769`) routes it to `start_setfit_server` and fails — and `apr inspect`, which CLAUDE.md
mandates as the diagnostic step before reading code, reports a healthy plain APR with no SetFit section
at all. The operator's mandated first tool contradicts the two commands that just failed.

This is exactly the outcome `inspect_setfit.rs:33-36` says it exists to avoid ("a future schema version
still shows its identity fields AND reports the parse failure, instead of showing nothing").

**Fix:** make `inspect` share the tag module's predicate. Change `MetadataInfo` to carry the tag decision
separately from the document, and have `build_setfit_inspection` emit the section with `null` fields plus
a note when the document is absent:

```rust
fn build_setfit_inspection(path: &Path, tagged: bool, doc: Option<&serde_json::Value>)
    -> Option<serde_json::Value>
{
    if !tagged { return None; }
    let doc = doc.cloned().unwrap_or(serde_json::Value::Null);
    // ... existing body; every copy_paths/pick_object already emits Null for an absent path
    // plus: section.insert("document_present", Value::Bool(!doc.is_null()));
}
```

### WR-06: `evaluate_test` scores an unknown predicted label as merely "wrong", and its `zip` cannot detect an over-long response

**File:** `crates/apr-cli/src/commands/eval/setfit.rs:499-512`

**Issue:** two gaps against the sibling implementation:

1. `let predicted = labels.iter().position(|label| label == result.label());` — if the classifier returns
   a label not in the artifact's own ordered set, `position` yields `None`, `None == Some(row.label)` is
   `false`, and the row is counted **incorrect**. If it happened for every row the command reports
   `accuracy = 0.000000` as a real measurement. `apr_evaluate.rs:251-259` refuses this with
   `UnknownPredictedLabel` and documents why: *"the alternative to a typed refusal is an `expect` in the
   one place a silent mis-mapping would turn every metric below into a confidently wrong number."*
2. `for (row, result) in chunk.iter().zip(response.results())` — `zip` stops at the shorter side, and
   `seen` counts only the pairs it produced. So `seen` always equals `rows.len()` when the response is
   *longer* than the chunk, and the guard at 507-512 cannot fire. `apr_evaluate.rs:245-275` pushes every
   result and then compares totals, which catches both directions.

**Fix:** mirror the sibling — refuse an unknown label, and count results independently of the zip:

```rust
let mut returned = 0_usize;
for chunk in rows.chunks(MAX_BATCH_TEXTS) {
    let response = credential.model().classify(&request)...;
    returned += response.results().len();
    for (row, result) in chunk.iter().zip(response.results()) {
        let predicted = labels.iter().position(|l| l == result.label()).ok_or_else(|| {
            CliError::InferenceFailed(format!(
                "the classifier returned the label `{}`, which is not in the artifact's own \
                 ordered label set", result.label()))
        })?;
        if predicted == row.label { correct += 1; }
        seen += 1;
    }
}
if returned != rows.len() { /* refuse, naming both counts */ }
```

---

## Info

### IN-01: `config_hash_of`'s canonicality argument is measurably wrong for this build

**File:** `crates/apr-cli/src/commands/eval/setfit.rs:534-539`

The doc states the hash is canonical *"because … `serde_json::Map` preserves object order as read."*
That is only true with the `preserve_order` feature, which is **not** enabled anywhere (`Cargo.toml:138`,
`crates/apr-cli/Cargo.toml:177` — plain `serde_json = "1.0"`). Without it, `Map` is a `BTreeMap` and sorts
keys, so it does *not* preserve order as read.

The outcome is still deterministic (sorted keys), so nothing is broken today. But `CONFIG_HASH_DERIVATION`
is published in the `--json` report as the recipe Phase 5 reproduces, and the stated recipe is not the one
the code runs. It is also a feature-unification hazard: any crate anywhere in the graph enabling
`preserve_order` would silently change every `config_hash` in every lock. Restate the derivation as
"sorted-key JSON as produced by `serde_json` without `preserve_order`", or pin it by serializing through
an explicitly ordered form.

### IN-02: two of the credential's three values are write-only in the lock chain

**Files:** `crates/aprender-train/src/train/setfit/lock.rs:594-617` and `796-816`;
`crates/aprender-train/src/train/setfit/credential.rs:84-99`

`SetFitCredential` exposes three values. `create_selection_lock` (`lock.rs:698-710`) reads all three and
*records* the latter two into the lock. `mint_test_token` re-checks only `artifact_hash` (line 605), and
`grant` re-checks `artifact_hash` plus the dataset fingerprint (lines 802, 809). Neither ever compares the
lock's recorded `selection_semantic_hash` / `ledger_hash` against the credential presenting itself.

Combined with `from_canonical_bytes`'s honest admission that a hand-written, internally consistent lock
passes its own integrity check (`lock.rs:419-429`), this means a lock's *recorded selection provenance*
can claim a selection the named artifact was never trained under, and no later door notices. The artifact
and corpus identity gates still hold, and `run_test`'s report does not surface the lock's selection
hashes, so nothing wrong is reported today — hence Info rather than Warning.

Cheap hardening: `mint_test_token` already holds the credential, so adding the two equalities is three
lines and makes the credential's surface exactly what the doors consume, which is the property
`credential.rs:74-77` claims for it.

### IN-03: a third hex encoder

**File:** `crates/apr-cli/src/commands/eval/setfit.rs:547-553`

`hex_of` is a hand-rolled lowercase-hex encoder alongside the `hex` crate (used throughout
`aprender-train`) and `aprender_contrastive_data::hash::hex` (used at `setfit_train.rs:52, 366-367`).
Three implementations of one operation is what OPS-03 exists to prevent, and this one allocates a
`String` per byte via `format!`. Replace with `hex::encode`.

### IN-04: `read_selection_manifest` was widened to `pub(crate)` and still reads unbounded

**File:** `crates/apr-cli/src/commands/data_contrastive.rs:610-611`

```rust
pub(crate) fn read_selection_manifest(path: &Path) -> Result<SelectionManifest> {
    let bytes = fs::read(path).map_err(...)?;
```

Every other reader this phase touched applies a stat-then-stream bound: the artifact door
(`setfit_io.rs:67-91`), the request document (`predict.rs:236-271`), the lock file
(`eval/setfit.rs:623-660`), the tag metadata (`setfit_tag.rs:111-133`), inspect's metadata block
(`inspect.rs:548-563`). This one takes an operator-supplied path and reads it whole. The visibility change
does not add a new external entry point (`apr data select` already reached it), so no new attack surface —
but it is now the only unbounded read on the `apr setfit train` and `apr eval` ingest paths, and it is the
one that will look like an oversight to the next reader.

### IN-05: `RetainedArtifactBytes`'s `Debug` mitigation is partial at the level it claims

**File:** `crates/aprender-train/src/train/setfit/mod.rs:388-410, 526-534`

The newtype's doc says retaining the bytes *"must not turn a debug print into a denial of service"*, and
scopes the claim to `{:?}` on any `SetFitRun`. But `SetFitRun` also derives `Debug` and holds
`encoder: SetFitMiniLm` and `dataset: PreparedDataset<Canonical>` — so `{:?}` on a run already renders
every encoder tensor and the whole corpus, orders of magnitude past the 1.8 MB the newtype removes. The
newtype is correct and worth keeping; the doc's claim about `{:?}` on a `SetFitRun` is not achieved by it
alone. Either narrow the claim to `ArtifactVerifiedEvidence` (where it *is* true) or give `SetFitRun` a
hand-written `Debug` too.

---

_Reviewed: 2026-08-15_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
_Diff base: 87b780eefabb4dbb6f5e64efe2f5b6698d058e73..HEAD_
