/* Who is doing this, on whose behalf, and under which correlation id.
 *
 * Every row `emit()` writes carries four fields that no command wants to pass
 * down its own call stack: the actor, the subject, the operator acting as the
 * actor, and the correlation id that ties one request's events, audit rows and
 * log lines together. They are request-scoped facts, so they live in a
 * request-scoped store rather than in a parameter list — the donor is
 * dentconnex `lib/log/context.ts`, which threads the same idea through
 * AsyncLocalStorage.
 *
 * Two stores, because there are two kinds of caller. Inside a request, the
 * scope is `React.cache`: a layout, its page and every action it renders share
 * one object, and the auth seam enriches that object in place with
 * `setRequestContext` once the session resolves — nothing has to be resolved
 * before the context exists. Outside a request — a script, a migration, a
 * worker, a test — there is no cache scope at all (React's `cache` degrades to
 * calling the factory again), so `withCorrelation` layers an
 * AsyncLocalStorage store over the top and that one wins whenever it is there.
 *
 * The correlation id is taken from `x-correlation-id` when the edge sent one,
 * so a request that crossed a proxy or arrived from another fleet app keeps
 * the id it was already travelling under; otherwise it is minted here. It is
 * never null: a row with no correlation id cannot be joined to anything, and
 * the column is NOT NULL for that reason. */

import { AsyncLocalStorage } from 'node:async_hooks';
import { headers } from 'next/headers';
import { cache } from 'react';

export const CORRELATION_HEADER = 'x-correlation-id';

export interface RequestContext {
  correlationId: string;
  /* The signed-in principal, as a public id. Null for an anonymous visitor —
   * a guest answering an invitation emits events with no actor. */
  actorId: string | null;
  /* Whose page this is, when that is somebody other than the actor: an
   * operator on a person's page, a host reading a guest's row. */
  subjectId: string | null;
  /* Set only while an operator is signed in as somebody else. The actor is
   * still the person being acted as; this is who is answerable. */
  actingOperatorId: string | null;
}

/** A correlation id: short, opaque, and unique enough to group a request's
 *  rows without being worth guessing. */
export function newCorrelationId(): string {
  return crypto.randomUUID();
}

/* The empty string stands for "not resolved yet" rather than "no id": the
 * store has to be constructible synchronously (React.cache takes a plain
 * factory) while reading the inbound header is asynchronous in Next 15. */
const requestScope = cache(
  (): RequestContext => ({
    correlationId: '',
    actorId: null,
    subjectId: null,
    actingOperatorId: null,
  }),
);

const overlay = new AsyncLocalStorage<RequestContext>();

/* `headers()` throws outside a request scope, and a great deal of this code
 * also runs in scripts and tests. A missing header store is simply no inbound
 * id. */
async function inboundCorrelationId(): Promise<string | null> {
  try {
    const value = (await headers()).get(CORRELATION_HEADER);
    return value && value.trim() !== '' ? value.trim().slice(0, 200) : null;
  } catch {
    return null;
  }
}

/** The context for the work in hand. Resolving it twice in one request gives
 *  the same object, so a later `setRequestContext` is visible to an earlier
 *  caller that kept the reference. */
export async function requestContext(): Promise<RequestContext> {
  const overlaid = overlay.getStore();
  if (overlaid) return overlaid;

  const ctx = requestScope();
  ctx.correlationId ||= (await inboundCorrelationId()) ?? newCorrelationId();
  return ctx;
}

/** Fill in the identity fields once the session has resolved. A no-op with no
 *  scope to fill, which is what a script wants. */
export function setRequestContext(
  patch: Partial<Omit<RequestContext, 'correlationId'>>,
): void {
  Object.assign(overlay.getStore() ?? requestScope(), patch);
}

/** Run `fn` under an explicit correlation id — a job resuming a request's
 *  work, a test asserting on the id it will see, a script that wants its whole
 *  run joinable. Identity fields carry over from the surrounding context when
 *  there is one; `setRequestContext` inside `fn` reaches this store and not
 *  the outer one. */
export function withCorrelation<T>(id: string, fn: () => T): T {
  const outer = overlay.getStore();
  return overlay.run(
    {
      correlationId: id,
      actorId: outer?.actorId ?? null,
      subjectId: outer?.subjectId ?? null,
      actingOperatorId: outer?.actingOperatorId ?? null,
    },
    fn,
  );
}
