/* The SSE side of the spine: one long-lived response per open tab, carrying
 * invalidation and never data. Node runtime only (it holds a Postgres
 * listener open; the edge runtime has neither the socket nor the lifetime).
 *
 * The route handlers are thin on purpose — they authenticate, decide what this
 * connection may see, and hand the decision here as `reach`. Everything below
 * is the same for `/u/<user>/events` and `/o/<org>/events`, and it is the part
 * that is easy to get subtly wrong.
 *
 * What a frame says is `{resource_kind, resource_id, seq, published}`: this
 * thing changed, go and read it. Not what it changed to. A stream that carried
 * data would be a second read path with its own authorization to keep in step
 * with the first, and the first is the one that has been audited. The client
 * refetches through the page it is already on.
 *
 * Resumption is by sequence and it is exact. `id: <seq>` on every frame means
 * the browser's own `Last-Event-ID` is a valid `since`, and because `seq` is
 * gapless per org a client that reconnects with `since=41` can be told
 * precisely what it missed. Precisely, up to a point: replaying an unbounded
 * backlog to a laptop that was shut for a week is neither kind nor necessary,
 * so past REPLAY_MAX rows we send one `event: reset` and the client reloads
 * from scratch.
 *
 * `reach` is applied to every row, replayed or live, because membership is not
 * frozen at connect time: a person removed from an organization mid-stream
 * must stop seeing its events on the next frame, not on the next reconnect. */

import { eventsSince, latestSeq, type DomainEventRow, type Queryable } from './events';
import { subscribe } from './stream';

/** Beyond this many missed rows, a reset is cheaper for everyone than a
 *  replay — for the client, which is going to refetch the page anyway, and for
 *  the leader, which would otherwise stream a week of history per reconnect. */
export const REPLAY_MAX = 500;
/** Events arrive in bursts — one command emits four rows — and a burst that
 *  woke four refetches would be four times the read path for one change. */
export const COALESCE_MS = 100;
/** Under a proxy that buffers, silence is indistinguishable from a dead
 *  connection. The comment costs nine bytes and keeps every hop honest. */
export const PING_MS = 20_000;
/** How long a browser waits before reconnecting, sent once at the top. */
export const RETRY_MS = 3_000;

/* `no-transform` matters as much as `no-cache`: a transforming proxy is
 * entitled to buffer and recompress a response it is allowed to transform, and
 * a buffered event stream is a stream that arrives all at once at the end.
 * `X-Accel-Buffering` says the same thing again to nginx, which does not
 * always take the first answer. */
export const SSE_HEADERS: Readonly<Record<string, string>> = Object.freeze({
  'Content-Type': 'text/event-stream; charset=utf-8',
  'Cache-Control': 'no-cache, no-transform',
  'X-Accel-Buffering': 'no',
});

/** What a frame carries. Snake case because it is a wire format shared with
 *  the other apps in the fleet, not a TypeScript object. */
export interface EventFrame {
  resource_kind: string;
  resource_id: string;
  seq: number;
  published: boolean;
}

/** Whether this connection may be told that this row changed. The route hands
 *  in the read path's own predicate; returning false is indistinguishable from
 *  the event not having happened, which is the point. */
export type Reach = (row: DomainEventRow) => boolean;

export interface StreamOptions {
  db: Queryable;
  orgId: string;
  /** The client's last seen sequence. 0 or less means "start from now". */
  since: number;
  reach: Reach;
}

export function frameOf(row: DomainEventRow): EventFrame {
  return {
    resource_kind: row.resourceKind,
    resource_id: row.resourceId,
    seq: row.seq,
    published: row.published,
  };
}

/** One `id:`/`data:` frame. Exported because the format is a contract shared
 *  with four other apps and a test should be able to assert on it. */
export function formatFrame(row: DomainEventRow): string {
  return `id: ${row.seq}\ndata: ${JSON.stringify(frameOf(row))}\n\n`;
}

/** Sent instead of a replay the client is too far behind to receive. It
 *  carries the sequence it should consider itself at, so the reload does not
 *  start another oversized replay. */
