import type { Metadata } from 'next';

import { deploymentReport } from '@/features/platform/deployment';
import { stamp } from '@/features/platform/format';
import { impersonationEnabled } from '@/features/platform/reauth';
import { requireOperator } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Deployment' };
export const dynamic = 'force-dynamic';

/* Every number here is one the process just measured or just counted. The
 * round trip is timed around one `select 1`; the migrations are the rows
 * drizzle wrote when it applied them, paired by order with the journal in the
 * tree. Nothing is a rate nobody observed. */
export default async function DeploymentPage() {
  await requireOperator();
  const report = await deploymentReport();

  return (
    <main className="indoors">
      <h1>Deployment</h1>
      <p className="note">What this process can see of itself, right now.</p>

      <section>
        <h2>Process</h2>
        <dl className="facts">
          <div>
            <dt>Version</dt>
            <dd>{report.version}</dd>
          </div>
          <div>
            <dt>Database round trip</dt>
            <dd>{report.latencyMs} ms</dd>
          </div>
          <div>
            <dt>Impersonation</dt>
            <dd>{impersonationEnabled() ? 'on' : 'off'}</dd>
          </div>
        </dl>
      </section>

      <section>
        <h2>Counts</h2>
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
        <h2>Migrations</h2>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>Migration</th>
                <th>Applied</th>
              </tr>
            </thead>
            <tbody>
              {report.migrations.map((row) => (
                <tr key={row.tag}>
                  <td className="mono">{row.tag}</td>
                  <td className="mono">
                    {row.appliedAt ? stamp(row.appliedAt) : <span className="note">not applied</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </main>
  );
}
