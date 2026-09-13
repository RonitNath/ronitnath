import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { generateGithubWorkflow } from '@isoastra/fleet-delivery/github';

async function main() {
  const path = '.github/workflows/deploy.yml';
  const expected = generateGithubWorkflow({
    packageVersion: '0.3.8',
    triggerBranches: ['deploy', 'staging', 'preview/**'],
    runnerLabels: ['self-hosted', 'ronitnath-delivery', 'delenda'],
    pipelineFile: 'delivery.pipeline.json',
    nodeVersion: '24.13.0',
    pnpmVersion: '10.28.1',
    concurrencyGroup: 'ronitnath-production',
    telemetryUrl: 'https://ronitnath.com/api/delivery/events',
    artifactDirectory: '.delivery',
    branchEnvironments: {
      previewPattern: 'preview/**',
      previewPipelineFile: 'delivery.preview.json',
      stagingBranch: 'staging',
      stagingPipelineFile: 'delivery.staging.json',
      deploymentScript: 'delivery:environment',
    },
    workflowEnvironment: {
      DOCKER_HOST: 'unix:///run/user/2101/docker.sock',
      XDG_RUNTIME_DIR: '/run/user/2101',
    },
  });
  if (process.argv.includes('--check')) {
    const actual = await readFile(path, 'utf8').catch(() => '');
    if (actual !== expected) throw new Error(`${path} has drifted; run pnpm delivery:workflow`);
  } else {
    await mkdir('.github/workflows', { recursive: true });
    await writeFile(path, expected);
  }
}

void main();
