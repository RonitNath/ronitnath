import type { Metadata } from 'next';
import Link from 'next/link';

import { grantOperator, revokeOperator } from '@/features/platform/actions';
import { ChoiceCommand, Command } from '@/features/platform/components/commands';
import { listOperators } from '@/features/platform/deployment';
import { stamp } from '@/features/platform/format';
import { livePeople } from '@/features/platform/match-queue';
import { isFresh, REAUTH_WINDOW_MINUTES } from '@/features/platform/reauth';
import { requireOperator } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Operators' };
export const dynamic = 'force-dynamic';

/* God mode is a relation held by a few, and it is delegated on the deployment
 * itself. The last one may not be taken away — a deployment with no operator
 * is a deployment nobody can fix. */
export default async function OperatorsPage() {
  const { principal } = await requireOperator();
  const [operators, people] = await Promise.all([listOperators(), livePeople()]);
  const held = new Set(operators.map((row) => row.publicId));
  const candidates = people.filter((row) => !held.has(row.publicId));
  const fresh = isFresh(principal.reauthenticatedAt);

  return (
    <main className="indoors">
      <h1>Operators</h1>
      <p className="note">
        The relation <span className="mono">operator</span> on{' '}
        <span className="mono">platform:*</span>. Granting and revoking it, and every command that
        destroys or impersonates, asks for a password inside the last {REAUTH_WINDOW_MINUTES}{' '}
        minutes.
      </p>

      <section>
        <h2>Held by</h2>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>Person</th>
                <th>Since</th>
                <th>Id</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {operators.map((row) => (
                <tr key={row.publicId}>
                  <td>
                    <Link href={`/platform/parties/${row.publicId}`}>{row.name}</Link>
                    {row.publicId === undefined ? null : null}
                  </td>
                  <td className="mono">{stamp(row.since)}</td>
                  <td className="mono">{row.publicId}</td>
                  <td className="actions">
                    <Command
                      action={revokeOperator}
                      fields={{ person: row.publicId }}
                      label="Revoke"
                      reason="Why"
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {operators.length === 0 ? (
          <p className="empty">Nobody holds it, which should be impossible from here.</p>
        ) : null}
      </section>

      <section>
        <h2>Grant</h2>
        {candidates.length === 0 ? (
          <p className="empty">Everybody live already holds it.</p>
        ) : (
          <ChoiceCommand
            action={grantOperator}
            name="person"
            label="Grant"
            reason="Why"
            choices={candidates.map((row) => ({ value: row.publicId, label: row.name }))}
          />
        )}
      </section>

      <section>
        <h2>This session</h2>
        <p className="note">
          {fresh
            ? `Confirmed within the last ${REAUTH_WINDOW_MINUTES} minutes.`
            : 'Not confirmed. A sharp command will ask.'}{' '}
          <Link href="/platform/reauth?next=/platform/operators">Confirm now</Link>
        </p>
      </section>
    </main>
  );
}
