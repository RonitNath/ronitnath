import { describe, expect, it } from 'vitest';

import { SIM_EPOCH_MS } from '../clock';
import { Observer, TRANSITION_MS } from '../observer';
import { observerAt } from '../track';

const NOW = 1_800_000_000_000;
const SYDNEY: [number, number] = [-33.8688, 151.2093];

const near = (a: readonly number[], b: readonly number[], tolerance: number): boolean =>
  Math.abs(a[0]! - b[0]!) < tolerance && Math.abs(a[1]! - b[1]!) < tolerance;

describe('the observer', () => {
  it('follows the shared orbit with no choice made', () => {
    const observer = new Observer();
    expect(observer.isManual()).toBe(false);
    expect(observer.resolve(SIM_EPOCH_MS, NOW)).toEqual(observerAt(SIM_EPOCH_MS));
  });

  it('reaches a chosen point at the end of its travel and not before', () => {
    const observer = new Observer();
    observer.set(SYDNEY[0], SYDNEY[1], SIM_EPOCH_MS, NOW, TRANSITION_MS);
    const start = observer.resolve(SIM_EPOCH_MS, NOW);
    expect(near(start, SYDNEY, 1)).toBe(false);

    const midway = observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS / 2);
    expect(near(midway, start, 1e-6)).toBe(false);
    expect(near(midway, SYDNEY, 1e-6)).toBe(false);

    expect(near(observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS), SYDNEY, 1e-6)).toBe(true);
    expect(near(observer.manual()!, SYDNEY, 1e-9)).toBe(true);
  });

  it('makes the same call an immediate assignment under reduced motion', () => {
    const observer = new Observer();
    observer.set(SYDNEY[0], SYDNEY[1], SIM_EPOCH_MS, NOW, 0);
    expect(near(observer.resolve(SIM_EPOCH_MS, NOW), SYDNEY, 1e-9)).toBe(true);
  });

  it('travels back to the orbit and then releases the view', () => {
    const observer = new Observer();
    observer.set(SYDNEY[0], SYDNEY[1], SIM_EPOCH_MS, NOW, 0);
    observer.resolve(SIM_EPOCH_MS, NOW);
    observer.resume(SIM_EPOCH_MS, NOW, TRANSITION_MS);

    expect(observer.isManual()).toBe(true);
    observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS / 2);
    expect(observer.isManual()).toBe(true);

    const landed = observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS);
    expect(observer.isManual()).toBe(false);
    expect(near(landed, observerAt(SIM_EPOCH_MS), 1e-9)).toBe(true);
  });

  it('changes nothing when resuming an orbit that was never left', () => {
    const observer = new Observer();
    observer.resume(SIM_EPOCH_MS, NOW, TRANSITION_MS);
    expect(observer.manual()).toBeNull();
    expect(observer.resolve(SIM_EPOCH_MS, NOW)).toEqual(observerAt(SIM_EPOCH_MS));
  });

  it('normalises a longitude past the antimeridian rather than storing it raw', () => {
    const observer = new Observer();
    observer.set(0, 200, SIM_EPOCH_MS, NOW, 0);
    expect(near(observer.manual()!, [0, -160], 1e-9)).toBe(true);
  });

  it('ignores a coordinate that is not a number rather than poisoning the view', () => {
    const observer = new Observer();
    observer.set(Number.NaN, 0, SIM_EPOCH_MS, NOW, 0);
    expect(observer.isManual()).toBe(false);
  });

  it('starts a second move from where the first one had got to', () => {
    const observer = new Observer();
    observer.set(SYDNEY[0], SYDNEY[1], SIM_EPOCH_MS, NOW, TRANSITION_MS);
    const midway = observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS / 2);
    observer.set(0, 0, SIM_EPOCH_MS, NOW + TRANSITION_MS / 2, TRANSITION_MS);
    const restarted = observer.resolve(SIM_EPOCH_MS, NOW + TRANSITION_MS / 2);
    expect(near(restarted, midway, 1e-6)).toBe(true);
  });
});
