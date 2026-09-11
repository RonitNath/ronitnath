import Link from 'next/link';

import { SkyBackdrop, SkySheet } from '@/features/sky/backdrop';

import { ThemeToggle } from './theme-toggle';

/* A URL that does not resolve, said in the site's own voice.
 *
 * A 404 is the one page nobody chose to visit, which is exactly why it should
 * look like somewhere rather than like a failure: the sky is behind it as it
 * is behind everything else a stranger can reach, and the sheet says the one
 * sentence there is to say and offers the way home. No apology, no list of
 * suggestions, no search box — the site is small enough that the landing page
 * is the answer to "where else".
 *
 * It carries the chrome of a public page rather than the application's,
 * because whoever is reading it may never have signed in.
 */
export default function NotFound() {
  return (
    <>
      <SkyBackdrop />
      <header className="topbar">
        <Link href="/">Ronit Nath</Link>
        <ThemeToggle />
      </header>
      <SkySheet>
        <section className="sky-note">
          <h1>Nothing here.</h1>
          <p>
            <Link href="/">Back to the sky</Link>
          </p>
        </section>
      </SkySheet>
    </>
  );
}
