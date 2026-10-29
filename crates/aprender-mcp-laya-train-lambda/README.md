# aprender-mcp-laya-train-lambda

The AWS deployment unit of the Laya training MCP server: the request function and
the AWS ends of the seams `aprender-mcp-laya-train` leaves open.

Part of the [aprender monorepo](https://github.com/paiml/aprender).

## What it is

`aprender-mcp-laya-train` defines the tools (`validate_dataset`, `train`,
`train_status`), the single-flight lease and the finalize decision, and hides
everything that touches AWS behind seams. This crate fills them and adds nothing
else:

| Seam | Here |
|------|------|
| `TaskBackend` | `DynamoDbTaskBackend` (from `aprender-setfit-train-lambda`, bound to this server's table) |
| `Dispatcher` | `SfnDispatcher`: one Step Functions execution per run, named `laya-<task_id>` |
| `DatasetSource` | `S3DatasetSource` over the server bucket's `datasets/` prefix, with size caps |
| terminal write | the finalizer (plan 14-05), not this crate's request function |
| links | presigned GETs minted by `train_status` on read, only for keys inside the run's own prefix |
| cancel | `SfnCancelSink`: a best-effort `StopExecution` |

## Two binaries

`bootstrap` is the **request function**: it validates the dataset, mints an MCP
task, takes the single-flight lock, starts one Step Functions execution and
answers `working`. It trains nothing. AWS Lambda's Custom Runtime requires the
handler binary to be named `bootstrap`, and `cargo pmcp deploy` finds the
deployable by that name, so only the request function carries it.

`aprender-laya-train-finalizer` (plan 14-05, renamed `bootstrap` by the CDK asset
build) writes the terminal result and reconciles stuck runs.

## No AWS permission of its own

The request function holds no IAM until the post-401 grant (D-10, plans 14-10 and
14-13). It reads `APRENDER_LAYA_TRAIN_TABLE`, `APRENDER_LAYA_TRAIN_BUCKET` and
`APRENDER_LAYA_TRAIN_STATE_MACHINE_ARN` and refuses to start when one is unset.

## Cancel is best effort

`tasks/cancel` writes the durable `Cancelled` record first and then fires the
cancel sink, which calls `StopExecution` with a bounded wait. If that call is
lost, nothing is lost: the lock stays held until the compute is terminal and the
finalizer's reconcile sweep re-issues the stop every sweep.

## Testing

Every test is offline. Fakes stand in for S3 and Step Functions; the one test
that uses a real SDK client points it at a closed loopback port.
