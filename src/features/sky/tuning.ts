/** The numbers the sky is drawn with, and why each one is what it is.
 *
 * Before S1 these were the pre-rebuild starscape's magnitude-to-pixel
 * response: a size curve, an alpha curve and four exponents fitted by eye. The
 * GL pass replaced that with photometry — a star's linear flux, accumulated
 * into a float buffer and tone mapped once — so the curves are gone and what
 * is left is the four numbers that response actually has: a reference
 * magnitude, an exposure, a core width and a glare weight, per theme.
 */
export const TUNING = {
  /** Atmospheric extinction, magnitudes per airmass. 0.28 is a humid
   * low-altitude site rather than a mountaintop's 0.20 — the honest end of the
   * real range, chosen because it makes the band visibly dim toward the frame
   * edge the way it does from a city. */
  extinctionK: 0.28,
  bandGain: 0.78,
  bandShape: 2.2,
} as const;

/** The response of one theme's sky to a star's magnitude.
 *
 * `b = 10^(-0.4 (m - mRef))` is the star's flux relative to a star at `mRef`,
 * which is the whole of the brightness model; everything else here decides how
 * that flux lands on a screen with a hundred-to-one contrast ratio and no
 * headroom above white.
 */
export interface StarResponse {
  /** The magnitude that comes out at unit flux: a star of exactly this
   * magnitude, at the zenith, arrives at the tone curve as 1. It is one
   * multiplier on the whole sky — every star moves by the same factor — so it
   * sets *where* the catalogue sits on the curve, and the exposure below sets
   * how hard the curve then bends. */
  magRef: number;
  /** The tone curve's rate: displayed = 1 - exp(-x · exposure). Below about
   * 0.3 of the curve this is linear, so faint stars keep their relative
   * brightness; above it the curve compresses, so a star ten times brighter
   * than white does not clip to a ten-pixel white square, it saturates its
   * core and spends the rest on its wings. */
  exposure: number;
  /** Half-width of the core Gaussian, in device pixels. 0.6 px puts most of a
   * faint star's energy inside one pixel while still spreading it over the
   * two or three it is between, which is what makes a sub-pixel star fade and
   * slide as it moves rather than blink from pixel to pixel. */
  coreSigmaPx: number;
  /** Weight of the glare wing, `b^glareExp · k / (d + eps)^2` — the eye's own
   * scatter, which is why a bright star looks *large* rather than merely
   * bright. */
  glareK: number;
  /** How fast the glare grows with flux. Not 1: a wing carrying the star's
   * whole flux would put Sirius, four hundred times the reference, across a
   * quarter of the frame. Not 0.5 either, which was the first cut and made
   * every star from magnitude 6 up wear the same halo. 0.75 puts the drawn
   * radius on `b^0.375`, so the thousand-to-one range of the catalogue lands
   * as the ten-to-one range of widths the sky actually shows. */
  glareExp: number;
  /** How far colour survives toward white. The light theme washes the colours
   * out because a hue on a bright ground reads as a printing error. */
  whiten: number;
}

/** Night: black ground, the full catalogue, colours at their catalogue value. */
export const NIGHT: StarResponse = {
  magRef: 5.1,
  exposure: 0.85,
  coreSigmaPx: 0.6,
  glareK: 0.3,
  glareExp: 0.75,
  whiten: 0,
};

/** Twilight: the sky under the stars is already two thirds of white, so
 * everything a star can do is *add*, and only what it adds over that ground is
 * visible. A lower exposure keeps the faint half of the catalogue from piling
 * grey haze onto a lit sky; a wider glare is what is left to say "bright" once
 * a core has nowhere above white to go. */
export const TWILIGHT: StarResponse = {
  magRef: 5.2,
  exposure: 0.4,
  coreSigmaPx: 0.62,
  glareK: 0.5,
  glareExp: 0.75,
  whiten: 0.3,
};

/** The widest a star may be drawn, in CSS pixels. Celestia bounds its glare at
 * roughly a degree of apparent field; at this focal length and viewport that
 * is about 24 px, and it is Sirius that reaches it. Without the bound the
 * inverse-square wing has no end and the brightest star grows until it is a
 * lens flare. */
export const MAX_STAR_PX = 24;

/** The faintest star worth drawing onto the light theme's dusk gradient: below
 * this the background is bright enough that the star has nothing to show
 * against and only adds grey haze. */
export const TWILIGHT_MAG_LIMIT = 4.6;

/** The linear flux of a star of magnitude `m` against a theme's reference,
 * after the atmosphere between it and the viewer has taken its share.
 *
 * `zenithCos` is the cosine of the zenith angle — `applyView`'s third
 * component, the same number the band shader calls `ray.z`. Airmass is the
 * plane-parallel secant, floored at 20 airmasses so a star exactly on the
 * horizon is dim rather than infinite. */
export function starFlux(mag: number, magRef: number, zenithCos: number): number {
  const airmass = 1 / Math.max(zenithCos, 0.05);
  const extinction = TUNING.extinctionK * (airmass - 1);
  return Math.pow(10, -0.4 * (mag - magRef + extinction));
}

/** Softens the glare wing's singularity at the star's centre, in CSS pixels.
 * `k/d²` is unbounded at d = 0; the core is what draws the middle of a star,
 * and the wing only has to be finite there. */
export const GLARE_EPS_PX = 0.9;

/** The level the glare wing is cut off at, and so what decides how wide each
 * star is drawn.
 *
 * An inverse-square wing has no end of its own; something has to say where it
 * stops. This is that number, and it is set by the other end of the scale:
 * with the responses above it puts the brightest star in the sky within a
 * fraction of a pixel of {@link MAX_STAR_PX}, which is where the cap then
 * holds it. Every fainter star is narrower in the same proportion,
 * which is why there are no size tiers — one continuous law, one bound.
 *
 * The wing is still faintly visible where it is cut, so the fragment fades the
 * profile out before the point's own quad edge rather than stopping at it: a
 * hard stop here would be a square rim around every bright star. */
export const GLARE_FLOOR = 0.15;

/** How wide to draw a star, in CSS pixels.
 *
 * Point size is not a look here, it is a bound on the profile: the fragment
 * shader evaluates a Gaussian core plus a `sqrt(b)·k/(d+eps)²` wing, and this
 * is the radius at which that wing drops below what the screen can show,
 * doubled. Which means the *only* reason a bright star is large is that its
 * glare really does reach further, and there are no size tiers to band.
 *
 * The cap is the reason Sirius is a star and not a lens flare; the floor keeps
 * the core Gaussian from being clipped by its own quad on the faintest stars.
 */
export function starDiameterPx(flux: number, response: StarResponse): number {
  const wingRadius = Math.sqrt(
    (Math.pow(flux, response.glareExp) * response.glareK) / GLARE_FLOOR,
  );
  const coreRadius = 3 * response.coreSigmaPx;
  return Math.min(MAX_STAR_PX, 2 * Math.max(2, coreRadius, wingRadius));
}

/** The tone curve, per channel: `1 - exp(-x · exposure)`.
 *
 * It has no ceiling to clip against, which is the point — a star a thousand
 * times over white saturates smoothly to white in its core while its wings,
 * still on the linear part of the curve, keep the colour the catalogue gave
 * it. That is the whole reason the pass accumulates in a float buffer first. */
export function toneMap(value: number, exposure: number): number {
  return 1 - Math.exp(-Math.max(0, value) * exposure);
}
