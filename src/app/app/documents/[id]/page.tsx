import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';

import {
  EditDocumentForm,
  PublishButton,
  RevokeShareButton,
  ShareForm,
  TransferDocumentForm,
} from '@/features/documents/components/docs';
import { documentDetail, shareCandidates } from '@/features/documents/queries';
import { renderBody } from '@/features/events/markup';
import { encodeId, tryDecodeId } from '@/lib/ids';
import { rankOf } from '@/lib/authority';
import { requireMember } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Document' };

export default async function DocumentPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const { principal } = await requireMember(`/app/documents/${id}`);
  const me = { personId: principal.personId, isOperator: principal.isOperator };
  const internal = tryDecodeId('document', id);
  if (internal === null) notFound();
  const document = await documentDetail(me, internal);
  if (!document) notFound();

  const mayEdit = rankOf('document', document.level) >= rankOf('document', 'editor');
  const owns = document.level === 'owner';
  const candidates = owns ? await shareCandidates(me.personId) : [];
  const choices = candidates.map((row) => ({
    value: encodeId(row.subject.kind, row.subject.id),
    label: row.displayName,
  }));

  return (
    <main className="indoors">
      <h1>{document.title}</h1>
      <p className="note">
        {document.ownerName}
        {' · '}
        {document.level}
        {' · '}
        <Link href="/app/documents">All documents</Link>
      </p>

      {mayEdit ? (
        <section>
          <EditDocumentForm
            document={id}
            title={document.title}
            body={document.body}
          />
        </section>
      ) : (
        <section>
          <h2>Reading</h2>
          <article
            className="prose"
            dangerouslySetInnerHTML={{ __html: renderBody(document.body) }}
          />
        </section>
      )}

      {owns ? (
        <>
          <section>
            <h2>Publication</h2>
            <PublishButton
              document={id}
              published={document.publishedAt !== null}
              slug={document.slug}
            />
          </section>

          <section>
            <h2>Shared with</h2>
            <div className="scroller">
              <table className="rows">
                <thead>
                  <tr>
                    <th>Who</th>
                    <th>Kind</th>
                    <th>As</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {document.shares.map((row) => (
                    <tr key={`${row.subject.kind}:${row.subject.id}`}>
                      <td>{row.displayName}</td>
                      <td className="mono">{row.subject.kind}</td>
                      <td>{row.level}</td>
                      <td className="actions">
                        <RevokeShareButton
                          document={id}
                          subject={encodeId(row.subject.kind, row.subject.id)}
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {document.shares.length === 0 ? (
              <p className="empty">Nobody but its owner.</p>
            ) : null}
            <ShareForm document={id} candidates={choices} />
          </section>

          <section>
            <h2>Ownership</h2>
            <TransferDocumentForm
              document={id}
              candidates={choices.filter((row) => !row.value.startsWith('g_'))}
            />
          </section>
        </>
      ) : null}
    </main>
  );
}
