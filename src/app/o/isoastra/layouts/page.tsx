import { database } from '@/db/client';
import { requireOperator } from '@/lib/tiers';
import { readLayoutDraft, listLayoutVersions } from '@isoastra/ui-layout/server';
import { defaultOperatorLayout, OPERATOR_LAYOUT_KEY, operatorLayoutContract } from '@/features/layouts/contract';
import { OperatorLayoutStudio } from '@/features/layouts/studio';

export const dynamic='force-dynamic';

export default async function LayoutsPage() {
  await requireOperator();
  const client=database();
  const [draft,history]=await Promise.all([
    readLayoutDraft(client,'isoastra',OPERATOR_LAYOUT_KEY,operatorLayoutContract),
    listLayoutVersions(client,'isoastra',OPERATOR_LAYOUT_KEY,operatorLayoutContract),
  ]);
  return <main className="indoors"><h1>Layout studio</h1><p className="note">Operator-only structured authoring. Copy can update live; structural changes publish as reviewed snapshots.</p><OperatorLayoutStudio initial={draft.document??defaultOperatorLayout()} version={draft.version} history={history.map(item=>({version:item.version,at:item.publishedAt.toISOString(),by:item.publishedBy}))}/></main>;
}
