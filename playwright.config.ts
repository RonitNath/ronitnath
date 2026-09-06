import { config as loadEnv } from 'dotenv';
import { defineConfig, devices } from '@playwright/test';

/* The spec files read `.env.local` to decide whether the OIDC door is
 * configured here; the server under test loads the same files itself. */
loadEnv({ path: ['.env.local', '.env'], quiet: true });

const port = Number(process.env.E2E_PORT ?? 3141);
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  reporter: [['list']],
  use: { baseURL, trace: 'off' },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    /* The smoke runs the artifact the image ships: the standalone server,
     * with the static assets laid out beside it the way the Dockerfile does. */
    command: [
      'pnpm build',
      'rm -rf .next/standalone/.next/static .next/standalone/public',
      'cp -R .next/static .next/standalone/.next/static',
      'cp -R public .next/standalone/public',
      `MAIL_DIR=${process.cwd()}/.mail PORT=${port} node .next/standalone/server.js`,
    ].join(' && '),
    url: `${baseURL}/healthz`,
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
});
