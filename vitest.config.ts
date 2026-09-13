import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

/* Tests never inherit an application database or mail provider. The isolated
 * PostgreSQL harness opts database suites in through TEST_DATABASE_URL. */
if (process.env.TEST_DATABASE_URL) process.env.DATABASE_URL = process.env.TEST_DATABASE_URL;
else delete process.env.DATABASE_URL;
/* Public-id tests use a deterministic test-only key. */
process.env.ID_KEY = '00112233445566778899aabbccddeeff';
delete process.env.ID_KEY_PREV;
delete process.env.USESEND_API_URL;
delete process.env.USESEND_API_KEY;
delete process.env.SMTP_URL;

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'runtime/**/*.test.ts'],
    testTimeout: 15_000,
  },
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
});
