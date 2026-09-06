import { fileURLToPath } from 'node:url';
import { config as loadEnv } from 'dotenv';
import { defineConfig } from 'vitest/config';

/* The units are pure; one suite is not (src/features/events/__tests__/
 * rsvp.test.ts asks a real database what its unique index does) and it skips
 * itself when nothing is listening. It is handed the DSN and nothing else:
 * a unit that reads the environment must see the environment a test runner
 * gives it, not a developer's .env — one of them asserts what an unset
 * allowlist does. */
const dsn = loadEnv({ path: ['.env.local', '.env'], processEnv: {}, quiet: true }).parsed
  ?.DATABASE_URL;
process.env.DATABASE_URL ??= dsn;

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
});
