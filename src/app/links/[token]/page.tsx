import type { Metadata } from 'next';
import Link from 'next/link';

import { SignInForm } from '@/features/auth/components/forms';
import { currentPrincipal } from '@/features/auth/session';
import { ClaimForm, ClaimRegisterForm } from '@/features/people/components/claim';
import { openClaimLink } from '@/features/people/invitations';
import { normalizeHandle } from '@/features/people/handles';
import { invitationDetail } from '@/features/people/queries';
import { database } from '@/db/client';
import { ThemeToggle } from '../../theme-toggle';

import '../../auth/auth.css';

export const metadata: Metadata = { title: 'An invitation' };
export const dynamic = 'force-dynamic';

/* Expired, revoked, already claimed, never existed: one page, one sentence.
 * A visitor who can tell those apart can use this URL to learn things about
 * people they were never handed a link for. */
function Declined() {
  return (
    <main className="door single">
      <h1>Ronit Nath</h1>
      <p className="note">This link does not work.</p>
      <div className="aside">
        <Link href="/">Back to the landing</Link>
      </div>
    </main>
  );
}

export default async function ClaimPage({ params }: { params: Promise<{ token: string }> }) {
  const { token } = await params;
  /* Opening the link is a fact worth recording — it is what the member who
   * sent it watches — so the stamp happens on the way in. */
  const opened = await database().transaction((tx) => openClaimLink(tx, token));

  const header = (
    <header className="topbar">
      <ThemeToggle />
    </header>
  );
  if (!opened) {
    return (
      <>
        {header}
        <Declined />
      </>
    );
  }

  const invitation = await invitationDetail(opened.personId, opened.createdBy);
  if (!invitation) {
    return (
      <>
        {header}
        <Declined />
      </>
    );
  }

  const principal = await currentPrincipal();
  const handle = invitation.handle ? normalizeHandle(invitation.handle) : null;
  const email = handle?.kind === 'email' ? handle.subject : '';
  const next = `/links/${token}`;

  return (
    <>
      {header}
      <main className={principal ? 'door single' : 'door'}>
        <h1>Ronit Nath</h1>
        <p className="lede">
          {invitation.inviterName ?? 'Someone'} holds a contact called{' '}
          <strong>{invitation.heldName}</strong>
          {invitation.handle ? (
            <>
              {' '}
              at <span className="mono">{invitation.handle}</span>
            </>
          ) : null}
          . If that is you, take it over — everything held under it becomes yours.
        </p>
        {principal ? (
          <ClaimForm token={token} as={principal.displayName} />
        ) : (
          <div className="panes">
            <ClaimRegisterForm token={token} name={invitation.heldName} email={email} />
            <SignInForm next={next} email={email} />
          </div>
        )}
        <div className="aside">
          <Link href="/">Back to the landing</Link>
        </div>
      </main>
    </>
  );
}
