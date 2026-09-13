'use client';

import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useEffect } from 'react';
import { useGrid, type Column } from '@isoastra/grid-react';
import { useQueryState } from '@isoastra/grid-next';
import { DataTable } from '@isoastra/grid-ui';
import { Status } from '@isoastra/ui';

export interface InspectorRow {
  id: string;
  who: string;
  path: string;
  status: string;
  lastSeen: string;
  visible: string;
  subscriptions: number;
  deliveries: number;
}

const columns: readonly Column<InspectorRow>[] = [
  {
    id: 'who',
    header: 'Who',
    searchBy: (row) => row.who,
    sortBy: (row) => row.who,
    cell: (row) => <Link href={`?view=${row.id}`}>{row.who}</Link>,
  },
  {
    id: 'path',
    header: 'View',
    searchBy: (row) => row.path,
    cell: (row) => <span className="mono">{row.path}</span>,
  },
  {
    id: 'status',
    header: 'State',
    filterBy: (row) => row.status,
    sortBy: (row) => row.status,
    cell: (row) => <Status tone={row.status.includes('visible') ? 'success' : 'neutral'}>{row.status}</Status>,
  },
  {
    id: 'visible',
    header: 'Viewport',
    searchBy: (row) => row.visible,
    cell: (row) => row.visible,
  },
  {
    id: 'subscriptions',
    header: 'Subscriptions',
    sortBy: (row) => row.subscriptions,
    className: 'mono',
    cell: (row) => row.subscriptions,
  },
  {
    id: 'deliveries',
    header: 'Deliveries',
    sortBy: (row) => row.deliveries,
    className: 'mono',
    cell: (row) => row.deliveries,
  },
  {
    id: 'lastSeen',
    header: 'Last seen',
    sortBy: (row) => row.lastSeen,
    className: 'mono',
    cell: (row) => row.lastSeen,
  },
];

export function RealtimeInspector({ rows }: { rows: InspectorRow[] }) {
  const [query, setQuery] = useQueryState({ size: 25 });
  const grid = useGrid({ rows, columns, query, onQuery: setQuery });
  const router = useRouter();
  useEffect(() => {
    const timer = setInterval(() => router.refresh(), 2_000);
    return () => clearInterval(timer);
  }, [router]);
  return (
    <DataTable grid={grid} columns={columns} rowKey={(row) => row.id} title="Browser views" empty="No browser views have reported yet." selectable filters={['status']} sizes={[25, 50, 100]} />
  );
}
