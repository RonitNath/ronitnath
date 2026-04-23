import { defineConfig, devices } from "@playwright/test";

const PORT = 8087;
const BASE_URL = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 30_000,
  expect: { timeout: 5_000 },
  reporter: [["list"]],
  outputDir: "test-results",
  use: {
    baseURL: BASE_URL,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "node tests/e2e/start-server.mjs",
    url: `${BASE_URL}/healthz`,
    reuseExistingServer: false,
    timeout: 60_000,
    stdout: "pipe",
    stderr: "pipe",
    env: {
      PORT: String(PORT),
      HOST: "127.0.0.1",
      PUBLIC_BASE_URL: BASE_URL,
      DATABASE_URL: "sqlite://./tests/e2e/.tmp/events-e2e.db",
      RONITNATH_DEV: "1",
      EVENT_TOKEN_SECRET: "e2e-test-secret-must-be-32-bytes-or-longer-xxxxxx",
      SESSION_COOKIE_SECURE: "false",
      ADMIN_IDENTITY_IDS: "dev-admin",
    },
  },
});
