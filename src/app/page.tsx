import { ThemeToggle } from './theme-toggle';

export default function Home() {
  return (
    <main
      style={{
        minHeight: '100dvh',
        display: 'grid',
        gridTemplateRows: 'auto 1fr',
        maxWidth: 'var(--content-max)',
        margin: '0 auto',
        padding: '1.5rem',
        gap: 'var(--gap-section)',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <ThemeToggle />
      </div>
      <div style={{ alignSelf: 'center', display: 'grid', gap: '0.75rem' }}>
        <h1
          style={{
            fontFamily: 'var(--font-display)',
            fontSize: 'var(--text-5)',
            fontWeight: 600,
            letterSpacing: '-0.01em',
            margin: 0,
          }}
        >
          <span style={{ color: 'var(--hero-red)' }}>Ronit</span>{' '}
          <span style={{ color: 'var(--hero-gold)' }}>Nath</span>
        </h1>
        <p style={{ color: 'var(--fg-muted)', fontSize: 'var(--text-2)', maxWidth: '34rem' }}>
          The site is being rebuilt. The sky comes back next.
        </p>
      </div>
    </main>
  );
}
