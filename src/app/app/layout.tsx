import { Shell } from '@/features/auth/components/shell';
import { requireMember } from '@/lib/tiers';

import './app.css';

export const dynamic = 'force-dynamic';

export default async function AppLayout({ children }: { children: React.ReactNode }) {
  const { principal } = await requireMember('/app');
  return (
    <>
      <Shell principal={principal} />
      {children}
    </>
  );
}
