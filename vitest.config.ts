import { fileURLToPath } from 'node:url';
import { config as loadEnv } from 'dotenv';
import { defineConfig } from 'vitest/config';

/* The units are pure; one suite is not (src/features/events/__tests__/
 * rsvp.test.ts asks a real database what its unique index does) and it skips
 * itself when nothing is listening. It is handed the DSN and nothing else:
 * a unit that reads the environment must see the environment a test runner
 * gives it, not a developer's .env — one of them asserts what an unset
 * allowlist does. */
const file = loadEnv({ path: ['.env.local', '.env'], processEnv: {}, quiet: true }).parsed ?? {};
process.env.DATABASE_URL ??= file.DATABASE_URL;
/* And the id key, for the same reason: the database-backed suites run real
 * commands, and a real command now appends a `domain_event` whose `org_id` is
 * a public id. Encoding one needs the key the deployment uses. */
process.env.ID_KEY ??= file.ID_KEY;

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
});
