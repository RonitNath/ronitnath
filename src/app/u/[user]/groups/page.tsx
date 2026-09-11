import type { Metadata } from 'next';

import {
  AddToGroupForm,
  CreateGroupForm,
  DeleteGroupButton,
  RemoveFromGroupButton,
  RenameGroupForm,
} from '@/features/groups/components/groups';
import { listGroupMembers, listGroups } from '@/features/groups/queries';
import { listMemberships } from '@/features/organizations/queries';
import { listContacts } from '@/features/people/queries';
import { encodeId } from '@/lib/ids';
import { requireSubjectPerson } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Groups' };

export default async function GroupsPage({ params }: { params: Promise<{ user: string }> }) {
  const { user } = await params;
  const { principal, reader, viewingOther } = await requireSubjectPerson(user, 'groups');
  const me = reader;
  const [groups, memberships, contacts] = await Promise.all([
    listGroups(me.personId),
    listMemberships(me.personId),
    listContacts(me.personId),
  ]);
  const members = await Promise.all(groups.map((group) => listGroupMembers(group.id)));

  const owners = [
    { value: 'me', label: principal.displayName },
    ...memberships
      .filter((row) => row.role !== 'member')
      .map((row) => ({ value: encodeId('organization', row.id), label: row.name })),
  ];

  return (
    <main className="indoors">
      <h1>Groups</h1>
      <p className="note">
        A group is who you mean when you say a word. It orders the guests on an event page, and it
        is the thing a document is shared with when the share is not one person.
      </p>

      {viewingOther ? null : (
        <section>
          <CreateGroupForm owners={owners} />
        </section>
      )}

      {groups.map((group, index) => {
        const id = encodeId('group', group.id);
        const inside = members[index] ?? [];
        const held = new Set(inside.map((row) => `${row.subject.kind}:${row.subject.id}`));
        const candidates = [
          ...contacts
            .filter((row) => !held.has(`person:${row.id}`))
            .map((row) => ({ value: encodeId('person', row.id), label: row.displayName })),
          ...groups
            .filter((row) => row.id !== group.id && !held.has(`group:${row.id}`))
            .map((row) => ({ value: encodeId('group', row.id), label: `${row.name} (group)` })),
        ];
        return (
          <section key={group.id}>
            <h2>
              {group.name} <span className="state" data-state="claimed">{group.role}</span>
            </h2>
            <p className="note">Owned by {group.ownerName}.</p>
            <div className="scroller">
              <table className="rows">
                <thead>
                  <tr>
                    <th>Member</th>
                    <th>Kind</th>
                    <th>Role</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {inside.map((row) => (
                    <tr key={`${row.subject.kind}:${row.subject.id}`}>
                      <td>{row.displayName}</td>
                      <td className="mono">{row.subject.kind === 'group' ? 'group' : row.held ? 'held' : 'person'}</td>
                      <td>{row.role}</td>
                      <td className="actions">
                        {group.role === 'member' || viewingOther ? null : (
                          <RemoveFromGroupButton
                            group={id}
                            member={encodeId(
                              row.subject.kind === 'group' ? 'group' : 'person',
                              row.subject.id,
                            )}
                          />
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {inside.length === 0 ? <p className="empty">Nobody in it yet.</p> : null}
            {group.role === 'member' || viewingOther ? null : (
              <>
                <AddToGroupForm group={id} candidates={candidates} />
                <RenameGroupForm group={id} name={group.name} />
              </>
            )}
            {group.role === 'owner' && !viewingOther ? (
              <div className="aside-line">
                <DeleteGroupButton group={id} />
              </div>
            ) : null}
          </section>
        );
      })}

      {groups.length === 0 ? (
        <p className="empty">No groups yet. One name, and whoever belongs under it.</p>
      ) : null}
    </main>
  );
}
