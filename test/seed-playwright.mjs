import { readFile } from 'node:fs/promises';
import pg from 'pg';

const databaseUrl = process.env.E2E_DATABASE_URL;
if (!databaseUrl) throw new Error('E2E_DATABASE_URL must name the isolated test database');

const fixture = JSON.parse(
  await readFile(
    new URL('../src/features/sky/__tests__/fixtures/alioth.json', import.meta.url),
    'utf8',
  ),
);
const client = new pg.Client({ connectionString: databaseUrl, connectionTimeoutMillis: 5_000 });
try {
  await client.connect();
  await client.query(
    `INSERT INTO sky_star_detail (id, payload, built_at)
     VALUES ($1, $2::jsonb, now())
     ON CONFLICT (id) DO UPDATE SET payload = excluded.payload, built_at = excluded.built_at`,
    ['g1576683529448755328', JSON.stringify(fixture)],
  );
} finally {
  await client.end();
}
