import { execFile } from 'node:child_process';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { promisify } from 'node:util';
import { defineApplication } from '@isoastra/fleet-delivery';
import { planEnvironment } from '@isoastra/fleet-delivery/environment';
import { IngressRegistryClient } from '@isoastra/fleet-delivery/ingress-node';
import { issueIngressLease } from '@isoastra/fleet-delivery/ingress';

const exec = promisify(execFile);
const postgresImage =
  'postgres:17-alpine@sha256:18cfe3ef5e6815560c98237d6216d1e5119702fb0f3894c8785dd58b8bbe5d73';
const hostCli = '/opt/fleet-delivery/0.3.8/dist/cli.js';
const knownHosts = '/etc/fleet-ingress/known_hosts';
const sshKey = '/var/lib/secrets/fleet-ingress/client-ssh';

interface Artifacts {
  schemaVersion: number;
  requestedSha: string;
  runtime: string;
  migrate: string;
  migrationChecksums: Record<string, string>;
}

async function run(file: string, args: string[], options: { quiet?: boolean } = {}) {
  const result = await exec(file, args, { maxBuffer: 4 * 1024 * 1024 });
  if (!options.quiet && result.stdout) process.stdout.write(result.stdout);
  if (!options.quiet && result.stderr) process.stderr.write(result.stderr);
  return result.stdout.trim();
}
async function ignore(file: string, args: string[]) {
  try { await run(file, args, { quiet: true }); } catch {}
}
function argument(name: string): string {
  const index = process.argv.indexOf(name);
  const value = index < 0 ? undefined : process.argv[index + 1];
  if (!value) throw new Error(`${name} is required`);
  return value;
}
function secret(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value;
}
async function ownedRemove(name: string, namespace: string) {
  try {
    const owner = await run('docker', ['inspect', name, '--format', '{{index .Config.Labels "io.isoastra.environment.namespace"}}'], { quiet: true });
    if (owner !== namespace) throw new Error(`refusing to remove ${name}: ownership label is ${owner || 'missing'}`);
    await run('docker', ['rm', '-f', name], { quiet: true });
  } catch (error) {
    if (error instanceof Error && error.message.startsWith('refusing')) throw error;
  }
}
async function ready(container: string) {
  for (let attempt = 0; attempt < 90; attempt++) {
    try {
      await run('docker', ['exec', container, 'curl', '-fsS', 'http://127.0.0.1:3140/readyz'], { quiet: true });
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 1_000));
    }
  }
  throw new Error('environment web service did not become ready');
}

