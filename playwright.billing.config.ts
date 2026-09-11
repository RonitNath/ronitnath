import { defineConfig, devices } from '@playwright/test';

const port = 3142;
export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  reporter: [['list']],
  use: { baseURL: `http://127.0.0.1:${port}` },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: `PUBLIC_ORIGIN=http://127.0.0.1:${port} pnpm dev --port ${port}`,
    url: `http://127.0.0.1:${port}/healthz`,
    reuseExistingServer: false,
    timeout: 90_000,
  },
});
