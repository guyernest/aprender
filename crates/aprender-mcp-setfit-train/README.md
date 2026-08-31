# aprender-mcp-setfit-train

Part of the [Aprender monorepo](https://github.com/paiml/aprender).

A **thin, single-algorithm MCP training server**: SetFit few-shot training
exposed as an async **MCP Task** (spec 2025-11-25), built on
[pmcp](https://github.com/paiml/rust-mcp-sdk). The training sibling of
`aprender-mcp-setfit` (predict): same thin philosophy, different lifecycle —
training and prediction are separate MCP endpoints because they have different
users at different times.

## Architecture

- **Training goes through the CLI door.** The server supervises a pinned
  `apr setfit train` child process — the CLI adapter is training's one
  validated door (attested ingest, file-first config, atomic artifact write),
  and its ingest sequence is deliberately private to apr-cli so no second
  reader can drift from it. The predict sibling is in-process because
  predict's one door is a public library API; each operation goes through its
  own door. The same child-process job shape runs unchanged in a Lambda
  container or as a SageMaker container entrypoint later.
- **Async as MCP Tasks.** `train` is a task-supported tool: a task-augmented
  call returns a store-minted task id; the client polls `tasks/get` and reads
  the terminal result from `tasks/result`. Clients without tasks support use
  the `train_status` polling tool — both roads serve the same status payload.
- **One job at a time.** A second submit while one trains is refused, naming
  the running job. This is a RESOURCE policy and nothing else: training
  saturates the CPU and the deploy target is one container per job. It does
  **not** make the task↔job pairing unambiguous — that reasoning was tried and
  is wrong (see the crate doc in `src/lib.rs`: single-flight bounds "one job
  RUNNING", never "one job unbound in history"). Pairing is established by
  observing `TaskStore::create`, inline in the spawned waiter; there is no
  mirror worker.
- **Fail on the request first.** Submit runs the CLI's `--dry-run` pre-flight
  synchronously, so a bad config/selection is a tool error in seconds, not a
  failed job minutes later.

## Tools

| Tool | Arguments | Returns |
|------|-----------|---------|
| `train` | `config: object` — the full twelve-knob SetFit training config, passed verbatim to `apr setfit train --config` | Task-augmented: an MCP task handle (poll `tasks/get`). Plain call: a `working` envelope carrying `job_id`. |
| `train_status` | `job_id?: string` (default: latest) | `{schema_version, job_id, phase, started/finished_unix_ms, artifact_path, report, error}` — `report` is the trainer's own `--json` report (artifact sha256, provenance, resolved config). |

## Run locally (stdio)

```bash
. scripts/apr_bin.sh || exit 1   # pin the setfit-featured apr
cargo run -p aprender-mcp-setfit-train -- \
  --apr-bin "$APR" \
  --data data/tweet-eval-stance \
  --selection data/tweet-eval-stance/selection-manifest.json \
  --model-dir ~/.cache/aprender/minilm-l6-v2-1110a243 \
  --output-dir /tmp/setfit-train-out
```

Every flag also reads `APRENDER_SETFIT_TRAIN_<FLAG>` from the environment.
The operator provisions the dataset, selection, encoder checkout and output
directory; the client varies only the training config. Measured envelope for
the 8-shot reference recipe: **127 s wall, 4.0 GB peak RSS** (M-series CPU).

## Tests

```bash
cargo test -p aprender-mcp-setfit-train -- --nocapture      # unit + E2E; --nocapture is
                                                            # what makes the SKIP line
                                                            # VISIBLE (libtest swallows a
                                                            # passing test's stdout)
APR_MCP_E2E_SETFIT_TRAIN_BIN="$APR" \
  cargo test -p aprender-mcp-setfit-train --test e2e_stdio  # armed: a REAL training run
                                                            # as an MCP task over stdio
```
