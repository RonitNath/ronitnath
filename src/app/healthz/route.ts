import { sql } from 'drizzle-orm';
import { database } from '@/db/client';

export const dynamic = 'force-dynamic';
export const revalidate = 0;

const headers = { 'cache-control': 'no-store' };
const version = process.env.APP_VERSION ?? 'dev';

export async function GET() {
  try {
    await database().execute(sql`select 1`);
  } catch (error) {
    console.error(JSON.stringify({ level: 'error', at: 'healthz', message: String(error) }));
    return Response.json({ ok: false, db: 'down', version }, { status: 503, headers });
  }
  return Response.json({ ok: true, db: 'ok', version }, { status: 200, headers });
}
