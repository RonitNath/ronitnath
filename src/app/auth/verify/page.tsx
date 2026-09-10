import type { Metadata } from 'next';
import Link from 'next/link';

import { ResendForm } from '@/features/auth/components/forms';
import { ThemeToggle } from '../../theme-toggle';

import '../auth.css';

export const metadata: Metadata = { title: 'Confirm your email' };
export const dynamic = 'force-dynamic';

/* Where better-auth's confirmation link lands after it has done its work.
 *
 * The click itself is the confirmation now: the library verifies at
 * `/api/auth/verify-email` and bounces the reader here, signed in, which is
 * why this page has no Confirm button. It carries an `error` when the token
 * was already spent or had expired — a mail scanner that followed the URL
 * before a person did is the usual reason — and the only thing to do about
 * that is another letter. */
export default async function VerifyPage({
  searchParams,
}: {
  searchParams: Promise<{ error?: string }>;
}) {
  const { error } = await searchParams;

  return (
    <>
      <header className="topbar">
        <ThemeToggle />
      </header>
      <main className="door single">
        <h1>Ronit Nath</h1>
        {error ? (
          <>
            <p className="note" data-state="invalid" role="alert">
              That link has been used or has expired.
            </p>
            <ResendForm />
          </>
        ) : (
          <p className="note" data-state="done" role="status">
            Address confirmed.
          </p>
        )}
        <div className="aside">
          <Link href="/app">Your account</Link>
          <Link href="/auth/sign-in">Sign in</Link>
        </div>
      </main>
    </>
  );
}
