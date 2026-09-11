import Link from 'next/link';

import { signOut } from '@/features/auth/actions';
import type { Principal } from '@/features/auth/principal';
import { operatorPath, userPath } from '@/lib/paths';
import { ThemeToggle } from '@/app/theme-toggle';

/* The signed-in chrome. The nav is filtered by what the principal holds, not
 * by what the page is: an operator link that only an operator can see is one
 * fewer surface to guess at, and the page behind it declines on its own.
 *
 * `user` is the public id the nav links under. It is the subject of the page
 * being drawn rather than always the signed-in person, because an operator
 * reading somebody else's account must be able to walk that person's pages
 * without the nav quietly taking them home on the first click. */
export function Shell({ principal, user }: { principal: Principal; user: string }) {
  return (
    <header className="shell">
      <nav>
        <Link href="/">Home</Link>
        <Link href={userPath(user)}>Account</Link>
        <Link href={userPath(user, 'people')}>People</Link>
        <Link href={userPath(user, 'events')}>Events</Link>
        <Link href={userPath(user, 'groups')}>Groups</Link>
        <Link href={userPath(user, 'documents')}>Documents</Link>
        <Link href={userPath(user, 'memos')}>Memos</Link>
        <Link href={userPath(user, 'sessions')}>Sessions</Link>
        {principal.isOperator ? <Link href={operatorPath()}>Platform</Link> : null}
      </nav>
      <span className="who">{principal.displayName}</span>
      <ThemeToggle />
      <form action={signOut}>
        <button type="submit" className="linkish">
          Sign out
        </button>
      </form>
    </header>
  );
}
