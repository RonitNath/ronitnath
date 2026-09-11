import type { Metadata } from 'next';
import Link from 'next/link';

import { stamp } from '@/features/platform/format';
import { listParties } from '@/features/platform/queries';
import { requireOperator } from '@/lib/tiers';
import { operatorPath } from '@/lib/paths';

export const metadata: Metadata = { title: 'Parties' };
export const dynamic = 'force-dynamic';

const KIND: Record<string, string> = {
  person: 'Person',
  organization: 'Organization',
  service: 'Service',
};

export default async function PartiesPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string }>;
}) {
  await requireOperator();
  const { q } = await searchParams;
  const parties = await listParties(q);

  return (
    <main className="indoors">
      <h1>Parties</h1>
      <p className="note">Every person and every organization this deployment knows.</p>

      <form className="filters" method="get">
        <div className="field">
          <label htmlFor="q">Search</label>
          <input id="q" name="q" type="search" defaultValue={q ?? ''} placeholder="Name, address, handle, or id" />
        </div>
        <button type="submit" className="commit">
          Find
        </button>
        {q ? <Link href={operatorPath('parties')}>Clear</Link> : null}
      </form>

      <section>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>Name</th>
                <th>Kind</th>
                <th>State</th>
                <th>Handle</th>
                <th>Id</th>
                <th>Created</th>
              </tr>
            </thead>
            <tbody>
              {parties.map((row) => (
                <tr key={row.publicId}>
                  <td>
                    <Link href={`${operatorPath('parties')}/${row.publicId}`}>{row.name}</Link>
                    {row.held ? <span className="note"> · held</span> : null}
                  </td>
                  <td>{KIND[row.kind] ?? row.kind}</td>
                  <td>
                    <span className="party-state" data-state={row.state}>
                      {row.state}
                    </span>
                  </td>
                  <td className="mono">{row.handle ?? '—'}</td>
                  <td className="mono">{row.publicId}</td>
                  <td className="mono">
                    <time dateTime={row.createdAt.toISOString()}>{stamp(row.createdAt)}</time>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {parties.length === 0 ? (
          <p className="empty">
            {q
              ? 'Nothing here spells that. A search is a seek, not a guess.'
              : 'No parties yet. A party appears when somebody registers, is held, or an organization is made.'}
          </p>
        ) : null}
      </section>
    </main>
  );
}
