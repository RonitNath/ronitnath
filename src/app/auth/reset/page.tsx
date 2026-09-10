import type { Metadata } from 'next';
import Link from 'next/link';

import { RequestResetForm, ResetForm } from '@/features/auth/components/forms';
import { ThemeToggle } from '../../theme-toggle';

import '../auth.css';

export const metadata: Metadata = { title: 'Reset password' };
export const dynamic = 'force-dynamic';

/* One page, two moments. Asking answers the same way whichever address is
 * typed; the letter's link is a GET on `/api/auth/reset-password/<token>`
 * which checks the token is live and bounces back here carrying it, so a
 * `?token=` in the URL is what turns this into the form that sets one. */
export default async function ResetPage({
  searchParams,
}: {
  searchParams: Promise<{ token?: string; error?: string }>;
}) {
  const { token, error } = await searchParams;

  return (
    <>
      <header className="topbar">
        <ThemeToggle />
      </header>
      <main className="door single">
        <h1>Ronit Nath</h1>
        {error ? (
          <p className="note" data-state="invalid" role="alert">
            That link has been used or has expired.
          </p>
        ) : null}
        {token ? <ResetForm token={token} /> : <RequestResetForm />}
        <div className="aside">
          <Link href="/auth/sign-in">Sign in</Link>
        </div>
      </main>
    </>
  );
}
