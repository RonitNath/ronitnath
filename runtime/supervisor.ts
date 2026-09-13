import { fork, type ChildProcess } from 'node:child_process';
import { watch, type FSWatcher } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import type { ChildMessage, LifecycleState, ParentMessage } from '@isoastra/fleet-runtime';
import { runtimeConfig } from './config';

if (process.env.NODE_ENV !== 'production') process.env.NEXT_DIST_DIR ??= '.next-dev';

type Role = 'http' | 'worker';
const roles: Role[] = ['http', 'worker'];
const children = new Map<Role, ChildProcess>();
const states = new Map<Role, LifecycleState>(roles.map((role) => [role, 'starting']));
const heartbeats = new Map<Role, number>();
const quiesced = new Set<Role>();
const expectedExits = new WeakSet<ChildProcess>();
let shutdown: Promise<never> | null = null;
let shutdownFailed = false;
let workerReplacement: Promise<void> = Promise.resolve();
let developmentWatcher: FSWatcher | null = null;

function childEntry(role: Role): string {
  const extension = fileURLToPath(import.meta.url).endsWith('.ts') ? 'ts' : 'mjs';
  return join(dirname(fileURLToPath(import.meta.url)), `${role}.${extension}`);
}

function prefix(
  role: Role,
  stream: NodeJS.ReadableStream | null,
  target: NodeJS.WriteStream,
): void {
  if (!stream) return;
  let pending = '';
  stream.setEncoding('utf8');
  stream.on('data', (chunk: string) => {
    pending += chunk;
    const lines = pending.split('\n');
    pending = lines.pop() ?? '';
    for (const line of lines) target.write(`[${role}] ${line}\n`);
  });
  stream.on('end', () => {
    if (pending) target.write(`[${role}] ${pending}\n`);
  });
}

function validMessage(value: unknown): value is ChildMessage {
  if (!value || typeof value !== 'object') return false;
  const message = value as Record<string, unknown>;
  if (message.role !== 'http' && message.role !== 'worker') return false;
  if (message.type === 'heartbeat')
    return typeof message.at === 'number' && Number.isFinite(message.at);
  if (message.type === 'quiesced') return true;
  return (
    message.type === 'state' &&
    ['starting', 'ready', 'draining', 'stopped', 'failed'].includes(String(message.state))
  );
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
    if (message.type === 'heartbeat') heartbeats.set(role, Date.now());
    else if (message.type === 'quiesced') quiesced.add(role);
    else {
      states.set(role, message.state);
      publishPeer(role, message.state);
      if (message.state === 'failed') {
        shutdownFailed = true;
        if (!shutdown) void beginShutdown(`${role} reported failure`, true);
      }
    }
  });
  child.once('error', (error) => {
    shutdownFailed = true;
    if (!shutdown) void beginShutdown(`${role} spawn/IPC error: ${error.message}`, true);
  });
  child.once('exit', (code, signal) => {
    if (expectedExits.has(child)) return;
    if (code !== 0) shutdownFailed = true;
    if (!shutdown) void beginShutdown(`${role} exited (${code ?? signal ?? 'unknown'})`, true);
  });
  return child;
}

async function replaceWorker(reason: string): Promise<void> {
  if (shutdown) return;
  const child = children.get('worker');
  if (!child || child.exitCode !== null) return;
  expectedExits.add(child);
  quiesced.delete('worker');
  states.set('worker', 'draining');
  publishPeer('worker', 'draining');
  const deadline = Date.now() + 25_000;
  const quiesceDeadline = Date.now() + 5_000;
  tell('worker', { type: 'quiesce', reason, deadline });
  while (Date.now() < Math.min(deadline, quiesceDeadline) && !quiesced.has('worker')) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  tell('worker', { type: 'drain', reason, deadline });
  const exited = await Promise.race([
    waitForExit(child).then(() => true),
    new Promise<false>((resolve) =>
      setTimeout(() => resolve(false), Math.max(0, deadline - Date.now())),
    ),
  ]);
  if (!exited) {
    child.kill('SIGKILL');
    await Promise.race([
      waitForExit(child),
      new Promise<void>((resolve) => setTimeout(resolve, 2_000)),
    ]);
  }
  if (shutdown) return;
  quiesced.delete('worker');
  heartbeats.delete('worker');
  states.set('worker', 'starting');
  publishPeer('worker', 'starting');
  spawn('worker');
}

