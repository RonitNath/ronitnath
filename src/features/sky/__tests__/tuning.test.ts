import { describe, expect, it } from 'vitest';

import {
  MAX_STAR_PX,
  NIGHT,
  starDiameterPx,
  starFlux,
  toneMap,
  TUNING,
  TWILIGHT,
  TWILIGHT_MAG_LIMIT,
  GLARE_FLOOR,
} from '../tuning';

/** Sirius, Vega and a representative faint star, as the shipped catalog has
 * them: the three points every claim about the response is checked at. */
const SIRIUS = -1.0876;
const VEGA = 0.0868;
const FAINT = 5;

describe('a star’s linear brightness', () => {
  it('is the magnitude scale: five magnitudes is a factor of a hundred', () => {
    const zenith = 1;
    expect(starFlux(0, 0, zenith)).toBeCloseTo(1, 12);
    expect(starFlux(5, 0, zenith) * 100).toBeCloseTo(1, 12);
    expect(starFlux(-5, 0, zenith)).toBeCloseTo(100, 10);
    // Sirius against the night reference really is that far above a mag-5 star.
    expect(starFlux(SIRIUS, NIGHT.magRef, zenith) / starFlux(FAINT, NIGHT.magRef, zenith))
      .toBeCloseTo(Math.pow(10, -0.4 * (SIRIUS - FAINT)), 6);
  });

  it('the reference magnitude moves the whole sky by one factor', () => {
    // Every star by the same amount, whichever star it is: `magRef` decides
    // where the catalogue sits on the tone curve and nothing else.
    const bright = starFlux(VEGA, TWILIGHT.magRef, 1) / starFlux(VEGA, NIGHT.magRef, 1);
    const faint = starFlux(FAINT, TWILIGHT.magRef, 1) / starFlux(FAINT, NIGHT.magRef, 1);
    expect(bright).toBeCloseTo(faint, 12);
    expect(starFlux(0, 4, 1)).toBeLessThan(starFlux(0, 5, 1));
  });

  it('takes the atmosphere’s share, growing with airmass toward the horizon', () => {
    // At the zenith one airmass is the reference, so nothing is taken.
    expect(starFlux(0, 0, 1)).toBeCloseTo(1, 12);
    const low = starFlux(0, 0, 0.5); // two airmasses
    expect(low).toBeCloseTo(Math.pow(10, -0.4 * TUNING.extinctionK), 12);
    expect(low).toBeLessThan(starFlux(0, 0, 1));
    expect(starFlux(0, 0, 0.2)).toBeLessThan(low);
    // Floored rather than divergent: a star exactly on the horizon is dim.
    expect(starFlux(0, 0, 0)).toBe(starFlux(0, 0, 0.05));
    expect(starFlux(0, 0, 0)).toBeGreaterThan(0);
  });
});

