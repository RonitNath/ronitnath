import type { Metadata } from 'next';
import Link from 'next/link';

import { RequestResetForm } from '@/features/auth/components/forms';
import { ThemeToggle } from '../../theme-toggle';

import '../auth.css';

export const metadata: Metadata = { title: 'Reset password' };

/* Asking answers the same way whichever address is typed. */
export default function RequestResetPage() {
  return (
    <>
      <header className="topbar">
        <ThemeToggle />
      </header>
      <main className="door single">
        <h1>Ronit Nath</h1>
        <RequestResetForm />
        <div className="aside">
          <Link href="/auth">Back to sign in</Link>
        </div>
      </main>
    </>
  );
}
