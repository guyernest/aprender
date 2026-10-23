import * as cdk from 'aws-cdk-lib';
import * as lambda from 'aws-cdk-lib/aws-lambda';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as ssm from 'aws-cdk-lib/aws-ssm';
import { Construct } from 'constructs';

/**
 * The embed endpoint's Lambda, with NO way in.
 *
 * Phase 11 (aprender-embed-endpoint) serves an OpenAI-shaped embeddings call from
 * one arm64 Lambda. R4 forbids public ingress, so this stack creates the function,
 * its log group and the identity policy a caller needs, and nothing that admits a
 * request from outside the account: no function URL, no API gateway of any kind,
 * no load balancer, no resource-based permission, no event source. A caller holds
 * lambda:InvokeFunction through an identity policy the operator binds, which is the
 * only door. The offline scanner (scripts/embed_endpoint/ingress_scan.py) fails the
 * synthesized template if any of those appear.
 *
 * Two variants share this construct. `probe` is the Graviton measurement harness of
 * plan 11-01 (a standalone project, deployed only to measure). `endpoint` is the
 * product function, finished by plan 11-10. Both are arm64, both run the custom
 * runtime, and both take their code as the deterministic .zip the package recipe
 * writes, so the live CodeSha256 can be compared with the record of the bytes tested.
 */
export type EmbedVariant = 'endpoint' | 'probe';

/**
 * Memory for the function. The tier is bought for vCPU (six at this size), not for
 * resident memory: the research measured about 3.5 GB peak RSS on 14 local threads.
 */
export const EMBED_MEMORY_MB = 10240;

/** Endpoint timeout. R6 requires at least 30 s; 60 s leaves room for a cold start. */
export const EMBED_TIMEOUT_SECONDS = 60;

/** Probe timeout. A 64-input cold sample must fit with room to spare for the measurement. */
export const PROBE_TIMEOUT_SECONDS = 120;

/**
 * Rayon pool width for the matrix kernels. Six vCPUs at the 10,240 MB tier. The probe
 * reports the width it actually engaged, so this value is evidence, not an assumption.
 */
export const EMBED_RAYON_THREADS = 6;

export interface EmbedEndpointStackProps extends cdk.StackProps {
  /** dev or prod. Names the function and sets log retention. */
  readonly environment: string;
  /** Which function this stack creates. */
  readonly variant: EmbedVariant;
  /** Path to the deterministic .zip the package recipe wrote. CDK uploads it byte for byte. */
  readonly assetPath: string;
  /** Memory in MB, 128 to 10240. Defaults to EMBED_MEMORY_MB. */
  readonly memoryMb?: number;
  /** Timeout in seconds. Defaults per variant. */
  readonly timeoutSeconds?: number;
}

export class EmbedEndpointStack extends cdk.Stack {
  public readonly fn: lambda.Function;

  constructor(scope: Construct, id: string, props: EmbedEndpointStackProps) {
    super(scope, id, props);

    const env = props.environment;
    const isProd = env === 'prod';
    const isProbe = props.variant === 'probe';
    const functionName = isProbe
      ? `aprender-embed-probe-${env}`
      : `aprender-embed-endpoint-${env}`;

    // Retention follows the environment. The probe's log group is always destroyed:
    // it is a measurement, and its logs are the evidence the owner has already kept.
    const logGroup = new logs.LogGroup(this, 'FunctionLogGroup', {
      logGroupName: `/aws/lambda/${functionName}`,
      retention: isProd
        ? logs.RetentionDays.THREE_MONTHS
        : logs.RetentionDays.ONE_WEEK,
      removalPolicy:
        isProbe || !isProd ? cdk.RemovalPolicy.DESTROY : cdk.RemovalPolicy.RETAIN,
    });

    this.fn = new lambda.Function(this, 'EmbedFunction', {
      functionName,
      runtime: lambda.Runtime.PROVIDED_AL2023,
      architecture: lambda.Architecture.ARM_64,
      handler: 'bootstrap',
      code: lambda.Code.fromAsset(props.assetPath),
      memorySize: props.memoryMb ?? EMBED_MEMORY_MB,
      timeout: cdk.Duration.seconds(
        props.timeoutSeconds ??
          (isProbe ? PROBE_TIMEOUT_SECONDS : EMBED_TIMEOUT_SECONDS),
      ),
      logGroup,
      environment: {
        // Paths inside the package, resolved by the function at start.
        APRENDER_EMBED_MODEL_DIR: '/var/task/model',
        RAYON_NUM_THREADS: String(EMBED_RAYON_THREADS),
        RUST_LOG: 'info',
      },
      // The endpoint writes nothing to /tmp, so the default ephemeral storage stands.
      // A synchronous caller never needs async retries, and an async invoke must not
      // triple the work.
      retryAttempts: 0,
    });

    cdk.Tags.of(this).add('project', 'aprender-embed-endpoint');
    cdk.Tags.of(this).add('phase', '11');
    cdk.Tags.of(this).add('variant', props.variant);

    // The endpoint's identity is published for the caller's deployment to read. The
    // probe publishes nothing: the owner-run recipes address it by name.
    if (!isProbe) {
      const params: Record<string, string> = {
        'function-name': this.fn.functionName,
        'function-arn': this.fn.functionArn,
      };
      for (const [key, value] of Object.entries(params)) {
        new ssm.StringParameter(this, `Param-${key}`, {
          parameterName: `/aprender/embed-endpoint/${env}/${key}`,
          stringValue: value,
        });
      }
    }

    new cdk.CfnOutput(this, 'FunctionName', { value: this.fn.functionName });
    new cdk.CfnOutput(this, 'FunctionArn', { value: this.fn.functionArn });

    // The identity policy a caller needs. Published, not attached: the caller's role
    // belongs to another stack, so this one states the requirement and the operator
    // binds it. Lambda's invoke check accepts the function ARN and its qualified form.
    new cdk.CfnOutput(this, 'InvokePolicy', {
      description:
        'Identity policy for the caller: lambda:InvokeFunction on this function only. The operator binds it to the caller; this stack states the requirement rather than reaching into another stack.',
      value: JSON.stringify({
        Version: '2012-10-17',
        Statement: [
          {
            Effect: 'Allow',
            Action: ['lambda:InvokeFunction'],
            Resource: [this.fn.functionArn, `${this.fn.functionArn}:*`],
          },
        ],
      }),
    });
  }
}
