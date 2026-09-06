import type { Metadata } from 'next';

import { listIdentities } from '@/features/auth/queries';
import {
  AddEmailForm,
  ConfirmMatchButton,
  DisplayNameForm,
  RemoveIdentityButton,
} from '@/features/people/components/identities';
import { HANDLE_LABEL, normalizeHandle } from '@/features/people/handles';
import { listProposals } from '@/features/people/queries';
import { encodeId } from '@/lib/ids';
import { requireMember } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Account' };

const SOURCE_LABEL: Record<string, string> = {
  local: 'Password',
  oidc: 'Isoastra',
  handle: 'Handle',
};

export default async function AppHome() {
  const { principal } = await requireMember('/app');
  const [identities, proposals] = await Promise.all([
    listIdentities(principal.personId),
    listProposals(principal.personId),
  ]);
  const doors = identities.filter((row) => row.source !== 'handle').length;

  return (
    <main className="indoors">
      <h1>{principal.displayName}</h1>
      <p className="note">{principal.isOperator ? 'Platform operator' : 'Member'}</p>

      {proposals.length > 0 ? (
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
        <h2>Name</h2>
        <DisplayNameForm displayName={principal.displayName} />
      </section>

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
                      {row.source === 'handle' || doors > 1 ? (
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
        <AddEmailForm />
      </section>
    </main>
  );
}
