import { managedPostgresListener } from '@isoastra/fleet-events/listener';

export const CHANNEL = 'domain_event';
export interface Notice { orgId: string; seq: number }
export type NoticeHandler = (notice: Notice) => void;

const listener = managedPostgresListener({
  databaseKey: 'ronitnath',
  connectionString: process.env.DATABASE_URL ?? 'postgres://unconfigured',
  channel: CHANNEL,
});

export function subscribe(orgId: string, handler: NoticeHandler): () => void {
  return listener.subscribe((payload) => {
    if (!payload) return;
    const bar = payload.lastIndexOf('|');
    if (bar <= 0 || payload.slice(0, bar) !== orgId) return;
    const seq = Number(payload.slice(bar + 1));
    if (Number.isFinite(seq)) handler({ orgId, seq });
  });
}
export function listening(): boolean { return listener.connected(); }
export function disposeStreamListener(): Promise<void> { return listener.dispose(); }
