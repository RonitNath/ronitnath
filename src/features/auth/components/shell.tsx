import Link from 'next/link';

import { signOut } from '@/features/auth/actions';
import type { Principal } from '@/features/auth/session';
import { ThemeToggle } from '@/app/theme-toggle';

/* The signed-in chrome. The nav is filtered by what the principal holds, not
 * by what the page is: an operator link that only an operator can see is one
 * fewer surface to guess at, and the page behind it declines on its own. */
export function Shell({ principal }: { principal: Principal }) {
  return (
    <header className="shell">
      <nav>
        <Link href="/">Home</Link>
        <Link href="/app">Account</Link>
        <Link href="/app/people">People</Link>
        <Link href="/app/sessions">Sessions</Link>
        {principal.isOperator ? <Link href="/platform">Platform</Link> : null}
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
