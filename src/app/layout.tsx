import type { Metadata, Viewport } from 'next';

import { ImpersonationBar } from '@/features/platform/components/bar';
import { RealtimeReporter } from '@/features/realtime/reporter';

import './globals.css';

export const metadata: Metadata = {
  title: 'Ronit Nath',
  description: 'ronitnath.com',
};

export const viewport: Viewport = {
  width: 'device-width',
  initialScale: 1,
};

/* Runs before first paint so a stored light choice never flashes dark. Dark is
 * the default: no attribute means the :root color-scheme, which is dark. */
const prePaint = `try{var t=localStorage.getItem('rn_theme');if(t==='light'||t==='dark')document.documentElement.dataset.theme=t}catch(e){}`;

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: prePaint }} />
      </head>
      <body>
        <RealtimeReporter />
        <ImpersonationBar />
        {children}
      </body>
    </html>
  );
}
