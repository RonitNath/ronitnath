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
const { encodeId } = await import('../ids');

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
    await expect(tiers.requireMember('/u/p_x/sessions')).rejects.toThrow(
      '/auth/sign-in?next=%2Fu%2Fp_x%2Fsessions',
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
      '/auth/sign-in?next=%2Fo%2Fisoastra',
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

/* The `/u/<user>` gate. Its self and refusal paths never touch the database —
 * the id decodes to a person or it does not, and it is the asker's own or it
 * is not — so they are unit-testable exactly as the other tiers are. The
 * operator-reading-somebody-else path does read a row and is exercised by the
 * Playwright flows. */
describe('the subject gate', () => {
  const me = encodeId('person', 8);
  const somebodyElse = encodeId('person', 9);

  it('sends a signed-out reader to the door, carrying the path they asked for', async () => {
    principal.mockResolvedValue(null);
    await expect(tiers.requireSubjectPerson(me, 'events')).rejects.toThrow(
      `/auth/sign-in?next=${encodeURIComponent(`/u/${me}/events`)}`,
    );
  });

  it('lets a member onto their own surfaces, reading as themselves', async () => {
    principal.mockResolvedValue(MEMBER);
    await expect(tiers.requireSubjectPerson(me)).resolves.toMatchObject({
      subjectPersonId: 8,
      viewingOther: false,
      reader: { personId: 8, isOperator: false },
    });
  });

  it('declines a member reading somebody else as 404, never 403', async () => {
    principal.mockResolvedValue(MEMBER);
    await expect(tiers.requireSubjectPerson(somebodyElse)).rejects.toBeInstanceOf(NotFound);
  });

  it('declines an id that is not a person id at all', async () => {
    principal.mockResolvedValue(MEMBER);
    await expect(tiers.requireSubjectPerson('p_not-a-real-id')).rejects.toBeInstanceOf(NotFound);
    await expect(
      tiers.requireSubjectPerson(encodeId('organization', 8)),
    ).rejects.toBeInstanceOf(NotFound);
  });
});

