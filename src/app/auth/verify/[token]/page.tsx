import type { Metadata } from 'next';
import Link from 'next/link';

import { VerifyForm } from '@/features/auth/components/forms';
import { ThemeToggle } from '../../../theme-toggle';

import '../../auth.css';

export const metadata: Metadata = { title: 'Confirm your email' };
export const dynamic = 'force-dynamic';

/* The link lands here and stops. Spending it is a button press, not a page
 * load: mail clients and link scanners follow URLs before a person does, and a
 * single-use token that a scanner burned is an account nobody can confirm. */
export default async function VerifyPage({ params }: { params: Promise<{ token: string }> }) {
  const { token } = await params;
  return (
    <>
      <header className="topbar">
        <ThemeToggle />
      </header>
      <main className="door single">
        <h1>Ronit Nath</h1>
        <VerifyForm token={token} />
        <div className="aside">
          <Link href="/auth">Sign in</Link>
        </div>
      </main>
    </>
  );
}
