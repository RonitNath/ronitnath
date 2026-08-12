import { expect, test } from "@playwright/test";

const site = process.env.RN_SITE_URL || "http://127.0.0.1:3000";

test("bright stars stream first and reveal on the first GPU batch", async ({ page }) => {
  await page.goto(`${site}/?debug=telemetry`);
  await page.waitForFunction(
    () => window.__rnTelemetry?.stars?.complete === true,
  );

  const result = await page.evaluate(async () => {
    const response = await fetch("/stars/bright.bin");
    const bytes = await response.arrayBuffer();
    const view = new DataView(bytes);
    const count = view.getUint32(4, true);
    const magnitudes = Array.from(
      { length: count },
      (_, index) => view.getFloat32(8 + index * 20 + 12, true),
    );
    const telemetry = window.__rnTelemetry;
    return {
      count,
      magnitudeSorted: magnitudes.every(
        (magnitude, index) => index === 0 || magnitudes[index - 1] <= magnitude,
      ),
      uploaded: telemetry.stars.uploaded,
      batchToRevealMs:
        telemetry.stars.revealedAtMs - telemetry.stars.firstBatchAtMs,
      active: document.documentElement.classList.contains("starscape-active"),
      canvasOpacity: Number.parseFloat(
        getComputedStyle(document.querySelector("canvas.starscape")!).opacity,
      ),
      transitionDuration: getComputedStyle(
        document.querySelector("canvas.starscape")!,
      ).transitionDuration,
    };
  });

  expect(result).toMatchObject({
    count: 12_191,
    magnitudeSorted: true,
    uploaded: 12_191,
    active: true,
  });
  expect(result.batchToRevealMs).toBeLessThan(100);
  expect(result.canvasOpacity).toBeLessThan(1);
  expect(result.transitionDuration).toContain("1.2s");

  await page.waitForFunction(
    () => window.__rnTelemetry?.stars?.galaxyRevealedAtMs > 0,
  );
  await expect
    .poll(() =>
      page.evaluate(() =>
        Number.parseFloat(
          getComputedStyle(document.querySelector("canvas.starscape")!).opacity,
        ),
      ),
    )
    .toBeGreaterThan(0.99);
  await expect
    .poll(() =>
      page.evaluate(
        () => getComputedStyle(document.querySelector(".starfield")!).visibility,
      ),
    )
    .toBe("hidden");

  const galaxyFadeMs = await page.evaluate(
    () =>
      window.__rnTelemetry.stars.galaxyRevealedAtMs -
      window.__rnTelemetry.stars.galaxyLoadedAtMs,
  );
  expect(galaxyFadeMs).toBeGreaterThanOrEqual(1_500);
});

test("CSS fallback remains when the star catalog cannot load", async ({ page }) => {
  await page.route("**/stars/bright.bin", (route) => route.abort());
  await page.goto(`${site}/?debug=telemetry`);
  await page.waitForFunction(() =>
    window.__rnTelemetry?.events?.some(
      (event: { name: string }) => event.name === "starscape-init",
    ),
  );
  await page.waitForTimeout(100);

  await expect
    .poll(() =>
      page.evaluate(() =>
        document.documentElement.classList.contains("starscape-active"),
      ),
    )
    .toBe(false);
});

test("hidden tabs pause both render loops and resume without object growth", async ({
  page,
}) => {
  await page.goto(`${site}/?debug=telemetry`);
  await page.waitForFunction(
    () =>
      window.__rnTelemetry?.stars?.complete === true &&
      window.__rnTelemetry?.globe?.ticks >= 2 &&
      window.__rnTelemetry?.globe?.textures >= 2,
  );

  const before = await page.evaluate(() => {
    Object.defineProperty(document, "hidden", {
      configurable: true,
      get: () => true,
    });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => "hidden",
    });
    document.dispatchEvent(new Event("visibilitychange"));
    return {
      ticks: window.__rnTelemetry.globe.ticks,
      frames: window.__rnTelemetry.starscape.animationFrames,
      geometries: window.__rnTelemetry.globe.geometries,
      textures: window.__rnTelemetry.globe.textures,
      programs: window.__rnTelemetry.globe.programs,
    };
  });

  await page.waitForTimeout(1_200);
  const hidden = await page.evaluate(() => ({
    ticks: window.__rnTelemetry.globe.ticks,
    frames: window.__rnTelemetry.starscape.animationFrames,
    paused: window.__rnTelemetry.globe.paused,
  }));
  expect(hidden).toEqual({ ticks: before.ticks, frames: before.frames, paused: true });

  await page.evaluate(() => {
    Object.defineProperty(document, "hidden", {
      configurable: true,
      get: () => false,
    });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => "visible",
    });
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await expect.poll(() => page.evaluate(() => window.__rnTelemetry.globe.ticks)).toBeGreaterThan(before.ticks);
  await page.waitForTimeout(1_200);

  const resumed = await page.evaluate(() => ({
    geometries: window.__rnTelemetry.globe.geometries,
    textures: window.__rnTelemetry.globe.textures,
    programs: window.__rnTelemetry.globe.programs,
    paused: window.__rnTelemetry.globe.paused,
  }));
  expect(resumed).toEqual({
    geometries: before.geometries,
    textures: before.textures,
    programs: before.programs,
    paused: false,
  });
});

declare global {
  interface Window {
    __rnTelemetry: any;
  }
}
