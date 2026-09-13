import { pool } from '@/db/client';

export const REQUIRED_APPLICATION_MIGRATIONS = 16;

export async function schemaCompatibility(): Promise<{ ok: boolean; detail?: string }> {
  const result = await pool().query<{ migrations: string; workflow: boolean; mail: boolean; generation: boolean }>(`
    SELECT
      (SELECT count(*)::text FROM drizzle.__drizzle_migrations) AS migrations,
      to_regclass('dbos.workflow_status') IS NOT NULL AS workflow,
      to_regclass('public.mail_intent') IS NOT NULL AS mail,
      EXISTS(SELECT 1 FROM information_schema.columns
        WHERE table_schema='public' AND table_name='mail_attempt' AND column_name='generation') AS generation`);
  const row = result.rows[0];
  if (!row || Number(row.migrations) < REQUIRED_APPLICATION_MIGRATIONS) {
    return { ok: false, detail: `application migrations ${row?.migrations ?? 'unknown'}/${REQUIRED_APPLICATION_MIGRATIONS}` };
  }
  if (!row.workflow || !row.mail || !row.generation) return { ok: false, detail: 'required DBOS or mail schema objects are absent' };
  return { ok: true };
}

export async function assertSchemaCompatibility(): Promise<void> {
  const result = await schemaCompatibility();
  if (!result.ok) throw new Error(`schema is incompatible: ${result.detail}`);
}
