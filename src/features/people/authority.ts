/* allows(ctx, want). Every command in this feature ends here, and the last
 * clause is always "…or a platform operator" (docs/plan.md §Model).
 *
 * A member may touch a person they *are* or a person they *hold* — the
 * `contact` edge written when they held them — and an invitation they minted.
 * Nothing else, and a refusal never says which of those it failed, because
 * the difference between "not yours" and "does not exist" is exactly the fact
 * a stranger would like to learn. */

import { and, eq, isNull } from 'drizzle-orm';

import { schema } from '@/db/client';
import type { Transaction } from '@/features/auth/db';

/* One actor shape for the whole deployment (src/lib/authority.ts). */
export type { Actor } from '@/lib/authority';
import type { Actor } from '@/lib/authority';

export type Want =
  /* Act on this person: be them, or hold them. */
  | { on: 'person'; id: number }
  /* Act on this invitation: have minted it. */
  | { on: 'link'; id: number };

export const CONTACT = 'contact';

/** Whether this member holds this person. */
export async function holds(tx: Transaction, personId: number, target: number): Promise<boolean> {
  const rows = await tx
    .select({ id: schema.relation.id })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.subjectId, personId),
        eq(schema.relation.verb, CONTACT),
        eq(schema.relation.resourceKind, 'person'),
        eq(schema.relation.resourceId, target),
      ),
    )
    .limit(1);
  return rows.length > 0;
}

export async function allows(tx: Transaction, actor: Actor, want: Want): Promise<boolean> {
  if (want.on === 'person') {
    if (want.id === actor.personId) return true;
    if (await holds(tx, actor.personId, want.id)) return true;
  } else {
    const rows = await tx
      .select({ id: schema.link.id })
      .from(schema.link)
      .where(and(eq(schema.link.id, want.id), eq(schema.link.createdBy, actor.personId)))
      .limit(1);
    if (rows.length > 0) return true;
  }
  return actor.isOperator;
}

/** The person a person id means now: null once it has been folded into
 *  another one, so nothing reachable resolves to a retired row. */
export async function livePerson(
  tx: Transaction,
  personId: number,
): Promise<{ id: number; displayName: string; held: boolean } | null> {
  const rows = await tx
    .select({
      id: schema.person.id,
      displayName: schema.person.displayName,
      held: schema.person.held,
    })
    .from(schema.person)
    .where(and(eq(schema.person.id, personId), isNull(schema.person.mergedInto)))
    .limit(1);
  return rows[0] ?? null;
}
