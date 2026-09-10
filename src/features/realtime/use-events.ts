'use client';

/* The browser half of the spine. Two hooks and one shared connection.
 *
 * A tab opens exactly one `EventSource` per stream path however many
 * components ask for it: a page with a guest list, a header count and three
 * live panels is one connection, not five, because the browser's per-origin
 * connection budget is six and a site that spends it on its own event streams
 * has no requests left to serve the refetches those streams provoke. The
 * connections live in a module-level map, reference counted; the last
 * component to unmount closes the socket.
 *
 * `streamPath` is the URL, not an org. The URL grammar (`/u/<user>/events`,
 * `/o/<org>/events`) belongs to the pages that know who they are showing, and
 * a hook that built the path itself would have to learn the difference between
 * a person and an organization to do it.
 *
 * Reconnection is ours rather than the browser's. `EventSource` reconnects to
 * the URL it was given, which carries the `since` we opened with — after an
 * hour offline it would ask to be caught up from where the tab started rather
 * than from where it got to, and be sent a reset it did not need. So an error
 * closes the socket and we open a new one at the current sequence.
 *
 * The event is invalidation, never data: `useRefreshOn` answers it with
 * `router.refresh()`, which refetches the server components through the same
 * authorized read path that rendered them. Nothing here trusts a frame for
 * anything except the fact that something moved. */

import { useRouter } from 'next/navigation';
import { useCallback, useEffect, useRef, useState } from 'react';

/** A frame off the wire. Snake case: it is the fleet's format, not ours. */
export interface FleetEvent {
  resource_kind: string;
  resource_id: string;
  seq: number;
  published: boolean;
}

export type EventStatus = 'connecting' | 'open' | 'closed';

/** How long to wait before reopening a socket that errored. It matches the
 *  `retry: 3000` the server sends, which is what the browser would have used
 *  had we let it reconnect for us. */
const RECONNECT_MS = 3_000;
const REFRESH_COALESCE_MS = 100;

type EventListener = (event: FleetEvent) => void;
type StatusListener = (status: EventStatus) => void;

interface Connection {
  source: EventSource | null;
  refs: number;
  /* Where this stream has got to. It survives a reconnect — that is the whole
   * reason the connection outlives the socket — and seeds the `since` of the
   * next one. */
  lastSeq: number;
  status: EventStatus;
  events: Set<EventListener>;
  statuses: Set<StatusListener>;
  retry: ReturnType<typeof setTimeout> | null;
}

const connections = new Map<string, Connection>();

function announce(conn: Connection, status: EventStatus): void {
  if (conn.status === status) return;
  conn.status = status;
  for (const listener of [...conn.statuses]) listener(status);
}

function open(path: string, conn: Connection): void {
  if (conn.source || conn.refs === 0) return;

  const url = conn.lastSeq > 0 ? `${path}?since=${conn.lastSeq}` : path;
  const source = new EventSource(url, { withCredentials: true });
  conn.source = source;
  announce(conn, 'connecting');

  source.onopen = () => announce(conn, 'open');

  source.onmessage = (message: MessageEvent<string>) => {
    let frame: FleetEvent;
    try {
      frame = JSON.parse(message.data) as FleetEvent;
    } catch {
      return;
    }
    if (typeof frame?.seq === 'number' && frame.seq > conn.lastSeq) conn.lastSeq = frame.seq;
    for (const listener of [...conn.events]) listener(frame);
  };

  /* A reset means the server declined to replay: too far behind, or it could
   * not read the backlog at all. Take its sequence and tell every listener
   * that everything it holds is suspect — the marker is a frame with no
   * resource, which every consumer treats as "refresh". */
  source.addEventListener('reset', (message) => {
    const data = (message as MessageEvent<string>).data;
    try {
      const seq = (JSON.parse(data) as { seq?: number }).seq;
      if (typeof seq === 'number') conn.lastSeq = seq;
    } catch {
      conn.lastSeq = 0;
    }
    for (const listener of [...conn.events]) {
      listener({ resource_kind: '', resource_id: '', seq: conn.lastSeq, published: true });
    }
  });

  source.onerror = () => {
    source.close();
    if (conn.source === source) conn.source = null;
    announce(conn, 'connecting');
    if (conn.refs === 0 || conn.retry) return;
    conn.retry = setTimeout(() => {
      conn.retry = null;
      open(path, conn);
    }, RECONNECT_MS);
  };
}

