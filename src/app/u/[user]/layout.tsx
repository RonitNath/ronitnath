/* The chrome over one person's surfaces.
 *
 * It draws the nav and the band that says whose page is being read, and it
 * decides nothing. A layout in Next is not a gate: it does not re-run for a
 * client-side navigation between two of its own children, and — the reason
 * this one was rewritten — it wraps pages whose authority is not the same
 * question. A document shared with somebody is opened at its owner's path by
 * a reader who is not the owner, and a layout that insisted on the subject
 * turned every share into a 404. Each page asks for what it needs.
 *
 * What the layout does own is the `[user]` segment, which every link in the
 * nav has to carry so that a reader stays on the subject they are reading. */

import { and, eq, isNull } from 'drizzle-orm';

import { database, schema } from '@/db/client';
import { Shell } from '@/features/auth/components/shell';
import { currentPrincipal } from '@/features/auth/principal';
import { tryDecodeId } from '@/lib/ids';

import '../../indoors.css';

export const dynamic = 'force-dynamic';

/** The subject's name, when the subject is not the reader. Null otherwise —
 *  including for an id that names nobody, because the page underneath is
 *  where that is answered. */
async function otherSubject(user: string, personId: number): Promise<string | null> {
  const subject = tryDecodeId('person', user);
  if (subject === null || subject === personId) return null;
  const rows = await database()
    .select({ displayName: schema.person.displayName })
    .from(schema.person)
    .where(and(eq(schema.person.id, subject), isNull(schema.person.mergedInto)))
    .limit(1);
  return rows[0]?.displayName ?? null;
}

export default async function SubjectLayout({
  children,
  params,
}: {
  children: React.ReactNode;
  params: Promise<{ user: string }>;
}) {
  const { user } = await params;
  const principal = await currentPrincipal();
  const subjectName = principal ? await otherSubject(user, principal.personId) : null;

  return (
    <>
      {principal ? <Shell principal={principal} user={user} /> : null}
      {/* Actor from the session, subject from the path: when they differ the
          shell says so on every page, and every write underneath carries both
          into the audit row (fleet-conventions §1). */}
      {subjectName ? (
        <div className="acting" role="status">
          <span>
            Reading <strong>{subjectName}</strong>
          </span>
        </div>
      ) : null}
      {children}
    </>
  );
}
