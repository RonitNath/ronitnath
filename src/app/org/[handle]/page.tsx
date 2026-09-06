import type { Metadata } from 'next';
import Link from 'next/link';

import { listOrganizationDocuments } from '@/features/documents/queries';
import { listOrganizationGroups } from '@/features/groups/queries';
import {
  InviteForm,
  LeaveButton,
  ProfileForm,
  RemoveMemberButton,
  RevokeInvitationButton,
  RoleForm,
  TransferButton,
} from '@/features/organizations/components/org';
import { listInvitations, listMembers } from '@/features/organizations/queries';
import type { LinkState } from '@/features/people/invitations';
import { encodeId } from '@/lib/ids';
import { requireOrgOperator } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Organization' };

/* A state is a word in its own colour (docs/design.md). */
const STATE_WORD: Record<LinkState, string> = {
  live: 'Not opened',
  opened: 'Opened',
  claimed: 'Claimed',
  revoked: 'Revoked',
  expired: 'Expired',
};

export default async function OrgPage({ params }: { params: Promise<{ handle: string }> }) {
  const { handle } = await params;
  const { organization, principal } = await requireOrgOperator(handle);
  const id = encodeId('organization', organization.id);

  const [members, invitations, groups, documents] = await Promise.all([
    listMembers(organization.id),
    listInvitations(organization.id),
    listOrganizationGroups(organization.id, principal.personId),
    listOrganizationDocuments(organization.id),
  ]);
  const pending = invitations.filter((row) => row.state !== 'claimed');
  const claimed = new Set(invitations.filter((row) => row.state === 'claimed').map((r) => r.personId));

  return (
    <main className="indoors">
      <h1>{organization.name}</h1>
      <p className="note">
        <span className="mono">/org/{organization.handle}</span>
      </p>

      <section>
        <h2>Members</h2>
        <div className="scroller">
          <table className="rows wide">
            <thead>
              <tr>
                <th>Name</th>
                <th>Role</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {members
                .filter((row) => !row.held || claimed.has(row.id))
                .map((row) => (
                  <tr key={row.id}>
                    <td>
                      {row.displayName}
                      {row.held ? (
                        <span className="state" data-state="live">
                          {' '}
                          · invited
                        </span>
                      ) : null}
                    </td>
                    <td>{row.role}</td>
                    <td className="actions">
                      <RoleForm
                        organization={id}
                        person={encodeId('person', row.id)}
                        role={row.role}
                      />
                      {/* Leaving is the verb for yourself, and handing an
                          organization to the person already holding it is
                          nothing at all. */}
                      {row.id === principal.personId ? null : (
                        <>
                          <RemoveMemberButton
                            organization={id}
                            person={encodeId('person', row.id)}
                          />
                          <TransferButton organization={id} person={encodeId('person', row.id)} />
                        </>
                      )}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
        <p className="aside-line">
          <LeaveButton organization={id} />
        </p>
      </section>

      <section>
        <h2>Invitations</h2>
        <div className="scroller">
          <table className="rows wide">
            <thead>
              <tr>
                <th>Name</th>
                <th>Role</th>
                <th>State</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {pending.map((row) => (
                <tr key={row.linkId}>
                  <td>{row.displayName}</td>
                  <td>{row.role}</td>
                  <td>
                    <span className="state" data-state={row.state}>
                      {STATE_WORD[row.state]}
                    </span>
                  </td>
                  <td className="actions">
                    {row.state === 'live' || row.state === 'opened' ? (
                      <RevokeInvitationButton organization={id} link={encodeId('link', row.linkId)} />
                    ) : null}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {pending.length === 0 ? <p className="empty">None out.</p> : null}
        <InviteForm organization={id} />
      </section>

      <section>
        <h2>Groups</h2>
        <div className="scroller">
          <table className="rows">
            <thead>
              <tr>
                <th>Name</th>
                <th className="num">Members</th>
              </tr>
            </thead>
            <tbody>
              {groups.map((row) => (
                <tr key={row.id}>
                  <td>{row.name}</td>
                  <td className="num">{row.members}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {groups.length === 0 ? (
          <p className="empty">
            None. A group is made on <Link href="/app/groups">your groups page</Link> and owned by
            whoever made it.
          </p>
        ) : null}
      </section>

      <section>
        <h2>Documents</h2>
        <div className="scroller">
          <table className="rows wide">
            <thead>
              <tr>
                <th>Title</th>
                <th>Public</th>
              </tr>
            </thead>
            <tbody>
              {documents.map((row) => (
                <tr key={row.id}>
                  <td>
                    <Link href={`/app/documents/${encodeId('document', row.id)}`}>{row.title}</Link>
                  </td>
                  <td>
                    {row.publishedAt && row.slug ? (
                      <span className="state" data-state="claimed">
                        published
                      </span>
                    ) : (
                      <span className="state" data-state="live">
                        draft
                      </span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {documents.length === 0 ? <p className="empty">None yet.</p> : null}
      </section>

      <section>
        <h2>Settings</h2>
        <ProfileForm organization={id} name={organization.name} />
      </section>
    </main>
  );
}
