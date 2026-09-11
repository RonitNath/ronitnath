import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { generateGithubWorkflow } from '@isoastra/fleet-delivery/github';

async function main() {
const path = '.github/workflows/deploy.yml';
const expected = generateGithubWorkflow({ packageVersion: '0.1.0', runnerLabels: ['self-hosted', 'ronitnath-delivery', 'delenda'], pipelineFile: 'delivery.pipeline.json', nodeVersion: '24.13.0', pnpmVersion: '10.28.1' })
  .replace('      - run: pnpm exec fleet-delivery run delivery.pipeline.json', '      - run: mkdir -p .delivery\n      - run: flock /run/fleet-admission/heavy.lock pnpm exec fleet-delivery run delivery.pipeline.json')
  .replace('          DELIVERY_INGEST_TOKEN: ${{ secrets.DELIVERY_INGEST_TOKEN }}', '          DELIVERY_INGEST_TOKEN: ${{ secrets.DELIVERY_INGEST_TOKEN }}\n          DELIVERY_TELEMETRY_URL: https://ronitnath.com/api/delivery/events')
  .replace('      - if: always()\n        uses:', '      - if: always()\n        run: pnpm delivery:cleanup\n      - if: always()\n        uses:');
if (process.argv.includes('--check')) {
  const actual = await readFile(path, 'utf8').catch(() => '');
  if (actual !== expected) throw new Error(`${path} has drifted; run pnpm delivery:workflow`);
} else {
  await mkdir('.github/workflows', { recursive: true });
  await writeFile(path, expected);
}
}

void main();
