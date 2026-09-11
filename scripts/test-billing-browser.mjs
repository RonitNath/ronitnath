import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';

const pgBin = process.env.PG_BIN ?? '/opt/homebrew/opt/postgresql@16/bin';
const data = await mkdtemp(join(tmpdir(), 'ronitnath-billing-browser-pg-'));
const port = 40000 + Math.floor(Math.random() * 10000);
const run = (command, args, env = process.env) => new Promise((resolve, reject) => {
  const child = spawn(command, args, { stdio: 'inherit', env });
  child.once('error', reject);
  child.once('exit', (code) => code === 0 ? resolve() : reject(new Error(`${command} exited ${code}`)));
});
try {
  await run(`${pgBin}/initdb`, ['-D', data, '--no-locale', '--encoding=UTF8', '--auth=trust']);
  await run(`${pgBin}/pg_ctl`, ['-D', data, '-o', `-h 127.0.0.1 -p ${port} -c fsync=on -c synchronous_commit=on -c full_page_writes=on`, '-w', 'start']);
  const env = { ...process.env, DATABASE_URL: `postgresql://${process.env.USER}@127.0.0.1:${port}/postgres`, ID_KEY: '0123456789abcdef0123456789abcdef', AUTH_SECRET: 'billing-browser-test-secret-at-least-32', PUBLIC_ORIGIN: 'http://127.0.0.1:3141' };
  await run('pnpm', ['db:migrate'], env);
  await run('pnpm', ['playwright', 'test', '--config', 'playwright.billing.config.ts', 'e2e/billing.spec.ts'], env);
} finally {
  await run(`${pgBin}/pg_ctl`, ['-D', data, '-m', 'fast', 'stop']).catch(() => {});
  await rm(data, { recursive: true, force: true });
}
