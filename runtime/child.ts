import { isParentMessage, reportState, startHeartbeat, type Lifecycle } from '@isoastra/fleet-runtime';

export function ownChildLifecycle(role: 'http' | 'worker', lifecycle: Lifecycle): void {
  const stopHeartbeat = startHeartbeat(role);
  lifecycle.register('heartbeat', () => stopHeartbeat());
  lifecycle.onState((state) => reportState(role, state));
  const drain = (reason: string, deadline = Date.now() + 45_000) => {
    void lifecycle.drain(reason, deadline).then(() => process.exit(0), (error) => {
      reportState(role, 'failed', error instanceof Error ? error.message : String(error));
      process.exit(1);
    });
  };
  process.on('message', (message: unknown) => {
    if (!isParentMessage(message)) return;
    if (message.type === 'drain') drain(message.reason, message.deadline);
  });
  if (!process.send) {
    process.once('SIGTERM', () => drain('SIGTERM'));
    process.once('SIGINT', () => drain('SIGINT'));
  }
}
