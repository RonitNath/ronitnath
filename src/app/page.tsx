import { Atmosphere } from '@/features/sky/atmosphere';

import './landing.css';
import { ThemeToggle } from './theme-toggle';

/** The public landing. Server-rendered whole; the only client code on the page
 * is the sky canvas and the theme toggle. */
export default function Home() {
  return (
    <>
      <Atmosphere />
      <header className="topbar">
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
