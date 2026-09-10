/* The wire format and the whole path behind it.
 *
 * The frame format is asserted byte for byte because it is a contract shared
 * with four other apps and with every browser's `EventSource`: a missing blank
 * line and the frame is never dispatched, a missing `id:` and a reconnect
 * cannot resume. Those assertions need no database.
 *
 * The rest does. The live half of the test emits into a real database and
 * waits for the frame to come back out of the response, which is the only way
 * to prove that the NOTIFY reached the process listener, that the listener
 * woke this connection, and that the connection read the row and let it
 * through its reach predicate. */

import { afterAll, describe, expect, it, vi } from 'vitest';

vi.mock('next/headers', () => ({ headers: async () => new Headers() }));

import { anOrg, closeDatabase, database, reachable } from './db';
import { emit, type DomainEventRow } from '@/lib/fleet/events';
import { listening } from '@/lib/fleet/stream';
import {
  SSE_HEADERS,
  formatFrame,
  formatReset,
  frameOf,
  streamResponse,
} from '@/lib/fleet/sse';

afterAll(closeDatabase);

const row: DomainEventRow = {
  id: 1,
  orgId: 'o_one',
  seq: 41,
  resourceKind: 'rsvp',
  resourceId: 'r_7',
  kind: 'answered',
  actorId: 'p_actor',
  subjectId: null,
  actingOperatorId: null,
  correlationId: 'corr-1',
  published: true,
  payload: { answer: 'yes' },
  at: new Date('2026-09-10T12:00:00Z'),
};

describe('frames', () => {
  it('carries invalidation and nothing else', () => {
    expect(frameOf(row)).toEqual({
      resource_kind: 'rsvp',
      resource_id: 'r_7',
      seq: 41,
      published: true,
    });
    /* Not the actor, not the correlation id, and above all not the payload:
     * everything a reader is entitled to see comes back through the read path
     * that authorised it. */
    expect(formatFrame(row)).not.toContain('p_actor');
    expect(formatFrame(row)).not.toContain('answer');
  });

  it('is an id line, a data line and a blank line', () => {
    expect(formatFrame(row)).toBe(
      'id: 41\ndata: {"resource_kind":"rsvp","resource_id":"r_7","seq":41,"published":true}\n\n',
    );
  });

  it('names the reset event and carries the sequence to resume from', () => {
    expect(formatReset(900)).toBe('event: reset\nid: 900\ndata: {"seq":900}\n\n');
  });

  it('tells every hop not to buffer it', () => {
    expect(SSE_HEADERS).toEqual({
      'Content-Type': 'text/event-stream; charset=utf-8',
      'Cache-Control': 'no-cache, no-transform',
      'X-Accel-Buffering': 'no',
    });
  });
});

type Reader = ReadableStreamDefaultReader<Uint8Array>;

/** Read decoded chunks until `want` says it has seen enough, or time is up.
 *  The response never ends on its own — that is the point of it — so every
 *  read is bounded here rather than by the stream, and the reader is held by
 *  the caller: cancelling it is what closes the connection. */
async function readUntil(
  reader: Reader,
  want: (text: string) => boolean,
  timeoutMs = 8_000,
  seen = '',
): Promise<string> {
  const decoder = new TextDecoder();
  const deadline = Date.now() + timeoutMs;
  let text = seen;
  while (!want(text) && Date.now() < deadline) {
    const chunk = await Promise.race([
      reader.read(),
      new Promise<{ done: true; value: undefined }>((resolve) =>
        setTimeout(() => resolve({ done: true, value: undefined }), Math.max(0, deadline - Date.now())),
      ),
    ]);
    if (chunk.done || !chunk.value) break;
    text += decoder.decode(chunk.value, { stream: true });
  }
  return text;
}

function readerFor(response: Response): Reader {
  const body = response.body;
  if (!body) throw new Error('the stream has no body');
  return body.getReader();
}

async function settle(predicate: () => boolean, timeoutMs = 5_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate() && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

describe.skipIf(!reachable)('streamResponse', () => {
  it('opens with the retry hint and the contract headers', async () => {
    const orgId = anOrg('open');
    const response = streamResponse({ db: database(), orgId, since: 0, reach: () => true });

    expect(response.headers.get('content-type')).toBe('text/event-stream; charset=utf-8');
    expect(response.headers.get('cache-control')).toBe('no-cache, no-transform');
    expect(response.headers.get('x-accel-buffering')).toBe('no');

    const reader = readerFor(response);
    const text = await readUntil(reader, (seen) => seen.includes('retry:'), 3_000);
    await reader.cancel();
    expect(text.startsWith('retry: 3000\n\n')).toBe(true);
  });

  it('replays what the client missed, oldest first', async () => {
    const db = database();
    const orgId = anOrg('replay');
    for (const id of ['e_1', 'e_2', 'e_3']) {
      await db.transaction((tx) =>
        emit(tx, { orgId, resourceKind: 'event', resourceId: id, kind: 'created' }),
      );
    }

    const response = streamResponse({ db, orgId, since: 1, reach: () => true });
    const reader = readerFor(response);
    const text = await readUntil(reader, (seen) => seen.includes('"seq":3'), 5_000);
    await reader.cancel();

    expect(text).toContain('id: 2\ndata: ');
    expect(text).toContain('"resource_id":"e_2"');
    expect(text).toContain('"resource_id":"e_3"');
    /* Already seen, so never sent. */
    expect(text).not.toContain('"resource_id":"e_1"');
    expect(text.indexOf('"e_2"')).toBeLessThan(text.indexOf('"e_3"'));
  });

  it('delivers a live event through LISTEN and honours the reach predicate', async () => {
    const db = database();
    const orgId = anOrg('live');

    /* Only documents reach this connection. The rsvp below is emitted into the
     * same org and must not appear. */
    const response = streamResponse({
      db,
      orgId,
      since: 0,
      reach: (event) => event.resourceKind === 'document',
    });

    /* The listener is opened asynchronously; emitting before it is up would
     * test nothing but the replay. */
    const reader = readerFor(response);
    const opened = await readUntil(reader, (seen) => seen.includes('retry:'), 2_000);
    await settle(listening);
    expect(listening()).toBe(true);

    await db.transaction((tx) =>
      emit(tx, { orgId, resourceKind: 'rsvp', resourceId: 'r_1', kind: 'answered' }),
    );
    await db.transaction((tx) =>
      emit(tx, { orgId, resourceKind: 'document', resourceId: 'r_2', kind: 'updated' }),
    );

    const text = await readUntil(
      reader,
      (seen) => seen.includes('"resource_id":"r_2"'),
      8_000,
      opened,
    );
    await reader.cancel();
    expect(text).toContain('"resource_kind":"document"');
    expect(text).toContain('id: 2\n');
    expect(text).not.toContain('"resource_id":"r_1"');

    /* Cancelling the response is the last subscriber leaving, and the process
     * listener goes with it rather than idling on the leader forever. */
    await settle(() => !listening());
    expect(listening()).toBe(false);
  });
});
