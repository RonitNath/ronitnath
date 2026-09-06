import Link from 'next/link';

import { Shell } from '@/features/auth/components/shell';
import { requireOperator } from '@/lib/tiers';

import '../app/app.css';
import './platform.css';

export const dynamic = 'force-dynamic';

/* The tier is one call, at the top, for every page under it: an operator
 * surface asked for by anybody else — a member, a visitor, a browser with no
 * cookie at all — is the 404 that a page which is not there gives. */
export default async function PlatformLayout({ children }: { children: React.ReactNode }) {
  const { principal } = await requireOperator();
  return (
    <>
      <Shell principal={principal} />
      <nav className="platform-nav">
        <Link href="/platform/parties">Parties</Link>
        <Link href="/platform/matches">Matches</Link>
        <Link href="/platform/sessions">Sessions</Link>
        <Link href="/platform/audit">Audit</Link>
        <Link href="/platform/operators">Operators</Link>
        <Link href="/platform/deployment">Deployment</Link>
      </nav>
      {children}
    </>
  );
}
