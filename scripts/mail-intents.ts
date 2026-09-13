import { pool, closePool } from '@/db/client';
import { DBOSClient } from '@isoastra/fleet-workflows';

const terminal = ['ERROR', 'CANCELLED', 'MAX_RECOVERY_ATTEMPTS_EXCEEDED'];

async function main(): Promise<void> {
  const [command = 'list', id] = process.argv.slice(2);
  if (command === 'list') {
    const rows = await pool().query(`SELECT * FROM (
      SELECT 'legacy' AS owner,id::text,kind,recipient AS recipient_identity,state,workflow_id,created_at,expires_at,completed_at
        FROM mail_intent WHERE state IN ('pending','dispatching','failed','expired')
      UNION ALL
      SELECT 'notifications' AS owner,id,kind,recipient_id AS recipient_identity,state,workflow_id,created_at,expires_at,completed_at
        FROM fleet_delivery_intent WHERE channel='auth-email' AND state IN ('pending','dispatching','rejected','expired','paused')
      ) intents ORDER BY created_at DESC LIMIT 100`);
    console.table(rows.rows);
    return;
  }
  if (command === 'summary') {
    const rows = await pool().query(`SELECT owner,state,count(*)::int AS count,
      extract(epoch FROM now()-min(created_at))::int AS oldest_seconds FROM (
        SELECT 'legacy' AS owner,state,created_at FROM mail_intent
        UNION ALL SELECT 'notifications',state,created_at FROM fleet_delivery_intent WHERE channel='auth-email'
      ) intents GROUP BY owner,state ORDER BY owner,state`);
    const stalled = await pool().query(`SELECT count(*)::int AS count FROM mail_intent i
      JOIN dbos.workflow_status w ON w.workflow_uuid=i.workflow_id
      WHERE i.state='dispatching' AND w.status=ANY($1::text[])`, [terminal]);
    console.table(rows.rows);
    console.log(`stalled workflows: ${stalled.rows[0]?.count ?? 0}`);
    return;
  }
  if (command === 'show' && id) {
    const intent = await pool().query(`SELECT id,kind,recipient,state,workflow_id,dispatch_generation,
      created_at,expires_at,completed_at FROM mail_intent WHERE id=$1`, [id]);
    const attempts = await pool().query(`SELECT generation,ordinal,provider_invocations,outcome,detail,attempted_at
      FROM mail_attempt WHERE intent_id=$1 ORDER BY generation,ordinal`, [id]);
    if (intent.rowCount) {
      console.table(intent.rows);
      console.table(attempts.rows);
      return;
    }
    const shared = await pool().query(`SELECT tenant_id,id,kind,channel,recipient_id,state,workflow_id,generation,
      correlation_id,provider_deduplication,deduplication_expires_at,created_at,expires_at,completed_at
      FROM fleet_delivery_intent WHERE id=$1`, [id]);
    const sharedAttempts = await pool().query(`SELECT generation,ordinal,provider_invocations,outcome,code,retry_after_ms,
      provider,receipt_id,attempted_at FROM fleet_delivery_attempt WHERE intent_id=$1 ORDER BY generation,ordinal`, [id]);
    if (!shared.rowCount) throw new Error('mail intent not found');
    console.table(shared.rows);
    console.table(sharedAttempts.rows);
    return;
  }
  if (command === 'resume' && id) {
    const client = await pool().connect();
    let workflowId = '';
    try {
      await client.query('BEGIN');
      await client.query(`SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, [`mail-intent:${id}`]);
      const result = await client.query<{ workflow_id: string }>(`SELECT i.workflow_id FROM mail_intent i
        JOIN dbos.workflow_status w ON w.workflow_uuid=i.workflow_id
        WHERE i.id=$1 AND i.state='dispatching' AND i.message IS NOT NULL AND i.expires_at>now()
          AND w.status=ANY($2::text[]) AND w.application_version=$3 FOR UPDATE OF i`,
        [id, terminal, process.env.APP_VERSION ?? 'dev']);
      workflowId = result.rows[0]?.workflow_id ?? '';
      if (!workflowId) throw new Error('intent is not eligible or belongs to another application version');
      await client.query('COMMIT');
    } catch (error) {
      await client.query('ROLLBACK');
      throw error;
    } finally { client.release(); }
    const database = process.env.DBOS_SYSTEM_DATABASE_URL ?? process.env.DATABASE_URL;
    if (!database) throw new Error('DATABASE_URL is required');
    const dbos = await DBOSClient.create({ systemDatabaseUrl: database, systemDatabaseSchemaName: 'dbos',
      systemDatabasePoolSize: 1, systemDatabasePollingConcurrency: 1, applicationName: 'ronitnath' });
    try { await dbos.resumeWorkflow(workflowId, { queueName: 'mail' }); } finally { await dbos.destroy(); }
    console.log(`mail intent ${id} resumed as ${workflowId}`);
    return;
  }
  if (command === 'retry' && id) {
    const client = await pool().connect();
    let changed = false;
    try {
      await client.query('BEGIN');
      await client.query(`SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, [`mail-intent:${id}`]);
      const shared = await client.query<{ tenant_id: string; generation: string }>(`UPDATE fleet_delivery_intent
        SET state='pending',workflow_id=NULL,generation=generation+1,completed_at=NULL,updated_at=now()
        WHERE id=$1 AND state='paused' AND message IS NOT NULL AND expires_at>now() RETURNING tenant_id,generation::text`, [id]);
      if (shared.rows[0]) {
        await client.query(`INSERT INTO fleet_notification_outbox(tenant_id,id,kind,generation,eligible_at,payload)
          VALUES($1,$2,'delivery',$3,now(),$4::jsonb) ON CONFLICT(tenant_id,id) DO UPDATE SET
          generation=EXCLUDED.generation,eligible_at=now(),payload=EXCLUDED.payload,state='pending',workflow_id=NULL,failure=NULL,updated_at=now()`,
          [shared.rows[0].tenant_id, `delivery:${id}`, shared.rows[0].generation, JSON.stringify({ intentId: id, generation: Number(shared.rows[0].generation) })]);
        changed = true;
      }
      const result = changed ? { rowCount: 0 } : await client.query(`UPDATE mail_intent SET state='pending',workflow_id=NULL,
        dispatch_generation=dispatch_generation+1,completed_at=NULL
        WHERE id=$1 AND message IS NOT NULL AND expires_at>now()
          AND (workflow_id IS NULL OR EXISTS (SELECT 1 FROM dbos.workflow_status w
            WHERE w.workflow_uuid=mail_intent.workflow_id AND w.status=ANY($2::text[]))) RETURNING id`, [id, terminal]);
      changed ||= Boolean(result.rowCount);
      await client.query('COMMIT');
    } catch (error) {
      await client.query('ROLLBACK');
      throw error;
    } finally { client.release(); }
    if (!changed) throw new Error('intent is not eligible: active, purged, expired, or already accepted');
    console.log(`mail intent ${id} queued for a fresh workflow`);
    return;
  }
  throw new Error('usage: pnpm mail:intents [list | summary | show <intent-id> | resume <intent-id> | retry <intent-id>]');
}

main().finally(closePool).catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
