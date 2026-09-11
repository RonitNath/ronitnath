import type { Metadata } from 'next';
import Link from 'next/link';

import { deploymentReport } from '@/features/platform/deployment';
import { requireOperator } from '@/lib/tiers';
import { operatorPath } from '@/lib/paths';

export const metadata: Metadata = { title: 'Platform' };
export const dynamic = 'force-dynamic';

/* The way in. Six surfaces and the five numbers that say how big the thing
 * behind them is; everything an operator does is one click from here. */
export default async function PlatformPage() {
  await requireOperator();
  const report = await deploymentReport();

  return (
    <main className="indoors">
      <h1>Platform</h1>
      <p className="note">Everything this deployment holds, and the commands over it.</p>

      <section>
        <h2>Now</h2>
        <dl className="facts">
          <div>
            <dt>People</dt>
            <dd>{report.counts.persons}</dd>
          </div>
          <div>
            <dt>Organizations</dt>
            <dd>{report.counts.organizations}</dd>
          </div>
          <div>
            <dt>Events</dt>
            <dd>{report.counts.events}</dd>
          </div>
          <div>
            <dt>Live sessions</dt>
            <dd>{report.counts.sessions}</dd>
          </div>
          <div>
            <dt>Audit rows</dt>
            <dd>{report.counts.audit}</dd>
          </div>
        </dl>
      </section>

      <section>
        <h2>Surfaces</h2>
        <ul className="pair-list">
          <li>
            <Link href={operatorPath('parties')}>Parties</Link>
          </li>
          <li>
            <Link href={operatorPath('matches')}>Matches</Link>
          </li>
          <li>
            <Link href={operatorPath('sessions')}>Sessions</Link>
          </li>
          <li>
            <Link href={operatorPath('audit')}>Audit</Link>
          </li>
          <li>
            <Link href={operatorPath('operators')}>Operators</Link>
          </li>
          <li>
            <Link href={operatorPath('deployment')}>Deployment</Link>
          </li>
          <li>
            <Link href={operatorPath('realtime')}>Realtime</Link>
          </li>
          <li>
            <Link href={operatorPath('configuration')}>Configuration</Link>
          </li>
        </ul>
      </section>
    </main>
  );
}
