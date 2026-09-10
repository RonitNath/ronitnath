/* The request context outside a request, which is where most of it is hard:
 * scripts, workers and this suite all run with no React cache scope and no
 * header store, and the rule is that they get a usable context anyway rather
 * than an exception halfway through a command. */

import { describe, expect, it, vi } from 'vitest';

/* `next/headers` refuses to be imported outside a server module. The context
 * only ever asks it for one optional header, so an empty one is a faithful
 * stand-in for "no request here". */
vi.mock('next/headers', () => ({ headers: async () => new Headers() }));

import {
  newCorrelationId,
  requestContext,
  setRequestContext,
  withCorrelation,
} from '@/lib/fleet/context';

describe('request context', () => {
  it('always has a correlation id', async () => {
    const ctx = await requestContext();
    expect(ctx.correlationId).toMatch(/^[0-9a-f-]{36}$/);
    expect(ctx.actorId).toBeNull();
    expect(ctx.subjectId).toBeNull();
    expect(ctx.actingOperatorId).toBeNull();
  });

  it('mints a distinct id each time', () => {
    expect(newCorrelationId()).not.toBe(newCorrelationId());
  });

  it('takes the id withCorrelation was given', async () => {
    const seen = await withCorrelation('corr-1', async () => (await requestContext()).correlationId);
    expect(seen).toBe('corr-1');
  });

  it('keeps identity written inside the scope inside it', async () => {
    const inside = await withCorrelation('corr-2', async () => {
      setRequestContext({ actorId: 'p_one', actingOperatorId: 'p_two' });
      return requestContext();
    });
    expect(inside.actorId).toBe('p_one');
    expect(inside.actingOperatorId).toBe('p_two');

    const outside = await withCorrelation('corr-3', async () => requestContext());
    expect(outside.actorId).toBeNull();
  });

  it('carries identity into a nested scope', async () => {
    const nested = await withCorrelation('outer', async () => {
      setRequestContext({ actorId: 'p_three' });
      return withCorrelation('inner', async () => requestContext());
    });
    expect(nested.correlationId).toBe('inner');
    expect(nested.actorId).toBe('p_three');
  });
});
