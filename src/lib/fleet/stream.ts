/* The LISTEN side of the spine. Node runtime only.
 *
 * One Postgres connection per process holds `LISTEN domain_event`; every open
 * SSE response in that process is a subscriber to this fan-out. A connection
 * per browser would be a connection per browser against the leader, which is
 * the failure this design exists to avoid.
 *
 * The notification carries `<orgId>|<seq>` and nothing else, deliberately.
 * `pg_notify` has an 8 kB payload ceiling, it is delivered at most once and
 * only to processes that were listening at the time, and a subscriber that
 * trusted its contents would be reading data that never passed an
 * authorization check. So the notify is a wake-up: the subscriber learns that
 * `orgId` has reached `seq` and goes and reads the rows itself, through the
 * same read path with the same reach predicate as any other read. That is also
 * what makes a missed notification survivable — the next one carries a higher
 * seq and the reader catches up on everything in between.
 *
 * The connection is opened on the first subscriber and closed after the last
 * one leaves, so a process that serves no streams holds no connection. While
 * it is wanted it is kept: an error or a server-side close schedules a
 * reconnect two seconds later and keeps every subscriber attached across it.
 * Subscribers are not told about the gap, because they do not need to be —
 * each one re-reads from its own last seq whenever it is woken. */

import { Client } from 'pg';

export const CHANNEL = 'domain_event';
const RECONNECT_MS = 2_000;

export interface Notice {
  orgId: string;
  seq: number;
}

export type NoticeHandler = (notice: Notice) => void;

interface Hub {
  client: Client | null;
  /* Subscribers by org. The map doubles as the reference count: an org with no
   * handlers is deleted, and an empty map means nobody wants the connection. */
  subscribers: Map<string, Set<NoticeHandler>>;
  reconnect: ReturnType<typeof setTimeout> | null;
  /* Set while a connect is in flight, so that a burst of subscribers opening
   * at once does not open a burst of connections. */
  connecting: boolean;
}

/* Next reloads modules in dev and route handlers are loaded per bundle; the
 * hub is parked on globalThis so that "one per process" survives both. */
const globalForStream = globalThis as unknown as { rnEventHub?: Hub };

function hub(): Hub {
  globalForStream.rnEventHub ??= {
    client: null,
    subscribers: new Map(),
    reconnect: null,
    connecting: false,
  };
  return globalForStream.rnEventHub;
}

function deliver(payload: string | undefined): void {
  if (!payload) return;
  const bar = payload.lastIndexOf('|');
  if (bar <= 0) return;
  const orgId = payload.slice(0, bar);
  const seq = Number(payload.slice(bar + 1));
  if (!Number.isFinite(seq)) return;

  const handlers = hub().subscribers.get(orgId);
  if (!handlers) return;
  for (const handler of [...handlers]) {
    try {
      handler({ orgId, seq });
    } catch {
      /* One subscriber's failure is its own. The others are still owed this
       * notice, and the connection must survive it. */
    }
  }
}

function scheduleReconnect(h: Hub): void {
  if (h.reconnect || h.subscribers.size === 0) return;
  h.reconnect = setTimeout(() => {
    h.reconnect = null;
    connect();
  }, RECONNECT_MS);
}

function connect(): void {
  const h = hub();
  if (h.client || h.connecting || h.subscribers.size === 0) return;

  const connectionString = process.env.DATABASE_URL;
  if (!connectionString) throw new Error('DATABASE_URL is not set');

  const client = new Client({ connectionString });
  h.connecting = true;

  const drop = () => {
    if (h.client !== client) return;
    h.client = null;
    scheduleReconnect(h);
  };

  client.on('notification', (message) => deliver(message.payload));
  client.on('error', () => {
    /* An idle listener is the first thing a failover kills. Ending here is
     * belt and braces: `end()` on an already-broken client resolves or
     * rejects, and either way the reconnect is what matters. */
    client.end().catch(() => {});
    drop();
  });
  client.on('end', drop);

  client
    .connect()
    .then(() => client.query(`LISTEN ${CHANNEL}`))
    .then(() => {
      h.connecting = false;
      /* The last subscriber may have left while we were connecting. */
      if (h.subscribers.size === 0) {
        client.end().catch(() => {});
        return;
      }
      h.client = client;
    })
    .catch(() => {
      h.connecting = false;
      client.end().catch(() => {});
      scheduleReconnect(h);
    });
}

/** Wake `handler` whenever `orgId` reaches a new sequence. Returns the
 *  unsubscribe; call it from the stream's cancel path, or the process keeps
 *  waking a response nobody is reading. */
export function subscribe(orgId: string, handler: NoticeHandler): () => void {
  const h = hub();
  const handlers = h.subscribers.get(orgId) ?? new Set<NoticeHandler>();
  handlers.add(handler);
  h.subscribers.set(orgId, handlers);
  connect();

  let live = true;
  return () => {
    if (!live) return;
    live = false;
    const current = h.subscribers.get(orgId);
    if (!current) return;
    current.delete(handler);
    if (current.size === 0) h.subscribers.delete(orgId);
    if (h.subscribers.size > 0) return;

    if (h.reconnect) {
      clearTimeout(h.reconnect);
      h.reconnect = null;
    }
    const client = h.client;
    h.client = null;
    client?.end().catch(() => {});
  };
}

/** Whether the listener is connected right now. `/readyz` and the tests ask;
 *  nothing on a request path should care, because a disconnected listener is a
 *  slower stream and not a broken one. */
export function listening(): boolean {
  return hub().client !== null;
}
