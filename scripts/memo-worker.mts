import { config as loadEnv } from 'dotenv';

loadEnv({ path: ['.env.local', '.env'], quiet: true });

const [{ Lifecycle }, { registerMemoAudioLifecycle, runMemoWorker }] = await Promise.all([
  import('@isoastra/fleet-runtime'),
  import('../src/features/memos/worker.ts'),
]);

const controller = new AbortController();
const lifecycle = new Lifecycle();
registerMemoAudioLifecycle(lifecycle);
lifecycle.ready();
const stop = (reason: string) => {
  controller.abort(new Error(reason));
  void lifecycle.drain(reason, Date.now() + 20_000).catch((error) => console.error(error));
};
process.on('SIGINT', () => stop('SIGINT'));
process.on('SIGTERM', () => stop('SIGTERM'));

void runMemoWorker(controller.signal).then(
  () => process.exit(0),
  (error: unknown) => {
    console.error(error);
    process.exit(1);
  },
);
