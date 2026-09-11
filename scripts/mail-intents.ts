import { pool, closePool } from '@/db/client';

async function main(): Promise<void> {
  const [command = 'list', id] = process.argv.slice(2);
  if (command === 'list') {
    const rows = await pool().query(`SELECT id,kind,recipient,state,workflow_id,created_at,expires_at,completed_at
      FROM mail_intent WHERE state IN ('pending','dispatching','failed','expired') ORDER BY created_at DESC LIMIT 100`);
    console.table(rows.rows);
    return;
  }
  if (command === 'retry' && id) {
    const result = await pool().query(`UPDATE mail_intent
      SET state='pending',workflow_id=NULL,dispatch_generation=dispatch_generation+1,completed_at=NULL
      WHERE id=$1 AND message IS NOT NULL AND expires_at>now()
        AND (workflow_id IS NULL OR EXISTS (
          SELECT 1 FROM dbos.workflow_status w WHERE w.workflow_uuid=mail_intent.workflow_id
          AND w.status IN ('ERROR','CANCELLED','MAX_RECOVERY_ATTEMPTS_EXCEEDED')
        )) RETURNING id`, [id]);
    if (!result.rowCount) throw new Error('intent is not eligible: content was purged, expired, or already accepted');
    console.log(`mail intent ${id} queued for a fresh workflow`);
    return;
  }
  throw new Error('usage: pnpm mail:intents [list | retry <intent-id>]');
}

main().finally(closePool).catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
