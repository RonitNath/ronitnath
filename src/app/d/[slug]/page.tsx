import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';

import { publishedDocument } from '@/features/documents/queries';
import { renderBody } from '@/features/events/markup';
import { ThemeToggle } from '../../theme-toggle';

import '../../app/app.css';
import './document.css';

export const dynamic = 'force-dynamic';

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug: string }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const document = await publishedDocument(slug);
  return { title: document?.title ?? 'Not here' };
}

/* An unpublished document, a document that was published and taken back, and
 * a slug nobody ever used are one answer. */
export default async function PublicDocument({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const document = await publishedDocument(slug);
  if (!document) notFound();

  return (
    <>
      <header className="topbar">
        <ThemeToggle />
      </header>
      <main className="indoors reading">
        <h1>{document.title}</h1>
        <p className="note">
          {document.ownerName}
          {' · '}
          <time dateTime={document.publishedAt.toISOString()}>
            {document.publishedAt.toISOString().slice(0, 10)}
          </time>
        </p>
        <article className="prose" dangerouslySetInnerHTML={{ __html: renderBody(document.body) }} />
        <p className="aside-line">
          <Link href="/">Ronit Nath</Link>
        </p>
      </main>
    </>
  );
}
