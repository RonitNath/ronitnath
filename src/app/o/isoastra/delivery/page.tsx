import { deliveryReport } from '@/features/delivery/model';
import { DeliveryInspector } from '@/features/delivery/inspector';
import { requireOperator } from '@/lib/tiers';

export const dynamic = 'force-dynamic';

export default async function DeliveryPage({ searchParams }: { searchParams: Promise<{ run?: string }> }) {
  await requireOperator();
  const { run } = await searchParams;
  const report = await deliveryReport(run);
  const rows = report.runs.map((item) => ({ id: item.id, sha: item.requestedSha, state: item.state, started: item.startedAt.toISOString(), updated: item.updatedAt.toISOString(), lastSeq: item.lastSeq }));
  return <main className="indoors" data-realtime-section="delivery-runs">
    <h1>Delivery</h1>
    <p className="note">Measured release execution, migration compatibility, and replica outcomes reported by the delivery runner.</p>
    <section><h2>Runs</h2><DeliveryInspector rows={rows} /></section>
    {report.selected ? <section data-realtime-section="delivery-run">
      <h2>{report.selected.requestedSha}</h2>
      <dl className="facts"><div><dt>State</dt><dd>{report.selected.state}</dd></div><div><dt>Last sequence</dt><dd>{report.selected.lastSeq}</dd></div><div><dt>Reporting</dt><dd>{Date.now() - report.selected.updatedAt.getTime() < 60_000 ? 'connected' : 'stale'}</dd></div></dl>
      <div className="scroller"><table className="rows dense"><thead><tr><th>Seq</th><th>Stage/event</th><th>State</th><th>Wall</th><th>CPU</th><th>Peak memory</th><th>I/O</th></tr></thead><tbody>{report.events.map(({ event }) => {
        const result = event.type === 'stage-finished' ? event.result : null;
        return <tr key={event.seq}><td className="mono">{event.seq}</td><td>{event.type === 'stage-started' ? event.stageId : result?.id ?? event.type}</td><td>{result?.state ?? '—'}</td><td className="mono">{result?.wallMs == null ? '—' : `${result.wallMs} ms`}</td><td className="mono">{result?.cpuMs == null ? '—' : `${result.cpuMs} ms`}</td><td className="mono">{result?.peakMemoryBytes == null ? '—' : `${Math.round(result.peakMemoryBytes / 1048576)} MiB`}</td><td className="mono">{result?.ioReadBytes == null ? '—' : `${result.ioReadBytes + (result.ioWriteBytes ?? 0)} B`}</td></tr>;
      })}</tbody></table></div>
    </section> : null}
  </main>;
}
