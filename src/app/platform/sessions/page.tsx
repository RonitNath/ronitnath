import type { Metadata } from 'next';
import Link from 'next/link';

import { revokeAllSessions, revokeSession } from '@/features/platform/actions';
import { Command } from '@/features/platform/components/commands';
import { agentOf, day, stamp } from '@/features/platform/format';
import { listSessions } from '@/features/platform/queries';
import { requireOperator } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Sessions' };
export const dynamic = 'force-dynamic';

const DOOR: Record<string, string> = { local: 'Password', oidc: 'Isoastra' };

export default async function PlatformSessionsPage() {
  await requireOperator();
  const sessions = await listSessions();

  return (
    <main className="indoors">
      <h1>Sessions</h1>
      <p className="note">Every browser currently holding a key to this deployment.</p>

      <section>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>Person</th>
                <th>Where</th>
                <th>Door</th>
                <th>Address</th>
                <th>Last seen</th>
                <th>Expires</th>
                <th>Acting operator</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {sessions.map((row) => (
                <tr key={row.publicId} title={row.userAgent ?? undefined}>
                  <td>
                    <Link href={`/platform/parties/${row.personPublicId}`}>{row.personName}</Link>
                  </td>
                  <td>{agentOf(row.userAgent)}</td>
                  <td>{DOOR[row.source] ?? row.source}</td>
                  <td className="mono">{row.ip ?? '—'}</td>
                  <td className="mono">{stamp(row.lastSeenAt)}</td>
                  <td className="mono">{day(row.expiresAt)}</td>
                  <td>{row.actingOperator ?? '—'}</td>
                  <td className="actions">
                    <Command
                      action={revokeSession}
                      fields={{ session: row.publicId }}
                      label="Revoke"
                    />
                    <Command
                      action={revokeAllSessions}
                      fields={{ person: row.personPublicId }}
                      label="All theirs"
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {sessions.length === 0 ? (
          <p className="empty">Nobody is signed in. A session appears the moment somebody is.</p>
        ) : null}
      </section>
    </main>
  );
}
