import * as cdk from 'aws-cdk-lib';
import * as cloudwatch from 'aws-cdk-lib/aws-cloudwatch';
import * as dynamodb from 'aws-cdk-lib/aws-dynamodb';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as ecs from 'aws-cdk-lib/aws-ecs';
import * as events from 'aws-cdk-lib/aws-events';
import * as targets from 'aws-cdk-lib/aws-events-targets';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as sfn from 'aws-cdk-lib/aws-stepfunctions';
import * as tasks from 'aws-cdk-lib/aws-stepfunctions-tasks';
import * as sqs from 'aws-cdk-lib/aws-sqs';
import { Construct } from 'constructs';

/**
 * Everything the Laya training MCP server needs that the pmcp.run platform does not provide
 * (Phase 14, plan 14-08): the run bucket, the task table, the Train -> PackVerify -> Finalize
 * state machine, the arm64 Fargate pack task, the finalizer and the three things that drive it.
 *
 * Why a separate CDK app: `deploy/lib/stack.ts` is rendered by cargo-pmcp and must not be
 * edited by hand (see setfit-training-stack.ts). The request function is deployed by
 * cargo-pmcp and gets NOTHING from this stack at deploy time: the policy it will need is only
 * an OUTPUT (RequestLambdaPolicy), attached by the owner after the 401 check (D-10). Until it
 * is attached every `train` call fails with AccessDenied and nothing can be spent.
 *
 * Every spend and access limit is a literal in this file and therefore in the template:
 * the instance type, count and runtime of the training job, the digest-pinned images, the
 * arm64 task size, the 36,000 s machine timeout. They are never read from the execution input
 * (T-14-31); `laya_trainer_stack_check.py` asserts each one from the synthesized template.
 */
export interface LayaTrainerStackProps extends cdk.StackProps {
  /** `dev` or `prod`; every resource name carries it. */
  readonly environment: string;
  /** Directory holding the finalizer's Lambda package (a `bootstrap` binary). */
  readonly finalizerAssetPath: string;
  /** `sha256:<64 hex>` of the pushed training image (x86_64, CUDA). */
  readonly trainImageDigest: string;
  /** `sha256:<64 hex>` of the pushed pack image (arm64). */
  readonly packImageDigest: string;
}

/** The training instance. A literal: a caller can never choose a bigger machine. */
export const TRAIN_INSTANCE_TYPE = 'ml.g5.xlarge';
export const TRAIN_INSTANCE_COUNT = 1;
/** MaxRuntimeInSeconds of the training job (layout::MAX_RUNTIME_SECS). */
export const TRAIN_MAX_RUNTIME_SECS = 21_600;
/** The whole machine's ceiling: train (6 h) + pack and verify (3 h) + slack. */
export const MACHINE_TIMEOUT_SECS = 36_000;
/** The pinned base-model snapshot's key prefix (layout::BASE_KEY's directory). */
export const BASE_PREFIX = 'base/laya-55cf4c4e/';
/** The retention tag every run_step upload carries (layout::RETENTION_TAG_KEY). */
export const RETENTION_TAG_KEY = 'laya-retention';

/**
 * A SageMaker channel location that carries no IAM grant of its own. The CDK's own
 * `S3Location.fromBucket` / `fromJsonExpression` add S3 grants (`s3:GetBucket*`, `s3:List*`, or
 * `*` resources) to the training role; this plan states the role's S3 rights exactly, once,
 * below, so a location here only supplies the URI.
 */
class LiteralLocation extends tasks.S3Location {
  constructor(private readonly uri: string) {
    super();
  }
  bind(): tasks.S3LocationConfig {
    return { uri: this.uri };
  }
}

export class LayaTrainerStack extends cdk.Stack {
  public readonly bucket: s3.Bucket;
  public readonly table: dynamodb.Table;
  public readonly finalizer: lambda.Function;
  public readonly machine: sfn.StateMachine;
  public readonly sagemakerRole: iam.Role;

