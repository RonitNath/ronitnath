import { describe, expect, it, vi } from 'vitest';

class Redirect extends Error {}
class NotFound extends Error {}

vi.mock('next/navigation', () => ({
  redirect: (to: string) => {
    throw new Redirect(to);
  },
  permanentRedirect: (to: string) => {
    throw new Redirect(to);
  },
  notFound: () => {
    throw new NotFound('not found');
  },
}));

/* The tiers ask exactly one question of the session store; the store itself is
 * better-auth's and is exercised against a real database by the Playwright
 * flows. */
const principal = vi.fn();
vi.mock('@/features/auth/principal', () => ({
  currentPrincipal: () => principal(),
  OPERATOR_RESOURCE: { kind: 'platform', id: 0 },
}));

/* The org tier asks one more question — whether this asker operates that
 * handle — and `allows` is tested on its own in authority.test.ts. */
const operated = vi.fn();
vi.mock('@/features/organizations/queries', () => ({
  operatedOrganization: (handle: string, actor: unknown) => operated(handle, actor),
  organizationByHandle: (handle: string) => operated(handle, null),
}));

const tiers = await import('../tiers');

const OPERATOR = {
  personId: 7,
  displayName: 'Ronit',
  sessionId: 'ses_3',
  source: 'oidc' as const,
  isOperator: true,
  actingOperatorId: null,
  reauthenticatedAt: null,
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

  it('sends a signed-out member to the door, carrying where they were going', async () => {
    principal.mockResolvedValue(null);
    await expect(tiers.requireMember()).rejects.toThrow('/auth/sign-in');
    await expect(tiers.requireMember('/app/sessions')).rejects.toThrow(
      '/auth/sign-in?next=%2Fapp%2Fsessions',
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

  it('sends a signed-out visitor from an org page to the door, carrying it', async () => {
    principal.mockResolvedValue(null);
    operated.mockResolvedValue(null);
    await expect(tiers.requireOrgOperator('isoastra')).rejects.toThrow(
      '/auth/sign-in?next=%2Forg%2Fisoastra',
    );
  });

  it('declines a member who does not operate that organization as 404', async () => {
    principal.mockResolvedValue(MEMBER);
    operated.mockResolvedValue(null);
    await expect(tiers.requireOrgOperator('isoastra')).rejects.toBeInstanceOf(NotFound);
  });

  it('lets an admin of that organization in, and hands the page the row', async () => {
    principal.mockResolvedValue(MEMBER);
    operated.mockResolvedValue({ id: 4, handle: 'isoastra', name: 'Isoastra' });
    await expect(tiers.requireOrgOperator('isoastra')).resolves.toMatchObject({
      tier: 'orgOperator',
      personId: 8,
      organization: { handle: 'isoastra' },
    });
  });
});
