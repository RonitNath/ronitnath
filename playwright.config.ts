import { defineConfig, devices } from '@playwright/test';

const databaseUrl = process.env.E2E_DATABASE_URL;
if (!databaseUrl) throw new Error('E2E_DATABASE_URL must name an isolated test database');
process.env.DATABASE_URL = databaseUrl;
process.env.ID_KEY = '00112233445566778899aabbccddeeff';
process.env.AUTH_SECRET = 'isolated-e2e-auth-secret-at-least-32-characters';

const port = Number(process.env.E2E_PORT ?? 3141);
const baseURL = `http://127.0.0.1:${port}`;
process.env.PUBLIC_ORIGIN = baseURL;
process.env.APP_VERSION = 'e2e';
process.env.REPLICA_ID = 'e2e';

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  reporter: [['list']],
  use: {
    baseURL,
    trace: 'off',
    screenshot: process.env.E2E_SCREENSHOT ? 'on' : 'only-on-failure',
    /* The sky is a WebGL2 canvas, and headless Chromium on a machine with no
     * GPU has no WebGL2 at all: every sky spec then waits twenty seconds for a
     * pixel that is never lit. `E2E_SOFTWARE_GL=1` swaps in ANGLE's software
     * rasteriser, which is how the suite runs at full parallelism on a
     * server. A workstation with a GPU wants the real driver, so it is opt-in.
     */
    launchOptions: {
      args: process.env.E2E_SOFTWARE_GL
        ? ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader']
        : [],
    },
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    /* The smoke runs the same supervised HTTP/worker process tree as the image. */
    command: [
      'pnpm db:migrate:all',
      'pnpm build',
      /* `MAIL_FAIL` names a substring of a recipient the transport must
       * refuse, so one run can watch a send fail for one visitor and land for
       * everyone else (src/lib/mail.ts). */
      /* The server is told the origin it is actually reachable at, because
         the door redirects to an absolute `PUBLIC_ORIGIN` URL: with `.env`'s
         development origin it sends the browser to a port nothing serves. */
      `MAIL_INTENT_KEY=e2e-mail-key MAIL_DIR=${process.cwd()}/.mail MAIL_FAIL=mailfail APP_VERSION=e2e REPLICA_ID=e2e PUBLIC_ORIGIN=${baseURL} HOSTNAME=127.0.0.1 PORT=${port} pnpm start`,
    ].join(' && '),
    url: `${baseURL}/readyz`,
    reuseExistingServer: false,
    timeout: 180_000,
  },
});
