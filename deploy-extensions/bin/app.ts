#!/usr/bin/env node
import * as path from 'path';
import * as fs from 'fs';
import * as cdk from 'aws-cdk-lib';
import { SetFitTrainingStack } from '../lib/setfit-training-stack';
import { EmbedEndpointStack, EmbedVariant } from '../lib/embed-endpoint-stack';

const app = new cdk.App();

/**
 * Environment selects the resource-name suffix and the retention posture, and
 * is the ONLY thing that differs between dev and prod. Pass it explicitly:
 *
 *   cdk deploy --context env=dev   --profile ze-kasher-dev
 *   cdk deploy --context env=prod  --profile <prod>
 *
 * It defaults to `dev` so a forgotten flag cannot silently target production;
 * the reverse default is how a test deploy overwrites a live table.
 */
const environment = String(app.node.tryGetContext('env') ?? 'dev');
if (!['dev', 'prod'].includes(environment)) {
  throw new Error(
    `--context env=${environment} is not a known environment (expected dev or prod)`,
  );
}

/**
 * Which stack this app synthesizes. `training` is the DEFAULT, so every existing
 * recipe (synth-training, diff-training, deploy-training) synthesizes exactly what
 * it did before. `embed` is the Phase 11 endpoint; `embed-probe` is the Graviton
 * measurement harness of plan 11-01. One stack per synth, so a probe synth never
 * demands the trainer package.
 */
const stack = String(app.node.tryGetContext('stack') ?? 'training');
if (!['training', 'embed', 'embed-probe'].includes(stack)) {
  throw new Error(
    `--context stack=${stack} is not a known stack (expected training, embed or embed-probe)`,
  );
}

if (stack === 'training') {
  /**
   * The worker package. `cdk synth` must FAIL when it is missing rather than
   * deploy a Lambda with no code — the asset is built by
   * `just build-trainer-asset`, and a stale or absent one is the difference
   * between "the trainer is broken" and "the trainer was never shipped".
   */
  const trainerAssetPath = path.resolve(__dirname, '..', 'assets', 'trainer');
  if (!fs.existsSync(path.join(trainerAssetPath, 'bootstrap'))) {
    throw new Error(
      `no worker package at ${trainerAssetPath} (expected a 'bootstrap' binary).\n` +
        `Build it first:  just build-trainer-asset`,
    );
  }

  /**
   * Worker memory override, for an account whose Lambda ceiling is below what the
   * measured envelope needs. Absent, the stack uses its justified default and the
   * deploy fails loudly against a low ceiling — which is the right failure, since
   * silently shrinking to fit would produce a worker that OOMs every run.
   */
  const memoryContext = app.node.tryGetContext('trainerMemoryMb');
  const trainerMemoryMb =
    memoryContext === undefined ? undefined : Number(memoryContext);
  if (trainerMemoryMb !== undefined && !Number.isFinite(trainerMemoryMb)) {
    throw new Error(
      `--context trainerMemoryMb=${memoryContext} is not a number`,
    );
  }

  new SetFitTrainingStack(app, `aprender-setfit-training-${environment}`, {
    env: {
      account: process.env.CDK_DEFAULT_ACCOUNT,
      region:
        process.env.AWS_REGION ?? process.env.CDK_DEFAULT_REGION ?? 'us-east-1',
    },
    environment,
    trainerAssetPath,
    trainerMemoryMb,
    description: `SetFit training: task state, artifact bucket and the training worker (${environment})`,
  });
} else {
  /**
   * The embed stacks take their code as a deterministic .zip, written by the package
   * recipe next to an unzipped copy. The zip is what CDK uploads byte for byte, which
   * is what lets the live CodeSha256 be compared with the package record.
   */
  const variant: EmbedVariant = stack === 'embed-probe' ? 'probe' : 'endpoint';
  const assetDir = path.resolve(
    __dirname,
    '..',
    'assets',
    stack === 'embed-probe' ? 'embed-probe' : 'embed',
  );
  const zipPath = `${assetDir}.zip`;
  const recipe =
    stack === 'embed-probe'
      ? 'just embed-probe-asset'
      : 'the endpoint package recipe (plan 11-10)';
  for (const required of [
    path.join(assetDir, 'bootstrap'),
    path.join(assetDir, 'model', 'full_model.apr'),
    zipPath,
  ]) {
    if (!fs.existsSync(required)) {
      throw new Error(
        `no embed package: missing ${required}.\nBuild it first:  ${recipe}`,
      );
    }
  }

  const memoryContext = app.node.tryGetContext('embedMemoryMb');
  const memoryMb = memoryContext === undefined ? undefined : Number(memoryContext);
  if (
    memoryMb !== undefined &&
    (!Number.isInteger(memoryMb) || memoryMb < 128 || memoryMb > 10240)
  ) {
    throw new Error(
      `--context embedMemoryMb=${memoryContext} must be a whole number of MB between 128 and 10240`,
    );
  }

  const stackId =
    stack === 'embed-probe'
      ? `aprender-embed-probe-${environment}`
      : `aprender-embed-endpoint-${environment}`;
  new EmbedEndpointStack(app, stackId, {
    env: {
      account: process.env.CDK_DEFAULT_ACCOUNT,
      region:
        process.env.AWS_REGION ?? process.env.CDK_DEFAULT_REGION ?? 'us-east-1',
    },
    environment,
    variant,
    assetPath: zipPath,
    memoryMb,
    description: `Phase 11 embed ${variant} (${environment}): arm64, no ingress, invoke by IAM only`,
  });
}

app.synth();