/** Attach to the stream at `path`, opening it if this is the first caller.
 *  Returns the detach, which closes the socket when it is the last one. */
function attach(path: string, onEvent: EventListener, onStatus: StatusListener): () => void {
  let conn = connections.get(path);
  if (!conn) {
    conn = {
      source: null,
      refs: 0,
      lastSeq: 0,
      status: 'closed',
      events: new Set(),
      statuses: new Set(),
      retry: null,
    };
    connections.set(path, conn);
  }

  const held = conn;
  held.refs += 1;
  held.events.add(onEvent);
  held.statuses.add(onStatus);
  onStatus(held.status);
  open(path, held);

  let live = true;
  return () => {
    if (!live) return;
    live = false;
    held.events.delete(onEvent);
    held.statuses.delete(onStatus);
    held.refs -= 1;
    if (held.refs > 0) return;

    if (held.retry) clearTimeout(held.retry);
    held.retry = null;
    held.source?.close();
    held.source = null;
    held.status = 'closed';
    /* The map entry goes with it. Keeping it for its `lastSeq` would mean a
     * component remounting an hour later asks to be caught up from a position
     * it no longer has any state for. */
    connections.delete(path);
  };
}

/** Watch a stream. One `EventSource` per tab per path however many components
 *  call this. `onChange` fires for every frame that reaches this tab; it is
 *  read through a ref, so passing a new closure each render does not reopen
 *  the connection. */
export function useEvents(
  streamPath: string,
  onChange?: (event: FleetEvent) => void,
): { status: EventStatus; lastSeq: number } {
  const [status, setStatus] = useState<EventStatus>('closed');
  const [lastSeq, setLastSeq] = useState(0);
  const handler = useRef(onChange);
  handler.current = onChange;

  useEffect(() => {
    return attach(
      streamPath,
      (event) => {
        setLastSeq((seen) => (event.seq > seen ? event.seq : seen));
        handler.current?.(event);
      },
      setStatus,
    );
  }, [streamPath]);

  return { status, lastSeq };
}

/** Refresh the current route whenever one of `kinds` changes.
 *
 *  `kinds` are resource kinds — the frame says nothing about what happened,
 *  only what it happened to. An empty list means every kind.
 *
 *  `publishedOnly` is for configured records: an editor's draft saves emit
 *  with `published: false` and a page showing the published body has no reason
 *  to refetch until somebody publishes. */
export function useRefreshOn(
  streamPath: string,
  kinds: readonly string[],
  options: { publishedOnly?: boolean } = {},
): { status: EventStatus; lastSeq: number } {
  const router = useRouter();
  const publishedOnly = options.publishedOnly ?? false;
  /* The list is almost always a literal at the call site, so comparing it by
   * value keeps the effect from resubscribing on every render. */
  const wanted = kinds.join(',');
  const pending = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (pending.current) clearTimeout(pending.current);
      pending.current = null;
    };
  }, []);

  const onChange = useCallback(
    (event: FleetEvent) => {
      if (publishedOnly && !event.published) return;
      /* The empty resource kind is the reset marker: it matches everything,
       * because after a reset nothing this page holds can be trusted. */
      if (wanted !== '' && event.resource_kind !== '') {
        if (!wanted.split(',').includes(event.resource_kind)) return;
      }
      if (pending.current) return;
      pending.current = setTimeout(() => {
        pending.current = null;
        router.refresh();
      }, REFRESH_COALESCE_MS);
    },
    [publishedOnly, router, wanted],
  );

  return useEvents(streamPath, onChange);
}
