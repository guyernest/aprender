import * as cdk from 'aws-cdk-lib';
import * as dynamodb from 'aws-cdk-lib/aws-dynamodb';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as ssm from 'aws-cdk-lib/aws-ssm';
import { Construct } from 'constructs';

/**
 * Everything the SetFit training MCP server needs that the pmcp.run platform
 * does not provide.
 *
 * The platform owns the MCP REQUEST Lambda (deployed by `cargo pmcp deploy`
 * from `.pmcp/deploy-train.toml`). That function is deliberately small: it
 * mints a task, writes the envelope, and asynchronously invokes the worker
 * defined here. It carries no `apr` binary, no dataset and no encoder — the
 * pmcp.run package ceiling is 250 MB and the platform's own deployment backend
 * has already OOM-killed one oversized bootstrap.
 *
 * This stack owns the parts with a different shape: durable task state, the
 * artifact bucket, and the heavy worker (6 GB, 15 minutes) that actually runs
 * `apr setfit train`. Two functions because their configurations are
 * irreconcilable, not because the code wants splitting.
 *
 * # Why a separate CDK app rather than edits to `deploy/`
 *
 * `deploy/lib/stack.ts` is rendered by cargo-pmcp. Hand edits flip it to
 * `cdk synth`, whose hashed logical IDs read as resource replacements and
 * collide with the platform's fixed names (that is what produced an
 * UPDATE_ROLLBACK_COMPLETE on the predict server). Extensions live in their own
 * app — the same separation chess-mcp uses.
 */
export interface SetFitTrainingStackProps extends cdk.StackProps {
  /**
   * Deployment environment. Every resource name carries it, so dev and prod are
   * addressable by convention and the request Lambda can be handed their names
   * without cross-stack references or a lookup at runtime.
   */
  readonly environment: string;

  /**
   * Directory holding the worker's Lambda package: the `bootstrap` binary plus
   * `assets/` (the aarch64 `apr`, the attested dataset, the pinned encoder).
   * Produced by `just build-trainer-asset`; absent until that has run, which is
   * why `cdk synth` fails loudly rather than deploying an empty function.
   */
  readonly trainerAssetPath: string;
}

export class SetFitTrainingStack extends cdk.Stack {
  public readonly tasksTable: dynamodb.Table;
  public readonly artifactBucket: s3.Bucket;
  public readonly trainer: lambda.Function;

