import { mkdtemp } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
import { promisify } from 'node:util';
import { execFile as execFileCallback } from 'node:child_process';
import pg from 'pg';

const execFile = promisify(execFileCallback);
const root = process.cwd();
const pgBin = process.env.TEST_PG_BIN ?? '/opt/homebrew/opt/postgresql@16/bin';
const scratch = await mkdtemp(join(tmpdir(), 'ronitnath-runtime-'));
let postgresStarted = false;
let provider;
const processes = new Set();
const runtimes = [];

function check(value, message) { if (!value) throw new Error(message); }
async function freePort() {
  const server = createServer();
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  await new Promise((resolve) => server.close(resolve));
  return port;
}
async function waitFor(test, message, timeout = 30_000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    try { const value = await test(); if (value) return value; } catch (error) { last = error; }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`${message}${last ? `: ${last}` : ''}`);
}
function run(command, args, env) {
  return execFile(command, args, { cwd: root, env: { ...process.env, ...env }, maxBuffer: 10 * 1024 * 1024 });
}
function launch(env) {
  const child = spawn(process.execPath, ['dist-runtime/runtime/supervisor.mjs'], {
    cwd: root, env: { ...process.env, ...env }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  processes.add(child);
  let output = '';
  child.stdout.on('data', (chunk) => { output += chunk; });
  child.stderr.on('data', (chunk) => { output += chunk; });
  child.once('exit', () => processes.delete(child));
  const runtime = { child, output: () => output };
  runtimes.push(runtime);
  return runtime;
}
function exited(child) {
  return new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
}

try {
  const databasePort = await freePort();
  const webPort = await freePort();
  const mailPort = await freePort();
  const data = join(scratch, 'data');
  await run(join(pgBin, 'initdb'), ['-D', data, '-A', 'trust', '-U', 'postgres']);
  await run(join(pgBin, 'pg_ctl'), ['-D', data, '-o', `-h 127.0.0.1 -p ${databasePort}`, '-l', join(scratch, 'postgres.log'), 'start']);
  postgresStarted = true;
  await run(join(pgBin, 'createdb'), ['-h', '127.0.0.1', '-p', String(databasePort), '-U', 'postgres', 'ronitnath']);
  const databaseUrl = `postgres://postgres@127.0.0.1:${databasePort}/ronitnath`;
  const baseEnv = {
    DATABASE_URL: databaseUrl,
    NODE_ENV: 'production', APP_VERSION: 'runtime-test-v1', REPLICA_ID: 'test-a',
    HOSTNAME: '127.0.0.1', PORT: String(webPort), PUBLIC_ORIGIN: `http://127.0.0.1:${webPort}`,
    AUTH_SECRET: 'isolated-runtime-auth-secret-at-least-32-characters',
    ID_KEY: '00112233445566778899aabbccddeeff', MAIL_INTENT_KEY: 'isolated-mail-intent-key',
    USESEND_API_URL: `http://127.0.0.1:${mailPort}`, USESEND_API_KEY: 'local-provider-key',
  };
  await run('pnpm', ['db:migrate:all'], baseEnv);
  await run('pnpm', ['test'], {
    ...baseEnv,
    TEST_DATABASE_URL: databaseUrl,
    PUBLIC_ORIGIN: 'http://localhost:3000',
  });
  const db = new pg.Pool({ connectionString: databaseUrl, max: 2 });

  let providerCalls = 0;
  let providerMode = 'success';
  const delivered = [];
  let firstRequestResolve;
  const firstRequest = new Promise((resolve) => { firstRequestResolve = resolve; });
  provider = createServer(async (request, response) => {
    let body = '';
    for await (const chunk of request) body += chunk;
    providerCalls += 1;
    if (providerCalls === 1) { firstRequestResolve(); return; }
    if (providerMode === 'transient') { response.writeHead(503).end(); return; }
    if (providerMode === 'permanent') { response.writeHead(400).end(); return; }
    delivered.push(JSON.parse(body));
    response.writeHead(202).end();
  });
  await new Promise((resolve) => provider.listen(mailPort, '127.0.0.1', resolve));

  const first = launch(baseEnv);
  await waitFor(async () => (await fetch(`${baseEnv.PUBLIC_ORIGIN}/readyz`)).ok, 'first runtime did not become ready', 60_000);
  const signup = await fetch(`${baseEnv.PUBLIC_ORIGIN}/api/auth/sign-up/email`, {
    method: 'POST', headers: { 'content-type': 'application/json', origin: baseEnv.PUBLIC_ORIGIN },
    body: JSON.stringify({ name: 'Runtime Test', email: 'runtime@example.test', password: 'correct-horse-battery-staple', callbackURL: '/auth/verify' }),
  });
  check(signup.ok, `signup failed: ${signup.status}`);
  await firstRequest;
  const workerPid = await waitFor(() => /worker pid=(\d+)/.exec(first.output())?.[1], 'worker pid was not reported');
  process.kill(Number(workerPid), 'SIGKILL');
  const firstExit = await exited(first.child);
  check(firstExit.code !== 0, 'supervisor concealed worker failure');

  const recovered = launch(baseEnv);
  await waitFor(async () => (await fetch(`${baseEnv.PUBLIC_ORIGIN}/readyz`)).ok, 'restarted runtime did not become ready', 60_000);
  await waitFor(async () => {
    const result = await db.query("SELECT state,message IS NULL AS purged FROM mail_intent WHERE recipient='runtime@example.test'");
    return result.rows[0]?.state === 'accepted' && result.rows[0]?.purged;
  }, 'mail workflow was not recovered', 60_000);
  check(providerCalls === 2, `expected one interrupted and one recovered provider call, got ${providerCalls}`);
  const verificationUrl = delivered[0]?.text?.match(/https?:\/\/\S+/)?.[0];
  check(verificationUrl, 'delivered verification URL missing');
  const verification = await fetch(verificationUrl, { redirect: 'manual' });
  check(verification.status === 302, `verification returned ${verification.status}`);
  const user = await db.query('SELECT email_verified FROM auth."user" WHERE email=$1', ['runtime@example.test']);
  check(user.rows[0]?.email_verified === true, 'verification token did not verify the account');

  providerMode = 'transient';
  const versionSignup = await fetch(`${baseEnv.PUBLIC_ORIGIN}/api/auth/sign-up/email`, {
    method: 'POST', headers: { 'content-type': 'application/json', origin: baseEnv.PUBLIC_ORIGIN },
    body: JSON.stringify({ name: 'Version Test', email: 'version@example.test', password: 'correct-horse-battery-staple', callbackURL: '/auth/verify' }),
  });
  check(versionSignup.ok, `version test signup failed: ${versionSignup.status}`);
  await waitFor(async () => {
    const result = await db.query(`SELECT 1 FROM mail_attempt a JOIN mail_intent i ON i.id=a.intent_id
      WHERE i.recipient='version@example.test' AND a.outcome='transient'`);
    return result.rowCount === 1;
  }, 'transient workflow attempt missing');
  const recoveredWorker = await waitFor(() => /worker pid=(\d+)/.exec(recovered.output())?.[1], 'recovered worker pid missing');
  process.kill(Number(recoveredWorker), 'SIGKILL');
  check((await exited(recovered.child)).code !== 0, 'version-test crash was concealed');
  const v2 = launch({ ...baseEnv, APP_VERSION: 'runtime-test-v2' });
  await waitFor(async () => (await fetch(`${baseEnv.PUBLIC_ORIGIN}/readyz`)).ok, 'v2 runtime did not become ready', 60_000);
  const callsBeforeV2 = providerCalls;
  await new Promise((resolve) => setTimeout(resolve, 1_500));
  check(providerCalls === callsBeforeV2, 'incompatible v2 process recovered a v1 workflow');
  const v2Exit = exited(v2.child);
  v2.child.kill('SIGTERM');
  check((await v2Exit).code === 0, 'v2 did not shut down cleanly');
  providerMode = 'success';
  const active = launch(baseEnv);
  await waitFor(async () => (await fetch(`${baseEnv.PUBLIC_ORIGIN}/readyz`)).ok, 'compatible v1 runtime did not return', 60_000);
  await waitFor(async () => {
    const result = await db.query("SELECT state FROM mail_intent WHERE recipient='version@example.test'");
    return result.rows[0]?.state === 'accepted';
  }, 'compatible version did not recover its workflow', 60_000);
  providerMode = 'permanent';
  const permanentSignup = await fetch(`${baseEnv.PUBLIC_ORIGIN}/api/auth/sign-up/email`, {
    method: 'POST', headers: { 'content-type': 'application/json', origin: baseEnv.PUBLIC_ORIGIN },
    body: JSON.stringify({ name: 'Permanent Test', email: 'permanent@example.test', password: 'correct-horse-battery-staple', callbackURL: '/auth/verify' }),
  });
  check(permanentSignup.ok, 'permanent-failure signup failed before durable delivery');
  await waitFor(async () => {
    const result = await db.query("SELECT state,message IS NULL AS purged FROM mail_intent WHERE recipient='permanent@example.test'");
    return result.rows[0]?.state === 'failed' && result.rows[0]?.purged;
  }, 'permanent provider failure was not finalized');
  providerMode = 'success';
  await db.query(`INSERT INTO mail_intent(id,deduplication_key,kind,recipient,message,correlation_id,expires_at)
    VALUES('00000000-0000-4000-8000-000000000099','expired-test','verify_email','expired@example.test','{}','test',now()-interval '1 second')`);
  await waitFor(async () => {
    const result = await db.query("SELECT state,message IS NULL AS purged FROM mail_intent WHERE recipient='expired@example.test'");
    return result.rows[0]?.state === 'expired' && result.rows[0]?.purged;
  }, 'expired pending intent was not purged');

  const replicaPort = await freePort();
  const replicaEnv = { ...baseEnv, REPLICA_ID: 'test-b', PORT: String(replicaPort), PUBLIC_ORIGIN: `http://127.0.0.1:${replicaPort}` };
  const replica = launch(replicaEnv);
  await waitFor(async () => (await fetch(`${replicaEnv.PUBLIC_ORIGIN}/readyz`)).ok, 'second replica did not become ready', 60_000);
  const replicaWorker = await waitFor(() => /worker pid=(\d+)/.exec(replica.output())?.[1], 'second replica worker pid missing');
  check(replica.output().includes('ronitnath-test-b'), 'second replica did not use its distinct executor id');
  process.kill(Number(replicaWorker), 'SIGKILL');
  const replicaExit = await exited(replica.child);
  check(replicaExit.code !== 0, 'second replica concealed its worker failure');
  check((await fetch(`${baseEnv.PUBLIC_ORIGIN}/readyz`)).ok, 'one replica failure took down its peer');

  const abort = new AbortController();
  const stream = await fetch(`${baseEnv.PUBLIC_ORIGIN}/api/realtime/stream?visitor=00000000-0000-4000-8000-000000000001&view=00000000-0000-4000-8000-000000000002`, { signal: abort.signal });
  check(stream.ok, `SSE open returned ${stream.status}`);
  const secondExitPromise = exited(active.child);
  const stopStarted = Date.now();
  active.child.kill('SIGTERM');
  const secondExit = await secondExitPromise;
  abort.abort();
  check(secondExit.code === 0, `graceful shutdown exited ${secondExit.code}`);
  check(Date.now() - stopStarted < 45_000, 'graceful shutdown exceeded its runtime budget');
  const combinedLogs = [first, recovered, v2, active, replica].map((runtime) => runtime.output()).join('\n');
  check(!combinedLogs.includes('secret-token') && !combinedLogs.includes('eyJ'), 'token-bearing content reached process logs');
  await db.end();
  console.log(JSON.stringify({ ok: true, providerCalls, recovered: true, versionIsolation: true, verified: true, replicas: true, gracefulShutdownMs: Date.now() - stopStarted }));
} catch (error) {
  console.error(error instanceof Error ? error.stack : String(error));
  for (const [index, runtime] of runtimes.entries()) {
    console.error(`runtime[${index}]\n${runtime.output().slice(-8_000)}`);
  }
  console.error(`runtime integration artifacts retained at ${scratch}`);
  process.exitCode = 1;
} finally {
  for (const child of processes) child.kill('SIGKILL');
  if (provider) await new Promise((resolve) => provider.close(resolve));
  if (postgresStarted) await execFile(join(pgBin, 'pg_ctl'), ['-D', join(scratch, 'data'), 'stop', '-m', 'fast']).catch(() => undefined);
}
