import { describe, expect, it, vi } from 'vitest';

class Redirect extends Error {}
class NotFound extends Error {}

vi.mock('next/navigation', () => ({
  redirect: (to: string) => {
    throw new Redirect(to);
  },
  notFound: () => {
    throw new NotFound('not found');
  },
}));

const tiers = await import('../tiers');

describe('tier helpers', () => {
  it('lets a visitor through', async () => {
    await expect(tiers.requireVisitor()).resolves.toEqual({ tier: 'visitor', personId: null });
  });

  it('sends a signed-out member to /auth', async () => {
    await expect(tiers.requireMember()).rejects.toThrow('/auth');
  });

  it('declines the operator tiers as 404, never 403', async () => {
    await expect(tiers.requireOrgOperator('isoastra')).rejects.toBeInstanceOf(NotFound);
    await expect(tiers.requireOperator()).rejects.toBeInstanceOf(NotFound);
  });
});
