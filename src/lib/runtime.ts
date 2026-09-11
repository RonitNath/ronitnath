import type { LifecycleState } from '@isoastra/fleet-runtime';

interface RuntimeState {
  draining: boolean;
  worker: LifecycleState;
  streams: Set<() => void>;
}

const globalRuntime = globalThis as unknown as { rnRuntimeState?: RuntimeState };
function state(): RuntimeState {
  return (globalRuntime.rnRuntimeState ??= { draining: false, worker: 'starting', streams: new Set() });
}

export function runtimeStatus(): { draining: boolean; worker: LifecycleState } {
  return { draining: state().draining, worker: state().worker };
}
export function setRuntimeDraining(draining: boolean): void { state().draining = draining; }
export function setWorkerState(worker: LifecycleState): void { state().worker = worker; }
export function registerStream(close: () => void): () => void {
  const current = state();
  if (current.draining) { close(); return () => undefined; }
  current.streams.add(close);
  return () => { current.streams.delete(close); };
}
export function drainStreams(): void {
  const current = state();
  current.draining = true;
  for (const close of [...current.streams]) close();
  current.streams.clear();
}