export function formatReset(seq: number): string {
  return `event: reset\nid: ${seq}\ndata: ${JSON.stringify({ seq })}\n\n`;
}

/** The whole endpoint: replay, live fan-out, coalescing, pings and teardown.
 *  A route handler is `return streamResponse({ db, orgId, since, reach })`
 *  after it has decided who is asking. */
export function streamResponse({ db, orgId, since, reach }: StreamOptions): Response {
  const encoder = new TextEncoder();
  let sent = Number.isFinite(since) && since > 0 ? Math.floor(since) : 0;

  /* Everything that outlives one callback and has to be torn down exactly
   * once, whether the browser closed the tab, the process is shutting down or
   * a read threw. */
  let unsubscribe: (() => void) | null = null;
  let ping: ReturnType<typeof setInterval> | null = null;
  let coalesce: ReturnType<typeof setTimeout> | null = null;
  let closed = false;
  /* A read in flight. Two concurrent catch-up reads would interleave their
   * writes and could send the same seq twice, so a notice arriving during a
   * read sets this flag and the read loops instead. */
  let reading = false;
  let again = false;

  const stream = new ReadableStream<Uint8Array>({
    async start(controller) {
      const teardown = () => {
        if (closed) return;
        closed = true;
        unsubscribe?.();
        unsubscribe = null;
        if (ping) clearInterval(ping);
        if (coalesce) clearTimeout(coalesce);
        ping = null;
        coalesce = null;
      };

      const write = (text: string): boolean => {
        if (closed) return false;
        try {
          controller.enqueue(encoder.encode(text));
          return true;
        } catch {
          /* The client went away between the check and the enqueue. */
          teardown();
          return false;
        }
      };

      const drain = async (): Promise<void> => {
        if (reading) {
          again = true;
          return;
        }
        reading = true;
        try {
          do {
            again = false;
            const rows = await eventsSince(db, orgId, sent, REPLAY_MAX);
            if (closed) return;
            for (const row of rows) {
              sent = row.seq;
              if (!reach(row)) continue;
              if (!write(formatFrame(row))) return;
            }
            /* A full page means there may be more waiting behind it. */
            if (rows.length === REPLAY_MAX) again = true;
          } while (again && !closed);
        } catch {
          /* A failed read is not a failed stream: the next notice or ping
           * retries, and `sent` has not moved, so nothing is skipped. */
        } finally {
          reading = false;
        }
      };

      write(`retry: ${RETRY_MS}\n\n`);

      /* The backlog. Asking for one more than we will send is how we learn
       * whether the client is too far behind to be caught up. */
      try {
        const backlog = sent > 0 ? await eventsSince(db, orgId, sent, REPLAY_MAX + 1) : [];
        if (backlog.length > REPLAY_MAX) {
          const head = await latestSeq(db, orgId);
          sent = head;
          write(formatReset(head));
        } else {
          for (const row of backlog) {
            sent = row.seq;
            if (reach(row)) write(formatFrame(row));
          }
          if (sent === 0) sent = await latestSeq(db, orgId);
        }
      } catch {
        /* No backlog we could read. Start from now rather than refusing the
         * connection: the client is subscribed either way and a reset tells it
         * not to trust its own position. */
        sent = 0;
        write(formatReset(0));
      }

      if (closed) return;

      unsubscribe = subscribe(orgId, ({ seq }) => {
        if (closed || seq <= sent) return;
        if (coalesce) return;
        coalesce = setTimeout(() => {
          coalesce = null;
          void drain();
        }, COALESCE_MS);
      });

      ping = setInterval(() => {
        write(': ping\n\n');
      }, PING_MS);
    },

    cancel() {
      closed = true;
      unsubscribe?.();
      unsubscribe = null;
      if (ping) clearInterval(ping);
      if (coalesce) clearTimeout(coalesce);
      ping = null;
      coalesce = null;
    },
  });

  return new Response(stream, { headers: { ...SSE_HEADERS } });
}
