import type { Metadata } from 'next';
import Link from 'next/link';

import { CreateDocumentForm } from '@/features/documents/components/docs';
import { listDocuments } from '@/features/documents/queries';
import { listMemberships } from '@/features/organizations/queries';
import { encodeId } from '@/lib/ids';
import { userPath } from '@/lib/paths';
import { requireSubjectPerson } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Documents' };

function day(at: Date): string {
  return at.toISOString().slice(0, 10);
}

export default async function DocumentsPage({ params }: { params: Promise<{ user: string }> }) {
  const { user } = await params;
  const { reader, subjectName, viewingOther } = await requireSubjectPerson(user, 'documents');
  const me = reader;
  const [documents, memberships] = await Promise.all([
    listDocuments(me),
    listMemberships(me.personId),
  ]);

  const owners = [
    { value: 'me', label: subjectName },
    ...memberships.map((row) => ({ value: encodeId('organization', row.id), label: row.name })),
  ];

  return (
    <main className="indoors">
      <h1>Documents</h1>
      <p className="note">
        What you wrote, and what somebody handed you. The word beside a row is what you may do to
        it.
      </p>

      {viewingOther ? null : (
        <section>
          <CreateDocumentForm owners={owners} />
        </section>
      )}

      <section>
        <h2>All of them</h2>
        <div className="scroller">
          <table className="rows wide">
            <thead>
              <tr>
                <th>Title</th>
                <th>Belongs to</th>
                <th>You are</th>
                <th>Public</th>
                <th>Changed</th>
              </tr>
            </thead>
            <tbody>
              {documents.map((row) => (
                <tr key={row.id}>
                  <td>
                    <Link href={userPath(user, `documents/${encodeId('document', row.id)}`)}>{row.title}</Link>
                  </td>
                  <td className="mono">{row.ownerName}</td>
                  <td>{row.level}</td>
                  <td>
                    {row.publishedAt && row.slug ? (
                      <Link href={`/d/${row.slug}`} className="state" data-state="claimed">
                        published
                      </Link>
                    ) : (
                      <span className="state" data-state="live">
                        draft
                      </span>
                    )}
                  </td>
                  <td className="num">
                    <time dateTime={row.updatedAt.toISOString()}>{day(row.updatedAt)}</time>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {documents.length === 0 ? (
          <p className="empty">Nothing yet. A document starts as a title.</p>
        ) : null}
      </section>
    </main>
  );
}
