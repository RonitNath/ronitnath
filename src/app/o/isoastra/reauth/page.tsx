import type { Metadata } from 'next';
import Link from 'next/link';

import { IsoastraButton } from '@/features/auth/components/forms';
import { ReauthForm } from '@/features/platform/components/reauth-form';
import { isFresh, REAUTH_WINDOW_MINUTES } from '@/features/platform/reauth';
import { OPERATOR_ROOT, operatorPath } from '@/lib/paths';
import { requireOperator } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Confirm' };
export const dynamic = 'force-dynamic';

/** Only a path back into the operator surface. */
function safeNext(raw: string | undefined): string {
  return raw && new RegExp(`^${OPERATOR_ROOT}(/[A-Za-z0-9\\-._~/]*)?$`).test(raw)
    ? raw
    : operatorPath();
}

/* ReAuthenticate. A cookie says who is here; it does not say the person
 * holding it is still at the keyboard. The operator who signs in through
 * ZITADEL has no password on this side at all, so their half of this page is
 * a fresh round trip: the OP is asked again, better-auth mints a new session
 * when the callback lands, and `/auth/confirmed` stamps it. */
export default async function ReauthPage({
  searchParams,
}: {
  searchParams: Promise<{ next?: string }>;
}) {
  const { principal } = await requireOperator();
  const { next: raw } = await searchParams;
  const next = safeNext(raw);
  const fresh = isFresh(principal.reauthenticatedAt);

  return (
    <main className="indoors">
      <h1>Confirm it is you</h1>
      <p className="note">
        Commands that destroy or impersonate ask for this inside the last {REAUTH_WINDOW_MINUTES}{' '}
        minutes.{' '}
        {fresh ? (
          <>
            The window is open. <Link href={next}>Back</Link>
          </>
        ) : null}
      </p>

      {principal.source === 'oidc' ? (
        <section>
          <p className="note">
            This account signs in through Isoastra and has no password here. One more round trip
            is the proof.
          </p>
          <IsoastraButton next={`/auth/confirmed?next=${encodeURIComponent(next)}`} />
        </section>
      ) : (
        <section>
          <ReauthForm next={next} />
        </section>
      )}
    </main>
  );
}
