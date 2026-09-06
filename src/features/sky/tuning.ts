/** The shipped sky-look constants.
 *
 * These are the magnitude-to-pixel response the pre-rebuild starscape shipped
 * (its `tuning.rs`), carried over unchanged: the sky is a designed asset and
 * re-picking its numbers would be redrawing it.
 */
export const TUNING = {
  sizeBase: 0.8,
  sizeScale: 3.2,
  sizeExp: 0.25,
  sizeMax: 9,
  alphaBase: 0.38,
  alphaScale: 0.72,
  alphaExp: 0.42,
  alphaMax: 1,
  /** Halo falloff: the exponential rate the point's brightness drops at. */
  glow: 2.5,
  /** How much the halo widens with brightness. The shipped value is 0, which
   * is the fixed falloff `glow` alone; it is carried rather than folded away
   * so the two stay one knob (`tuning.rs`'s `halo`). */
  halo: 0,
  /** Atmospheric extinction, magnitudes per airmass. 0.28 is a humid
   * low-altitude site rather than a mountaintop's 0.20 — the honest end of the
   * real range, chosen because it makes the band visibly dim toward the frame
   * edge the way it does from a city. */
  extinctionK: 0.28,
  bandGain: 0.78,
  bandShape: 2.2,
} as const;

/** The faintest star worth drawing onto the light theme's dusk gradient: below
 * this the background is bright enough that the star has nothing to show
 * against and only adds grey haze. */
export const TWILIGHT_MAG_LIMIT = 4.6;
