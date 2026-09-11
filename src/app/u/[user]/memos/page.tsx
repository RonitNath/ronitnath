import type { Metadata } from 'next';
import { MemoStudio } from '@/features/memos/components/studio';
import { listMemos } from '@/features/memos/queries';
import { requireSubjectPerson } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Voice memos' };

export default async function MemosPage({ params, searchParams }: { params: Promise<{ user: string }>; searchParams: Promise<{ trash?: string }> }) {
  const { user } = await params;
  const trash = (await searchParams).trash === '1';
  const subject = await requireSubjectPerson(user, 'memos');
  const memos = await listMemos(subject.subjectPersonId, trash);
  return <main className="indoors memos"><h1>Voice memos</h1><p className="note">Recordings stay private to this account. Capture continues locally through a lost connection.</p><MemoStudio user={user} memos={memos} trash={trash} canCreate={!subject.viewingOther} /></main>;
}
