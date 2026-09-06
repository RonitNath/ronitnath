/** The simulation clock.
 *
 * The sky runs 60x wall time so its drift is perceptible, but the simulated
 * *date* is pulled back every full rotation instead of running away: the
 * accumulated lead is taken modulo one sidereal day, and GMST has exactly that
 * period, so the sky either side of the seam is identical to the bit. Nothing
 * on screen moves; only the date resets. This is tonight's sky spun fast, not
 * an invented decade's.
 */

import { SIDEREAL_DAY_MS } from './sidereal';

/** Fixed simulation epoch, 2026-01-01T00:00:00Z. Sky state is a pure function
 * of (server time, this constant, {@link SPEED}) and nothing else. */
export const SIM_EPOCH_MS = 1_767_225_600_000;

export const SPEED = 60;

function remEuclid(value: number, modulus: number): number {
  return ((value % modulus) + modulus) % modulus;
}

/** Accelerated simulation time for a wall-clock unix time. */
export function simTimeMs(nowMs: number): number {
  return nowMs + remEuclid((nowMs - SIM_EPOCH_MS) * (SPEED - 1), SIDEREAL_DAY_MS);
}

/** Reconstruct server wall time from a client timestamp plus the offset
 * sampled when the island mounted. Sampling at mount rather than after the
 * assets land means fetch and decode latency cannot make the sky stale. */
export function syncedSimTimeMs(
  serverEpochMs: number,
  clientMountMs: number,
  clientNowMs: number,
): number {
  return simTimeMs(clientNowMs + serverEpochMs - clientMountMs);
}
