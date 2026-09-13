import { defineConfig, devices } from '@playwright/test';

const port = Number(process.env.E2E_BILLING_PORT ?? 3142);
const build = process.env.BILLING_SKIP_BUILD === '1' ? 'true' : 'NEXT_DIST_DIR=.next-billing-build pnpm build';
export default defineConfig({
  testDir: './e2e',
  timeout: 600_000,
  fullyParallel: false,
  reporter: [['list']],
  use: { baseURL: `http://127.0.0.1:${port}`, actionTimeout: 30_000, navigationTimeout: 90_000 },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: [
      build,
      'mkdir -p .next-billing-build/standalone/.next-billing-build/static .next-billing-build/standalone/public',
      'cp -R .next-billing-build/static/. .next-billing-build/standalone/.next-billing-build/static/',
      'cp -R public/. .next-billing-build/standalone/public/',
      `MAIL_DIR=${process.cwd()}/.mail PUBLIC_ORIGIN=http://127.0.0.1:${port} PORT=${port} node .next-billing-build/standalone/server.js`,
    ].join(' && '),
    url: `http://127.0.0.1:${port}/billing-test-ready.txt`,
    reuseExistingServer: false,
    timeout: 600_000,
  },
});