describe('the tone curve', () => {
  it('is linear where the sky is faint, so faint stars keep their ratios', () => {
    const a = toneMap(0.01, NIGHT.exposure);
    const b = toneMap(0.02, NIGHT.exposure);
    expect(b / a).toBeGreaterThan(1.98);
    expect(b / a).toBeLessThanOrEqual(2);
  });

  it('saturates toward white instead of clipping at it', () => {
    expect(toneMap(0, 1)).toBe(0);
    for (const value of [1, 10, 20]) {
      expect(toneMap(value, NIGHT.exposure)).toBeLessThan(1);
    }
    // It reaches 1 only where a double has no room left below it — some forty
    // times the brightness that already rounds to white on screen.
    expect(toneMap(10_000, NIGHT.exposure)).toBe(1);
    expect(toneMap(10, NIGHT.exposure)).toBeGreaterThan(0.999);
    // Monotonic across the range a star can land in, which is what stops two
    // stars a magnitude apart from coming out the same brightness.
    let previous = -1;
    for (let value = 0; value < 30; value += 0.25) {
      const level = toneMap(value, NIGHT.exposure);
      expect(level).toBeGreaterThan(previous);
      previous = level;
    }
  });

  it('sends a bright star’s core to white while its wings keep their colour', () => {
    const core = starFlux(SIRIUS, NIGHT.magRef, 1) / (2 * Math.PI * NIGHT.coreSigmaPx ** 2);
    const blue = [0.64, 0.75, 1] as const;
    const centre = blue.map((channel) => toneMap(core * channel, NIGHT.exposure));
    // Every channel is over 254/255: the core reads as white, not as blue.
    for (const channel of centre) expect(Math.round(channel * 255)).toBe(255);

    // Out in the wing the same colour is still a colour.
    const wing = core * 0.002;
    const [red, , wingBlue] = blue.map((channel) => toneMap(wing * channel, NIGHT.exposure));
    expect(wingBlue).toBeGreaterThan(red! * 1.3);
  });

  it('twilight’s lower exposure keeps the faint half off a lit sky', () => {
    // The faintest star twilight still draws, against the same star at night:
    // on a sky already two thirds of white, a level like that is haze rather
    // than a star, and the exposure is what holds it down.
    const twilight = toneMap(
      starFlux(TWILIGHT_MAG_LIMIT, TWILIGHT.magRef, 1) /
        (2 * Math.PI * TWILIGHT.coreSigmaPx ** 2),
      TWILIGHT.exposure,
    );
    const night = toneMap(
      starFlux(TWILIGHT_MAG_LIMIT, NIGHT.magRef, 1) / (2 * Math.PI * NIGHT.coreSigmaPx ** 2),
      NIGHT.exposure,
    );
    expect(twilight).toBeLessThan(night);
  });
});

describe('how wide a star is drawn', () => {
  const diameter = (mag: number, response = NIGHT) =>
    starDiameterPx(starFlux(mag, response.magRef, 1), response);

  it('grows with brightness and never past the cap', () => {
    // Sirius is the star the cap is set for: the glare floor puts it within a
    // fraction of a pixel of it, and the cap holds it there.
    expect(diameter(SIRIUS)).toBeCloseTo(MAX_STAR_PX, 1);
    expect(diameter(SIRIUS)).toBeLessThanOrEqual(MAX_STAR_PX);
    expect(diameter(VEGA)).toBeLessThan(MAX_STAR_PX);
    expect(diameter(VEGA)).toBeGreaterThan(diameter(FAINT));
    // Sirius against a magnitude-5 star: visibly wider, which is the gate.
    expect(diameter(SIRIUS)).toBeGreaterThan(diameter(FAINT) * 3);
  });

  it('is the radius the glare wing reaches, not a tier', () => {
    // Wherever the glare is what decides the width, every step of magnitude
    // changes it: a continuous law, nothing quantised.
    const widths = new Set<number>();
    for (let mag = 0; mag <= 4; mag += 0.25) widths.add(diameter(mag));
    expect(widths.size).toBe(17);

    // And that width really is where the wing falls below the cutoff.
    const flux = starFlux(3, NIGHT.magRef, 1);
    const radius = starDiameterPx(flux, NIGHT) / 2;
    const wing = (Math.pow(flux, NIGHT.glareExp) * NIGHT.glareK) / (radius * radius);
    expect(wing).toBeCloseTo(GLARE_FLOOR, 6);
  });

  it('never draws a star too small to hold its own core', () => {
    // Below about magnitude 4.5 the glare wing is narrower than the core, and
    // the width stops changing: those stars are a point of light and nothing
    // else, and drawing them into a smaller quad would clip the Gaussian.
    const floor = 6 * NIGHT.coreSigmaPx;
    for (const mag of [FAINT, 6, 8, 12, 20]) {
      expect(diameter(mag)).toBeGreaterThanOrEqual(floor);
      expect(diameter(mag)).toBe(diameter(20));
    }
  });

  it('twilight draws a bright star wider, because width is all it has left', () => {
    // On a lit sky a core has nowhere above white to go, so the only thing
    // that can still say "bright" is how far the halo reaches.
    expect(diameter(VEGA, TWILIGHT)).toBeGreaterThan(diameter(VEGA, NIGHT));
    // And the faint half is not drawn at all: it has nothing to show against.
    expect(TWILIGHT_MAG_LIMIT).toBeLessThan(6.5);
  });
});
