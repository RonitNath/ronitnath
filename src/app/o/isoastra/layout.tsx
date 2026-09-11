import Link from 'next/link';

import { Shell } from '@/features/auth/components/shell';
import { encodeId } from '@/lib/ids';
import { operatorPath } from '@/lib/paths';
import { requireOperator } from '@/lib/tiers';

import '../../indoors.css';
import './platform.css';

export const dynamic = 'force-dynamic';

/* The tier is one call, at the top, for every page under it: an operator
 * surface asked for by anybody else — a member, a visitor, a browser with no
 * cookie at all — is the 404 that a page which is not there gives. */
export default async function PlatformLayout({ children }: { children: React.ReactNode }) {
  const { principal } = await requireOperator();
  return (
    <>
      <Shell principal={principal} user={encodeId('person', principal.personId)} />
      <nav className="platform-nav">
        <Link href={operatorPath('parties')}>Parties</Link>
        <Link href={operatorPath('matches')}>Matches</Link>
        <Link href={operatorPath('sessions')}>Sessions</Link>
        <Link href={operatorPath('audit')}>Audit</Link>
        <Link href={operatorPath('operators')}>Operators</Link>
        <Link href={operatorPath('deployment')}>Deployment</Link>
        <Link href={operatorPath('realtime')}>Realtime</Link>
        <Link href={operatorPath('configuration')}>Configuration</Link>
      </nav>
      {children}
    </>
  );
}
