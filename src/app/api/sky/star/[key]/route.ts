/** `GET /api/sky/star/<key>` — everything the site knows about one star.
 *
 * The whole of S3's server side. It reads one row from `sky_star_detail` by
 * primary key, normalises it (`star-detail.ts`) and returns it. It is a read:
 * no audit row, no session, no transaction, nothing written (docs/plan.md).
 * It contacts nothing — the datasets are local, and the browser and the server
 * both stay off ESA and CDS at runtime (docs/sky-plan.md S3).
 *
 * A star that the detail build never saw is not an error. The bright catalogue
 * and the detail dataset were built from different cuts of Gaia, so a pick can
 * name a star that has no row; the answer is then what the key itself says,
 * with a 200 and the same cache lifetime, because it is just as true a day
 * from now.
 *
 * A `hip-` key is looked up twice. The bright catalogue calls a first-magnitude
 * star by its Hipparcos number, but the detail build filed it under its Gaia
 * source id wherever the positional match succeeded: Alioth is `hip-62956` to
 * the sky and `g1576683529448755328` to the table, with `hip: "62956"` in the
 * payload. Only the 66 Hipparcos stars Gaia has no row for are keyed `h<n>`.
 * So the primary key first, and the payload's own HIP number second, over the
 * index migration 0007 adds — otherwise every bright named star a visitor
 * clicks answers "no further record" about the best-known stars in the sky.
 */

import { sql } from 'drizzle-orm';

import { database } from '@/db/client';
import { callerKey, tokenBucket } from '@/lib/rate-limit';
import { catalogOnly, normaliseStarDetail, parseStarKey } from '@/features/sky/star-detail';

export const dynamic = 'force-dynamic';

/** The catalogue is fixed for the life of a deployment: a star's parallax does
 * not change, and a rebuilt dataset arrives with a new image. */
const CACHE = { 'cache-control': 'public, max-age=86400' };
const NO_STORE = { 'cache-control': 'no-store' };

/** Sixty a minute per address. Opening the panel is one request and the
 * browser caches it for a day, so a visitor clicking across the whole sky for
 * an hour never reaches this; a crawler walking three million ids does, on its
 * first second. */
const limiter = tokenBucket(60, 60_000);

export async function GET(
  request: Request,
  context: { params: Promise<{ key: string }> },
): Promise<Response> {
  const key = parseStarKey((await context.params).key);
  if (!key) {
    return Response.json({ error: 'unknown star key' }, { status: 400, headers: NO_STORE });
  }
  if (!limiter.take(callerKey(request.headers))) {
    return Response.json({ error: 'too many requests' }, { status: 429, headers: NO_STORE });
  }
  try {
    const db = database();
    let found = await db.execute<{ payload: unknown }>(
      sql`select payload from sky_star_detail where id = ${key.rowId} limit 1`,
    );
    if (found.rows.length === 0 && key.kind === 'hip') {
      found = await db.execute<{ payload: unknown }>(
        sql`select payload from sky_star_detail where payload->>'hip' = ${key.number} limit 1`,
      );
    }
    const row = found.rows[0];
    const detail = row ? normaliseStarDetail(key, row.payload) : catalogOnly(key);
    return Response.json(detail, { status: 200, headers: CACHE });
  } catch (error) {
    console.error(
      JSON.stringify({ level: 'error', at: 'sky.star', key: key.key, message: String(error) }),
    );
    return Response.json({ error: 'unavailable' }, { status: 503, headers: NO_STORE });
  }
}
