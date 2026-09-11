import { managedPostgresListener } from '@isoastra/fleet-events/listener';

const listener = managedPostgresListener({
  databaseKey: 'ronitnath',
  connectionString: process.env.DATABASE_URL ?? 'postgres://unconfigured',
  channel: 'realtime_delivery',
});
export function subscribeDeliveries(handler: () => void): () => void {
  return listener.subscribe(() => handler());
}
export function disposeDeliveryListener(): Promise<void> { return listener.dispose(); }
