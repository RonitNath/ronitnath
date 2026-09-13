import { execFile as execFileCallback, spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';

const execFile = promisify(execFileCallback);
const pgBin = process.env.TEST_PG_BIN ?? '/opt/homebrew/opt/postgresql@16/bin';
const scratch = await mkdtemp(join(tmpdir(), 'ronitnath-playwright-'));
const socket = createServer();
await new Promise((resolve) => socket.listen(0, '127.0.0.1', resolve));
const address = socket.address();
const port = typeof address === 'object' && address ? address.port : 0;
await new Promise((resolve) => socket.close(resolve));
const data = join(scratch, 'data');
let started = false;
let child;
let interrupted = false;
const inherited = Object.fromEntries(['PATH', 'HOME', 'TMPDIR', 'LANG', 'LC_ALL'].flatMap((key) =>
  process.env[key] === undefined ? [] : [[key, process.env[key]]]));
function stopChild(signal = 'SIGTERM') {
  if (!child?.pid || child.exitCode !== null) return;
  try { process.kill(-child.pid, signal); } catch { /* already stopped */ }
}
function interrupt(signal) {
  interrupted = true;
  stopChild(signal);
}
process.once('SIGINT', interrupt);
process.once('SIGTERM', interrupt);
try {
  await execFile(join(pgBin, 'initdb'), ['-D', data, '-A', 'trust', '-U', 'postgres']);
  await execFile(join(pgBin, 'pg_ctl'), ['-D', data, '-o', `-h 127.0.0.1 -p ${port}`, '-l', join(scratch, 'postgres.log'), 'start']);
  started = true;
  await execFile(join(pgBin, 'createdb'), ['-h', '127.0.0.1', '-p', String(port), '-U', 'postgres', 'ronitnath']);
  child = spawn('pnpm', ['exec', 'playwright', 'test', ...process.argv.slice(2)], {
    cwd: process.cwd(), stdio: 'inherit', detached: true,
    env: { ...inherited, E2E_DATABASE_URL: `postgres://postgres@127.0.0.1:${port}/ronitnath`,
      ...(process.env.E2E_SOFTWARE_GL ? { E2E_SOFTWARE_GL: process.env.E2E_SOFTWARE_GL } : {}),
      ...(process.env.E2E_SCREENSHOT ? { E2E_SCREENSHOT: process.env.E2E_SCREENSHOT } : {}) },
  });
  const code = await new Promise((resolve) => child.once('exit', (value) => resolve(value ?? 1)));
  if (code !== 0 && !interrupted) {
    console.error(`Playwright artifacts and database logs retained at ${scratch}`);
    process.exitCode = code;
  }
} finally {
  stopChild('SIGTERM');
  process.removeListener('SIGINT', interrupt);
  process.removeListener('SIGTERM', interrupt);
  if (started) await execFile(join(pgBin, 'pg_ctl'), ['-D', data, 'stop', '-m', 'fast']).catch(() => undefined);
}
