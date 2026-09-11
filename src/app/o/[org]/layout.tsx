import { Shell } from '@/features/auth/components/shell';
import { currentPrincipal } from '@/features/auth/principal';
import { encodeId } from '@/lib/ids';

import '../../indoors.css';

export const dynamic = 'force-dynamic';

/* The same chrome as `/u/<user>`: an organization's page is a member's page
 * with one more thing they are allowed to do. The layout only draws the
 * chrome — the tier check is the page's, because the page is the thing that
 * knows the handle and can send a visitor back to it after they sign in.
 *
 * The nav here links to the reader's own surfaces: there is no subject in an
 * `/o/<org>` path, so the person the nav is about is the one asking. */
export default async function OrgLayout({ children }: { children: React.ReactNode }) {
  const principal = await currentPrincipal();
  return (
    <>
      {principal ? (
        <Shell principal={principal} user={encodeId('person', principal.personId)} />
      ) : null}
      {children}
    </>
  );
}
