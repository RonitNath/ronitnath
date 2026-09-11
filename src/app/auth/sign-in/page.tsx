import type { Metadata } from 'next';
import Link from 'next/link';
import { redirect } from 'next/navigation';

import { IsoastraButton, RegisterForm, SignInForm } from '@/features/auth/components/forms';
import { AUTH_FAILED } from '@/features/auth/form-state';
import { currentPrincipal } from '@/features/auth/principal';
import { ThemeToggle } from '../../theme-toggle';

import '../auth.css';
import { encodeId } from '@/lib/ids';
import { LEGACY_APP_ROOT, userPath } from '@/lib/paths';

export const metadata: Metadata = { title: 'Sign in' };
export const dynamic = 'force-dynamic';

/* Two peer forms and the other door. The page is a door, and a door does not
 * explain itself: the only prose here is a refusal, and every refusal says
 * the same thing.
 *
 * It lives at `/auth/sign-in` rather than `/auth` because the fleet's auth
 * routes are named for what they do (`fleet-conventions.md` §1); `/auth` is a
 * permanent redirect to here. */
export default async function SignInPage({
  searchParams,
}: {
  searchParams: Promise<{ next?: string; declined?: string }>;
}) {
  const principal = await currentPrincipal();
  if (principal) redirect(userPath(encodeId('person', principal.personId)));
  const params = await searchParams;
  /* Nobody is signed in yet, so the fallback cannot name a person: `/app` is
   * the stub that resolves the session and forwards to `/u/<me>` once there
   * is one. */
  const next = params.next && params.next.startsWith('/') ? params.next : LEGACY_APP_ROOT;

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
          <IsoastraButton next={next} />
          <Link href="/auth/reset">Forgot your password</Link>
          <Link href="/">Back to the landing</Link>
        </div>
      </main>
    </>
  );
}
