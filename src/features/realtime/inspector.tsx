'use client';

import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useEffect } from 'react';
import { Grid, FindBox, Pager, useGrid, type Column } from '@isoastra/grid-react';
import { useQueryState } from '@isoastra/grid-next';

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
    cell: (row) => row.status,
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
    <>
      <div className="filters">
        <FindBox grid={grid} placeholder="Find visitor, path, or viewport" />
      </div>
      <div className="scroller">
        <Grid
          grid={grid}
          columns={columns}
          rowKey={(row) => row.id}
          classes={{ table: 'rows dense' }}
          rowProps={(row) =>
            ({ 'data-realtime-row': row.id }) as React.HTMLAttributes<HTMLTableRowElement>
          }
          empty="No browser views have reported yet."
        />
      </div>
      <Pager grid={grid} className="pager" sizes={[25, 50, 100]} />
    </>
  );
}
