import type { Metadata } from 'next';

import { HoldForm, InviteButton, RevokeLinkButton } from '@/features/people/components/held';
import { HANDLE_LABEL, normalizeHandle } from '@/features/people/handles';
import type { LinkState } from '@/features/people/invitations';
import { listContacts } from '@/features/people/queries';
import { encodeId } from '@/lib/ids';
import { requireSubjectPerson } from '@/lib/tiers';

export const metadata: Metadata = { title: 'People' };

/* A state is a word in its own colour (docs/design.md). Five words, and none
 * of them is a capsule. */
const STATE_WORD: Record<LinkState, string> = {
  live: 'Not opened',
  opened: 'Opened',
  claimed: 'Claimed',
  revoked: 'Revoked',
  expired: 'Expired',
};

function day(at: Date | null): string {
  return at ? at.toISOString().slice(0, 10) : '';
}

export default async function PeoplePage({ params }: { params: Promise<{ user: string }> }) {
  const { user } = await params;
  const { subjectPersonId, viewingOther } = await requireSubjectPerson(user, 'people');
  const people = await listContacts(subjectPersonId);

  return (
    <main className="indoors">
      <h1>People</h1>
      <p className="note">
        People you have written down. Most of them have no account here; an invitation lets one of
        them take their entry over, and after that they are the same row and their own person.
      </p>

      {viewingOther ? null : (
        <section>
          <HoldForm />
        </section>
      )}

      <section>
        <h2>Contacts</h2>
        <div className="scroller">
          <table className="rows wide">
            <thead>
              <tr>
                <th>Name</th>
                <th>Handle</th>
                <th>Since</th>
                <th>Invitation</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {people.map((row) => {
                const handle = row.handle ? normalizeHandle(row.handle) : null;
                const state = row.link?.state;
                return (
                  <tr key={row.id}>
                    <td>
                      {row.displayName}
                      {row.held ? null : <span className="state" data-state="claimed"> · joined</span>}
                    </td>
                    <td className="mono" title={handle ? HANDLE_LABEL[handle.kind] : undefined}>
                      {row.handle ?? '—'}
                    </td>
                    <td className="num">
                      <time dateTime={row.createdAt.toISOString()}>{day(row.createdAt)}</time>
                    </td>
                    <td>
                      {state ? (
                        <span className="state" data-state={state}>
                          {STATE_WORD[state]}
                          {row.link?.claimedAt ? ` ${day(row.link.claimedAt)}` : null}
                          {state === 'opened' && row.link?.openedAt
                            ? ` ${day(row.link.openedAt)}`
                            : null}
                        </span>
                      ) : (
                        <span className="note">None</span>
                      )}
                    </td>
                    <td className="actions">
                      {row.held && !viewingOther ? (
                        <InviteButton
                          person={encodeId('person', row.id)}
                          label={row.link ? 'New link' : 'Invite'}
                        />
                      ) : null}
                      {row.link && !viewingOther && (state === 'live' || state === 'opened') ? (
                        <RevokeLinkButton link={encodeId('link', row.link.id)} />
                      ) : null}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
        {people.length === 0 ? (
          <p className="empty">
            Nobody yet. Holding someone writes down a name and a way to reach them; it creates no
            account and sends no mail.
          </p>
        ) : null}
      </section>
    </main>
  );
}
