import type { Metadata } from 'next';
import Link from 'next/link';

import { proposeMatch, ruleMatch, split } from '@/features/platform/actions';
import { ChoiceCommand, Command, PairCommand } from '@/features/platform/components/commands';
import { stamp } from '@/features/platform/format';
import { listMerges, listQueue, livePeople } from '@/features/platform/match-queue';
import { encodeId } from '@/lib/ids';
import { requireOperator } from '@/lib/tiers';
import { operatorPath } from '@/lib/paths';

export const metadata: Metadata = { title: 'Matches' };
export const dynamic = 'force-dynamic';

/* Two questions on one page. The open ones — proposed by the model or by an
 * operator's own eye — and the ones already answered by a merge, because the
 * only thing to do about a wrong answer is take it back. */
export default async function MatchesPage() {
  await requireOperator();
  const [queue, merges, people] = await Promise.all([listQueue(), listMerges(), livePeople()]);
  const choices = people.map((row) => ({ value: row.publicId, label: row.name }));

  return (
    <main className="indoors">
      <h1>Matches</h1>
      <p className="note">Two identities that might be one person. Nothing merges on a score.</p>

      <section id="open">
        <h2>Open</h2>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>Signal</th>
                <th>Sides</th>
                <th>Evidence</th>
                <th>Asked</th>
                <th>Rule</th>
              </tr>
            </thead>
            <tbody>
              {queue.map((row) => (
                <tr key={row.publicId}>
                  <td>{row.signal.replace('_', ' ')}</td>
                  <td>
                    {row.sides.map((side) => (
                      <div key={side.publicId}>
                        <Link href={`${operatorPath('parties')}/${side.personPublicId}`}>{side.name}</Link>{' '}
                        <span className="note">
                          {side.source} · {side.subject}
                        </span>
                      </div>
                    ))}
                  </td>
                  <td className="wrap">{row.evidence ?? '—'}</td>
                  <td className="mono">{stamp(row.createdAt)}</td>
                  <td className="actions">
                    <ChoiceCommand
                      action={ruleMatch}
                      fields={{ match: row.publicId }}
                      name="survivor"
                      label="Merge"
                      reason="Why"
                      choices={row.sides.map((side) => ({
                        value: side.personPublicId,
                        label: `${side.name} survives`,
                      }))}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {queue.length === 0 ? (
          <p className="empty">
            No open questions. One appears when a confirmed address meets somebody&apos;s contact
            card, or when an operator proposes a pair below.
          </p>
        ) : null}
      </section>

      <section>
        <h2>Propose</h2>
        <p className="note">Two parties you believe are one person. It writes a question.</p>
        {choices.length >= 2 ? (
          <PairCommand action={proposeMatch} label="Propose" choices={choices} />
        ) : (
          <p className="empty">There have to be two live people to propose a pair.</p>
        )}
      </section>

      <section id="merged">
        <h2>Merged</h2>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>When</th>
                <th>Survivor</th>
                <th>Absorbed</th>
                <th>How</th>
                <th>Split</th>
              </tr>
            </thead>
            <tbody>
              {merges.map((row) => (
                <tr key={row.auditId}>
                  <td className="mono">{stamp(row.at)}</td>
                  <td>
                    <Link href={`${operatorPath('parties')}/${row.survivor.publicId}`}>
                      {row.survivor.name}
                    </Link>
                  </td>
                  <td>
                    <Link href={`${operatorPath('parties')}/${row.absorbed.publicId}`}>
                      {row.absorbed.name}
                    </Link>
                  </td>
                  <td>{row.method}</td>
                  <td className="actions">
                    <Command
                      action={split}
                      fields={{ merge: encodeId('audit', row.auditId) }}
                      label="Split"
                      reason="Why"
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {merges.length === 0 ? <p className="empty">No merges to take back.</p> : null}
      </section>
    </main>
  );
}
