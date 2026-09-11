import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound, permanentRedirect } from 'next/navigation';

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
import { userPath } from '@/lib/paths';
import { requireMember } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Document' };

export default async function DocumentPage({
  params,
}: {
  params: Promise<{ user: string; id: string }>;
}) {
  const { user, id } = await params;
  /* A document is the one thing on these surfaces that is shared with people
   * rather than owned by the path. The link a reader is handed is the owner's
   * — `/u/<the owner>/documents/<doc>` — and the reader is not the owner, so
   * the subject gate that every other page under `/u` uses would turn a
   * perfectly good share into a 404.
   *
   * So the segment canonicalises instead: a signed-in reader asking for
   * somebody else's shelf is sent, permanently, to the same document on their
   * own, which is the link worth keeping. An operator stays on the subject
   * they asked for, because reading somebody's page as it is is the whole
   * point of the tier. What decides whether the document opens at all is
   * unchanged and is the document's own authority (`documentDetail`). */
  const { principal } = await requireMember(userPath(user, `documents/${id}`));
  const subject = tryDecodeId('person', user);
  if (subject === null) notFound();
  const viewingOther = subject !== principal.personId;
  if (viewingOther && !principal.isOperator) {
    permanentRedirect(userPath(encodeId('person', principal.personId), `documents/${id}`));
  }
  const me = {
    personId: viewingOther ? subject : principal.personId,
    isOperator: principal.isOperator,
  };
  const internal = tryDecodeId('document', id);
  if (internal === null) notFound();
  const document = await documentDetail(me, internal);
  if (!document) notFound();

  /* An operator reading somebody else's page reads it: the commands on this
   * page write as the actor, and the actor is not the person whose path this
   * is. */
  const mayEdit =
    !viewingOther && rankOf('document', document.level) >= rankOf('document', 'editor');
  const owns = !viewingOther && document.level === 'owner';
  const candidates = owns ? await shareCandidates(me.personId) : [];
  const choices = candidates.map((row) => ({
    value: encodeId(row.subject.kind, row.subject.id),
    label: row.displayName,
  }));

  return (
    <main className="indoors" data-realtime-resource={`document:${id}`}>
      <h1>{document.title}</h1>
      <p className="note">
        {document.ownerName}
        {' · '}
        {document.level}
        {' · '}
        <Link href={userPath(user, 'documents')}>All documents</Link>
      </p>

      {mayEdit ? (
        <section>
          <EditDocumentForm document={id} title={document.title} body={document.body} />
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

          <section data-realtime-section="revision-history">
            <h2>Revision history</h2>
            <div className="scroller">
              <table className="rows dense">
                <thead>
                  <tr>
                    <th>Revision</th>
                    <th>Change</th>
                    <th>Fields</th>
                    <th>At</th>
                  </tr>
                </thead>
                <tbody>
                  {document.revisions.map((revision) => (
                    <tr
                      key={`${revision.kind}:${revision.version}:${revision.createdAt.toISOString()}`}
                    >
                      <td className="mono">{revision.version}</td>
                      <td>{revision.kind}</td>
                      <td>{revision.fields.join(', ') || 'snapshot'}</td>
                      <td className="mono">{revision.createdAt.toISOString()}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
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
