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
| terminal write | the finalizer bin (plan 14-05): `FinalizerCtx`, `handle`, `reconcile` over the `ObjectStore`, `ExecutionReader`, `TrainingJobs` and `PackTasks` seams |
| links | presigned GETs minted by `train_status` on read, only for keys inside the run's own prefix |
| cancel | `SfnCancelSink`: a best-effort `StopExecution` |

## Two binaries

`bootstrap` is the **request function**: it validates the dataset, mints an MCP
task, takes the single-flight lock, starts one Step Functions execution and
answers `working`. It trains nothing. AWS Lambda's Custom Runtime requires the
handler binary to be named `bootstrap`, and `cargo pmcp deploy` finds the
deployable by that name, so only the request function carries it.

`aprender-laya-train-finalizer` (plan 14-05, renamed `bootstrap` by the CDK asset
build) is the ONE writer of a run's terminal record, `verdict.json`,
`manifest.json`, retention tags and lock release. It accepts exactly three
inputs and refuses anything else:

| Input | From |
|-------|------|
| `{"source":"state-machine","execution_arn","start_time","input",["error"]}` | the state machine's `Finalize` state (normal end and `Catch` end) |
| an EventBridge `Step Functions Execution Status Change` event, `ABORTED \| TIMED_OUT \| FAILED` | a rule on the one configured state machine (best effort, may repeat) |
| `{"source":"reconcile"}` | a schedule, every 10 minutes |

One finalization is ordered: compute is terminal (the SageMaker job and every ECS
pack task of the run), then the DynamoDB record (the arbiter, first writer wins),
`verdict.json`, `manifest.json`, the retention tags read back, and the lock
release LAST. A failure at any step after the record returns `Err` with the lock
held; the next event or the sweep resumes from the stored record. While compute is
still live the finalizer stops it (`StopTrainingJob`, ECS `StopTask`), writes
nothing and keeps the lock. The sweep also re-issues a lost `StopExecution` for a
cancelled run, resolves a dispatch that never confirmed after 900 s, and stops any
RUNNING execution that is not the lock holder's.

It reads `APRENDER_LAYA_TRAIN_TABLE`, `_BUCKET`, `_STATE_MACHINE_ARN`,
`_CLUSTER_ARN` and `_PACK_FAMILY`, and refuses to start when one is unset. Its role
needs: S3 `Get/Put/PutObjectTagging/GetObjectTagging/ListBucket` on `runs/*`;
`states:DescribeExecution`/`StopExecution` on the machine's executions and
`states:ListExecutions` on the machine; `sagemaker:DescribeTrainingJob`/
`StopTrainingJob` on `training-job/laya-*`; `ecs:ListTasks`/`DescribeTasks`/
`StopTask` conditioned on the cluster; the DynamoDB item actions on the table.

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

Every test is offline. Fakes stand in for S3 (which can fail the n-th call of an
operation), Step Functions, SageMaker and ECS; the one test that uses a real SDK
client points it at a closed loopback port. The finalizer's tests (`fin_*`) run the
real store, lock and decision over the in-memory backend.
