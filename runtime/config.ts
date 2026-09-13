export function runtimeConfig() {
  const production = process.env.NODE_ENV === 'production';
  const appVersion = process.env.APP_VERSION ?? (production ? '' : 'dev');
  const replicaId = process.env.REPLICA_ID ?? (production ? '' : 'local');
  const databaseUrl = process.env.DATABASE_URL ?? '';
  const workflowDatabaseUrl = process.env.DBOS_SYSTEM_DATABASE_URL ?? databaseUrl;
  const publicOrigin = process.env.PUBLIC_ORIGIN?.replace(/\/+$/, '') ?? (production ? '' : 'http://localhost:3000');
  if (!databaseUrl) throw new Error('DATABASE_URL is required');
  if (workflowDatabaseUrl !== databaseUrl) throw new Error('DBOS_SYSTEM_DATABASE_URL must equal DATABASE_URL');
  if (!appVersion || appVersion === 'dev' && production) throw new Error('APP_VERSION must identify the production build');
  if (!replicaId) throw new Error('REPLICA_ID is required in production');
  if (!publicOrigin) throw new Error('PUBLIC_ORIGIN is required in production');
  const port = Number(process.env.PORT ?? 3140);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) throw new Error('PORT must be an integer from 1 to 65535');
  const parsedOrigin = new URL(publicOrigin);
  if ((parsedOrigin.protocol !== 'http:' && parsedOrigin.protocol !== 'https:') || parsedOrigin.origin !== publicOrigin) {
    throw new Error('PUBLIC_ORIGIN must be an HTTP(S) origin without a path');
  }
  return {
    appVersion,
    replicaId,
    databaseUrl,
    workflowDatabaseUrl,
    publicOrigin,
    host: process.env.HOSTNAME ?? '127.0.0.1',
    port,
  };
}
