'use client';

import { useEffect, useState } from 'react';

type Theme = 'dark' | 'light';

function stored(): Theme | null {
  try {
    const t = localStorage.getItem('rn_theme');
    return t === 'light' || t === 'dark' ? t : null;
  } catch {
    return null;
  }
}

export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>('dark');

  useEffect(() => {
    setTheme(
      (document.documentElement.dataset.theme as Theme | undefined) ?? stored() ?? 'dark',
    );
  }, []);

  function choose(next: Theme) {
    document.documentElement.dataset.theme = next;
    setTheme(next);
    try {
      localStorage.setItem('rn_theme', next);
    } catch {
      /* a browser that refuses storage still gets the toggle for this page */
    }
  }

  return (
    <button
      type="button"
      onClick={() => choose(theme === 'dark' ? 'light' : 'dark')}
      aria-label={theme === 'dark' ? 'Use the light theme' : 'Use the dark theme'}
      style={{
        border: '1px solid var(--border)',
        borderRadius: 'var(--radius)',
        padding: '0.25rem 0.625rem',
        fontSize: 'var(--text-1)',
        color: 'var(--fg-muted)',
        background: 'var(--surface-glass)',
        cursor: 'pointer',
      }}
    >
      {theme === 'dark' ? 'Light' : 'Dark'}
    </button>
  );
}
