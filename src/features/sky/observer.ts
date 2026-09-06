/** Where the visitor is standing, to the nearest tenth of a degree.
 *
 * The sky is drawn for a real place, but not at the cost of a permission
 * prompt: a landing page that asks for your location before it will draw a
 * background has got the trade backwards. So this reads the browser's position
 * only when the visitor has *already* granted it to this origin, and otherwise
 * stands in San Francisco. `getCurrentPosition` is never called in any state
 * but `granted`, which is the whole point of the Permissions query.
 *
 * A tenth of a degree is ~11 km, which is four orders of magnitude finer than
 * the sky can show — it is rounded because nothing more precise is wanted,
 * kept, or sent anywhere.
 */

export interface Observer {
  latDeg: number;
  lonDeg: number;
}

/** Where the sky is drawn from when the browser will not say. */
export const FALLBACK_OBSERVER: Observer = { latDeg: 37.7749, lonDeg: -122.4194 };

const COARSE_TIMEOUT_MS = 4_000;

export async function coarseObserver(): Promise<Observer> {
  if (typeof navigator === 'undefined' || !navigator.geolocation || !navigator.permissions) {
    return FALLBACK_OBSERVER;
  }

  let granted = false;
  try {
    granted = (await navigator.permissions.query({ name: 'geolocation' })).state === 'granted';
  } catch {
    // Safari has refused this query for some name values in the past; a
    // browser that will not answer is treated as one that has not granted.
    return FALLBACK_OBSERVER;
  }
  if (!granted) return FALLBACK_OBSERVER;

  return new Promise<Observer>((resolve) => {
    navigator.geolocation.getCurrentPosition(
      ({ coords }) =>
        resolve({
          latDeg: Math.round(coords.latitude * 10) / 10,
          lonDeg: Math.round(coords.longitude * 10) / 10,
        }),
      () => resolve(FALLBACK_OBSERVER),
      { enableHighAccuracy: false, timeout: COARSE_TIMEOUT_MS, maximumAge: 3_600_000 },
    );
  });
}