  constructor(scope: Construct, id: string, props: SetFitTrainingStackProps) {
    super(scope, id, props);

    const env = props.environment;
    // Retain data in prod, destroy it in dev: a dev redeploy that leaves an
    // orphaned table behind is a slow-growing bill nobody reads.
    const isProd = env === 'prod';
    const removalPolicy = isProd
      ? cdk.RemovalPolicy.RETAIN
      : cdk.RemovalPolicy.DESTROY;

    // ---------------------------------------------------------------------
    // Task state
    // ---------------------------------------------------------------------
    // Keyed (owner_id, task_id) because that is exactly the scope the
    // TaskStore trait enforces: every read is owner-scoped, and a task
    // belonging to another owner must read as NotFound rather than as a
    // permission error that confirms it exists.
    this.tasksTable = new dynamodb.Table(this, 'TrainingTasksTable', {
      tableName: `aprender-setfit-training-tasks-${env}`,
      partitionKey: { name: 'owner_id', type: dynamodb.AttributeType.STRING },
      sortKey: { name: 'task_id', type: dynamodb.AttributeType.STRING },
      billingMode: dynamodb.BillingMode.PAY_PER_REQUEST,
      // The store writes an absolute epoch-seconds expiry; DynamoDB reclaims on
      // its own schedule. That deletion is asynchronous, which is why the store
      // ALSO filters expired records on read rather than trusting TTL alone.
      timeToLiveAttribute: 'expires_at',
      pointInTimeRecoverySpecification: { pointInTimeRecoveryEnabled: isProd },
      removalPolicy,
    });

    // ---------------------------------------------------------------------
    // Artifacts
    // ---------------------------------------------------------------------
    this.artifactBucket = new s3.Bucket(this, 'ArtifactBucket', {
      bucketName: `aprender-setfit-artifacts-${env}-${this.account}`,
      encryption: s3.BucketEncryption.S3_MANAGED,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      enforceSSL: true,
      versioned: isProd,
      removalPolicy,
      autoDeleteObjects: !isProd,
      lifecycleRules: [
        {
          // A trained artifact is ~87 MB and a superseded one has no readers:
          // the task that names it has a 1-hour TTL. Keeping them forever is
          // how an object store quietly becomes the largest line on the bill.
          id: 'expire-training-artifacts',
          expiration: cdk.Duration.days(isProd ? 90 : 7),
        },
      ],
    });

    // ---------------------------------------------------------------------
    // The worker
    // ---------------------------------------------------------------------
    const trainerLogs = new logs.LogGroup(this, 'TrainerLogGroup', {
      logGroupName: `/aws/lambda/aprender-setfit-trainer-${env}`,
      retention: isProd
        ? logs.RetentionDays.THREE_MONTHS
        : logs.RetentionDays.ONE_WEEK,
      removalPolicy,
    });

    this.trainer = new lambda.Function(this, 'TrainerFunction', {
      functionName: `aprender-setfit-trainer-${env}`,
      // PROVIDED_AL2023 + arm64: the worker is a Rust custom runtime, and the
      // `apr` it spawns is cross-compiled aarch64-unknown-linux-gnu. A mismatch
      // here is an exec-format error at the first submit, not at deploy.
      runtime: lambda.Runtime.PROVIDED_AL2023,
      architecture: lambda.Architecture.ARM_64,
      handler: 'bootstrap',
      code: lambda.Code.fromAsset(props.trainerAssetPath),
      // MEASURED, not guessed: the 8-shot reference train peaks at 4.0 GB RSS
      // and runs 127 s wall on an M-series CPU. 6 GB leaves headroom for a
      // slower core and a larger shot count; 900 s is Lambda's ceiling and
      // ~7x the measured run.
      memorySize: 6144,
      timeout: cdk.Duration.seconds(900),
      // The trained artifact is ~87 MB and is written to /tmp before upload,
      // alongside the config. 512 MB (the default) would fit today and leaves
      // nothing for a larger model.
      ephemeralStorageSize: cdk.Size.mebibytes(2048),
      logGroup: trainerLogs,
      environment: {
        APRENDER_SETFIT_TASKS_TABLE: this.tasksTable.tableName,
        APRENDER_SETFIT_ARTIFACT_BUCKET: this.artifactBucket.bucketName,
        // Paths inside the package, resolved against $LAMBDA_TASK_ROOT by the
        // worker. Named here so the layout is declared in ONE place rather than
        // agreed by convention between the build script and the Rust.
        APRENDER_SETFIT_TRAIN_APR_BIN: '/var/task/assets/apr',
        APRENDER_SETFIT_TRAIN_DATA: '/var/task/assets/data',
        APRENDER_SETFIT_TRAIN_SELECTION:
          '/var/task/assets/data/selection-manifest.json',
        APRENDER_SETFIT_TRAIN_MODEL_DIR: '/var/task/assets/encoder',
        APRENDER_SETFIT_TRAIN_OUTPUT_DIR: '/tmp/setfit-out',
        RUST_LOG: 'info',
      },
      // A retry re-runs a 127-second CPU-saturating job. The terminal write is
      // guarded against a second writer, so a retry cannot corrupt state — but
      // it also cannot help, because a failure here is a bad config or a broken
      // package, not a transient. Failures are visible as a `failed` task.
      retryAttempts: 0,
    });

    this.tasksTable.grantReadWriteData(this.trainer);
    this.artifactBucket.grantWrite(this.trainer);

    // ---------------------------------------------------------------------
    // What the request Lambda needs to know
    // ---------------------------------------------------------------------
    // Published to SSM as well as CfnOutput: the request Lambda is deployed by
    // cargo-pmcp from a different app, so it cannot take a cross-stack
    // reference. SSM is the seam, and the names are predictable per
    // environment so `.pmcp/deploy-train.toml` can name them directly.
    const params: Record<string, string> = {
      'tasks-table': this.tasksTable.tableName,
      'artifact-bucket': this.artifactBucket.bucketName,
      'trainer-function-arn': this.trainer.functionArn,
      'trainer-function-name': this.trainer.functionName,
    };
    for (const [key, value] of Object.entries(params)) {
      new ssm.StringParameter(this, `Param-${key}`, {
        parameterName: `/aprender/setfit-train/${env}/${key}`,
        stringValue: value,
      });
      new cdk.CfnOutput(this, `Out-${key}`, { value, exportName: `aprender-setfit-train-${env}-${key}` });
    }

    // The policy the request Lambda's execution role needs. Published rather
    // than attached: that role belongs to the platform-owned stack, so this
    // stack states the requirement and the operator binds it, instead of
    // reaching into a stack it does not own.
    new cdk.CfnOutput(this, 'RequestLambdaPolicy', {
      description:
        'Attach to the pmcp.run request Lambda role: DynamoDB RW on the tasks table + lambda:InvokeFunction on the trainer',
      value: JSON.stringify({
        Version: '2012-10-17',
        Statement: [
          {
            Effect: 'Allow',
            Action: [
              'dynamodb:GetItem',
              'dynamodb:PutItem',
              'dynamodb:UpdateItem',
              'dynamodb:DeleteItem',
              'dynamodb:Query',
            ],
            Resource: this.tasksTable.tableArn,
          },
          {
            Effect: 'Allow',
            Action: ['lambda:InvokeFunction'],
            Resource: this.trainer.functionArn,
          },
        ],
      }),
    });
  }
}
