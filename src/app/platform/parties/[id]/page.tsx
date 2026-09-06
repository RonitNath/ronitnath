import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';

import { disableParty, enableParty, revokeAllSessions, revokeSession, split } from '@/features/platform/actions';
import { Command } from '@/features/platform/components/commands';
import { agentOf, pretty, stamp } from '@/features/platform/format';
import { signInAs } from '@/features/platform/impersonation';
import { partyDetail } from '@/features/platform/queries';
import { impersonationEnabled } from '@/features/platform/reauth';
import { encodeId, tryDecodeId } from '@/lib/ids';
import { requireOperator } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Party' };
export const dynamic = 'force-dynamic';

export default async function PartyPage({ params }: { params: Promise<{ id: string }> }) {
  await requireOperator();
  const { id } = await params;
  const internal = tryDecodeId('person', id) ?? tryDecodeId('organization', id);
  if (internal === null) notFound();
  const party = await partyDetail(internal);
  if (party === null) notFound();

  const canImpersonate =
    impersonationEnabled() && party.kind === 'person' && party.state === 'active';

  return (
    <main className="indoors">
      <h1>{party.name}</h1>
      <p className="note">
        <span className="party-state" data-state={party.state}>
          {party.state}
        </span>{' '}
        · {party.kind} · <span className="mono">{party.publicId}</span>
        {party.mergedIntoName ? <> · folded into {party.mergedIntoName}</> : null}
      </p>

      <section>
        <h2>Commands</h2>
        <div className="command-row">
          {party.state === 'disabled' ? (
            <Command
              action={enableParty}
              fields={{ party: party.publicId }}
              label="Enable"
              weight="commit"
            />
          ) : (
            <Command
              action={disableParty}
              fields={{ party: party.publicId }}
              label="Disable"
              reason="Why"
              weight="commit"
            />
          )}
          {canImpersonate ? (
            <Command
              action={signInAs}
              fields={{ person: party.publicId }}
              label="Sign in as"
              reason="Why"
              weight="commit"
            />
          ) : null}
          {party.sessions.length > 0 ? (
            <Command
              action={revokeAllSessions}
              fields={{ person: party.publicId }}
              label="End every session"
            />
          ) : null}
        </div>
      </section>

      <section>
        <h2>Identities</h2>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>Source</th>
                <th>Subject</th>
                <th>Confirmed</th>
                <th>Factors</th>
              </tr>
            </thead>
            <tbody>
              {party.identities.map((row) => (
                <tr key={row.publicId}>
                  <td>{row.source}</td>
                  <td className="mono">{row.subject}</td>
                  <td className="mono">
                    {row.verifiedAt ? stamp(row.verifiedAt) : <span className="note">no</span>}
                  </td>
                  <td>
                    {row.factors.length === 0
                      ? '—'
                      : row.factors.map((factor) => factor.kind).join(', ')}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {party.identities.length === 0 ? (
          <p className="empty">No identities. An organization has none, and a held person may
            have only a handle.</p>
        ) : null}
      </section>

      <section>
        <h2>Sessions</h2>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>Where</th>
                <th>Door</th>
                <th>Address</th>
                <th>Last seen</th>
                <th>Acting operator</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {party.sessions.map((row) => (
                <tr key={row.publicId} title={row.userAgent ?? undefined}>
                  <td>{agentOf(row.userAgent)}</td>
                  <td>{row.source}</td>
                  <td className="mono">{row.ip ?? '—'}</td>
                  <td className="mono">{stamp(row.lastSeenAt)}</td>
                  <td>{row.actingOperator ?? '—'}</td>
                  <td className="actions">
                    <Command
                      action={revokeSession}
                      fields={{ session: row.publicId }}
                      label="Revoke"
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {party.sessions.length === 0 ? <p className="empty">No live sessions.</p> : null}
      </section>

      <section>
        <h2>Relations</h2>
        <ul className="pair-list">
          {party.relationsHeld.map((row, index) => (
            <li key={`held-${index}`}>
              holds {row.verb} on {row.label}
            </li>
          ))}
          {party.relationsGranted.map((row, index) => (
            <li key={`granted-${index}`}>
              {row.label} holds {row.verb} here
            </li>
          ))}
        </ul>
        {party.relationsHeld.length + party.relationsGranted.length === 0 ? (
          <p className="empty">No relations either way.</p>
        ) : null}
      </section>

      <section>
        <h2>Contacts</h2>
        <ul className="pair-list">
          {party.held_by.map((row) => (
            <li key={`by-${row.publicId}`}>
              held by <Link href={`/platform/parties/${row.publicId}`}>{row.name}</Link>
            </li>
          ))}
          {party.holds.map((row) => (
            <li key={`of-${row.publicId}`}>
              holds <Link href={`/platform/parties/${row.publicId}`}>{row.name}</Link>
            </li>
          ))}
        </ul>
        {party.held_by.length + party.holds.length === 0 ? (
          <p className="empty">Nobody holds this party, and it holds nobody.</p>
        ) : null}
      </section>

      <section>
        <h2>Merges</h2>
        <ul className="pair-list">
          {party.absorbed.map((row) => (
            <li key={row.publicId}>
              absorbed {row.name}
              {row.auditId === null ? (
                <span className="note"> · no undo record</span>
              ) : (
                <>
                  {' '}
                  <Command
                    action={split}
                    fields={{ merge: encodeId('audit', row.auditId) }}
                    label="Split"
                    reason="Why"
                  />
                </>
              )}
            </li>
          ))}
        </ul>
        {party.absorbed.length === 0 ? (
          <p className="empty">
            {party.mergedIntoName
              ? 'This person was folded into another one; the split is on the survivor.'
              : 'No merges.'}
          </p>
        ) : null}
      </section>

      <section>
        <h2>Audit</h2>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>When</th>
                <th>Command</th>
                <th>Actor</th>
                <th>Payload</th>
              </tr>
            </thead>
            <tbody>
              {party.audit.map((row) => (
                <tr key={row.id}>
                  <td className="mono">{stamp(row.at)}</td>
                  <td>{row.command}</td>
                  <td>{row.actorName ?? '—'}</td>
                  <td>
                    {row.payload ? <pre className="payload">{pretty(row.payload)}</pre> : '—'}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {party.audit.length === 0 ? <p className="empty">Nothing recorded about this party.</p> : null}
      </section>
    </main>
  );
}
