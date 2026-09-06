import type { Metadata } from 'next';

import { requireOperator } from '@/lib/tiers';

import '../app/app.css';

export const metadata: Metadata = { title: 'Platform' };
export const dynamic = 'force-dynamic';

/* R6 fills this in. Until then it exists so that the operator tier is a real
 * boundary rather than a promise: anyone who is not an operator gets a 404,
 * which is what a page that is not theirs looks like. */
export default async function PlatformPage() {
  await requireOperator();
  return (
    <main className="indoors">
      <h1>Platform</h1>
      <p className="empty">
        Parties, identities, matches, sessions and the audit feed land here at R6.
      </p>
    </main>
  );
}
