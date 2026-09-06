import type { Metadata } from 'next';

import { listIdentities } from '@/features/auth/queries';
import { requireMember } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Account' };

const SOURCE_LABEL: Record<string, string> = { local: 'Password', oidc: 'Isoastra' };

export default async function AppHome() {
  const { principal } = await requireMember('/app');
  const identities = await listIdentities(principal.personId);

  return (
    <main className="indoors">
      <h1>{principal.displayName}</h1>
      <p className="note">{principal.isOperator ? 'Platform operator' : 'Member'}</p>

      <section>
        <h2>Identities</h2>
        <div className="scroller">
          <table className="rows">
            <thead>
              <tr>
                <th>Door</th>
                <th>Subject</th>
                <th>Confirmed</th>
              </tr>
            </thead>
            <tbody>
              {identities.map((row) => (
                <tr key={row.id}>
                  <td>{SOURCE_LABEL[row.source] ?? row.source}</td>
                  <td className="mono">{row.subject}</td>
                  <td>
                    {row.verifiedAt ? (
                      <time dateTime={row.verifiedAt.toISOString()}>
                        {row.verifiedAt.toISOString().slice(0, 10)}
                      </time>
                    ) : (
                      <span className="note">not yet</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {identities.length === 0 ? (
          <p className="empty">
            The doors this account can be reached through. There are none on file.
          </p>
        ) : null}
      </section>
    </main>
  );
}
