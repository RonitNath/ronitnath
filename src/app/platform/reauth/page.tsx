import type { Metadata } from 'next';
import Link from 'next/link';

import { ReauthForm } from '@/features/platform/components/reauth-form';
import { isFresh, REAUTH_WINDOW_MINUTES } from '@/features/platform/reauth';
import { requireOperator } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Confirm' };
export const dynamic = 'force-dynamic';

/** Only a path back into the operator surface. */
function safeNext(raw: string | undefined): string {
  return raw && /^\/platform(\/[A-Za-z0-9\-._~/]*)?$/.test(raw) ? raw : '/platform';
}

/* ReAuthenticate. A cookie says who is here; it does not say the person
 * holding it is still at the keyboard. The operator who signs in through
 * ZITADEL has no password on this side at all, so their half of this page is
 * a fresh round trip with `prompt=login` — the OP is asked to prove it, not
 * to remember it. */
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
          <p>
            <a
              className="commit"
              href={`/auth/oidc/start?reauth=1&next=${encodeURIComponent(next)}`}
            >
              Confirm with Isoastra
            </a>
          </p>
        </section>
      ) : (
        <section>
          <ReauthForm next={next} />
        </section>
      )}
    </main>
  );
}
