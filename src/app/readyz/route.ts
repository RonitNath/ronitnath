/* Ready, as distinct from alive.
 *
 * `/healthz` answers the question the edge's prober asks — is this process
 * serving, and can it reach a database. `/readyz` answers the one a deploy
 * asks: is the database this process reached actually the shape this build
 * needs. They separate because the interesting failure is the one where both
 * halves are healthy and the schema between them is a migration behind.
 *
 * The three triggers are what it checks, because they are the part of the
 * spine that is not in the application (contracts.md: `/readyz` asserts the
 * three triggers exist). A `domain_event` table without them still accepts
 * inserts — it simply assigns no sequence, notifies nobody, and lets anything
 * rewrite history. That is a failure no request would report and no test on a
 * machine with a correct database would catch, which is exactly the kind worth
 * a probe of its own. */

import { sql } from 'drizzle-orm';
import { database } from '@/db/client';

export const dynamic = 'force-dynamic';
export const revalidate = 0;

const headers = { 'cache-control': 'no-store' };
const version = process.env.APP_VERSION ?? 'dev';

const REQUIRED = ['domain_event_assign_seq', 'domain_event_notify', 'domain_event_append_only'];

export async function GET() {
  try {
    const found = await database().execute<{ tgname: string }>(sql`
      select t.tgname from pg_trigger t
      join pg_class c on c.oid = t.tgrelid
      where c.relname = 'domain_event' and not t.tgisinternal`);
    const names = new Set(found.rows.map((row) => row.tgname));
    const missing = REQUIRED.filter((name) => !names.has(name));
    if (missing.length > 0) {
      return Response.json({ ok: false, missing, version }, { status: 503, headers });
    }
    return Response.json({ ok: true, spine: 'ok', version }, { status: 200, headers });
  } catch (error) {
    console.error(JSON.stringify({ level: 'error', at: 'readyz', message: String(error) }));
    return Response.json({ ok: false, db: 'down', version }, { status: 503, headers });
  }
}
