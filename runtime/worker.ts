import { DBOS, WorkflowQueue } from '@isoastra/fleet-workflows';
import { Lifecycle } from '@isoastra/fleet-runtime';
import { closePool } from '@/db/client';
import {
  deliverMailIntent,
  expirePendingMailIntents,
  markMailDispatched,
  pendingMailIntents,
  recordMailAttempt,
} from '@/lib/mail-intents';
import { closeMailTransport } from '@/lib/mail';
import { assertSchemaCompatibility } from '@/lib/schema-compatibility';
import { Client } from 'pg';
import { ownChildLifecycle } from './child';
import { runtimeConfig } from './config';

const RETRY_DELAYS = [1_000, 5_000, 30_000, 120_000, 300_000] as const;
new WorkflowQueue('mail', { concurrency: 4 });
const deliverMail = DBOS.registerWorkflow(
  async (intentId: string, generation: number) => {
    for (let index = 0; index < 6; index += 1) {
      const result = await DBOS.runStep(
        () => deliverMailIntent(intentId, generation, index + 1),
        { name: `send-mail-${index + 1}`, retriesAllowed: true, maxAttempts: 3 },
      );
      const recorded = await DBOS.runStep(
        () => recordMailAttempt(intentId, generation, index + 1, result, index === 5),
        { name: `record-mail-${index + 1}`, retriesAllowed: true, maxAttempts: 3 },
      );
      if (recorded !== 'retry') return recorded;
      const configured = RETRY_DELAYS[index] ?? RETRY_DELAYS.at(-1)!;
      const requested = Math.max(
        configured,
        Math.min(300_000, result.outcome === 'retry' ? (result.retryAfterMs ?? 0) : 0),
      );
      const remaining =
        result.outcome === 'retry' ? new Date(result.expiresAt).getTime() - Date.now() : 0;
      if (remaining <= 0) continue;
      await DBOS.sleep(Math.min(requested, remaining));
    }
    return 'failed';
  },
  { name: 'deliverMail' },
);

function delay(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(finish, ms);
    timer.unref();
    signal.addEventListener('abort', finish, { once: true });
    function finish() {
      clearTimeout(timer);
      signal.removeEventListener('abort', finish);
      resolve();
    }
  });
}

async function main(): Promise<void> {
  const config = runtimeConfig();
  const lifecycle = new Lifecycle();
  const stopping = new AbortController();
  let dispatchDone: Promise<void> = Promise.resolve();
  ownChildLifecycle('worker', lifecycle, {
    quiesce: async () => {
      stopping.abort();
      await dispatchDone;
    },
  });
  try {
    lifecycle.register('application database pool', closePool);
    lifecycle.register('mail transport', closeMailTransport);
    const replicaLock = new Client({
      connectionString: config.databaseUrl,
      connectionTimeoutMillis: 5_000,
      query_timeout: 15_000,
      statement_timeout: 15_000,
      lock_timeout: 5_000,
    });
    replicaLock.on('error', (error) => {
      console.error(
        JSON.stringify({ level: 'error', event: 'replica-lock-lost', message: error.message }),
      );
      process.exit(1);
    });
    lifecycle.register('replica identity lock', () => replicaLock.end());
    await replicaLock.connect();
    const lock = await replicaLock.query<{ acquired: boolean }>(
      `SELECT pg_try_advisory_lock(hashtextextended($1, 0)) AS acquired`,
      [`ronitnath:${config.replicaId}`],
    );
    if (!lock.rows[0]?.acquired)
      throw new Error(`replica identity is already active: ${config.replicaId}`);
    DBOS.setConfig({
      name: 'ronitnath',
      applicationVersion: config.appVersion,
      executorID: `ronitnath-${config.replicaId}`,
      systemDatabaseUrl: config.workflowDatabaseUrl,
      systemDatabaseSchemaName: 'dbos',
      runMigrations: false,
      runAdminServer: false,
      systemDatabasePoolSize: 10,
      systemDatabasePollingConcurrency: 4,
    });
    await DBOS.launch();
    lifecycle.register('dbos', () => DBOS.shutdown({ workflowCompletionTimeoutMS: 20_000 }));
    lifecycle.register('mail dispatcher', async () => {
      stopping.abort();
    });
    await assertSchemaCompatibility();
    lifecycle.ready();

    dispatchDone = (async () => {
      while (!stopping.signal.aborted) {
        try {
          await expirePendingMailIntents();
          for (const { id, generation } of await pendingMailIntents()) {
            if (stopping.signal.aborted) break;
            const workflowId = `mail:${id}:${generation}`;
            await DBOS.startWorkflow(deliverMail, {
              workflowID: workflowId,
              queueName: 'mail',
            })(id, generation);
            await markMailDispatched(id, generation, workflowId);
          }
        } catch (error) {
          console.error(
            JSON.stringify({
              level: 'error',
              event: 'mail.dispatch-failed',
              message: String(error),
            }),
          );
        }
        await delay(1_000, stopping.signal);
      }
    })();
    await dispatchDone;
  } catch (error) {
    await lifecycle
      .drain('startup or dispatch failure', Date.now() + 5_000)
      .catch(() => undefined);
    throw error;
  }
}

main().catch((error) => {
  console.error(
    JSON.stringify({ level: 'error', event: 'worker.start-failed', message: String(error) }),
  );
  process.exit(1);
});
