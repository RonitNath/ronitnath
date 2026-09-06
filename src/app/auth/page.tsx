import type { Metadata } from 'next';
import Link from 'next/link';
import { redirect } from 'next/navigation';

import { RegisterForm, SignInForm } from '@/features/auth/components/forms';
import { AUTH_FAILED } from '@/features/auth/form-state';
import { currentPrincipal } from '@/features/auth/session';
import { ThemeToggle } from '../theme-toggle';

import './auth.css';

export const metadata: Metadata = { title: 'Sign in' };
export const dynamic = 'force-dynamic';

/* Two peer forms and the other door. The page is a door, and a door does not
 * explain itself: the only prose here is a refusal, and every refusal says
 * the same thing. */
export default async function AuthPage({
  searchParams,
}: {
  searchParams: Promise<{ next?: string; declined?: string }>;
}) {
  if (await currentPrincipal()) redirect('/app');
  const params = await searchParams;
  const next = params.next && params.next.startsWith('/') ? params.next : '/app';
  const federated = `/auth/oidc/start?next=${encodeURIComponent(next)}`;

  return (
    <>
      <header className="topbar">
        <ThemeToggle />
      </header>
      <main className="door">
        <h1>Ronit Nath</h1>
        {params.declined ? (
          <p className="note" data-state="invalid" role="alert">
            {AUTH_FAILED}
          </p>
        ) : null}
        <div className="panes">
          <SignInForm next={next} />
          <RegisterForm />
        </div>
        <div className="aside">
          <a className="federated" href={federated}>
            Sign in with Isoastra
          </a>
          <Link href="/auth/reset">Forgot your password</Link>
          <Link href="/">Back to the landing</Link>
        </div>
      </main>
    </>
  );
}
