import { desc, eq } from 'drizzle-orm';

import { database, schema } from '@/db/client';

export async function deliveryReport(selectedId?: string) {
  const runs = await database().select().from(schema.deliveryRun).orderBy(desc(schema.deliveryRun.startedAt)).limit(100);
  const selected = selectedId ? runs.find((run) => run.id === selectedId) : runs[0];
  const events = selected
    ? await database().select().from(schema.deliveryRunEvent).where(eq(schema.deliveryRunEvent.runId, selected.id)).orderBy(schema.deliveryRunEvent.seq)
    : [];
  return { runs, selected, events };
}
