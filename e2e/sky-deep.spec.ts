import { expect, test, type Page } from '@playwright/test';

/** S2: the streamed Gaia catalogue — that it arrives in the right order, that
 * it arrives nearest the view first, and that it costs what it is budgeted.
 *
 * The queue's own order is only on `window.__sky` under `?skydebug=1`; the
 * page a visitor gets exposes nothing.
 */

interface Readout {
  points: number;
  residentTiles: number;
  gpuBytes: number;
  stream: { order: number[]; fetched: number[]; bytes: number; coveredDeg: number } | null;
}

const readout = (page: Page): Promise<Readout | null> =>
  page.evaluate(() => (globalThis as { __sky?: () => Readout }).__sky?.() ?? null);

const resources = (page: Page): Promise<{ url: string; startMs: number; bytes: number }[]> =>
  page.evaluate(() =>
    (performance.getEntriesByType('resource') as PerformanceResourceTiming[]).map((entry) => ({
      url: entry.name.replace(location.origin, ''),
      startMs: entry.startTime,
      bytes: entry.transferSize,
    })),
  );

async function streamed(page: Page, tiles: number): Promise<Readout> {
  await page.goto('/?skydebug=1');
  await expect
    .poll(async () => (await readout(page))?.stream?.fetched.length ?? 0, { timeout: 60_000 })
    .toBeGreaterThanOrEqual(tiles);
  return (await readout(page))!;
}

test('the deep sky lands after the catalogue, nearest the view first', async ({ page }) => {
  const state = await streamed(page, 2);
  const seen = await resources(page);
  const at = (match: RegExp): number =>
    seen.find((entry) => match.test(entry.url))?.startMs ?? Infinity;

  // The bright catalogue is the picture; g9 is depth added to it, and the
  // tiles are depth added to that. Nothing overtakes what it is drawn over.
  expect(at(/\/stars\/bright\.bin$/)).toBeLessThan(at(/\/stars\/lod\/g9\.bin$/));
  expect(at(/\/stars\/lod\/g9\.bin$/)).toBeLessThan(at(/\/stars\/lod\/\d+\.bin$/));
  expect(at(/\/stars\/lod\/manifest\.json$/)).toBeLessThan(at(/\/stars\/lod\/g9\.bin$/));

  // The first tile fetched is one of the handful the view is under. The queue
  // is re-scored twice a second and the sky turns, so the assertion is
  // membership of the head of the queue rather than a single id.
  const first = state.stream!.fetched[0]!;
  expect(state.stream!.order.slice(0, 6)).toContain(first);

  // Every tile file that was fetched is one the queue asked for.
  const fetchedFiles = seen
    .filter((entry) => /\/stars\/lod\/\d+\.bin$/.test(entry.url))
    .map((entry) => Number(entry.url.split('/').at(-1)!.replace('.bin', '')));
  expect(new Set(fetchedFiles)).toEqual(new Set(state.stream!.fetched));
  expect(state.residentTiles).toBeGreaterThan(0);
  expect(state.points).toBeGreaterThan(150_000);
});

/** What the first minute is allowed to cost, and where the number comes from.
 *
 * The page's own assets are 1.60 MB — document, JS, the bright catalogue, the
 * Milky Way map, the constellation figures, the city list and the globe's
 * three textures — and `g9.bin` is 2.65 MB more, so 4.25 MB is spent before a
 * single tile is asked for. The streamer's first minute is its 640 KB burst
 * plus 60 s at 5 KB/s, which is 0.94 MB: 5.19 MB in total, and 5.4 leaves room
 * for the page itself to grow a little without this becoming a test of the JS
 * bundle. The budget that actually shapes the streamer is the plan's 6 MB in
 * five minutes, which the rate below it is set by.
 *
 * S4's re-bake is what moved this: the band was a 1024x512 map at 19.5 KB and
 * is now 4096x2048 at 315 KB, of which this viewport fetches the 2048x1024
 * downscale at 145 KB. 126 KB once, for a band that no longer reads as blobs
 * when the galactic centre fills the frame. */
const FIRST_MINUTE_BYTES = 5.4e6;
const FIRST_MINUTE_TILE_BYTES = 1.0e6;

test('the first minute of sky fits the budget', async ({ page }) => {
  test.setTimeout(120_000);
  await page.goto('/?skydebug=1');
  await page.waitForTimeout(60_000);

  const seen = (await resources(page)).filter((entry) => entry.startMs < 60_000);
  const total = seen.reduce((sum, entry) => sum + entry.bytes, 0);
  const tiles = seen.filter((entry) => /\/stars\/lod\/\d+\.bin$/.test(entry.url));
  const tileBytes = tiles.reduce((sum, entry) => sum + entry.bytes, 0);

  expect(tiles.length).toBeGreaterThan(4);
  expect(tileBytes).toBeLessThan(FIRST_MINUTE_TILE_BYTES);
  expect(total).toBeLessThan(FIRST_MINUTE_BYTES);
  // And the 51 MB on disk is nowhere near being spent: a ninth of it.
  expect(total).toBeLessThan(0.11 * 51.9e6);
});

test('the light theme keeps the sky it had', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('rn_theme', 'light'));
  const state = await streamed(page, 1);
  // Tiles still stream — the theme can be flipped back — but under a sky that
  // is two thirds of white the deep catalogue draws nothing: the twilight
  // magnitude cut is well above every star in it.
  expect(state.residentTiles).toBeGreaterThan(0);
  expect(state.points).toBeLessThan(12_191);
});
