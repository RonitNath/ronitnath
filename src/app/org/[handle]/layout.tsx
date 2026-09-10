import { Shell } from '@/features/auth/components/shell';
import { currentPrincipal } from '@/features/auth/principal';

import '../../app/app.css';

export const dynamic = 'force-dynamic';

/* The same chrome as `/app`: an organization's page is a member's page with
 * one more thing they are allowed to do. The layout only draws the chrome —
 * the tier check is the page's, because the page is the thing that knows the
 * handle and can send a visitor back to it after they sign in. */
export default async function OrgLayout({ children }: { children: React.ReactNode }) {
  const principal = await currentPrincipal();
  return (
    <>
      {principal ? <Shell principal={principal} /> : null}
      {children}
    </>
  );
}
