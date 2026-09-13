import { execFile } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { createHash, randomUUID } from 'node:crypto';
import { manifestDigest } from '@isoastra/fleet-delivery/node';

const exec = promisify(execFile);
const dbName = 'ronitnath-delivery-db';
const network = 'ronitnath-delivery';
const webName = 'ronitnath-delivery-web';
const postgresImage =
  'postgres:17-alpine@sha256:18cfe3ef5e6815560c98237d6216d1e5119702fb0f3894c8785dd58b8bbe5d73';
const playwrightImage =
  'mcr.microsoft.com/playwright:v1.63.0-noble@sha256:eff16c30e6f3f4af0a03fa4b706120d5e9b0891c344a27d64559aff5900a4a27';
const repository = 'ghcr.io/ronitnath/ronitnath-app';
interface Baseline {
  agreed: boolean;
  runtime: {
    repository: string;
    tag: string;
    digest: `sha256:${string}`;
    sourceSha: string;
    platform: string;
    sizeBytes: number | null;
  };
  migration: {
    repository: string;
    tag: string;
    digest: `sha256:${string}`;
    sourceSha: string;
    platform: string;
    sizeBytes: number | null;
  };
  schemaChecksums: Record<string, string>;
}
interface MigrationPolicySource {
  schemaVersion: 2;
  entries: Array<{
    name: string;
    mode: 'expand' | 'transition' | 'contract';
    transactional: boolean;
    lockTimeoutMs: number;
    statementTimeoutMs: number;
    backfill: { required: boolean; complete: boolean; probe: string | null };
  }>;
}

function sha(): string {
  const value = process.env.GITHUB_SHA ?? process.env.DELIVERY_SHA;
  if (!value || !/^[a-f0-9]{40}$/.test(value))
    throw new Error('GITHUB_SHA or DELIVERY_SHA must be a full SHA');
  return value;
}
function tag(kind: 'runtime' | 'migrate'): string {
  return `${repository}:${sha()}${kind === 'migrate' ? '-migrate' : ''}`;
}
async function baseline(): Promise<Baseline> {
  const value = JSON.parse(await readFile('.delivery/baseline.json', 'utf8')) as Baseline;
  if (!value.agreed || !value.runtime?.digest || !value.migration?.digest)
    throw new Error('production replicas do not have an agreed, testable baseline');
  return value;
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
  DELIVERY_ACCEPTANCE_EMAIL: 'delivery-seed@example.com',
  DELIVERY_ACCEPTANCE_TOKEN: 'delivery-acceptance-test-token',
  MAIL_DIR: join(process.cwd(), '.delivery/mail'),
  CI: 'true',
};

async function eligibility() {
  const candidate = sha();
  if (process.env.CI === 'true') await run('fleet-delivery-verify', []);
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
    await run('pnpm', ['vitest', 'run', '--maxWorkers=2'], { env: testEnv });
    await run('pnpm', ['delivery:protocol:test'], { env: testEnv });
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

async function seedArtifactOperator() {
  await run(
    'pnpm',
    [
      'seed:operator',
      '--email',
      'artifact-operator@example.com',
      '--password',
      'a-long-enough-password',
    ],
    { env: testEnv, quiet: true },
  );
}

async function artifact() {
  await startDb();
  await mkdir('.delivery/mail', { recursive: true });
  const previous = await baseline();
  const previousRuntime = `${previous.runtime.repository}@${previous.runtime.digest}`;
  const previousMigration = `${previous.migration.repository}@${previous.migration.digest}`;
  try {
    await run('docker', ['pull', previousRuntime]);
    await run('docker', ['pull', previousMigration]);
    await run('docker', [
      'run',
      '--rm',
      '--network',
      network,
      '-e',
      `DATABASE_URL=${containerDb}`,
      previousMigration,
    ]);
    await seedArtifactOperator();
    await run('docker', [
      'run',
      '--rm',
      '--network',
      network,
      '-e',
      `DATABASE_URL=${containerDb}`,
      tag('migrate'),
    ]);
    await startWeb(previousRuntime);
    await exercisePreviousWriter();
    await seedArtifactOperator();
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
    await startWeb(previousRuntime);
    await exercisePreviousWriter();
    await run('docker', [
      'run',
      '--rm',
      '--network',
      network,
      '-e',
      `DATABASE_URL=${containerDb}`,
      tag('migrate'),
    ]);
  } finally {
    await ignore('docker', ['rm', '-f', webName]);
    await ignore('docker', ['rm', '-f', dbName]);
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
  const previous = await baseline();
  const policy = JSON.parse(
    await readFile('delivery.migrations.json', 'utf8'),
  ) as MigrationPolicySource;
  const runtime = {
    repository,
    tag: tag('runtime'),
    digest: runtimeDigest,
    sourceSha: sha(),
    platform: 'linux/amd64',
    sizeBytes: Number(
      await run('docker', ['image', 'inspect', tag('runtime'), '--format', '{{.Size}}'], {
        quiet: true,
      }),
    ),
  };
  const migration = {
    repository,
    tag: tag('migrate'),
    digest: migrateDigest,
    sourceSha: sha(),
    platform: 'linux/amd64',
    sizeBytes: Number(
      await run('docker', ['image', 'inspect', tag('migrate'), '--format', '{{.Size}}'], {
        quiet: true,
      }),
    ),
  };
  const body = {
    schemaVersion: 2 as const,
    releaseId: process.env.DELIVERY_RELEASE_ID ?? randomUUID(),
    requestedSha: sha(),
    createdAt: new Date().toISOString(),
    artifacts: { runtime, migration },
    migrations: {
      schemaVersion: 2 as const,
      entries: policy.entries.map((entry) => ({
        ...entry,
        checksum: migrationChecksums[entry.name]!,
        compatibleRuntimeDigests: [previous.runtime.digest],
      })),
    },
    compatibility: {
      previousRuntime: previous.runtime,
      previousMigration: previous.migration,
      previousSchemaChecksums: previous.schemaChecksums,
      candidateSchemaChecksums: migrationChecksums,
      previousRead: true,
      previousWrite: true,
      candidateRead: true,
      candidateWrite: true,
      rollbackRead: true,
      repeatedMigration: true,
      testedAt: new Date().toISOString(),
    },
    previousRuntimeDigest: previous.runtime.digest,
  };
  const release = { ...body, manifestDigest: manifestDigest(body) };
  await writeFile(
    '.delivery/artifacts.json',
    JSON.stringify(
      {
        schemaVersion: 2,
        requestedSha: sha(),
        runtime: `${repository}@${runtimeDigest}`,
        migrate: `${repository}@${migrateDigest}`,
        migrationChecksums,
      },
      null,
      2,
    ),
  );
  await writeFile('.delivery/release.json', JSON.stringify(release, null, 2));
}

async function cleanup() {
  await ignore('docker', ['rm', '-f', webName]);
  await ignore('docker', ['rm', '-f', dbName]);
  await ignore('docker', ['network', 'rm', network]);
}

async function main() {
  const [command] = process.argv.slice(2);
  const commands: Record<string, () => Promise<void>> = {
    eligibility,
    static: staticChecks,
    build,
    artifact,
    publish,
    cleanup,
  };
  if (!command || !commands[command])
    throw new Error(`unknown delivery command: ${command ?? ''}`);
  await commands[command]();
}

void main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
