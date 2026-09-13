import type { StoredRunEvent } from '@isoastra/fleet-delivery';

import { DeliveryInspector } from '@/features/delivery/inspector';
import { deliveryReport } from '@/features/delivery/model';
import { requireOperator } from '@/lib/tiers';

export const dynamic = 'force-dynamic';

function eventRow(event: StoredRunEvent) {
  if (event.schemaVersion === 2 && event.type === 'stage-finished') {
    const { result } = event;
    return {
      id: `${event.producerId}:${event.seq}`,
      seq: event.seq,
      producer: event.producerId,
      label: result.stageId,
      state: result.state,
      queueMs: result.queueMs,
      admissionMs: result.admissionWaitMs,
      wallMs: result.wallMs,
      cpuMs: result.measurement.cpuMs,
      peakMemoryBytes: result.measurement.peakMemoryBytes,
      ioBytes:
        result.measurement.ioReadBytes === null || result.measurement.ioWriteBytes === null
          ? null
          : result.measurement.ioReadBytes + result.measurement.ioWriteBytes,
      scope: `${result.measurement.scope}/${result.measurement.quality}`,
    };
  }
  if (event.schemaVersion === 2 && event.type === 'coordinator-checkpoint') {
    return {
      id: `${event.producerId}:${event.seq}`,
      seq: event.seq,
      producer: event.producerId,
      label: event.step,
      state: event.state,
      queueMs: null,
      admissionMs: null,
      wallMs: null,
      cpuMs: null,
      peakMemoryBytes: null,
      ioBytes: null,
      scope: `host/${event.migration}`,
    };
  }
  return {
    id: `${event.schemaVersion === 2 ? event.producerId : 'legacy'}:${event.seq}`,
    seq: event.seq,
    producer: event.schemaVersion === 2 ? event.producerId : 'legacy',
    label: event.type,
    state: '—',
    queueMs: null,
    admissionMs: null,
    wallMs: null,
    cpuMs: null,
    peakMemoryBytes: null,
    ioBytes: null,
    scope: event.schemaVersion === 1 ? 'historical/unavailable' : 'event',
  };
}

const duration = (value: number | null) => (value === null ? '—' : `${value} ms`);

export default async function DeliveryPage({
  searchParams,
}: {
  searchParams: Promise<{ run?: string }>;
}) {
  await requireOperator();
  const { run } = await searchParams;
  const report = await deliveryReport(run);
  const rows = report.runs.map((item) => ({
    id: item.id,
    sha: item.requestedSha,
    state: item.state,
    started: item.startedAt.toISOString(),
    updated: item.updatedAt.toISOString(),
    lastSeq: item.lastSeq,
  }));
  const manifest = report.events
    .map(({ event }) =>
      event.schemaVersion === 2 && event.type === 'coordinator-checkpoint'
        ? event.manifest
        : null,
    )
    .find((value) => value !== null);
  return (
    <main className="indoors" data-realtime-section="delivery-runs">
      <h1>Delivery</h1>
      <p className="note">
        Release evidence from runner and host journals. Reporting freshness is separate from
        release state.
      </p>
      <section>
        <h2>Runs</h2>
        <DeliveryInspector rows={rows} />
      </section>
      {report.selected ? (
        <section data-realtime-section="delivery-run">
          <h2>{report.selected.requestedSha}</h2>
          <dl className="facts">
            <div>
              <dt>State</dt>
              <dd>{report.selected.state}</dd>
            </div>
            <div>
              <dt>Last sequence</dt>
              <dd>{report.selected.lastSeq}</dd>
            </div>
            <div>
              <dt>Reporting</dt>
              <dd>
                {Date.now() - report.selected.updatedAt.getTime() < 60_000
                  ? 'connected'
                  : 'stale'}
              </dd>
            </div>
          </dl>
          {manifest ? (
            <dl className="facts">
              <div><dt>Runtime digest</dt><dd className="mono">{manifest.artifacts.runtime.digest.slice(0, 19)}…</dd></div>
              <div><dt>Migration digest</dt><dd className="mono">{manifest.artifacts.migration.digest.slice(0, 19)}…</dd></div>
              <div><dt>Migration policy</dt><dd>{manifest.migrations.entries.map((entry) => `${entry.name} (${entry.mode})`).join(', ') || 'none'}</dd></div>
              <div><dt>Compatibility</dt><dd>{manifest.compatibility.previousRead && manifest.compatibility.previousWrite && manifest.compatibility.rollbackRead && manifest.compatibility.repeatedMigration ? 'previous/candidate/rollback verified' : 'incomplete'}</dd></div>
            </dl>
          ) : null}
          <div className="scroller">
            <table className="rows dense">
              <thead>
                <tr>
                  <th>Producer/seq</th>
                  <th>Stage/event</th>
                  <th>State</th>
                  <th>Queue</th>
                  <th>Admission</th>
                  <th>Execution</th>
                  <th>CPU</th>
                  <th>Peak</th>
                  <th>I/O</th>
                  <th>Scope</th>
                </tr>
              </thead>
              <tbody>
                {report.events.map(({ event }) => {
                  const item = eventRow(event);
                  return (
                    <tr key={item.id}>
                      <td className="mono">
                        {item.producer}/{item.seq}
                      </td>
                      <td>{item.label}</td>
                      <td>{item.state}</td>
                      <td className="mono">{duration(item.queueMs)}</td>
                      <td className="mono">{duration(item.admissionMs)}</td>
                      <td className="mono">{duration(item.wallMs)}</td>
                      <td className="mono">{duration(item.cpuMs)}</td>
                      <td className="mono">
                        {item.peakMemoryBytes === null
                          ? '—'
                          : `${Math.round(item.peakMemoryBytes / 1048576)} MiB`}
                      </td>
                      <td className="mono">
                        {item.ioBytes === null ? '—' : `${item.ioBytes} B`}
                      </td>
                      <td>{item.scope}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}
    </main>
  );
}
