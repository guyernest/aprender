# aprender-mcp-laya-train

Part of the [aprender monorepo](https://github.com/paiml/aprender).

The library behind the **Laya training MCP server**: three tools
(`validate_dataset`, `train`, `train_status`), MCP Tasks for the long-running
`train` call, a single-flight lease so one job runs at a time, the one status
payload every surface serves, and the pure finalize decision that maps a
finished run's files onto a phase (`completed`, `gate_failed`, `failed`,
`cancelled`). It is transport and bookkeeping only: it never recomputes a
metric, a gate clause or a probability, and every verdict field is copied from
`gate-report.json` and `pack-outcome.json` (D-05).

It builds on `aprender-mcp-setfit-train` for the task store, the in-memory
backend and the cancel sink, and on `aprender-decide` for the `task.json`
parser, so there is one implementation of each.

| Seam | What it hides | Local implementation (here) | AWS implementation |
|------|---------------|-----------------------------|--------------------|
| `TaskBackend` | where task and lock records persist | `InMemoryTaskBackend` | DynamoDB, in `aprender-mcp-laya-train-lambda` (plan 14-04) |
| `Dispatcher` | how a run is started, confirmed and linked | a recording fake in the tests | Step Functions and S3 presigning, in `aprender-mcp-laya-train-lambda` |
| `DatasetSource` | where the dataset files are read from | `LocalDatasetSource` (a directory) | S3, in `aprender-mcp-laya-train-lambda` |

This crate has no binary by design (D-22): the request function, the finalizer
and both containers are separate deployment units that depend on it.
