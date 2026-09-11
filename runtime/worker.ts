import { DBOS, WorkflowQueue } from '@isoastra/fleet-workflows';
import { Lifecycle } from '@isoastra/fleet-runtime';
import { closePool } from '@/db/client';
import { attemptMailIntent, expirePendingMailIntents, markMailDispatched, pendingMailIntents } from '@/lib/mail-intents';
import { closeMailTransport } from '@/lib/mail';
import { ownChildLifecycle } from './child';
import { runtimeConfig } from './config';

const RETRY_DELAYS = [1_000, 5_000, 30_000, 120_000, 300_000] as const;
new WorkflowQueue('mail', { concurrency: 4 });
const deliverMail = DBOS.registerWorkflow(async (intentId: string) => {
  for (let index = 0; index < 6; index += 1) {
    const result = await DBOS.runStep(
      () => attemptMailIntent(intentId, index + 1, index === 5),
      { name: `deliver-mail-${index + 1}`, retriesAllowed: true, maxAttempts: 3 },
    );
    if (result.outcome !== 'retry') return result.outcome;
    const configured = RETRY_DELAYS[index] ?? RETRY_DELAYS.at(-1)!;
    await DBOS.sleep(Math.max(configured, Math.min(300_000, result.retryAfterMs ?? 0)));
  }
  return 'failed';
}, { name: 'deliverMail' });

function delay(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(finish, ms);
    timer.unref();
    signal.addEventListener('abort', finish, { once: true });
    function finish() { clearTimeout(timer); signal.removeEventListener('abort', finish); resolve(); }
  });
}

async function main(): Promise<void> {
  const config = runtimeConfig();
  const lifecycle = new Lifecycle();
  ownChildLifecycle('worker', lifecycle);
  const stopping = new AbortController();
  lifecycle.register('application database pool', closePool);
  lifecycle.register('mail transport', closeMailTransport);
  DBOS.setConfig({
    name: 'ronitnath',
    applicationVersion: config.appVersion,
    executorID: `ronitnath-${config.replicaId}`,
    systemDatabaseUrl: config.workflowDatabaseUrl,
    systemDatabaseSchemaName: 'dbos',
    runMigrations: false,
    runAdminServer: false,
  });
  await DBOS.launch();
  lifecycle.register('dbos', () => DBOS.shutdown({ workflowCompletionTimeoutMS: 20_000 }));
  lifecycle.register('mail dispatcher', async () => { stopping.abort(); });
  lifecycle.ready();

  while (!stopping.signal.aborted) {
    try {
      await expirePendingMailIntents();
      for (const { id, generation } of await pendingMailIntents()) {
        if (stopping.signal.aborted) break;
        const workflowId = `mail:${id}:${generation}`;
        await DBOS.startWorkflow(deliverMail, { workflowID: workflowId, queueName: 'mail' })(id);
        await markMailDispatched(id, workflowId);
      }
    } catch (error) {
      console.error(JSON.stringify({ level: 'error', event: 'mail.dispatch-failed', message: String(error) }));
    }
    await delay(1_000, stopping.signal);
  }
}

main().catch((error) => {
  console.error(JSON.stringify({ level: 'error', event: 'worker.start-failed', message: String(error) }));
  process.exit(1);
});
