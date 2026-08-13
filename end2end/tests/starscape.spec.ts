import { expect, test } from "@playwright/test";

const site = process.env.RN_SITE_URL || "http://127.0.0.1:3004";

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
      window.__rnTelemetry?.globe?.textures >= 2 &&
      window.__rnTelemetry?.globe?.geometries >= 50,
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
  await expect(page.getByRole("button", { name: "Loading..." })).toBeDisabled();
  await expect(page.getByRole("dialog", { name: "StarScape celestial atlas" })).toBeVisible();
  await page.waitForFunction(() => window.__rnTelemetry?.explorer?.firstFrameAtMs > 0);
  await expect(page.getByRole("button", { name: "StarScape ready" })).toBeVisible();
  await expect(page.getByRole("button", { name: "StarScape ready" })).toBeDisabled();
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
  await expect(page.getByRole("button", { name: "Load StarScape" })).toBeEnabled();
  expect(await page.evaluate(() => window.__rnTrack.viewerState)).toBeUndefined();

  await page.goto(`${site}/auth`);
  await expect(page.getByRole("button", { name: "Load StarScape" })).toHaveCount(0);
});

test("globe click establishes the observer before the delayed explorer loads", async ({ page }) => {
  await page.route("**/js/starscape-explorer.js", async route => {
    await new Promise(resolve => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto(`${site}/?debug=telemetry`);
  await page.waitForFunction(() =>
    window.__rnTelemetry?.events?.some(
      (event: { name: string }) => event.name === "globe-ready",
    ),
  );
  const globe = page.getByRole("application", { name: "Choose an observer location on the globe" });
  await expect(globe).toBeVisible();
  const canvas = globe.locator("canvas");
  const box = await canvas.boundingBox();
  expect(box).not.toBeNull();
  await page.mouse.click(box!.x + box!.width * 0.62, box!.y + box!.height * 0.45);
  await expect(page.getByRole("button", { name: "Loading..." })).toBeDisabled();
  await expect.poll(() => page.evaluate(() => window.__rnTrack.manualObserver)).not.toBeNull();
  const whileLoading = await page.evaluate(() => ({
    observer: window.__rnTrack.manualObserver,
    label: document.querySelector(".grounding")?.textContent,
    dialog: Boolean(document.querySelector(".starscape-explorer")),
  }));
  expect(whileLoading.label).toMatch(/[NS].*[EW]/);
  expect(whileLoading.dialog).toBe(false);
  await expect(page.getByRole("dialog", { name: "StarScape celestial atlas" })).toBeVisible();
  await expect(page.getByRole("button", { name: "StarScape ready" })).toBeDisabled();
  const zenith = await page.evaluate(() => {
    const state = window.__rnTrack.viewerState;
    const matrix = Array.from(window.__rnTrack.viewMatrix(state.initialSimMs)) as number[];
    return { actual: state.initialForward, expected: [matrix[2], matrix[5], matrix[8]] };
  });
  zenith.actual.forEach((value: number, index: number) =>
    expect(value).toBeCloseTo(zenith.expected[index], 5),
  );
});

test("globe drag launches, retains the observer on close, and Resume Orbit clears it", async ({ page }) => {
  await page.goto(`${site}/?debug=telemetry`);
  await page.waitForFunction(
    () =>
      typeof window.__rnTrack?.setManualObserver === "function" &&
      window.__rnTelemetry?.globe?.ticks >= 1,
  );
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
  await expect(page.getByRole("button", { name: "Loading..." })).toBeDisabled();
  await expect(page.getByRole("dialog", { name: "StarScape celestial atlas" })).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => window.__rnTrack.manualObserver != null))
    .toBe(true);

  await page.evaluate(() => window.__rnTrack.setObserverImmediate(-33.8688, 151.2093));
  await expect
    .poll(() => page.evaluate(() => window.__rnTrack.manualObserver?.lat))
    .toBeCloseTo(-33.8688, 3);
  await expect
    .poll(() => page.locator(".grounding").textContent())
    .toContain("33.87° S, 151.21° E");
  await page.getByRole("button", { name: "Close StarScape" }).click();
  await expect.poll(() => page.evaluate(() => window.__rnTrack.manualObserver)).not.toBeNull();
  await page.getByRole("button", { name: "Resume Orbit" }).click();
  await expect
    .poll(() => page.evaluate(() => window.__rnTrack.manualObserver))
    .toBeNull();
});

test("annotation launches share status and the forced fallback contract", async ({ page }) => {
  await page.goto(`${site}/?debug=telemetry`);
  await expect.poll(() => page.locator(".star-callout").count()).toBeGreaterThan(0);
  await page.locator(".home-card").evaluate(node => {
    (node as HTMLElement).style.cssText =
      "position:fixed;inset:0;width:100vw;height:100vh;max-width:none;max-height:none";
  });
  await expect(page.locator(".star-callout.is-forced")).toHaveCount(1);
  const annotation = page.locator(".star-callout.is-forced");
  const name = await annotation.locator("strong").textContent();
  await annotation.click();
  await expect(page.getByRole("button", { name: "Loading..." })).toBeDisabled();
  await expect(page.getByRole("button", { name: "StarScape ready" })).toBeDisabled();
  await expect(page.locator(".explorer-target")).toContainText(name!.split(" · ")[0]);
  await page.getByRole("button", { name: "Close StarScape" }).click();
});

test("failed explorer import exposes a working retry", async ({ page }) => {
  let failed = false;
  await page.route("**/js/starscape-explorer.js*", async route => {
    if (!failed) { failed = true; await route.abort(); }
    else await route.continue();
  });
  await page.goto(`${site}/?debug=telemetry`);
  await page.waitForFunction(() => typeof window.__rnLaunchStarScape === "function");
  await page.getByRole("button", { name: "Load StarScape" }).click();
  await expect(page.getByRole("button", { name: "Try StarScape again" })).toBeEnabled();
  await page.getByRole("button", { name: "Try StarScape again" }).click();
  await expect(page.getByRole("button", { name: "StarScape ready" })).toBeDisabled();
});

test("annotation failure leaves the primary launcher usable", async ({ page }) => {
  await page.route("**/stars/named.json", route => route.abort());
  await page.goto(`${site}/?debug=telemetry`);
  await expect(page.locator(".starscape-controls")).toHaveAttribute("data-annotations-ready", "false");
  await expect(page.getByRole("button", { name: "Load StarScape" })).toBeEnabled();
  await page.getByRole("button", { name: "Load StarScape" }).click();
  await expect(page.getByRole("button", { name: "StarScape ready" })).toBeDisabled();
});

test("duplicate launches are suppressed while the explorer module is loading", async ({ page }) => {
  let explorerRequests = 0;
  await page.route("**/js/starscape-explorer.js*", async route => {
    explorerRequests += 1;
    await new Promise(resolve => setTimeout(resolve, 500));
    await route.continue();
  });
  await page.goto(`${site}/?debug=telemetry`);
  await page.waitForFunction(() => typeof window.__rnLaunchStarScape === "function");
  await page.evaluate(() => {
    window.__rnLaunchStarScape({ source: "button" });
    window.__rnLaunchStarScape({ source: "button" });
  });
  await expect(page.getByRole("button", { name: "Loading..." })).toBeDisabled();
  await expect(page.getByRole("button", { name: "StarScape ready" })).toBeDisabled();
  expect(explorerRequests).toBe(1);
});

declare global {
  interface Window {
    __rnTelemetry: any;
    __rnTrack: any;
    __rnBrightCatalog: Uint8Array;
    __rnLaunchStarScape: (request: any) => void;
  }
}
