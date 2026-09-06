import { eq } from 'drizzle-orm';

import { database, schema } from '@/db/client';
import { currentPrincipal } from '@/features/auth/session';
import { endImpersonation } from '../impersonation';

/* The bar. It is in the root layout, so it is on the landing, on a guest
 * page, on `/app` and on a document — everywhere, because the one thing an
 * operator wearing somebody else's name must never do is forget that they
 * are. Nothing is drawn for anybody else. */
export async function ImpersonationBar() {
  const principal = await currentPrincipal();
  if (!principal?.actingOperatorId) return null;

  const rows = await database()
    .select({ name: schema.person.displayName })
    .from(schema.person)
    .where(eq(schema.person.id, principal.actingOperatorId))
    .limit(1);

  return (
    <div className="acting" role="status">
      <span>
        Acting as <strong>{principal.displayName}</strong>
        {rows[0] ? <> · {rows[0].name}</> : null}
      </span>
      <form action={endImpersonation}>
        <button type="submit">End</button>
      </form>
    </div>
  );
}
