import { createServer } from 'node:http';
import type { Socket } from 'node:net';
import next from 'next';
import { Lifecycle, isParentMessage } from '@isoastra/fleet-runtime';
import { closePool } from '@/db/client';
import { disposeDeliveryListener } from '@/features/realtime/delivery-stream';
import { disposeStreamListener } from '@/lib/fleet/stream';
import { drainStreams, runtimeStatus, setRuntimeDraining, setWorkerState } from '@/lib/runtime';
import { ownChildLifecycle } from './child';
import { runtimeConfig } from './config';

async function main(): Promise<void> {
  const config = runtimeConfig();
  const lifecycle = new Lifecycle();
  ownChildLifecycle('http', lifecycle);
  process.on('message', (message: unknown) => {
    if (isParentMessage(message) && message.type === 'peer' && message.role === 'worker') setWorkerState(message.state);
  });
  const app = next({ dev: process.env.NODE_ENV !== 'production', dir: process.cwd(), hostname: config.host, port: config.port });
  await app.prepare();
  const handler = app.getRequestHandler();
  const upgrades = app.getUpgradeHandler();
  const sockets = new Set<Socket>();
  let active = 0;
  const server = createServer((request, response) => {
    // Replace proxy-origin headers at the Node boundary. Middleware runs in an
    // Edge bundle and cannot reliably read runtime environment variables.
    const publicUrl = new URL(config.publicOrigin);
    request.headers['x-forwarded-host'] = publicUrl.host;
    request.headers['x-forwarded-proto'] = publicUrl.protocol.slice(0, -1);
    const setHeader = response.setHeader.bind(response);
    response.setHeader = (name, value) => setHeader(name, name.toLowerCase() === 'location'
      ? canonicalLocation(value, config.port, publicUrl)
      : value);
    if (runtimeStatus().draining && request.url !== '/healthz' && request.url !== '/readyz') {
      response.writeHead(503, { connection: 'close', 'retry-after': '5' });
      response.end('draining');
      return;
    }
    active += 1;
    let finished = false;
    const done = () => { if (!finished) { finished = true; active -= 1; } };
    response.once('finish', done);
    response.once('close', done);
    void handler(request, response).catch((error) => {
      console.error(JSON.stringify({ level: 'error', event: 'http.request-failed', message: String(error) }));
      if (!response.headersSent) response.writeHead(500);
      response.end();
    });
  });
  server.on('connection', (socket) => {
    sockets.add(socket);
    socket.once('close', () => sockets.delete(socket));
  });
  server.on('upgrade', (request, socket, head) => {
    if (runtimeStatus().draining) { socket.destroy(); return; }
    void upgrades(request, socket, head);
  });

  lifecycle.register('application database pool', closePool);
  lifecycle.register('postgres listeners', async () => { await Promise.all([disposeStreamListener(), disposeDeliveryListener()]); });
  lifecycle.register('next server', () => app.close());
  lifecycle.register('http drain', async ({ deadline }) => {
    setRuntimeDraining(true);
    drainStreams();
    server.closeIdleConnections();
    const requestDeadline = Math.min(deadline, Date.now() + 15_000);
    while (active > 0 && Date.now() < requestDeadline) await new Promise((resolve) => setTimeout(resolve, 25));
    for (const socket of sockets) socket.destroy();
    server.closeAllConnections();
    if (server.listening) await new Promise<void>((resolve) => server.close(() => resolve()));
  });

  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(config.port, config.host, () => resolve());
  });
  lifecycle.ready();
}

function canonicalLocation(
  value: number | string | readonly string[],
  internalPort: number,
  publicUrl: URL,
): number | string | readonly string[] {
  const rewrite = (candidate: string): string => {
    try {
      const url = new URL(candidate);
      const internal = ['localhost', '127.0.0.1', '0.0.0.0'].includes(url.hostname) &&
        url.port === String(internalPort);
      if (!internal) return candidate;
      url.protocol = publicUrl.protocol;
      url.host = publicUrl.host;
      return url.toString();
    } catch {
      return candidate;
    }
  };
  if (typeof value === 'string') return rewrite(value);
  if (Array.isArray(value)) return value.map(rewrite);
  return value;
}

main().catch((error) => {
  console.error(JSON.stringify({ level: 'error', event: 'http.start-failed', message: String(error) }));
  process.exit(1);
});
