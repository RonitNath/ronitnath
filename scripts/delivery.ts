import { execFile } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { createHash } from 'node:crypto';
import { flushTelemetry } from '@isoastra/fleet-delivery/node';

const exec = promisify(execFile);
const dbName = 'ronitnath-delivery-db';
const network = 'ronitnath-delivery';
const webName = 'ronitnath-delivery-web';
const postgresImage =
  'postgres:17-alpine@sha256:18cfe3ef5e6815560c98237d6216d1e5119702fb0f3894c8785dd58b8bbe5d73';
const playwrightImage =
  'mcr.microsoft.com/playwright:v1.63.0-noble@sha256:eff16c30e6f3f4af0a03fa4b706120d5e9b0891c344a27d64559aff5900a4a27';
const repository = 'ghcr.io/ronitnath/ronitnath-app';
const compatibilitySha = 'd2373d4823d43beb4a7d2b24741636ee7b449098';

function sha(): string {
  const value = process.env.GITHUB_SHA ?? process.env.DELIVERY_SHA;
  if (!value || !/^[a-f0-9]{40}$/.test(value))
    throw new Error('GITHUB_SHA or DELIVERY_SHA must be a full SHA');
  return value;
}
function tag(kind: 'runtime' | 'migrate'): string {
  return `${repository}:${sha()}${kind === 'migrate' ? '-migrate' : ''}`;
}
function compatibilityTag(kind: 'runtime' | 'migrate'): string {
  return `ronitnath-compatibility:${compatibilitySha}-${kind}`;
}

