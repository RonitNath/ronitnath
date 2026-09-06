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

/* The tiers ask exactly one question of the session store; the store itself is
 * exercised against a real database by the Playwright flows. */
const principal = vi.fn();
vi.mock('@/features/auth/session', () => ({ currentPrincipal: () => principal() }));

const tiers = await import('../tiers');

const OPERATOR = {
  personId: 7,
  displayName: 'Ronit',
  sessionId: 3,
  source: 'oidc' as const,
  isOperator: true,
};
const MEMBER = { ...OPERATOR, personId: 8, isOperator: false, source: 'local' as const };

describe('tier helpers', () => {
  it('lets a visitor through', async () => {
    principal.mockResolvedValue(null);
    await expect(tiers.requireVisitor()).resolves.toEqual({
      tier: 'visitor',
      personId: null,
      principal: null,
    });
  });

  it('sends a signed-out member to /auth, carrying where they were going', async () => {
    principal.mockResolvedValue(null);
    await expect(tiers.requireMember()).rejects.toThrow('/auth');
    await expect(tiers.requireMember('/app/sessions')).rejects.toThrow(
      '/auth?next=%2Fapp%2Fsessions',
    );
  });

  it('lets a signed-in person into the member tier', async () => {
    principal.mockResolvedValue(MEMBER);
    await expect(tiers.requireMember()).resolves.toMatchObject({
      tier: 'member',
      personId: 8,
    });
  });

  it('declines a member at the operator tier as 404, never 403', async () => {
    principal.mockResolvedValue(MEMBER);
    await expect(tiers.requireOperator()).rejects.toBeInstanceOf(NotFound);
  });

  it('lets an operator into the operator tier', async () => {
    principal.mockResolvedValue(OPERATOR);
    await expect(tiers.requireOperator()).resolves.toMatchObject({
      tier: 'operator',
      personId: 7,
    });
  });

  it('declines the org tier for everybody until R5 fills it', async () => {
    principal.mockResolvedValue(OPERATOR);
    await expect(tiers.requireOrgOperator('isoastra')).rejects.toBeInstanceOf(NotFound);
  });
});
