/** Whose sky is on screen: the canonical orbit, or a place the viewer picked.
 *
 * The default observer is the shared, server-synchronised track — every
 * browser on the site is looking out from the same point at the same instant.
 * Dragging the globe overrides that, and the override is deliberately
 * tab-local: it is a viewer's own detour, not a change to the site.
 *
 * A move is interpolated along the great circle rather than snapped, and
 * "resume orbit" is the same interpolation with the manual point cleared when
 * it lands. Under reduced motion the duration is zero, which makes both an
 * immediate assignment without a second code path.
 */

import { normalizeLonDeg } from './sidereal';
import { greatCircleLerp, observerAt } from './track';

/** How long a hand-driven move takes, when motion is not reduced. */
export const TRANSITION_MS = 800;

export type Point = [number, number];

interface Transition {
  from: Point;
  startedAtMs: number;
  durationMs: number;
  /** Clear the manual observer when this lands: the move is a return to orbit. */
  clearAtEnd: boolean;
}

export class Observer {
  private manualPoint: Point | null = null;
  private transition: Transition | null = null;

  /** Where the sky is being viewed from, resolving any move in flight. A
   * landed transition is retired here: the alternative is a timer whose only
   * job is to notice that a lerp finished. */
  resolve(simMs: number, nowMs: number): Point {
    const target = this.manualPoint ?? observerAt(simMs);
    const transition = this.transition;
    if (!transition) return target;

    const progress =
      transition.durationMs <= 0
        ? 1
        : Math.min(1, Math.max(0, (nowMs - transition.startedAtMs) / transition.durationMs));
    if (progress >= 1) {
      this.transition = null;
      if (transition.clearAtEnd) {
        this.manualPoint = null;
        return observerAt(simMs);
      }
      return target;
    }
    return greatCircleLerp(transition.from, target, progress);
  }

  /** The viewer's chosen point, if they have one. `null` means the shared orbit. */
  manual(): Point | null {
    return this.manualPoint;
  }

  isManual(): boolean {
    return this.manualPoint !== null;
  }

  /** Move to a chosen point, travelling there from wherever the view is now. */
  set(lat: number, lon: number, simMs: number, nowMs: number, durationMs: number): void {
    if (!Number.isFinite(lat) || !Number.isFinite(lon)) return;
    const from = this.resolve(simMs, nowMs);
    this.manualPoint = [Math.min(90, Math.max(-90, lat)), normalizeLonDeg(lon)];
    this.transition = { from, startedAtMs: nowMs, durationMs, clearAtEnd: false };
  }

  /** Travel back to the shared orbit and hand control of the view back to it. */
  resume(simMs: number, nowMs: number, durationMs: number): void {
    if (this.manualPoint === null) return;
    const from = this.resolve(simMs, nowMs);
    this.transition = { from, startedAtMs: nowMs, durationMs, clearAtEnd: true };
  }
}