function watchWorkerSources(): void {
  if (process.env.NODE_ENV === 'production') return;
  let timer: NodeJS.Timeout | null = null;
  developmentWatcher = watch(process.cwd(), { recursive: true }, (_event, filename) => {
    const path = filename?.toString().replaceAll('\\', '/');
    if (
      !path ||
      (!path.startsWith('runtime/worker.') &&
        !path.startsWith('src/lib/mail') &&
        path !== 'src/lib/schema-compatibility.ts' &&
        path !== 'src/db/client.ts')
    )
      return;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      workerReplacement = workerReplacement
        .then(() => replaceWorker(`development source changed: ${path}`))
        .catch(
          (error) => void beginShutdown(`worker replacement failed: ${String(error)}`, true),
        );
    }, 100);
  });
  developmentWatcher.on(
    'error',
    (error) => void beginShutdown(`development watcher failed: ${error.message}`, true),
  );
}

function waitForExit(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
  return new Promise((resolve) => child.once('exit', () => resolve()));
}

function beginShutdown(reason: string, failure: boolean): Promise<never> {
  if (shutdown) return shutdown;
  shutdownFailed ||= failure;
  shutdown = (async () => {
    const deadline = Date.now() + 45_000;
    clearInterval(monitor);
    clearTimeout(startup);
    developmentWatcher?.close();
    process.stderr.write(`[supervisor] drain: ${reason}\n`);
    for (const role of roles) {
      states.set(role, 'draining');
      publishPeer(role, 'draining');
    }
    for (const role of roles) tell(role, { type: 'quiesce', reason, deadline });
    const quiesceDeadline = Math.min(deadline, Date.now() + 5_000);
    while (
      Date.now() < quiesceDeadline &&
      roles.some((role) => {
        const child = children.get(role);
        return child && child.exitCode === null && !quiesced.has(role);
      })
    )
      await new Promise((resolve) => setTimeout(resolve, 25));
    if (
      roles.some((role) => {
        const child = children.get(role);
        return child && child.exitCode === null && !quiesced.has(role);
      })
    ) {
      shutdownFailed = true;
      process.stderr.write('[supervisor] quiesce deadline exceeded\n');
    }
    // Stop accepting and finish HTTP before asking DBOS to use its own
    // workflow-completion budget. Both phases share the container deadline.
    tell('http', { type: 'drain', reason, deadline });
    await Promise.race([
      children.get('http') ? waitForExit(children.get('http')!) : Promise.resolve(),
      new Promise<void>((resolve) => setTimeout(resolve, Math.max(0, deadline - Date.now()))),
    ]);
    tell('worker', { type: 'drain', reason, deadline });
    const exits = Promise.all([...children.values()].map(waitForExit));
    const timedOut = await Promise.race([
      exits.then(() => false),
      new Promise<true>((resolve) =>
        setTimeout(() => resolve(true), Math.max(0, deadline - Date.now())),
      ),
    ]);
    if (timedOut) {
      process.stderr.write('[supervisor] shutdown deadline exceeded; killing children\n');
      shutdownFailed = true;
      for (const child of children.values()) if (child.exitCode === null) child.kill('SIGKILL');
      await Promise.race([
        Promise.all([...children.values()].map(waitForExit)),
        new Promise<void>((resolve) => setTimeout(resolve, 2_000)),
      ]);
    }
    process.exit(shutdownFailed || timedOut ? 1 : 0);
  })() as Promise<never>;
  return shutdown;
}

runtimeConfig();
const startup = setTimeout(() => {
  if (roles.some((role) => states.get(role) !== 'ready'))
    void beginShutdown('startup deadline exceeded', true);
}, 120_000);
const monitor = setInterval(() => {
  const now = Date.now();
  for (const role of roles) {
    const last = heartbeats.get(role);
    if (states.get(role) === 'ready' && last !== undefined && now - last > 10_000) {
      void beginShutdown(`${role} heartbeat expired`, true);
      break;
    }
  }
}, 2_000);
for (const role of roles) spawn(role);
watchWorkerSources();
process.on('SIGTERM', () => {
  void beginShutdown('SIGTERM', false);
});
process.on('SIGINT', () => {
  void beginShutdown('SIGINT', false);
});
