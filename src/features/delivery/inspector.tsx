'use client';

import Link from 'next/link';
import { Grid, type Column, useGrid } from '@isoastra/grid-react';
import { useQueryState } from '@isoastra/grid-next';
import { useRefreshOn } from '@/features/realtime/use-events';

export interface DeliveryRow { id: string; sha: string; state: string; started: string; updated: string; lastSeq: number }

const columns: readonly Column<DeliveryRow>[] = [
  { id: 'sha', header: 'Candidate', sortBy: (row) => row.sha, cell: (row) => <Link href={`?run=${row.id}`} className="mono">{row.sha.slice(0, 12)}</Link> },
  { id: 'state', header: 'State', sortBy: (row) => row.state, cell: (row) => <span className="party-state" data-state={row.state === 'deployed' ? 'active' : row.state === 'failed' ? 'disabled' : ''}>{row.state}</span> },
  { id: 'started', header: 'Started', sortBy: (row) => row.started, className: 'mono', cell: (row) => row.started },
  { id: 'updated', header: 'Updated', sortBy: (row) => row.updated, className: 'mono', cell: (row) => row.updated },
  { id: 'lastSeq', header: 'Events', sortBy: (row) => row.lastSeq, className: 'mono', cell: (row) => row.lastSeq + 1 },
];

export function DeliveryInspector({ rows }: { rows: DeliveryRow[] }) {
  useRefreshOn('/o/isoastra/events', ['delivery-run']);
  const [query, setQuery] = useQueryState({ size: 100 });
  const grid = useGrid({ rows, columns, query, onQuery: setQuery });
  return <div className="scroller"><Grid grid={grid} columns={columns} rowKey={(row) => row.id} classes={{ table: 'rows dense' }} empty="No delivery runs have reported yet." /></div>;
}