async function run(
  file: string,
  args: string[],
  options: { env?: NodeJS.ProcessEnv; cwd?: string; quiet?: boolean } = {},
) {
  let result: { stdout: string; stderr: string };
  try {
    result = await exec(file, args, {
      env: { ...process.env, ...options.env },
      cwd: options.cwd,
      maxBuffer: 20 * 1024 * 1024,
    });
  } catch (error: unknown) {
    const failed = error as { stdout?: string; stderr?: string };
    if (!options.quiet && failed.stdout) process.stdout.write(failed.stdout);
    if (!options.quiet && failed.stderr) process.stderr.write(failed.stderr);
    throw error;
  }
  if (!options.quiet) {
    if (result.stdout) process.stdout.write(result.stdout);
    if (result.stderr) process.stderr.write(result.stderr);
  }
  return result.stdout.trim();
}
async function ignore(file: string, args: string[]) {
  try {
    await run(file, args, { quiet: true });
  } catch {}
}
async function waitForDb() {
  for (let attempt = 0; attempt < 40; attempt++) {
    try {
      await run('docker', ['exec', dbName, 'pg_isready', '-U', 'postgres'], { quiet: true });
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  }
  throw new Error('disposable PostgreSQL did not become ready');
}
async function startDb() {
  await ignore('docker', ['rm', '-f', dbName]);
  try {
    await run('docker', ['network', 'inspect', network], { quiet: true });
  } catch {
    await run('docker', ['network', 'create', network], { quiet: true });
  }
  await run(
    'docker',
    [
      'run',
      '-d',
      '--name',
      dbName,
      '--network',
      network,
      '-e',
      'POSTGRES_PASSWORD=postgres',
      '-e',
      'POSTGRES_DB=ronitnath',
      '-p',
      '127.0.0.1:55432:5432',
      '--memory',
      '1g',
      '--cpus',
      '1',
      postgresImage,
    ],
    { quiet: true },
  );
  await waitForDb();
}
const hostDb = 'postgres://postgres:postgres@127.0.0.1:55432/ronitnath';
const containerDb = `postgres://postgres:postgres@${dbName}:5432/ronitnath`;
const testEnv: NodeJS.ProcessEnv = {
  NODE_ENV: 'test',
  DATABASE_URL: hostDb,
  FLEET_TEST_DATABASE_URL: hostDb,
  ID_KEY: '0123456789abcdef0123456789abcdef',
  AUTH_SECRET: 'delivery-auth-secret-delivery-auth-secret',
  MAIL_DIR: join(process.cwd(), '.delivery/mail'),
  CI: 'true',
};

async function eligibility() {
  const candidate = sha();
  await run('git', ['fetch', '--no-tags', 'origin', 'main']);
  try {
    await run('git', ['merge-base', '--is-ancestor', candidate, 'origin/main'], {
      quiet: true,
    });
  } catch {
    throw new Error(`${candidate} is not reachable from origin/main`);
  }
}

async function staticChecks() {
  await startDb();
  try {
    await mkdir('.delivery/mail', { recursive: true });
    await run('pnpm', ['db:migrate'], { env: testEnv });
    await run('pnpm', ['delivery:workflow:check'], { env: testEnv });
    await run('pnpm', ['typecheck'], { env: testEnv });
    await run('pnpm', ['lint'], { env: testEnv });
    await run('pnpm', ['vitest', 'run', '--maxWorkers=2'], { env: testEnv });
    const email = 'delivery-seed@example.com';
    await run(
      'pnpm',
      ['seed:operator', '--email', email, '--password', 'a-long-enough-password'],
      { env: testEnv, quiet: true },
    );
    const before = await run(
      'docker',
      [
        'exec',
        dbName,
        'psql',
        '-U',
        'postgres',
        '-d',
        'ronitnath',
        '-Atc',
        `select p.id||':'||count(r.id) from auth."user" u join person p on p.user_id=u.id left join relation r on r.subject_id=p.id and r.verb='operator' where u.email='${email}' group by p.id`,
      ],
      { quiet: true },
    );
    await run(
      'pnpm',
      ['seed:operator', '--email', email, '--password', 'a-long-enough-password'],
      { env: testEnv, quiet: true },
    );
    const after = await run(
      'docker',
      [
        'exec',
        dbName,
        'psql',
        '-U',
        'postgres',
        '-d',
        'ronitnath',
        '-Atc',
        `select p.id||':'||count(r.id) from auth."user" u join person p on p.user_id=u.id left join relation r on r.subject_id=p.id and r.verb='operator' where u.email='${email}' group by p.id`,
      ],
      { quiet: true },
    );
    if (!before || before !== after || !before.endsWith(':1'))
      throw new Error(`operator seeding was not idempotent (${before} -> ${after})`);
  } finally {
    await ignore('docker', ['rm', '-f', dbName]);
  }
}

async function build() {
  await mkdir('.delivery', { recursive: true });
  const common = [
    'buildx',
    'build',
    '--load',
    '--build-arg',
    `APP_VERSION=${sha()}`,
    '--label',
    'org.opencontainers.image.source=https://github.com/RonitNath/ronitnath',
    '--secret',
    'id=npm_token,env=FLEET_PACKAGES_TOKEN',
  ];
  await run('docker', [...common, '--target', 'runtime', '-t', tag('runtime'), '.']);
  await run('docker', [...common, '--target', 'migrate', '-t', tag('migrate'), '.']);
}

async function buildCompatibilityArtifacts(): Promise<string> {
  const directory = await mkdtemp('/tmp/ronitnath-compatibility-');
  const archive = join(directory, 'source.tar');
  await run('git', ['archive', '--format=tar', `--output=${archive}`, compatibilitySha]);
  await run('tar', ['-xf', archive, '-C', directory]);
  const common = [
    'buildx',
    'build',
    '--load',
    '--build-arg',
    `APP_VERSION=${compatibilitySha}`,
    '--secret',
    'id=npm_token,env=FLEET_PACKAGES_TOKEN',
  ];
  await run(
    'docker',
    [...common, '--target', 'runtime', '-t', compatibilityTag('runtime'), '.'],
    { cwd: directory },
  );
  await run(
    'docker',
    [...common, '--target', 'migrate', '-t', compatibilityTag('migrate'), '.'],
    { cwd: directory },
  );
  return directory;
}

async function startWeb(image: string) {
  await ignore('docker', ['rm', '-f', webName]);
  await run(
    'docker',
    [
      'run',
      '-d',
      '--name',
      webName,
      '--network',
      network,
      '--memory',
      '2g',
      '--cpus',
      '2',
      '-e',
      `DATABASE_URL=${containerDb}`,
      '-e',
      `ID_KEY=${testEnv.ID_KEY}`,
      '-e',
      `AUTH_SECRET=${testEnv.AUTH_SECRET}`,
      '-e',
      'PUBLIC_ORIGIN=http://localhost:3140',
      '-e',
      'MAIL_DIR=/app/.mail',
      '-v',
      `${join(process.cwd(), '.delivery/mail')}:/app/.mail`,
      image,
    ],
    { quiet: true },
  );
  for (let attempt = 0; attempt < 60; attempt++) {
    try {
      await run('docker', ['exec', webName, 'curl', '-fsS', 'http://127.0.0.1:3140/readyz'], {
        quiet: true,
      });
      return;
    } catch {
      if (attempt === 59) throw new Error(`${image} did not become ready`);
      await new Promise((resolve) => setTimeout(resolve, 1000));
    }
  }
}

async function exercisePreviousWriter() {
  const id = '10000000-0000-4000-8000-000000000001';
  const payload = JSON.stringify({
    action: 'register',
    view: {
      visitorId: id,
      tabId: '20000000-0000-4000-8000-000000000002',
      viewId: '30000000-0000-4000-8000-000000000003',
      path: '/',
      title: 'Compatibility probe',
      subscriptions: [],
      viewport: { visible: true, sectionIds: [], rowIds: [] },
      interactedAt: new Date().toISOString(),
    },
  });
  await run(
    'docker',
    [
      'exec',
      webName,
      'curl',
      '-fsS',
      '-H',
      'content-type: application/json',
      '--data-binary',
      payload,
      'http://127.0.0.1:3140/api/realtime',
    ],
    { quiet: true },
  );
}

async function artifact() {
  await startDb();
  await mkdir('.delivery/mail', { recursive: true });
  let compatibilityDirectory: string | undefined;
  try {
    compatibilityDirectory = await buildCompatibilityArtifacts();
    await run('docker', [
      'run',
      '--rm',
      '--network',
      network,
      '-e',
      `DATABASE_URL=${containerDb}`,
      compatibilityTag('migrate'),
    ]);
    await run('docker', [
      'run',
      '--rm',
      '--network',
      network,
      '-e',
      `DATABASE_URL=${containerDb}`,
      '-e',
      `ID_KEY=${testEnv.ID_KEY}`,
      compatibilityTag('migrate'),
      'pnpm',
      'seed:operator',
      '--email',
      'artifact-operator@example.com',
      '--password',
      'a-long-enough-password',
    ]);
    await run('docker', [
      'run',
      '--rm',
      '--network',
      network,
      '-e',
      `DATABASE_URL=${containerDb}`,
      tag('migrate'),
    ]);
    await startWeb(compatibilityTag('runtime'));
    await exercisePreviousWriter();
    await run('docker', [
      'run',
      '--rm',
      '--network',
      network,
      '-e',
      `DATABASE_URL=${containerDb}`,
      '-e',
      `ID_KEY=${testEnv.ID_KEY}`,
      compatibilityTag('migrate'),
      'pnpm',
      'seed:operator',
      '--email',
      'artifact-operator@example.com',
      '--password',
      'a-long-enough-password',
    ]);
    await startWeb(tag('runtime'));
    await run('docker', [
      'run',
      '--rm',
      '--network',
      `container:${webName}`,
      '-v',
      `${process.cwd()}:/work`,
      '-w',
      '/work',
      '-e',
      `DATABASE_URL=${containerDb}`,
      '-e',
      'E2E_BASE_URL=http://localhost:3140',
      '-e',
      'MAIL_DIR=/work/.delivery/mail',
      playwrightImage,
      'bash',
      '-lc',
      './node_modules/.bin/playwright test --config playwright.delivery.config.ts',
    ]);
    await startWeb(compatibilityTag('runtime'));
    await exercisePreviousWriter();
  } finally {
    await ignore('docker', ['rm', '-f', webName]);
    await ignore('docker', ['rm', '-f', dbName]);
    if (compatibilityDirectory)
      await rm(compatibilityDirectory, { recursive: true, force: true });
  }
}

function digestFromInspect(output: string): string {
  const found = output.match(/Digest:\s+(sha256:[a-f0-9]{64})/);
  if (!found) throw new Error('registry did not return an OCI digest');
  return found[1]!;
}
async function publish() {
  await run('docker', ['push', tag('runtime')]);
  await run('docker', ['push', tag('migrate')]);
  const runtimeDigest = digestFromInspect(
    await run('docker', ['buildx', 'imagetools', 'inspect', tag('runtime')], { quiet: true }),
  );
  const migrateDigest = digestFromInspect(
    await run('docker', ['buildx', 'imagetools', 'inspect', tag('migrate')], { quiet: true }),
  );
  const journal = JSON.parse(await readFile('drizzle/meta/_journal.json', 'utf8')) as {
    entries: { tag: string }[];
  };
  const migrationChecksums = Object.fromEntries(
    await Promise.all(
      journal.entries.map(async ({ tag }) => [
        tag,
        createHash('sha256')
          .update(await readFile(`drizzle/${tag}.sql`))
          .digest('hex'),
      ]),
    ),
  );
  await writeFile(
    '.delivery/artifacts.json',
    JSON.stringify(
      {
        schemaVersion: 1,
        requestedSha: sha(),
        runtime: `${repository}@${runtimeDigest}`,
        migrate: `${repository}@${migrateDigest}`,
        migrationChecksums,
      },
      null,
      2,
    ),
  );
}

async function deployKey(): Promise<string> {
  const path = join(process.cwd(), '.delivery/deploy-key');
  await writeFile(path, process.env.DEPLOY_SSH_KEY ?? '', { mode: 0o600 });
  return path;
}
async function remote(host: string, args: string[]) {
  const key = await deployKey();
  return run('ssh', [
    '-i',
    key,
    '-o',
    'IdentitiesOnly=yes',
    '-o',
    'StrictHostKeyChecking=accept-new',
    '-o',
    'UserKnownHostsFile=.delivery/known-hosts',
    `root@${host}`,
    ...args,
  ]);
}
async function artifacts(): Promise<{ runtime: string; migrate: string }> {
  return JSON.parse(await readFile('.delivery/artifacts.json', 'utf8')) as {
    runtime: string;
    migrate: string;
  };
}
async function productionPreflight() {
  const value = await artifacts();
  for (const host of ['nyc', 'sfo'])
    await remote(host, ['preflight', sha(), value.runtime, value.migrate]);
}
async function migrate() {
  const value = await artifacts();
  await remote('nyc', ['migrate', sha(), value.migrate]);
}
async function roll(host: string) {
  const value = await artifacts();
  await remote(host, ['roll', sha(), value.runtime]);
}
async function acceptance() {
  const response = await fetch('https://ronitnath.com/healthz', { cache: 'no-store' });
  if (!response.ok) throw new Error(`public health returned ${response.status}`);
  const health = (await response.json()) as { ok?: boolean; version?: string };
  if (!health.ok || health.version !== sha())
    throw new Error(`public version mismatch: ${JSON.stringify(health)}`);
  const denied = await fetch('https://ronitnath.com/o/isoastra/delivery', {
    redirect: 'manual',
  });
  if (denied.status !== 404)
    throw new Error(`anonymous delivery inspector returned ${denied.status}`);
  const endpoint = 'https://ronitnath.com/api/delivery/events';
  const token = process.env.DELIVERY_INGEST_TOKEN;
  if (token)
    await flushTelemetry({
      endpoint,
      token,
      bufferPath: '.delivery/telemetry-buffer.jsonl',
    }).catch((error) => console.warn(`telemetry reconciliation remains buffered: ${error}`));
}
async function cleanup() {
  await ignore('docker', ['rm', '-f', webName]);
  await ignore('docker', ['rm', '-f', dbName]);
  await ignore('docker', ['network', 'rm', network]);
}

async function main() {
  const [command, argument] = process.argv.slice(2);
  const commands: Record<string, () => Promise<void>> = {
    eligibility,
    static: staticChecks,
    build,
    artifact,
    publish,
    'production-preflight': productionPreflight,
    migrate,
    acceptance,
    cleanup,
    roll: () => roll(argument ?? ''),
  };
  if (!command || !commands[command])
    throw new Error(`unknown delivery command: ${command ?? ''}`);
  await commands[command]();
}

void main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
