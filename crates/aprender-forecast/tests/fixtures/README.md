# Oracle fixtures for `aprender-forecast`

These 17 files are the phase's **correctness bar**. The parity ladder does not check that
the Rust port is self-consistent; it checks that the port reproduces a specific number
produced by a specific version of a specific Python library on a specific dataset. That is
what makes a green ladder evidence.

**They are committed on purpose, and their absence is a defect.** `test_support::load_json`
`expect`s — no test in this crate skips because a fixture is missing. A skipped parity test
is a parity test that proves nothing (CLAUDE.md Verification Discipline #5).

Every file here is **byte-identical** to its original under `.planning/spikes/NNN-*/fixtures/`;
`06-01` Task 2's `<verify>` re-establishes that with `cmp` on every run, so a drifted copy
turns the plan red rather than quietly changing a tolerance. Where a file appears in more
than one spike (`peyton_manning.csv` in 7, `air_passengers.csv` in 6, `wp_log_R.csv` in 2)
the copies were verified identical to each other before ONE was taken.

## Provenance

| File | Spike | Oracle environment | Regenerate with |
|---|---|---|---|
| `peyton_manning_prophet140.json` | `001-prophet-map-fit-lbfgs` | Python Prophet 1.4.0 | `.planning/spikes/001-prophet-map-fit-lbfgs/tools/` |
| `air_passengers_prophet140.json` | `001-prophet-map-fit-lbfgs` | Python Prophet 1.4.0 | `.planning/spikes/001-prophet-map-fit-lbfgs/tools/` |
| `retail_sales_prophet140.json` | `001-prophet-map-fit-lbfgs` | Python Prophet 1.4.0 | `.planning/spikes/001-prophet-map-fit-lbfgs/tools/` |
| `np_oracle_peyton.json` | `002-neuralprophet-autograd` | NeuralProphet 0.9.0 | `.planning/spikes/002-neuralprophet-autograd/tools/` |
| `peyton_default_prophet140.json` | `003-prophet-intervals-and-components` | Python Prophet 1.4.0 | `.planning/spikes/003-prophet-intervals-and-components/tools/` |
| `peyton_holidays_prophet140.json` | `003-prophet-intervals-and-components` | Python Prophet 1.4.0 | `.planning/spikes/003-prophet-intervals-and-components/tools/` |
| `wp_log_R_logistic_prophet140.json` | `003-prophet-intervals-and-components` | Python Prophet 1.4.0 | `.planning/spikes/003-prophet-intervals-and-components/tools/` |
| `air_multiplicative_prophet140.json` | `003-prophet-intervals-and-components` | Python Prophet 1.4.0 | `.planning/spikes/003-prophet-intervals-and-components/tools/` |
| `chronos_bolt_tiny_fixture.json` | `005-chronos-bolt-tiny-parity` | chronos-forecasting 2.3.1 | `.planning/spikes/005-chronos-bolt-tiny-parity/tools/` |
| `chronos_probes.json` | `005-chronos-bolt-tiny-parity` | chronos-forecasting 2.3.1 | `.planning/spikes/005-chronos-bolt-tiny-parity/tools/` |
| `weights_index.json` | `005-chronos-bolt-tiny-parity` | chronos-forecasting 2.3.1 | `.planning/spikes/005-chronos-bolt-tiny-parity/tools/` |
| `chronos_holdout_oracle.json` | `006-chronos-vs-prophet-holdout` | chronos-forecasting 2.3.1 | `.planning/spikes/006-chronos-vs-prophet-holdout/tools/` |
| `peyton_tiny_oracle.json` | `007-chronos-mcp-thin-server` | chronos-forecasting 2.3.1 | `.planning/spikes/007-chronos-mcp-thin-server/tools/` |
| `peyton_manning.csv` | `002` (7 identical copies across 001-007) | raw dataset (Prophet's example data) | — |
| `air_passengers.csv` | `004` (6 identical copies) | raw dataset | — |
| `wp_log_R.csv` | `003` (2 identical copies) | raw dataset (Prophet's logistic example) | — |
| `retail_sales.csv` | `006` | raw dataset | — |

## Gotchas the readers must handle (`src/test_support.rs`)

- `wp_log_R.csv` is **not chronological** — sort and de-duplicate by `ds` before use.
- Two of the four CSVs (`peyton_manning`, `wp_log_R`) carry `"`-quoted fields, two
  (`air_passengers`, `retail_sales`) do not. All four are LF-terminated as committed
  (measured 2026-09-05); the reader strips a trailing `\r` anyway so a CRLF checkout on
  another platform cannot silently change a parsed value.
- Prophet's objective is stored at `log_posterior_at_map_unnormalized` as a **log
  posterior** (positive), so the Rust objective (a negative log posterior) is compared as
  `f_rust + log_posterior_at_map_unnormalized`, and the fit's `1/T` scale must be undone
  first.

## Which plan consumes what

| Fixture family | Plan |
|---|---|
| `peyton_manning_prophet140` (rung 2) | `06-01` (this plan's one ladder rung) |
| the four remaining `*_prophet140` | `06-03` (the seven-fixture ladder) |
| `np_oracle_peyton` | `06-04` (NeuralProphet-lite) |
| `chronos_*`, `weights_index`, `peyton_tiny_oracle` | `06-05` / `06-07` (Chronos) |
