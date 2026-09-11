import { Client } from 'pg';

type Handler = () => void;
interface Hub {
  client: Client | null;
  handlers: Set<Handler>;
  connecting: boolean;
  reconnect: ReturnType<typeof setTimeout> | null;
}
const globalHub = globalThis as unknown as { rnDeliveryHub?: Hub };
function state(): Hub {
  return (globalHub.rnDeliveryHub ??= {
    client: null,
    handlers: new Set(),
    connecting: false,
    reconnect: null,
  });
}

function connect(): void {
  const hub = state();
  if (hub.client || hub.connecting || hub.handlers.size === 0) return;
  const connectionString = process.env.DATABASE_URL;
  if (!connectionString) throw new Error('DATABASE_URL is not set');
  const client = new Client({ connectionString });
  hub.connecting = true;
  const retry = () => {
    if (hub.client === client) hub.client = null;
    hub.connecting = false;
    if (!hub.reconnect && hub.handlers.size)
      hub.reconnect = setTimeout(() => {
        hub.reconnect = null;
        connect();
      }, 2_000);
  };
  client.on('notification', () => {
    for (const handler of [...hub.handlers]) handler();
  });
  client.on('error', retry);
  client.on('end', retry);
  client
    .connect()
    .then(() => client.query('LISTEN realtime_delivery'))
    .then(() => {
      hub.client = client;
      hub.connecting = false;
    })
    .catch(retry);
}

export function subscribeDeliveries(handler: Handler): () => void {
  const hub = state();
  hub.handlers.add(handler);
  connect();
  return () => {
    hub.handlers.delete(handler);
    if (hub.handlers.size) return;
    if (hub.reconnect) clearTimeout(hub.reconnect);
    hub.reconnect = null;
    const client = hub.client;
    hub.client = null;
    void client?.end().catch(() => {});
  };
}
