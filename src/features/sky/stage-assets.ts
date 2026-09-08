/** The order the sky's assets arrive in, and why that order.
 *
 * The catalogue first and alone: the sky is the picture, and on a slow link
 * every other byte in flight is a byte the stars are waiting behind. Then the
 * name list, the city list and the Milky Way's map, which are small. Then the
 * globe's three textures — the largest, and the last, because the globe draws
 * as a blue sphere in the meantime and the corner of the frame is not what a
 * visitor is waiting for. Then the streamed deep sky, competing with nothing.
 *
 * Every step is best-effort. A failed fetch leaves whatever has landed as the
 * picture, which a moment ago it was.
 *
 * It lives beside `stage.ts` rather than in it because it is a *sequence*, not
 * a state machine: the stage owns the clock, the observer and the frame, and
 * this owns the order.
 */

import { loadCities, loadLines, loadNamed, loadStars } from './assets';
import { namedVectors, type NamedCatalog, type StarCatalog } from './catalog';
import type { CityCatalog } from './cities';
import type { LinePairs } from './lines';
import type { Vec3 } from './sidereal';

/** What the loader hands back, in the order it gets it. Each call is made only
 * while the stage is still live; `live()` is asked before every one. */
export interface AssetSink {
  live(): boolean;
  stars(catalog: StarCatalog): void;
  named(named: NamedCatalog, vectors: Vec3[]): void;
  cities(cities: CityCatalog): void;
  lines(pairs: LinePairs): void;
  band(): Promise<void>;
  globe(): Promise<void>;
  deep(): Promise<void>;
}

export async function loadSkyAssets(sink: AssetSink): Promise<void> {
  try {
    const stars = await loadStars();
    if (!sink.live()) return;
    sink.stars(stars);
    const named = await loadNamed();
    if (!sink.live()) return;
    sink.named(named, namedVectors(stars, named));
  } catch {
    // No catalog is no stars; the CSS starfield is still the picture.
  }
  await Promise.allSettled([
    loadCities().then((cities) => {
      if (sink.live()) sink.cities(cities);
    }),
    // 2.7 KB of segment indices, fetched whether or not the toggle is on: a
    // control that has to wait for a download after it is pressed is a control
    // that does not work.
    loadLines().then((pairs) => {
      if (sink.live()) sink.lines(pairs);
    }),
    sink.band(),
  ]);
  if (!sink.live()) return;
  await sink.globe();
  if (!sink.live()) return;
  await sink.deep();
}
