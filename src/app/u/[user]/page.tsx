import type { Metadata } from 'next';
import Link from 'next/link';

import { listIdentities } from '@/features/auth/queries';
import {
  ConfirmMatchButton,
  DisplayNameForm,
  RemoveIdentityButton,
} from '@/features/people/components/identities';
import { HANDLE_LABEL, normalizeHandle } from '@/features/people/handles';
import { CreateOrganizationForm } from '@/features/organizations/components/org';
import { listMemberships } from '@/features/organizations/queries';
import { listProposals } from '@/features/people/queries';
import { encodeId } from '@/lib/ids';
import { orgPath } from '@/lib/paths';
import { requireSubjectPerson } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Account' };

const SOURCE_LABEL: Record<string, string> = {
  local: 'Password',
  oidc: 'Isoastra',
  handle: 'Handle',
};

/* The account page of whoever the path names. An operator reading somebody
 * else's sees the same rows and none of the controls: every command on this
 * page acts on the actor's own account, and a button that quietly did that
 * from under another person's name is the bug this page must not have. */
export default async function AccountPage({ params }: { params: Promise<{ user: string }> }) {
  const { user } = await params;
  const { principal, subjectPersonId, subjectName, viewingOther } =
    await requireSubjectPerson(user);
  const [identities, proposals, memberships] = await Promise.all([
    listIdentities(subjectPersonId),
    listProposals(subjectPersonId),
    listMemberships(subjectPersonId),
  ]);
  const doors = identities.filter((row) => row.source !== 'handle').length;

  return (
    <main className="indoors">
      <h1>{subjectName}</h1>
      <p className="note">{principal.isOperator ? 'Platform operator' : 'Member'}</p>

      {proposals.length > 0 && !viewingOther ? (
        <section>
          <h2>Is this you?</h2>
          {proposals.map((row) => (
            <div className="proposal" key={row.id}>
              <p>
                {row.holderName ?? 'Someone'} holds a contact called{' '}
                <strong>{row.heldName}</strong> at <span className="mono">{row.subject}</span>, an
                address you have confirmed.
              </p>
              <ConfirmMatchButton match={encodeId('match', row.id)} />
            </div>
          ))}
        </section>
      ) : null}

      <section>
        <h2>Organizations</h2>
        <div className="scroller">
          <table className="rows">
            <thead>
              <tr>
                <th>Name</th>
                <th>Handle</th>
                <th>You are</th>
              </tr>
            </thead>
            <tbody>
              {memberships.map((row) => (
                <tr key={row.id}>
                  <td>
                    {row.role === 'member' ? (
                      row.name
                    ) : (
                      <Link href={orgPath(row.handle)}>{row.name}</Link>
                    )}
                  </td>
                  <td className="mono">{orgPath(row.handle)}</td>
                  <td>{row.role}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {memberships.length === 0 ? (
          <p className="empty">None. An organization is a name, a handle, and whoever you ask in.</p>
        ) : null}
        {viewingOther ? null : <CreateOrganizationForm />}
      </section>

      {viewingOther ? null : (
        <section>
          <h2>Name</h2>
          <DisplayNameForm displayName={principal.displayName} />
        </section>
      )}

      <section>
        <h2>Identities</h2>
        <div className="scroller">
          <table className="rows">
            <thead>
              <tr>
                <th>Door</th>
                <th>Subject</th>
                <th>Confirmed</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {identities.map((row) => {
                const handle = row.source === 'handle' ? normalizeHandle(row.subject) : null;
                return (
                  <tr key={row.id}>
                    <td title={handle ? HANDLE_LABEL[handle.kind] : undefined}>
                      {SOURCE_LABEL[row.source] ?? row.source}
                    </td>
                    <td className="mono">{row.subject}</td>
                    <td>
                      {row.verifiedAt ? (
                        <time dateTime={row.verifiedAt.toISOString()}>
                          {row.verifiedAt.toISOString().slice(0, 10)}
                        </time>
                      ) : (
                        <span className="state" data-state={row.source === 'handle' ? 'live' : 'opened'}>
                          {row.source === 'handle' ? 'not a door' : 'not yet'}
                        </span>
                      )}
                    </td>
                    <td className="actions">
                      {!viewingOther && (row.source === 'handle' || doors > 1) ? (
                        <RemoveIdentityButton identity={encodeId('identity', row.id)} />
                      ) : null}
                    </td>
                  </tr>
                );
              })}
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
