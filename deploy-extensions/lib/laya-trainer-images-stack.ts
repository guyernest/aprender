import * as cdk from 'aws-cdk-lib';
import * as codebuild from 'aws-cdk-lib/aws-codebuild';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as s3 from 'aws-cdk-lib/aws-s3';
import { Construct } from 'constructs';

/**
 * The images the Laya training server runs (Phase 14, plan 14-08): two ECR repositories, a
 * build-context bucket and two privileged CodeBuild projects.
 *
 * Why CodeBuild and not a laptop build: the training image is x86_64 + CUDA (SageMaker
 * ml.g5.xlarge) and the pack image is linux/arm64 (Graviton Fargate). Neither architecture is
 * the other's, and an emulated build on Apple silicon is slow and is not the artifact that
 * runs. Each project builds on its own architecture, so what is pushed is what runs.
 *
 * The run's identity is the pushed image DIGEST. Tags are IMMUTABLE and are the 40-hex commit
 * the build was started for, so no tag can later point at different bytes, and the ECR
 * lifecycle expires UNTAGGED images only: a deployed or reproduction-referenced digest can
 * never be reclaimed by a count rule (T-14-67).
 */
export interface LayaTrainerImagesStackProps extends cdk.StackProps {
  /** `dev` or `prod`; every resource name carries it. */
  readonly environment: string;
}

/** The Dockerfile each project builds by default (a start-build override can name another). */
export const TRAIN_DOCKERFILE = 'scripts/laya_train/container/Dockerfile.train';
export const PACK_DOCKERFILE = 'deploy-extensions/container/pack-arm64/Dockerfile';

/**
 * The buildspec both projects share. It refuses anything that is not a 40-hex commit as the
 * tag (IMMUTABLE tags make a wrong one permanent), logs in to ECR, builds, pushes, and prints
 * the digest ECR assigned to the pushed manifest in a form the owner can copy and a script can
 * parse: `IMAGE DIGEST sha256:<64 hex>`.
 */
const BUILDSPEC = {
  version: '0.2',
  phases: {
    pre_build: {
      commands: [
        'echo "$GIT_COMMIT" | grep -Eq "^[0-9a-f]{40}$" || { echo "REFUSED GIT_COMMIT must be a 40-hex commit, got: $GIT_COMMIT"; exit 1; }',
        'aws ecr get-login-password --region "$AWS_REGION" | docker login --username AWS --password-stdin "${REPO_URI%%/*}"',
      ],
    },
    build: {
      commands: [
        'docker build -f "$DOCKERFILE" --build-arg GIT_COMMIT="$GIT_COMMIT" -t "$REPO_URI:$GIT_COMMIT" .',
        'docker push "$REPO_URI:$GIT_COMMIT"',
        'DIGEST=$(docker inspect --format "{{index .RepoDigests 0}}" "$REPO_URI:$GIT_COMMIT" | cut -d@ -f2)',
        'echo "$DIGEST" | grep -Eq "^sha256:[0-9a-f]{64}$" || { echo "REFUSED no digest after the push"; exit 1; }',
        'echo "IMAGE DIGEST $DIGEST"',
      ],
    },
  },
};

export class LayaTrainerImagesStack extends cdk.Stack {
  public readonly trainRepo: ecr.Repository;
  public readonly packRepo: ecr.Repository;
  public readonly contextBucket: s3.Bucket;
  public readonly trainProject: codebuild.Project;
  public readonly packProject: codebuild.Project;

  constructor(scope: Construct, id: string, props: LayaTrainerImagesStackProps) {
    super(scope, id, props);
    const env = props.environment;

    const repo = (logicalId: string, name: string): ecr.Repository =>
      new ecr.Repository(this, logicalId, {
        repositoryName: name,
        imageTagMutability: ecr.TagMutability.IMMUTABLE,
        imageScanOnPush: true,
        // Untagged images only. Every image this stack's builds push carries its commit tag,
        // so nothing a run references can match this rule.
        lifecycleRules: [
          {
            rulePriority: 1,
            description: 'expire untagged images after 14 days',
            tagStatus: ecr.TagStatus.UNTAGGED,
            maxImageAge: cdk.Duration.days(14),
          },
        ],
        removalPolicy: cdk.RemovalPolicy.RETAIN,
      });
    this.trainRepo = repo('TrainRepo', `aprender-laya-train-${env}`);
    this.packRepo = repo('PackRepo', `aprender-laya-pack-${env}`);

    this.contextBucket = new s3.Bucket(this, 'ContextBucket', {
      bucketName: `aprender-laya-build-context-${env}-${this.account}`,
      encryption: s3.BucketEncryption.S3_MANAGED,
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      enforceSSL: true,
      removalPolicy: cdk.RemovalPolicy.RETAIN,
      lifecycleRules: [{ id: 'expire-build-contexts', expiration: cdk.Duration.days(7) }],
    });

    const project = (
      logicalId: string,
      name: string,
      target: ecr.Repository,
      buildImage: codebuild.IBuildImage,
      dockerfile: string,
    ): codebuild.Project => {
      const logGroup = new logs.LogGroup(this, `${logicalId}Logs`, {
        logGroupName: `/aws/codebuild/${name}`,
        retention: logs.RetentionDays.ONE_MONTH,
        removalPolicy: cdk.RemovalPolicy.RETAIN,
      });
      const p = new codebuild.Project(this, logicalId, {
        projectName: name,
        // The source is overridden per build (`--source-type-override S3 --source-location-override`);
        // this default only has to be a well-formed S3 location.
        source: codebuild.Source.s3({ bucket: this.contextBucket, path: 'context/placeholder.zip' }),
        environment: {
          buildImage,
          computeType: codebuild.ComputeType.LARGE,
          privileged: true,
          environmentVariables: {
            REPO_URI: { value: target.repositoryUri },
            DOCKERFILE: { value: dockerfile },
            GIT_COMMIT: { value: 'unset' },
          },
        },
        buildSpec: codebuild.BuildSpec.fromObject(BUILDSPEC),
        logging: { cloudWatch: { logGroup } },
        timeout: cdk.Duration.minutes(120),
      });
      // Push to the one repository this project builds, nothing else. The build role is
      // created by the project; the managed-by-CDK grants are replaced by these.
      target.grantPullPush(p);
      this.contextBucket.grantRead(p);
      return p;
    };

    // x86_64 builder for the CUDA training image.
    this.trainProject = project(
      'TrainImageProject',
      `aprender-laya-train-image-${env}`,
      this.trainRepo,
      codebuild.LinuxBuildImage.STANDARD_7_0,
      TRAIN_DOCKERFILE,
    );
    // Graviton builder for the arm64 pack image (the same architecture the Fargate task runs).
    this.packProject = project(
      'PackImageProject',
      `aprender-laya-pack-image-${env}`,
      this.packRepo,
      codebuild.LinuxArmBuildImage.AMAZON_LINUX_2023_STANDARD_3_0,
      PACK_DOCKERFILE,
    );

    new cdk.CfnOutput(this, 'TrainRepoUri', { value: this.trainRepo.repositoryUri });
    new cdk.CfnOutput(this, 'PackRepoUri', { value: this.packRepo.repositoryUri });
    new cdk.CfnOutput(this, 'TrainProjectName', { value: this.trainProject.projectName });
    new cdk.CfnOutput(this, 'PackProjectName', { value: this.packProject.projectName });
    new cdk.CfnOutput(this, 'ContextBucketName', { value: this.contextBucket.bucketName });
  }
}