async function main() {
  const profile = argument('--profile');
  if (profile !== 'preview' && profile !== 'staging') throw new Error('profile must be preview or staging');
  const definition = defineApplication(JSON.parse(await readFile('delivery.application.json', 'utf8')));
  const ref = secret('GITHUB_REF_NAME');
  const sha = secret('GITHUB_SHA');
  const owner = `github:${secret('GITHUB_REPOSITORY')}:${ref}`;
  const planned = planEnvironment(definition, { profile, owner, workspace: `${definition.name}:${ref}` });
  const namespace = planned.namespace;
  // The runner's Nix StateDirectory is explicitly writable inside its hardened
  // mount namespace. Keep durable environment state there so recovery and the
  // host reaper observe the same files.
  const root = `/var/lib/github-runner/ronitnath/environments/${namespace}`;
  const artifacts = JSON.parse(await readFile('.delivery/artifacts.json', 'utf8')) as Artifacts;
  if (artifacts.requestedSha !== sha || !artifacts.runtime.includes('@sha256:') || !artifacts.migrate.includes('@sha256:'))
    throw new Error('environment artifacts are not bound to this candidate');
  await mkdir(root, { recursive: true, mode: 0o700 });
  const unit = `fleet-environment-${namespace}`;
  await ignore('systemctl', ['--user', 'stop', unit]);
  const db = `${namespace}-database`, web = `${namespace}-web`;
  await ownedRemove(web, namespace); await ownedRemove(db, namespace);
  const networkExists = await run('docker', ['network', 'ls', '--filter', `name=^${namespace}$`, '--format', '{{.Name}}'], { quiet: true });
  if (networkExists) {
    const ownerLabel = await run('docker', ['network', 'inspect', namespace, '--format', '{{index .Labels "io.isoastra.environment.namespace"}}'], { quiet: true });
    if (ownerLabel !== namespace) throw new Error('refusing to reuse an unowned network');
  } else await run('docker', ['network', 'create', '--label', `io.isoastra.environment.namespace=${namespace}`, namespace], { quiet: true });
  const volume = `${namespace}-postgres`;
  const volumeExists = await run('docker', ['volume', 'ls', '--filter', `name=^${volume}$`, '--format', '{{.Name}}'], { quiet: true });
  if (volumeExists) {
    const ownerLabel = await run('docker', ['volume', 'inspect', volume, '--format', '{{index .Labels "io.isoastra.environment.namespace"}}'], { quiet: true });
    if (ownerLabel !== namespace) throw new Error('refusing to reuse an unowned volume');
  } else await run('docker', ['volume', 'create', '--label', `io.isoastra.environment.namespace=${namespace}`, volume], { quiet: true });
  const credentialsPath = `${root}/credentials.json`;
  let credentials: { postgres: string; id: string; auth: string };
  try { credentials = JSON.parse(await readFile(credentialsPath, 'utf8')); }
  catch {
    credentials = { postgres: randomBytes(24).toString('hex'), id: randomBytes(32).toString('hex'), auth: randomBytes(32).toString('hex') };
    await writeFile(credentialsPath, JSON.stringify(credentials), { mode: 0o600 });
  }
  const label = `io.isoastra.environment.namespace=${namespace}`;
  await run('docker', ['run', '-d', '--name', db, '--network', namespace, '--label', label, '--memory', '1g', '--cpus', '1', '-v', `${volume}:/var/lib/postgresql/data`, '-e', `POSTGRES_PASSWORD=${credentials.postgres}`, '-e', 'POSTGRES_DB=ronitnath', postgresImage], { quiet: true });
  for (let attempt = 0; attempt < 60; attempt++) {
    try { await run('docker', ['exec', db, 'pg_isready', '-U', 'postgres', '-d', 'ronitnath'], { quiet: true }); break; }
    catch { if (attempt === 59) throw new Error('environment PostgreSQL did not become ready'); await new Promise((resolve) => setTimeout(resolve, 500)); }
  }
  const databaseUrl = `postgres://postgres:${credentials.postgres}@${db}:5432/ronitnath`;
  await run('docker', ['run', '--rm', '--network', namespace, '-e', `DATABASE_URL=${databaseUrl}`, artifacts.migrate]);
  const hostname = `${namespace}.rdndev.com`;
  await run('docker', ['run', '-d', '--name', web, '--network', namespace, '--label', label, '--memory', '2g', '--cpus', '2', '-p', '127.0.0.1::3140', '-e', `DATABASE_URL=${databaseUrl}`, '-e', `ID_KEY=${credentials.id}`, '-e', `AUTH_SECRET=${credentials.auth}`, '-e', `PUBLIC_ORIGIN=https://${hostname}`, '-e', `APP_VERSION=${sha}`, '-e', 'NODE_ENV=production', artifacts.runtime], { quiet: true });
  await ready(web);
  const port = Number(await run('docker', ['port', web, '3140/tcp'], { quiet: true }).then((value) => value.match(/:(\d+)$/)?.[1]));
  if (!Number.isInteger(port)) throw new Error('Docker did not publish the environment port');
  const lease = issueIngressLease({
    leaseId: randomUUID(), instanceId: planned.instanceId, owner, bindingId: 'web', provider: 'ronitnath', exclusive: false,
    hostname, target: `http://127.0.0.1:${port}`,
    exposedPaths: definition.exposures.map(({ path, kind, authentication }) => ({ path, kind, authentication })),
  });
  const client = new IngressRegistryClient({ endpoint: 'https://ingress.rdndev.com', token: secret('ENVIRONMENT_INGRESS_TOKEN') });
  const registration = await client.register(lease);
  const registrationPath = `${root}/registration.json`, environmentPath = `${root}/environment.json`, clientEnvironment = `${root}/ingress.env`;
  await writeFile(registrationPath, JSON.stringify(registration, null, 2), { mode: 0o600 });
  await writeFile(clientEnvironment, `INGRESS_REGISTRY_URL=https://ingress.rdndev.com\nINGRESS_REGISTRY_TOKEN=${secret('ENVIRONMENT_INGRESS_TOKEN')}\nINGRESS_SSH_KEY_PATH=${sshKey}\nINGRESS_KNOWN_HOSTS_PATH=${knownHosts}\nINGRESS_LOCAL_PORT=${port}\n`, { mode: 0o600 });
  await run('systemd-run', ['--user', `--unit=${unit}`, '--property=Restart=on-failure', `--property=EnvironmentFile=${clientEnvironment}`, '/run/current-system/sw/bin/node', hostCli, 'ingress-maintain', registrationPath]);
  const instance = { ...planned, state: 'ready', updatedAt: new Date().toISOString(), resources: planned.resources.map((resource) => resource.service === 'web' ? { ...resource, port } : resource), exposures: planned.exposures.map((exposure) => ({ ...exposure, leaseId: lease.leaseId })) };
  const state = { schemaVersion: 3, namespace, sha, profile, unit, containers: [db, web], artifacts, instance, registrationPath, artifactFingerprint: createHash('sha256').update(JSON.stringify(artifacts)).digest('hex') };
  await writeFile(environmentPath, JSON.stringify(state, null, 2), { mode: 0o600 });
  const basic = Buffer.from(`environment:${registration.browserAccess.password}`).toString('base64');
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      const response = await fetch(`https://${hostname}/readyz`, { headers: { authorization: `Basic ${basic}` }, signal: AbortSignal.timeout(2_000) });
      if (response.ok) break;
      if (attempt === 29) throw new Error(`public environment returned ${response.status}`);
    } catch (error) { if (attempt === 29) throw error; await new Promise((resolve) => setTimeout(resolve, 1_000)); }
  }
  const telemetryUrl = process.env.DELIVERY_TELEMETRY_URL, telemetryToken = process.env.DELIVERY_INGEST_TOKEN;
  if (telemetryUrl && telemetryToken) {
    const event = { schemaVersion: 3, instanceId: instance.instanceId, producerId: `environment:${namespace}`, seq: 1, at: new Date().toISOString(), type: 'environment-state', instance, services: definition.services.map(({ id }) => ({ service: id, state: 'ready', detail: null })), reporting: 'connected' };
    const response = await fetch(telemetryUrl, { method: 'POST', headers: { authorization: `Bearer ${telemetryToken}`, 'content-type': 'application/json' }, body: JSON.stringify(event), signal: AbortSignal.timeout(2_000) });
    if (!response.ok) console.warn(`environment telemetry returned ${response.status}`);
  }
  console.log(JSON.stringify({ namespace, profile, sha, url: `https://${hostname}/`, expiresAt: instance.expiresAt }, null, 2));
}

void main().catch((error: unknown) => { console.error(error instanceof Error ? error.message : error); process.exitCode = 1; });
