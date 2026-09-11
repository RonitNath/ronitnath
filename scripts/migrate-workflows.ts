import { DBOS } from '@isoastra/fleet-workflows';

async function main(): Promise<void> {
  const database = process.env.DBOS_SYSTEM_DATABASE_URL ?? process.env.DATABASE_URL;
  if (!database) throw new Error('DBOS_SYSTEM_DATABASE_URL or DATABASE_URL is required');
  DBOS.setConfig({
    name: 'ronitnath',
    applicationVersion: process.env.APP_VERSION ?? 'migration',
    executorID: `migration-${process.pid}`,
    systemDatabaseUrl: database,
    systemDatabaseSchemaName: 'dbos',
    runMigrations: true,
    runAdminServer: false,
  });
  await DBOS.launch();
  await DBOS.shutdown();
  console.log('workflow migration: dbos schema is current');
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
