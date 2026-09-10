import Link from 'next/link';

import { currentPrincipal } from '@/features/auth/principal';
import { Atmosphere } from '@/features/sky/atmosphere';

import './landing.css';
import { ThemeToggle } from './theme-toggle';

/** The public landing. Server-rendered whole; the only client code on the page
 * is the sky canvas and the theme toggle. */
export default async function Home() {
  /* The chrome says who is here, and nothing more: a name when the visitor is
   * signed in, the way in when they are not. */
  const principal = await currentPrincipal();
  return (
    <>
      <Atmosphere />
      <header className="topbar">
        <Link href="/about">About the sky</Link>
        {principal ? (
          <Link href="/app">{principal.displayName}</Link>
        ) : (
          <Link href="/auth/sign-in">Sign in</Link>
        )}
        <ThemeToggle />
      </header>
      <main className="home-hero">
        <div className="home-card">
          <h1>Ronit Nath</h1>
          <p className="tagline">
            Founder of{' '}
            <a href="https://isoastra.com" rel="noopener" target="_blank">
              Isoastra
            </a>
          </p>
          <ul className="social-links">
            <li>
              <a href="https://github.com/RonitNath" rel="me noopener" target="_blank">
                GitHub
              </a>
            </li>
            <li>
              <a href="https://instagram.com/ronit_nath" rel="me noopener" target="_blank">
                Instagram
              </a>
            </li>
            <li>
              <a href="https://linkedin.com/in/ronitn" rel="me noopener" target="_blank">
                LinkedIn
              </a>
            </li>
            <li>
              <a href="mailto:ronit@isoastra.com">Email</a>
            </li>
          </ul>
        </div>
      </main>
    </>
  );
}
