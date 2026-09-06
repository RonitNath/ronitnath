import type { Metadata } from 'next';
import Link from 'next/link';

import { ResetForm } from '@/features/auth/components/forms';
import { ThemeToggle } from '../../../theme-toggle';

import '../../auth.css';

export const metadata: Metadata = { title: 'Set a new password' };
export const dynamic = 'force-dynamic';

export default async function ResetPage({ params }: { params: Promise<{ token: string }> }) {
  const { token } = await params;
  return (
    <>
      <header className="topbar">
        <ThemeToggle />
      </header>
      <main className="door single">
        <h1>Ronit Nath</h1>
        <ResetForm token={token} />
        <div className="aside">
          <Link href="/auth">Sign in</Link>
        </div>
      </main>
    </>
  );
}
