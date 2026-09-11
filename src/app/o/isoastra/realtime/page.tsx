import { presenceStatus } from '@isoastra/fleet-events/presence';
import { inspectRealtime } from '@/features/realtime/server';
import { RealtimeInspector } from '@/features/realtime/inspector';
import { requireOperator } from '@/lib/tiers';

export const dynamic = 'force-dynamic';
export default async function RealtimePage({
  searchParams,
}: {
  searchParams: Promise<{ view?: string }>;
}) {
  await requireOperator();
  const { view: selectedId } = await searchParams;
  const report = await inspectRealtime();
  const rows = report.views.map((view) => ({
    id: view.id,
    who: view.personId ? `person ${view.personId}` : `visitor ${view.visitorId.slice(0, 8)}`,
    path: view.path,
    status: presenceStatus(view).join(' · '),
    lastSeen: view.lastHeartbeatAt.toISOString(),
    visible:
      [
        ...view.visibleSections.map((item) => `§${item}`),
        ...view.visibleRows.map((item) => `row:${item}`),
      ].join(', ') || '—',
    subscriptions: report.subscriptions.filter((row) => row.viewId === view.id).length,
    deliveries: report.deliveries.filter((row) => row.viewId === view.id).length,
  }));
  const selected = report.views.find((view) => view.id === selectedId);
  const subscriptions = selected
    ? report.subscriptions.filter((row) => row.viewId === selected.id)
    : [];
  const deliveries = selected
    ? report.deliveries.filter((row) => row.viewId === selected.id)
    : [];
  return (
    <main className="indoors" data-realtime-section="views">
      <h1>Realtime</h1>
      <p className="note">
        What the server knows each browser is viewing, what it subscribed to, and why updates
        were delivered.
      </p>
      <section>
        <h2>Browser views</h2>
        <RealtimeInspector rows={rows} />
      </section>
      {selected ? (
        <section data-realtime-section="selected-view">
          <h2>{selected.title || selected.path}</h2>
          <dl className="facts">
            <div>
              <dt>Visitor</dt>
              <dd>{selected.visitorId}</dd>
            </div>
            <div>
              <dt>Tab</dt>
              <dd>{selected.tabId}</dd>
            </div>
            <div>
              <dt>Session</dt>
              <dd>{selected.sessionId ?? 'anonymous'}</dd>
            </div>
            <div>
              <dt>Path</dt>
              <dd>{selected.path}</dd>
            </div>
          </dl>
          <h3>Authorized subscriptions</h3>
          <pre className="payload">
            {JSON.stringify(
              subscriptions.map((row) => row.descriptor),
              null,
              2,
            )}
          </pre>
          <h3>Delivery decisions</h3>
          <div className="scroller">
            <table className="rows dense">
              <thead>
                <tr>
                  <th>Revision</th>
                  <th>Reason</th>
                  <th>Selected</th>
                  <th>Sent</th>
                  <th>Received</th>
                  <th>Applied</th>
                </tr>
              </thead>
              <tbody>
                {deliveries.map((row) => (
                  <tr key={row.id}>
                    <td className="mono">{row.revision}</td>
                    <td>{row.reason}</td>
                    <td className="mono">{row.selectedAt.toISOString()}</td>
                    <td>{row.sentAt ? 'yes' : 'waiting'}</td>
                    <td>{row.receivedAt ? 'yes' : 'waiting'}</td>
                    <td>{row.appliedAt ? 'yes' : 'waiting'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}
    </main>
  );
}
