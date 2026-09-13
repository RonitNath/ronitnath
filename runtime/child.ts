import { isParentMessage, reportQuiesced, reportState, startHeartbeat, type Lifecycle } from '@isoastra/fleet-runtime';

export function ownChildLifecycle(
  role: 'http' | 'worker',
  lifecycle: Lifecycle,
  hooks: { quiesce?: () => void | Promise<void> } = {},
): void {
  const stopHeartbeat = startHeartbeat(role);
  lifecycle.register('heartbeat', () => stopHeartbeat());
  lifecycle.onState((state) => reportState(role, state));
  let quiescing: Promise<void> | null = null;
  const quiesce = () => {
    quiescing ??= Promise.resolve().then(hooks.quiesce).then(() => {
      reportQuiesced(role);
    });
    return quiescing;
  };
  const drain = (reason: string, deadline = Date.now() + 45_000) => {
    void quiesce().then(() => lifecycle.drain(reason, deadline)).then(() => process.exit(0), (error) => {
      reportState(role, 'failed', error instanceof Error ? error.message : String(error));
      process.exit(1);
    });
  };
  process.on('message', (message: unknown) => {
    if (!isParentMessage(message)) return;
    if (message.type === 'quiesce') void quiesce().catch((error) => {
      reportState(role, 'failed', error instanceof Error ? error.message : String(error));
      process.exit(1);
    });
    if (message.type === 'drain') drain(message.reason, message.deadline);
  });
  if (process.send) process.on('disconnect', () => drain('parent IPC disconnected'));
  else {
    process.on('SIGTERM', () => drain('SIGTERM'));
    process.on('SIGINT', () => drain('SIGINT'));
  }
}
