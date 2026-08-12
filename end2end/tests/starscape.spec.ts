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
      retainedCatalogBytes: window.__rnBrightCatalog?.byteLength,
    };
  });

  expect(result).toMatchObject({
    count: 12_191,
    magnitudeSorted: true,
    uploaded: 12_191,
    active: true,
    retainedCatalogBytes: 243_828,
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

test("StarScape is home-only and its heavy module is interaction-gated", async ({ page }) => {
  await page.goto(`${site}/?debug=telemetry`);
  await expect(page.getByRole("button", { name: "Load StarScape" })).toBeVisible();
  await expect(page.locator(".starscape-controls")).toHaveAttribute(
    "data-annotations-ready",
    "true",
  );
  await expect.poll(() => page.locator(".star-callout").count()).toBeGreaterThan(0);
  expect(
    await page.evaluate(() =>
      performance
        .getEntriesByType("resource")
        .some((entry) => entry.name.includes("starscape-explorer.js")),
    ),
  ).toBe(false);
  expect(
    await page.evaluate(() =>
      performance
        .getEntriesByType("resource")
        .some((entry) => entry.name.includes("/stars/lod/")),
    ),
  ).toBe(false);

  await page.getByRole("button", { name: "Load StarScape" }).click();
  await expect(page.getByRole("dialog", { name: "StarScape celestial atlas" })).toBeVisible();
  await page.waitForFunction(() => window.__rnTelemetry?.explorer?.firstFrameAtMs > 0);
  expect(await page.evaluate(() => window.__rnTelemetry.starscape.pausedForExplorer)).toBe(true);
  expect(
    await page.evaluate(() =>
      performance
        .getEntriesByType("resource")
        .some((entry) => entry.name.includes("starscape-explorer.js")),
    ),
  ).toBe(true);

  // Button-open starts wide, so zoom into the regional threshold and prove
  // the range-backed cache receives tiles without downloading the 47 MB file.
  for (let index = 0; index < 5; index += 1) {
    await page.getByRole("button", { name: "Zoom in" }).click();
  }
  await page.waitForFunction(() => window.__rnTelemetry?.explorer?.residentTiles > 0);
  const deepTransfers = await page.evaluate(() =>
    performance
      .getEntriesByType("resource")
      .filter((entry) => entry.name.endsWith("/stars/lod/g12.bin"))
      .map((entry) => (entry as PerformanceResourceTiming).decodedBodySize),
  );
  expect(deepTransfers.length).toBeGreaterThan(0);
  expect(Math.max(...deepTransfers)).toBeLessThan(2 * 1024 * 1024);

  await page.getByRole("button", { name: "Pause" }).click();
  const pausedAt = await page.evaluate(() => window.__rnTrack.viewerState.simMs);
  await page.waitForTimeout(250);
  expect(await page.evaluate(() => window.__rnTrack.viewerState.simMs)).toBe(pausedAt);
  await page.getByRole("button", { name: "Close StarScape" }).click();
  await expect(page.getByRole("dialog", { name: "StarScape celestial atlas" })).toHaveCount(0);
  expect(await page.evaluate(() => window.__rnTrack.viewerState)).toBeUndefined();

  await page.goto(`${site}/auth`);
  await expect(page.getByRole("button", { name: "Load StarScape" })).toHaveCount(0);
});

test("the mini-globe opens StarScape before it becomes an observer control", async ({ page }) => {
  await page.goto(`${site}/?debug=telemetry`);
  const globe = page.getByRole("button", { name: "Open StarScape from the globe" });
  await expect(globe).toBeVisible();
  await globe.dispatchEvent("pointerdown");
  await expect(page.getByRole("dialog", { name: "StarScape celestial atlas" })).toBeVisible();
});

test("manual observer state is shared and resume orbit clears it", async ({ page }) => {
  await page.goto(`${site}/?debug=telemetry`);
  await page.waitForFunction(
    () =>
      typeof window.__rnTrack?.setManualObserver === "function" &&
      window.__rnTelemetry?.globe?.ticks >= 1,
  );
  await page.getByRole("button", { name: "Load StarScape" }).click();
  const globe = page.locator(".mini-globe canvas");
  await expect(globe).toBeVisible();
  const box = await globe.boundingBox();
  expect(box).not.toBeNull();
  await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
  await page.mouse.down();
  await page.mouse.move(box!.x + box!.width * 0.75, box!.y + box!.height * 0.35, {
    steps: 8,
  });
  await page.mouse.up();
  await expect
    .poll(() => page.evaluate(() => window.__rnTrack.manualObserver != null))
    .toBe(true);

  await page.evaluate(() => window.__rnTrack.setManualObserver(-33.8688, 151.2093));
  await expect
    .poll(() => page.evaluate(() => window.__rnTrack.manualObserver?.lat))
    .toBeCloseTo(-33.8688, 3);
  await expect
    .poll(() => page.locator(".grounding").textContent())
    .toContain("33.87° S, 151.21° E");
  await page.evaluate(() => window.__rnTrack.resumeOrbit());
  await expect
    .poll(() => page.evaluate(() => window.__rnTrack.manualObserver))
    .toBeNull();
});

declare global {
  interface Window {
    __rnTelemetry: any;
    __rnTrack: any;
    __rnBrightCatalog: Uint8Array;
  }
}
