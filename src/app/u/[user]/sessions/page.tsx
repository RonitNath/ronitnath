import type { Metadata } from 'next';
import { notFound } from 'next/navigation';

import { RevokeButton } from '@/features/auth/components/revoke';
import { listSessions } from '@/features/auth/queries';
import { requireSubjectPerson } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Sessions' };

const SOURCE_LABEL: Record<string, string> = { local: 'Password', oidc: 'Isoastra' };

/* One account, one door: which one it is belongs to the account, not to each
 * session, so every row says the same thing and it is still the thing a
 * reader wants to see beside "this one". */

/* A user agent string is a paragraph nobody reads. What a person recognises is
 * the browser and the platform, so that is what the column says; the whole
 * string is on the row's title for anyone who wants it. */
function agentOf(raw: string | null): string {
  if (!raw) return 'Unknown';
  const browser = /Firefox\/[\d.]+/.test(raw)
    ? 'Firefox'
    : /Edg\//.test(raw)
      ? 'Edge'
      : /Chrome\//.test(raw)
        ? 'Chrome'
        : /Safari\//.test(raw)
          ? 'Safari'
          : 'Browser';
  const platform = /Android/.test(raw)
    ? 'Android'
    : /iPhone|iPad/.test(raw)
      ? 'iOS'
      : /Mac OS X/.test(raw)
        ? 'macOS'
        : /Windows/.test(raw)
          ? 'Windows'
          : /Linux/.test(raw)
            ? 'Linux'
            : '';
  return platform ? `${browser} on ${platform}` : browser;
}

/* Self only. A session belongs to the browser holding it, and the list an
 * operator wants — every live session on the site, with the command to end
 * one — is its own surface at `/o/isoastra/sessions`. Reading it from under
 * somebody else's path would be a second, unaudited way to the same rows. */
export default async function SessionsPage({ params }: { params: Promise<{ user: string }> }) {
  const { user } = await params;
  const { principal, viewingOther } = await requireSubjectPerson(user, 'sessions');
  if (viewingOther) notFound();
  const sessions = await listSessions();

  return (
    <main className="indoors">
      <h1>Sessions</h1>
      <p className="note">Every browser currently holding a key to this account.</p>

      <section>
        <div className="scroller">
          <table className="rows">
            <thead>
              <tr>
                <th>Where</th>
                <th>Door</th>
                <th>Address</th>
                <th>Last seen</th>
                <th>Expires</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {sessions.map((row) => {
                const current = row.id === principal.sessionId;
                return (
                  <tr key={row.id} data-current={current} title={row.userAgent ?? undefined}>
                    <td>
                      {agentOf(row.userAgent)}
                      {current ? <span className="current"> · this one</span> : null}
                    </td>
                    <td>{SOURCE_LABEL[principal.source] ?? principal.source}</td>
                    <td className="mono">{row.ip ?? '—'}</td>
                    <td className="num">
                      <time dateTime={row.lastSeenAt.toISOString()}>
                        {row.lastSeenAt.toISOString().slice(0, 16).replace('T', ' ')}
                      </time>
                    </td>
                    <td className="num">
                      <time dateTime={row.expiresAt.toISOString()}>
                        {row.expiresAt.toISOString().slice(0, 10)}
                      </time>
                    </td>
                    <td>{current ? null : <RevokeButton session={row.handle} />}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
        {sessions.length === 0 ? (
          <p className="empty">No live sessions. Signing in anywhere adds one here.</p>
        ) : null}
      </section>
    </main>
  );
}
