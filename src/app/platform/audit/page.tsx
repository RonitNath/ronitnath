import type { Metadata } from 'next';
import Link from 'next/link';

import { pretty, stamp } from '@/features/platform/format';
import { auditCommands, listParties, readAudit } from '@/features/platform/queries';
import { tryDecodeId } from '@/lib/ids';
import { requireOperator } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Audit' };
export const dynamic = 'force-dynamic';

const TARGETS = ['person', 'organization', 'identity', 'session', 'link', 'match', 'document', 'event', 'group', 'party'];

/* The feed, read forward by id. The table is append-only, so a cursor is the
 * last id the previous page showed and nothing can slide underneath it — a
 * page numbered by offset would show the same row twice the moment a command
 * ran between two clicks. */
export default async function AuditPage({
  searchParams,
}: {
  searchParams: Promise<{ after?: string; actor?: string; command?: string; target?: string }>;
}) {
  await requireOperator();
  const query = await searchParams;
  const actor = query.actor ? tryDecodeId('person', query.actor) : null;
  const { rows, next } = await readAudit({
    after: query.after ? Number(query.after) : undefined,
    actor: actor ?? undefined,
    command: query.command || undefined,
    targetKind: query.target || undefined,
  });
  const [commands, parties] = await Promise.all([auditCommands(), listParties()]);
  const people = parties.filter((row) => row.kind === 'person');

  const params = new URLSearchParams();
  if (query.actor) params.set('actor', query.actor);
  if (query.command) params.set('command', query.command);
  if (query.target) params.set('target', query.target);
  const nextParams = new URLSearchParams(params);
  if (next !== null) nextParams.set('after', String(next));

  return (
    <main className="indoors">
      <h1>Audit</h1>
      <p className="note">
        One row per command, written inside that command&apos;s own transaction. Payloads carry
        what happened and never a secret.
      </p>

      <form className="filters" method="get">
        <div className="field">
          <label htmlFor="actor">Actor</label>
          <select id="actor" name="actor" defaultValue={query.actor ?? ''}>
            <option value="">Anyone</option>
            {people.map((row) => (
              <option key={row.publicId} value={row.publicId}>
                {row.name}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label htmlFor="command">Command</label>
          <select id="command" name="command" defaultValue={query.command ?? ''}>
            <option value="">Any</option>
            {commands.map((row) => (
              <option key={row} value={row}>
                {row}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label htmlFor="target">Target</label>
          <select id="target" name="target" defaultValue={query.target ?? ''}>
            <option value="">Any</option>
            {TARGETS.map((row) => (
              <option key={row} value={row}>
                {row}
              </option>
            ))}
          </select>
        </div>
        <button type="submit" className="commit">
          Filter
        </button>
      </form>

      <section>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>Id</th>
                <th>When</th>
                <th>Command</th>
                <th>Actor</th>
                <th>Target</th>
                <th>Payload</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.id}>
                  <td className="mono">{row.id}</td>
                  <td className="mono">{stamp(row.at)}</td>
                  <td>{row.command}</td>
                  <td>{row.actorName ?? '—'}</td>
                  <td>{row.targetKind ?? '—'}</td>
                  <td>{row.payload ? <pre className="payload">{pretty(row.payload)}</pre> : '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {rows.length === 0 ? (
          <p className="empty">Nothing matches that. The feed starts at the first command.</p>
        ) : null}
        <p className="pager">
          {next === null ? (
            <span className="note">The end of the feed.</span>
          ) : (
            <Link href={`/platform/audit?${nextParams.toString()}`}>Next</Link>
          )}
          {query.after ? (
            <>
              {' · '}
              <Link href={`/platform/audit?${params.toString()}`}>Back to the start</Link>
            </>
          ) : null}
        </p>
      </section>
    </main>
  );
}
