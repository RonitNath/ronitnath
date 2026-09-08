/** Load the built star detail into Postgres, with nothing but Node and `pg`.
 *
 *   node scripts/load-sky.mjs [path-to-sky_star_detail.csv.zst]
 *
 * The catalogue is 3,087,894 rows and 257 MB compressed, and the obvious way
 * to move it is `COPY ... FROM STDIN`. That needs either `psql` or
 * `pg-copy-streams`, and the deployment has neither: the runtime image is
 * `node:24-alpine` with the application's own dependencies and nothing else,
 * and adding a Postgres client to it to run one command once per environment
 * is a permanent cost for a one-off. So this streams the file through Node's
 * own zstd decoder (`node:zlib`, since v22.15) and inserts in batches of a
 * thousand rows — 3,088 round trips instead of one, which is about five
 * minutes and exactly as correct.
 *
 * The load is one transaction and a full replacement, the same shape
 * `tools/starcat/load_detail.sql` has: the build is the whole catalogue, so
 * TRUNCATE + INSERT beats an upsert plus an anti-join delete over three
 * million rows, and a failure anywhere leaves the old rows in place.
 *
 * The table is created by the drizzle migration, never here. Loading into a
 * table that does not exist would be a silent success against nothing.
 */

import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { access, readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { createInterface } from 'node:readline';
import { createZstdDecompress } from 'node:zlib';

import pg from 'pg';

/** A thousand rows is two thousand bind parameters, well inside Postgres's
 * limit of 65,535, and about a megabyte on the wire. */
const BATCH = 1_000;
const PROGRESS_EVERY = 250_000;

const path = process.argv[2] ?? 'data/sky_star_detail.csv.zst';
const url = process.env.DATABASE_URL;
if (!url) fail('DATABASE_URL is not set');
await access(path).catch(() => fail(`${path} is not there — build it first (tools/starcat)`));

function fail(message) {
  console.error(message);
  process.exit(1);
}

/** Two CSV columns written by `build_detail.py`: an id, then a JSON payload as
 * one quoted field. The payload is compact JSON, so a row is a line — checked
 * at build time, where the row count and the line count agree exactly — and
 * the only quoting to undo is CSV's doubled quote. */
function parse(line) {
  const comma = line.indexOf(',');
  if (comma < 1) return null;
  const id = line.slice(0, comma);
  let payload = line.slice(comma + 1);
  if (payload.startsWith('"') && payload.endsWith('"')) {
    payload = payload.slice(1, -1).replaceAll('""', '"');
  }
  return [id, payload];
}

/** `insert into … values ($1,$2),($3,$4),…`, built once per batch size. */
const statements = new Map();
function insertText(rows) {
  let text = statements.get(rows);
  if (text) return text;
  const tuples = Array.from({ length: rows }, (_, i) => `($${i * 2 + 1},$${i * 2 + 2})`);
  text = `insert into sky_star_detail_incoming (id, payload) values ${tuples.join(',')}`;
  statements.set(rows, text);
  return text;
}

/** The checksum beside the file is committed (`data/sky_star_detail.sha256`),
 * so a build that travelled over scp can say it is the build that was
 * measured. Checked before a byte is loaded: a truncated file would fail in
 * the zstd decoder anyway, but a *different* file would not. */
async function checksum() {
  const sidecar = join(dirname(path), 'sky_star_detail.sha256');
  const expected = await readFile(sidecar, 'utf8').catch(() => null);
  const digest = createHash('sha256');
  for await (const chunk of createReadStream(path)) digest.update(chunk);
  const actual = digest.digest('hex');
  if (!expected) return `sha256 ${actual} (no ${sidecar} to check it against)`;
  const want = expected.trim().split(/\s+/)[0];
  if (want !== actual) fail(`sha256 ${actual} is not the built ${want} — wrong file`);
  return `sha256 ${actual}, as built`;
}

console.log(await checksum());

const client = new pg.Client({ connectionString: url });
await client.connect();

const exists = await client.query(`select to_regclass('public.sky_star_detail') as table`);
if (!exists.rows[0].table) {
  await client.end();
  fail('sky_star_detail does not exist — run the drizzle migration first (pnpm db:migrate)');
}

const startedAt = Date.now();
let rows = 0;
let batch = [];

async function flush() {
  if (!batch.length) return;
  await client.query(insertText(batch.length / 2), batch);
  batch = [];
}

try {
  await client.query('begin');
  await client.query(
    `create temp table sky_star_detail_incoming (id text not null, payload jsonb not null)
     on commit drop`,
  );

  const lines = createInterface({
    input: createReadStream(path).pipe(createZstdDecompress()),
    crlfDelay: Infinity,
  });
  for await (const line of lines) {
    if (!line) continue;
    const row = parse(line);
    if (!row) fail(`row ${rows + 1} is not two columns`);
    batch.push(row[0], row[1]);
    rows += 1;
    if (batch.length >= BATCH * 2) await flush();
    if (rows % PROGRESS_EVERY === 0) {
      console.log(`${rows.toLocaleString('en-US')} rows in ${seconds()}s`);
    }
  }
  await flush();

  // The replacement itself, inside the same transaction that read the file.
  await client.query('truncate sky_star_detail');
  await client.query(
    `insert into sky_star_detail (id, payload, built_at)
     select id, payload, now() from sky_star_detail_incoming`,
  );
  await client.query('commit');
} catch (error) {
  await client.query('rollback').catch(() => {});
  await client.end();
  fail(`load failed after ${rows} rows, table unchanged: ${error}`);
}

const count = await client.query('select count(*)::bigint as rows from sky_star_detail');
await client.end();
console.log(`loaded ${Number(count.rows[0].rows).toLocaleString('en-US')} rows in ${seconds()}s`);

function seconds() {
  return ((Date.now() - startedAt) / 1_000).toFixed(0);
}
