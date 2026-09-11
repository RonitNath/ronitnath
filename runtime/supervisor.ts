import { fork, type ChildProcess } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import type { ChildMessage, LifecycleState, ParentMessage } from '@isoastra/fleet-runtime';
import { runtimeConfig } from './config';

type Role = 'http' | 'worker';
const roles: Role[] = ['http', 'worker'];
const children = new Map<Role, ChildProcess>();
const states = new Map<Role, LifecycleState>(roles.map((role) => [role, 'starting']));
const heartbeats = new Map<Role, number>(roles.map((role) => [role, Date.now()]));
let shutdown: Promise<never> | null = null;

function childEntry(role: Role): string {
  const extension = fileURLToPath(import.meta.url).endsWith('.ts') ? 'ts' : 'mjs';
  return join(dirname(fileURLToPath(import.meta.url)), `${role}.${extension}`);
}

function prefix(role: Role, stream: NodeJS.ReadableStream | null, target: NodeJS.WriteStream): void {
  if (!stream) return;
  let pending = '';
  stream.setEncoding('utf8');
  stream.on('data', (chunk: string) => {
    pending += chunk;
    const lines = pending.split('\n');
    pending = lines.pop() ?? '';
    for (const line of lines) target.write(`[${role}] ${line}\n`);
  });
  stream.on('end', () => { if (pending) target.write(`[${role}] ${pending}\n`); });
}

function validMessage(value: unknown): value is ChildMessage {
  if (!value || typeof value !== 'object') return false;
  const message = value as Record<string, unknown>;
  if (message.role !== 'http' && message.role !== 'worker') return false;
  if (message.type === 'heartbeat') return typeof message.at === 'number' && Number.isFinite(message.at);
  return message.type === 'state' && ['starting', 'ready', 'draining', 'stopped', 'failed'].includes(String(message.state));
}

function tell(role: Role, message: ParentMessage): void {
  const child = children.get(role);
  if (child?.connected) child.send(message);
}

function publishPeer(role: Role, state: LifecycleState): void {
  for (const peer of roles) if (peer !== role) tell(peer, { type: 'peer', role, state });
}

function spawn(role: Role): ChildProcess {
  const child = fork(childEntry(role), [], {
    cwd: process.cwd(),
    env: { ...process.env, RUNTIME_ROLE: role },
    stdio: ['inherit', 'pipe', 'pipe', 'ipc'],
  });
  children.set(role, child);
  process.stdout.write(`[supervisor] ${role} pid=${child.pid ?? 'unknown'}\n`);
  prefix(role, child.stdout, process.stdout);
  prefix(role, child.stderr, process.stderr);
  child.on('message', (message: unknown) => {
    if (!validMessage(message) || message.role !== role) return;
    if (message.type === 'heartbeat') heartbeats.set(role, message.at);
    else {
      states.set(role, message.state);
      publishPeer(role, message.state);
    }
  });
  child.once('exit', (code, signal) => {
    if (!shutdown) void beginShutdown(`${role} exited (${code ?? signal ?? 'unknown'})`, true);
  });
  return child;
}

function waitForExit(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
  return new Promise((resolve) => child.once('exit', () => resolve()));
}

function beginShutdown(reason: string, failure: boolean): Promise<never> {
  if (shutdown) return shutdown;
  shutdown = (async () => {
    const deadline = Date.now() + 45_000;
    clearInterval(monitor);
    clearTimeout(startup);
    process.stderr.write(`[supervisor] drain: ${reason}\n`);
    for (const role of roles) {
      states.set(role, 'draining');
      publishPeer(role, 'draining');
    }
    // Stop accepting and finish HTTP before asking DBOS to use its own
    // workflow-completion budget. Both phases share the container deadline.
    tell('http', { type: 'drain', reason, deadline });
    await Promise.race([
      waitForExit(children.get('http')!),
      new Promise<void>((resolve) => setTimeout(resolve, Math.max(0, deadline - Date.now()))),
    ]);
    tell('worker', { type: 'drain', reason, deadline });
    const exits = Promise.all([...children.values()].map(waitForExit));
    const timedOut = await Promise.race([
      exits.then(() => false),
      new Promise<true>((resolve) => setTimeout(() => resolve(true), Math.max(0, deadline - Date.now()))),
    ]);
    if (timedOut) {
      process.stderr.write('[supervisor] shutdown deadline exceeded; killing children\n');
      for (const child of children.values()) if (child.exitCode === null) child.kill('SIGKILL');
      await Promise.all([...children.values()].map(waitForExit));
    }
    process.exit(failure || timedOut ? 1 : 0);
  })() as Promise<never>;
  return shutdown;
}

runtimeConfig();
const startup = setTimeout(() => {
  if (roles.some((role) => states.get(role) !== 'ready')) void beginShutdown('startup deadline exceeded', true);
}, 120_000);
const monitor = setInterval(() => {
  const now = Date.now();
  for (const role of roles) {
    if (now - (heartbeats.get(role) ?? 0) > 10_000) {
      void beginShutdown(`${role} heartbeat expired`, true);
      break;
    }
  }
}, 2_000);
for (const role of roles) spawn(role);
process.once('SIGTERM', () => { void beginShutdown('SIGTERM', false); });
process.once('SIGINT', () => { void beginShutdown('SIGINT', false); });
