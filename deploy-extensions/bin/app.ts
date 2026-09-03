#!/usr/bin/env node
import * as path from 'path';
import * as fs from 'fs';
import * as cdk from 'aws-cdk-lib';
import { SetFitTrainingStack } from '../lib/setfit-training-stack';

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

new SetFitTrainingStack(app, `aprender-setfit-training-${environment}`, {
  env: {
    account: process.env.CDK_DEFAULT_ACCOUNT,
    region:
      process.env.AWS_REGION ?? process.env.CDK_DEFAULT_REGION ?? 'us-east-1',
  },
  environment,
  trainerAssetPath,
  description: `SetFit training: task state, artifact bucket and the training worker (${environment})`,
});

app.synth();