  constructor(scope: Construct, id: string, props: LayaTrainerStackProps) {
    super(scope, id, props);

    const env = props.environment;
    const isProd = env === 'prod';
    const tableRemoval = isProd ? cdk.RemovalPolicy.RETAIN : cdk.RemovalPolicy.DESTROY;
    const logRetention = isProd ? logs.RetentionDays.THREE_MONTHS : logs.RetentionDays.ONE_MONTH;

    const machineName = `aprender-laya-train-${env}`;
    const trainRepoName = `aprender-laya-train-${env}`;
    const packRepoName = `aprender-laya-pack-${env}`;
    const packFamily = `aprender-laya-pack-${env}`;
    const trainRepo = ecr.Repository.fromRepositoryName(this, 'TrainRepo', trainRepoName);
    const packRepo = ecr.Repository.fromRepositoryName(this, 'PackRepo', packRepoName);
    const trainImage = trainRepo.repositoryUriForDigest(props.trainImageDigest);
    const packImage = packRepo.repositoryUriForDigest(props.packImageDigest);

    // The machine's and its executions' ARNs, derived from the fixed name rather than from the
    // machine construct: the finalizer is the machine's Finalize target AND an env-var holder of
    // the machine's ARN, which would be a circular dependency through the resource.
    const machineArn = this.formatArn({
      service: 'states',
      resource: 'stateMachine',
      resourceName: machineName,
      arnFormat: cdk.ArnFormat.COLON_RESOURCE_NAME,
    });
    const executionArns = this.formatArn({
      service: 'states',
      resource: 'execution',
      resourceName: `${machineName}:*`,
      arnFormat: cdk.ArnFormat.COLON_RESOURCE_NAME,
    });

    // ---------------------------------------------------------------------
    // Bucket (D-19): one place for datasets, the pinned base, every run and SageMaker's output
    // ---------------------------------------------------------------------
    this.bucket = new s3.Bucket(this, 'RunBucket', {
      bucketName: `aprender-laya-train-${env}-${this.account}`,
      encryption: s3.BucketEncryption.S3_MANAGED,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      enforceSSL: true,
      // RETAIN in every env: datasets are immutable inputs and a kept run is the product.
      removalPolicy: cdk.RemovalPolicy.RETAIN,
      lifecycleRules: [
        {
          // Only a run the finalizer tagged `expire` (a failed, cancelled, gate-failed or
          // waived run) ever ages out. A `keep` run carries a different tag value and
          // matches nothing. Nothing here touches datasets/ or base/ (T-14-33).
          id: 'expire-unkept-runs',
          prefix: 'runs/',
          tagFilters: { [RETENTION_TAG_KEY]: 'expire' },
          expiration: cdk.Duration.days(30),
        },
        {
          id: 'expire-sagemaker-output',
          prefix: 'sagemaker-output/',
          expiration: cdk.Duration.days(30),
        },
        {
          id: 'abort-incomplete-multipart',
          abortIncompleteMultipartUploadAfter: cdk.Duration.days(7),
        },
      ],
    });
    const bucketArn = this.bucket.bucketArn;
    const objects = (prefix: string): string => `${bucketArn}/${prefix}`;

    // ---------------------------------------------------------------------
    // Task state (the SetFit key schema: DynamoDbTaskBackend expects it)
    // ---------------------------------------------------------------------
    this.table = new dynamodb.Table(this, 'TasksTable', {
      tableName: `aprender-laya-train-tasks-${env}`,
      partitionKey: { name: 'owner_id', type: dynamodb.AttributeType.STRING },
      sortKey: { name: 'task_id', type: dynamodb.AttributeType.STRING },
      billingMode: dynamodb.BillingMode.PAY_PER_REQUEST,
      timeToLiveAttribute: 'expires_at',
      pointInTimeRecoverySpecification: { pointInTimeRecoveryEnabled: isProd },
      removalPolicy: tableRemoval,
    });

    // ---------------------------------------------------------------------
    // Network and cluster for the arm64 pack task. Two public subnets, no NAT: the task needs
    // S3 and ECR only, and a NAT gateway is a standing hourly charge for nothing. The security
    // group has no ingress at all.
    // ---------------------------------------------------------------------
    // maxAzs 2: with no account (the offline synth) the CDK emits Fn::GetAZs; with a concrete
    // account `cdk diff` / `cdk deploy` makes ONE read-only availability-zone lookup and caches it
    // in cdk.context.json (gitignored: it embeds the account id).
    const vpc = new ec2.Vpc(this, 'PackVpc', {
      maxAzs: 2,
      natGateways: 0,
      subnetConfiguration: [{ name: 'public', subnetType: ec2.SubnetType.PUBLIC, cidrMask: 24 }],
    });
    const securityGroup = new ec2.SecurityGroup(this, 'PackSg', {
      vpc,
      description: 'Laya pack task: no ingress',
      allowAllOutbound: true,
    });
    const cluster = new ecs.Cluster(this, 'PackCluster', {
      clusterName: `aprender-laya-train-${env}`,
      vpc,
    });

    // ---------------------------------------------------------------------
    // Roles (T-14-30): four principals, four different rights over one bucket
    // ---------------------------------------------------------------------
    const ecrPull = (role: iam.IRole, repo: ecr.IRepository): void => {
      role.addToPrincipalPolicy(
        new iam.PolicyStatement({ actions: ['ecr:GetAuthorizationToken'], resources: ['*'] }),
      );
      role.addToPrincipalPolicy(
        new iam.PolicyStatement({
          actions: ['ecr:BatchCheckLayerAvailability', 'ecr:GetDownloadUrlForLayer', 'ecr:BatchGetImage'],
          resources: [repo.repositoryArn],
        }),
      );
    };

    // SageMaker execution role: reads datasets/* and base/*, writes runs/* (every run_step
    // upload is tagged, so PutObject AND PutObjectTagging) and sagemaker-output/* only.
    this.sagemakerRole = new iam.Role(this, 'SageMakerRole', {
      assumedBy: new iam.ServicePrincipal('sagemaker.amazonaws.com'),
      description: 'Laya training job: reads datasets/ and base/, writes runs/ and sagemaker-output/',
    });
    ecrPull(this.sagemakerRole, trainRepo);
    this.sagemakerRole.addToPolicy(
      new iam.PolicyStatement({
        actions: ['s3:GetObject'],
        resources: [objects('datasets/*'), objects(`${BASE_PREFIX}*`)],
      }),
    );
    this.sagemakerRole.addToPolicy(
      new iam.PolicyStatement({
        // S3Prefix channels list the prefix before downloading it.
        actions: ['s3:ListBucket'],
        resources: [bucketArn],
        // StringLikeIfExists, not StringLike: GetObject/HeadObject carry no s3:prefix key, and a
        // ListBucket condition that fails there turns a missing key's 404 into 403.
        conditions: { StringLikeIfExists: { 's3:prefix': ['datasets/*', `${BASE_PREFIX}*`] } },
      }),
    );
    this.sagemakerRole.addToPolicy(
      new iam.PolicyStatement({
        actions: ['s3:PutObject', 's3:PutObjectTagging'],
        resources: [objects('runs/*')],
      }),
    );
    this.sagemakerRole.addToPolicy(
      new iam.PolicyStatement({
        actions: ['s3:PutObject', 's3:AbortMultipartUpload', 's3:ListMultipartUploadParts'],
        resources: [objects('sagemaker-output/*')],
      }),
    );
    this.sagemakerRole.addToPolicy(
      new iam.PolicyStatement({
        actions: ['logs:CreateLogGroup', 'logs:CreateLogStream', 'logs:PutLogEvents', 'logs:DescribeLogStreams'],
        resources: [
          this.formatArn({
            service: 'logs',
            resource: 'log-group',
            resourceName: '/aws/sagemaker/TrainingJobs:*',
            arnFormat: cdk.ArnFormat.COLON_RESOURCE_NAME,
          }),
        ],
      }),
    );
    this.sagemakerRole.addToPolicy(
      new iam.PolicyStatement({
        actions: ['cloudwatch:PutMetricData'],
        resources: ['*'],
        conditions: { StringEquals: { 'cloudwatch:namespace': '/aws/sagemaker/TrainingJobs' } },
      }),
    );

    // Fargate task role (what the pack container's own code may do): reads runs/* and base/*,
    // writes runs/* (PutObject and PutObjectTagging). Its execution role only pulls the image
    // and writes its own log group.
    const packTaskRole = new iam.Role(this, 'PackTaskRole', {
      assumedBy: new iam.ServicePrincipal('ecs-tasks.amazonaws.com'),
      description: 'Laya pack task: reads runs/ and base/, writes runs/',
    });
    packTaskRole.addToPolicy(
      new iam.PolicyStatement({
        actions: ['s3:GetObject'],
        resources: [objects('runs/*'), objects(`${BASE_PREFIX}*`)],
      }),
    );
    packTaskRole.addToPolicy(
      new iam.PolicyStatement({
        actions: ['s3:PutObject', 's3:PutObjectTagging'],
        resources: [objects('runs/*')],
      }),
    );
    const packExecutionRole = new iam.Role(this, 'PackExecutionRole', {
      assumedBy: new iam.ServicePrincipal('ecs-tasks.amazonaws.com'),
      description: 'Laya pack task: pulls its image and writes its logs',
    });
    ecrPull(packExecutionRole, packRepo);

    const packLogs = new logs.LogGroup(this, 'PackLogGroup', {
      logGroupName: `/aws/ecs/aprender-laya-pack-${env}`,
      retention: logRetention,
      removalPolicy: tableRemoval,
    });
    packExecutionRole.addToPolicy(
      new iam.PolicyStatement({
        actions: ['logs:CreateLogStream', 'logs:PutLogEvents'],
        resources: [packLogs.logGroupArn],
      }),
    );

    // ---------------------------------------------------------------------
    // The arm64 pack-and-verify task (D-14: the training job never packs)
    // ---------------------------------------------------------------------
    const packTaskDefinition = new ecs.FargateTaskDefinition(this, 'PackTaskDefinition', {
      family: packFamily,
      cpu: 8192,
      memoryLimitMiB: 16384,
      ephemeralStorageGiB: 60,
      runtimePlatform: {
        cpuArchitecture: ecs.CpuArchitecture.ARM64,
        operatingSystemFamily: ecs.OperatingSystemFamily.LINUX,
      },
      taskRole: packTaskRole,
      executionRole: packExecutionRole,
    });
    const packContainer = packTaskDefinition.addContainer('pack', {
      // Digest-pinned: the task runs exactly the bytes whose digest this template names.
      image: ecs.ContainerImage.fromRegistry(packImage),
      command: ['pack'],
      logging: ecs.LogDrivers.awsLogs({ logGroup: packLogs, streamPrefix: 'pack' }),
    });

    // ---------------------------------------------------------------------
    // The finalizer (14-05): the one writer of a run's terminal record
    // ---------------------------------------------------------------------
    const finalizerLogs = new logs.LogGroup(this, 'FinalizerLogGroup', {
      logGroupName: `/aws/lambda/aprender-laya-train-finalizer-${env}`,
      retention: logRetention,
      removalPolicy: tableRemoval,
    });
    this.finalizer = new lambda.Function(this, 'Finalizer', {
      functionName: `aprender-laya-train-finalizer-${env}`,
      runtime: lambda.Runtime.PROVIDED_AL2023,
      architecture: lambda.Architecture.ARM_64,
      handler: 'bootstrap',
      code: lambda.Code.fromAsset(props.finalizerAssetPath),
      memorySize: 512,
      timeout: cdk.Duration.seconds(300),
      logGroup: finalizerLogs,
      environment: {
        APRENDER_LAYA_TRAIN_TABLE: this.table.tableName,
        APRENDER_LAYA_TRAIN_BUCKET: this.bucket.bucketName,
        APRENDER_LAYA_TRAIN_STATE_MACHINE_ARN: machineArn,
        APRENDER_LAYA_TRAIN_CLUSTER_ARN: cluster.clusterArn,
        APRENDER_LAYA_TRAIN_PACK_FAMILY: packFamily,
        RUST_LOG: 'info',
      },
      // A retry is the sweep's job, not the platform's: an async retry races the sweep for the
      // same record. The two EventBridge targets carry their own retry and DLQ below.
      retryAttempts: 0,
    });
    const finalizer = this.finalizer;
    finalizer.addToRolePolicy(
      new iam.PolicyStatement({
        // Read, write, tag and read the tags back, on the runs only.
        actions: ['s3:GetObject', 's3:PutObject', 's3:PutObjectTagging', 's3:GetObjectTagging'],
        resources: [objects('runs/*')],
      }),
    );
    finalizer.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ['s3:ListBucket'],
        resources: [bucketArn],
        // StringLikeIfExists: a missing key must answer NoSuchKey (404), not AccessDenied (403); the
        // finalizer's ObjectStore::get treats only NoSuchKey as absent.
        conditions: { StringLikeIfExists: { 's3:prefix': ['runs/*'] } },
      }),
    );
    finalizer.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ['dynamodb:GetItem', 'dynamodb:PutItem', 'dynamodb:UpdateItem', 'dynamodb:DeleteItem', 'dynamodb:Query'],
        resources: [this.table.tableArn],
      }),
    );
    finalizer.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ['states:DescribeExecution', 'states:StopExecution'],
        resources: [executionArns],
      }),
    );
    finalizer.addToRolePolicy(
      new iam.PolicyStatement({ actions: ['states:ListExecutions'], resources: [machineArn] }),
    );
    finalizer.addToRolePolicy(
      new iam.PolicyStatement({
        actions: ['sagemaker:DescribeTrainingJob', 'sagemaker:StopTrainingJob'],
        resources: [
          this.formatArn({
            service: 'sagemaker',
            resource: 'training-job',
            resourceName: 'laya-*',
            arnFormat: cdk.ArnFormat.SLASH_RESOURCE_NAME,
          }),
        ],
      }),
    );
    finalizer.addToRolePolicy(
      new iam.PolicyStatement({
        // ListTasks / DescribeTasks / StopTask accept no resource-level ARN for the task set the
        // finalizer needs, so they are conditioned on the one cluster instead.
        actions: ['ecs:ListTasks', 'ecs:DescribeTasks', 'ecs:StopTask'],
        resources: ['*'],
        conditions: { ArnEquals: { 'ecs:cluster': cluster.clusterArn } },
      }),
    );

    // ---------------------------------------------------------------------
    // The state machine (D-02, D-12, D-13): Train -> PackVerify -> Finalize
    // ---------------------------------------------------------------------
    const bucketName = this.bucket.bucketName;
    const executionRef = {
      source: 'state-machine',
      execution_arn: sfn.JsonPath.stringAt('$$.Execution.Id'),
      start_time: sfn.JsonPath.stringAt('$$.Execution.StartTime'),
      input: sfn.JsonPath.objectAt('$$.Execution.Input'),
    };

    const train = new tasks.SageMakerCreateTrainingJob(this, 'Train', {
      stateName: 'Train',
      integrationPattern: sfn.IntegrationPattern.RUN_JOB,
      trainingJobName: sfn.JsonPath.format('laya-{}', sfn.JsonPath.stringAt('$.task_id')),
      role: this.sagemakerRole,
      algorithmSpecification: {
        trainingImage: tasks.DockerImage.fromRegistry(trainImage),
        trainingInputMode: tasks.InputMode.FILE,
      },
      inputDataConfig: [
        {
          // The caller's dataset prefix was normalized and validated by the request function
          // (datasets/<segs>/); it reaches the job only as this one intrinsic over the input.
          channelName: 'data',
          dataSource: {
            s3DataSource: {
              s3DataType: tasks.S3DataType.S3_PREFIX,
              s3Location: new LiteralLocation(
                sfn.JsonPath.format(`s3://${bucketName}/{}`, sfn.JsonPath.stringAt('$.dataset_prefix')),
              ),
            },
          },
        },
        {
          channelName: 'base',
          dataSource: {
            s3DataSource: {
              s3DataType: tasks.S3DataType.S3_PREFIX,
              s3Location: new LiteralLocation(`s3://${bucketName}/${BASE_PREFIX}`),
            },
          },
        },
      ],
      outputDataConfig: {
        s3OutputLocation: new LiteralLocation(`s3://${bucketName}/sagemaker-output/`),
      },
      resourceConfig: {
        instanceCount: TRAIN_INSTANCE_COUNT,
        // The CDK renders "ml." + this, so the prefix is dropped here and the template says
        // exactly ml.g5.xlarge (the check script asserts the rendered literal).
        instanceType: new ec2.InstanceType(TRAIN_INSTANCE_TYPE.replace(/^ml\./, '')),
        volumeSize: cdk.Size.gibibytes(100),
      },
      stoppingCondition: { maxRuntime: cdk.Duration.seconds(TRAIN_MAX_RUNTIME_SECS) },
      environment: {
        LAYA_STEP: 'train',
        LAYA_TASK_ID: sfn.JsonPath.stringAt('$.task_id'),
        LAYA_NAME: sfn.JsonPath.stringAt('$.name'),
        LAYA_BUCKET: bucketName,
        LAYA_RUN_PREFIX: sfn.JsonPath.stringAt('$.run_prefix'),
        LAYA_EPOCHS: sfn.JsonPath.stringAt('$.epochs'),
        // 14-01/14-06's validated-bytes binding: the job refuses bytes that are not the ones
        // the request function validated.
        LAYA_EXPECTED_INPUTS: sfn.JsonPath.stringAt('$.inputs_sha256'),
        // A literal equal to the digest in TrainingImage above, never read from the input.
        LAYA_TRAIN_IMAGE_DIGEST: props.trainImageDigest,
      },
      resultPath: sfn.JsonPath.DISCARD,
    });

    const packVerify = new tasks.EcsRunTask(this, 'PackVerify', {
      stateName: 'PackVerify',
      integrationPattern: sfn.IntegrationPattern.RUN_JOB,
      cluster,
      taskDefinition: packTaskDefinition,
      launchTarget: new tasks.EcsFargateLaunchTarget({
        platformVersion: ecs.FargatePlatformVersion.LATEST,
      }),
      assignPublicIp: true,
      subnets: { subnetType: ec2.SubnetType.PUBLIC },
      securityGroups: [securityGroup],
      containerOverrides: [
        {
          containerDefinition: packContainer,
          environment: [
            { name: 'LAYA_TASK_ID', value: sfn.JsonPath.stringAt('$.task_id') },
            { name: 'LAYA_NAME', value: sfn.JsonPath.stringAt('$.name') },
            { name: 'LAYA_BUCKET', value: bucketName },
            { name: 'LAYA_RUN_PREFIX', value: sfn.JsonPath.stringAt('$.run_prefix') },
            // A literal equal to the digest in the task definition's image.
            { name: 'LAYA_PACK_IMAGE_DIGEST', value: props.packImageDigest },
          ],
        },
      ],
      taskTimeout: sfn.Timeout.duration(cdk.Duration.hours(3)),
      resultPath: sfn.JsonPath.DISCARD,
    });

    const finalize = new tasks.LambdaInvoke(this, 'Finalize', {
      stateName: 'Finalize',
      lambdaFunction: finalizer,
      payload: sfn.TaskInput.fromObject(executionRef),
      resultPath: sfn.JsonPath.DISCARD,
    });
    const catchFinalize = new tasks.LambdaInvoke(this, 'CatchFinalize', {
      stateName: 'CatchFinalize',
      lambdaFunction: finalizer,
      payload: sfn.TaskInput.fromObject({ ...executionRef, error: sfn.JsonPath.objectAt('$.error') }),
      resultPath: sfn.JsonPath.DISCARD,
    });
    // A failed Train or PackVerify still finalizes, carrying the error the Catch recorded.
    train.addCatch(catchFinalize, { errors: [sfn.Errors.ALL], resultPath: '$.error' });
    packVerify.addCatch(catchFinalize, { errors: [sfn.Errors.ALL], resultPath: '$.error' });

    // No state-machine log group on purpose: execution history is kept by Step Functions
    // itself, and the CDK's log-delivery grant adds logs:PutResourcePolicy on every resource
    // to the machine's role, which is more privilege than this machine's observability needs.
    this.machine = new sfn.StateMachine(this, 'Machine', {
      stateMachineName: machineName,
      stateMachineType: sfn.StateMachineType.STANDARD,
      definitionBody: sfn.DefinitionBody.fromChainable(train.next(packVerify).next(finalize)),
      timeout: cdk.Duration.seconds(MACHINE_TIMEOUT_SECS),
    });

    // ---------------------------------------------------------------------
    // What drives the finalizer besides the machine's own Finalize state
    // ---------------------------------------------------------------------
    const dlq = new sqs.Queue(this, 'FinalizerDlq', {
      queueName: `aprender-laya-train-finalizer-dlq-${env}`,
      retentionPeriod: cdk.Duration.days(14),
      enforceSSL: true,
    });
    const statusRule = new events.Rule(this, 'ExecutionStatusRule', {
      description: 'A failed, timed-out or aborted Laya run is finalized (best effort; the schedule below is the floor)',
      eventPattern: {
        source: ['aws.states'],
        detailType: ['Step Functions Execution Status Change'],
        detail: {
          status: ['FAILED', 'TIMED_OUT', 'ABORTED'],
          stateMachineArn: [machineArn],
        },
      },
    });
    statusRule.addTarget(new targets.LambdaFunction(finalizer, { retryAttempts: 4, deadLetterQueue: dlq }));
    // Step Functions status events are delivered best effort and may be lost; this sweep
    // re-drives every run whose finalization is incomplete (14-05's reconcile).
    const reconcileRule = new events.Rule(this, 'ReconcileRule', {
      description: 'Every 10 minutes: the finalizer reconcile sweep',
      schedule: events.Schedule.rate(cdk.Duration.minutes(10)),
    });
    reconcileRule.addTarget(
      new targets.LambdaFunction(finalizer, {
        event: events.RuleTargetInput.fromObject({ source: 'reconcile' }),
        retryAttempts: 4,
        deadLetterQueue: dlq,
      }),
    );
    const errorsAlarm = new cloudwatch.Alarm(this, 'FinalizerErrors', {
      alarmName: `aprender-laya-train-FinalizerErrors-${env}`,
      alarmDescription: 'The Laya finalizer returned an error: a run may be stuck holding the single-flight lock',
      metric: finalizer.metricErrors({ period: cdk.Duration.minutes(5), statistic: 'Sum' }),
      threshold: 1,
      evaluationPeriods: 1,
      comparisonOperator: cloudwatch.ComparisonOperator.GREATER_THAN_OR_EQUAL_TO_THRESHOLD,
      treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
    });
    void errorsAlarm;

    // ---------------------------------------------------------------------
    // Outputs. RequestLambdaPolicy is an OUTPUT ONLY (D-03 / D-10): the pmcp.run request
    // function is deployed by cargo-pmcp from another app, and the owner attaches this after
    // the 401 check. No statement of this stack grants it anything.
    // ---------------------------------------------------------------------
    const out = (name: string, value: string, description?: string): void => {
      new cdk.CfnOutput(this, name, { value, description });
    };
    out('BucketName', this.bucket.bucketName);
    out('TableName', this.table.tableName);
    out('StateMachineArn', this.machine.stateMachineArn);
    out('FinalizerName', finalizer.functionName);
    out('SageMakerRoleArn', this.sagemakerRole.roleArn);
    out('TrainRepoUri', trainRepo.repositoryUri);
    out('PackTaskDefinitionArn', packTaskDefinition.taskDefinitionArn);
    out('ClusterArn', cluster.clusterArn);
    out('SubnetIds', vpc.publicSubnets.map((s) => s.subnetId).join(','));
    out('SecurityGroupId', securityGroup.securityGroupId);
    out('FinalizerDlqUrl', dlq.queueUrl);
    out(
      'RequestLambdaPolicy',
      JSON.stringify({
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
            Resource: this.table.tableArn,
          },
          {
            Effect: 'Allow',
            Action: ['states:StartExecution'],
            Resource: this.machine.stateMachineArn,
          },
          {
            // DescribeExecution is 14-04's dispatch confirm; StopExecution is the cancel sink.
            Effect: 'Allow',
            Action: ['states:DescribeExecution', 'states:StopExecution'],
            Resource: executionArns,
          },
          {
            // validate_dataset reads the dataset; the presigned links train_status mints carry
            // these rights, so runs/* is readable but never writable by this principal.
            Effect: 'Allow',
            Action: ['s3:GetObject'],
            Resource: [objects('datasets/*'), objects('runs/*')],
          },
          {
            // Without ListBucket S3 answers HeadObject on a MISSING key with 403, not 404.
            Effect: 'Allow',
            Action: ['s3:ListBucket'],
            Resource: bucketArn,
            // StringLikeIfExists keeps real list calls on datasets/* while a HeadObject (no s3:prefix key) passes.
            Condition: { StringLikeIfExists: { 's3:prefix': ['datasets/*'] } },
          },
        ],
      }),
      'Attach to the pmcp.run request Lambda role ONLY after the 401 check (14-13). Until then every train call compensates to failed with AccessDenied and nothing can be spent.',
    );
  }
}
