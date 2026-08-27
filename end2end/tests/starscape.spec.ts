import { expect, test } from "@playwright/test";

// The dev server: `cargo run -p rn-site` binds 127.0.0.1:3004 from the
// checked-in config, which is also this suite's baseURL.
const site = process.env.RN_SITE_URL || "http://127.0.0.1:3004";

/** Telemetry is opt-in, so every test that reads counters asks for it. */
const withTelemetry = `${site}/?debug=telemetry`;

test("the star catalog streams and the canvas reveals on the first batch", async ({ page }) => {
  await page.goto(withTelemetry);
  await page.waitForFunction(() => window.__rnTelemetry?.stars?.complete === true);

  const result = await page.evaluate(async () => {
    // Read the shipped asset independently of the renderer: the count and the
    // brightest-first ordering are properties of the file, and a renderer that
    // agreed with itself about a truncated catalog would still pass.
    const bytes = await (await fetch("/static/stars/bright.bin")).arrayBuffer();
    const view = new DataView(bytes);
    const count = view.getUint32(4, true);
    const magnitudes = Array.from({ length: count }, (_, index) =>
      view.getFloat32(8 + index * 20 + 12, true),
    );
    const canvas = document.querySelector("canvas.starscape")!;
    return {
      count,
      brightestFirst: magnitudes.every((m, i) => i === 0 || magnitudes[i - 1] <= m),
      uploaded: window.__rnTelemetry.stars.uploaded,
      batches: window.__rnTelemetry.stars.uploadBatches,
      batchToRevealMs:
        window.__rnTelemetry.stars.revealedAtMs - window.__rnTelemetry.stars.firstBatchAtMs,
      active: document.documentElement.classList.contains("starscape-active"),
      transitionDuration: getComputedStyle(canvas).transitionDuration,
    };
  });

  expect(result).toMatchObject({
    count: 12_191,
    uploaded: 12_191,
    brightestFirst: true,
    active: true,
  });
  // The reveal is on the first batch, not the last byte.
  expect(result.batchToRevealMs).toBeLessThan(100);
  expect(result.transitionDuration).toContain("1.2s");

  // The Milky Way arrives behind the stars and fades in over its own second.
  await page.waitForFunction(() => window.__rnTelemetry?.stars?.galaxyRevealedAtMs > 0);
  const fadeMs = await page.evaluate(
    () =>
      window.__rnTelemetry.stars.galaxyRevealedAtMs -
      window.__rnTelemetry.stars.galaxyLoadedAtMs,
  );
  expect(fadeMs).toBeGreaterThanOrEqual(1_500);

  await expect
    .poll(() =>
      page.evaluate(() => getComputedStyle(document.querySelector(".starfield")!).visibility),
    )
    .toBe("hidden");
});

test("the CSS starfield stays when the star catalog cannot load", async ({ page }) => {
  await page.route("**/static/stars/bright.bin", route => route.abort());
  await page.goto(withTelemetry);
  await page.waitForFunction(() =>
    window.__rnTelemetry?.events?.some((event: { name: string }) => event.name === "sky-init"),
  );
  await page.waitForTimeout(500);

  expect(
    await page.evaluate(() => ({
      active: document.documentElement.classList.contains("starscape-active"),
      starfield: getComputedStyle(document.querySelector(".starfield")!).visibility,
      hero: document.querySelector(".home-card h1")?.textContent,
    })),
  ).toEqual({ active: false, starfield: "visible", hero: "Ronit Nath" });
});

test("a hidden tab stops repainting and resumes without leaking GPU objects", async ({ page }) => {
  await page.goto(withTelemetry);
  await page.waitForFunction(
    () => window.__rnTelemetry?.stars?.complete === true && window.__rnTelemetry?.globe?.ticks >= 2,
  );

  await page.evaluate(() => {
    for (const [name, value] of [
      ["hidden", true],
      ["visibilityState", "hidden"],
    ] as const) {
      Object.defineProperty(document, name, { configurable: true, get: () => value });
    }
    document.dispatchEvent(new Event("visibilitychange"));
  });

  // Let the tail of the boot sequence — the last paint after the globe's
  // texture lands — settle before the counters become the baseline.
  await page.waitForTimeout(300);
  const before = await page.evaluate(() => ({
    frames: window.__rnTelemetry.sky.animationFrames,
    ticks: window.__rnTelemetry.globe.ticks,
    textures: window.__rnTelemetry.globe.textures,
  }));

  await page.waitForTimeout(1_000);
  expect(
    await page.evaluate(() => ({
      frames: window.__rnTelemetry.sky.animationFrames,
      ticks: window.__rnTelemetry.globe.ticks,
      paused: window.__rnTelemetry.globe.paused,
    })),
  ).toEqual({ frames: before.frames, ticks: before.ticks, paused: true });

  await page.evaluate(() => {
    for (const [name, value] of [
      ["hidden", false],
      ["visibilityState", "visible"],
    ] as const) {
      Object.defineProperty(document, name, { configurable: true, get: () => value });
    }
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await expect
    .poll(() => page.evaluate(() => window.__rnTelemetry.sky.animationFrames))
    .toBeGreaterThan(before.frames);
  // Resuming re-enters the loop; it does not re-create the scene.
  expect(await page.evaluate(() => window.__rnTelemetry.globe.textures)).toBe(before.textures);
});

test("the grounding readout names where the observer is, and keeps moving", async ({ page }) => {
  await page.goto(site);
  const grounding = page.locator("#grounding");
  await expect(grounding).toHaveText(/\d+\.\d{2}° [NS], \d+\.\d{2}° [EW]/);

  const first = await grounding.textContent();
  await expect.poll(() => grounding.textContent(), { timeout: 5_000 }).not.toBe(first);
});

test("picking a place on the globe moves the sky, and resume orbit gives it back", async ({
  page,
}) => {
  await page.goto(withTelemetry);
  await page.waitForFunction(() =>
    window.__rnTelemetry?.events?.some((event: { name: string }) => event.name === "globe-ready"),
  );
  await expect(page.locator("#mini-globe")).toHaveClass(/is-ready/);
  await expect(page.locator("#resume-orbit")).toBeHidden();

  // Sydney, through the same entry point the globe's own click uses.
  await page.evaluate(() => window.__rnStarscape.setObserver(-33.8688, 151.2093));
  await expect
    .poll(() => page.evaluate(() => window.__rnStarscape.manualObserver()?.lat))
    .toBeCloseTo(-33.8688, 3);
  await expect(page.locator("#grounding")).toHaveText("33.87° S, 151.21° E · over Sydney, AU");
  await expect(page.locator("#resume-orbit")).toBeVisible();

  // The sky follows the observer, not just the caption.
  const zenith = await page.evaluate(() => {
    const matrix = window.__rnStarscape.viewMatrix();
    return [matrix[2], matrix[5], matrix[8]];
  });
  expect(zenith.reduce((sum, value) => sum + value * value, 0)).toBeCloseTo(1, 5);

  await page.getByRole("button", { name: "Resume orbit" }).click();
  await expect
    .poll(() => page.evaluate(() => window.__rnStarscape.manualObserver()))
    .toBeNull();
  await expect(page.locator("#resume-orbit")).toBeHidden();
});

test("dragging the globe takes the viewpoint with it", async ({ page }) => {
  await page.goto(withTelemetry);
  await page.waitForFunction(() =>
    window.__rnTelemetry?.events?.some((event: { name: string }) => event.name === "globe-ready"),
  );

  const globe = page.locator("#mini-globe");
  const box = await globe.boundingBox();
  expect(box).not.toBeNull();
  const centre = { x: box!.x + box!.width / 2, y: box!.y + box!.height / 2 };

  await page.mouse.move(centre.x, centre.y);
  await page.mouse.down();
  await page.mouse.move(centre.x + 40, centre.y, { steps: 8 });
  await page.mouse.up();

  // Dragging right spins the Earth right, so the viewpoint moves west.
  const observer = await page.evaluate(() => window.__rnStarscape.manualObserver());
  expect(observer).not.toBeNull();
  const orbit = await page.evaluate(() =>
    window.__rnStarscape.orbitAt(window.__rnStarscape.simTimeMs()),
  );
  expect(observer.lon).not.toBeCloseTo(orbit.lon, 2);
});

test("reduced motion freezes the sky instead of animating it", async ({ browser }) => {
  const context = await browser.newContext({ reducedMotion: "reduce" });
  const page = await context.newPage();
  await page.goto(withTelemetry);
  await page.waitForFunction(() => window.__rnTelemetry?.stars?.complete === true);

  const before = await page.evaluate(() => window.__rnStarscape.simTimeMs());
  await page.waitForTimeout(800);
  expect(
    await page.evaluate(() => ({
      frames: window.__rnTelemetry.sky.animationFrames ?? 0,
      sim: window.__rnStarscape.simTimeMs(),
      // The single painted frame still replaces the fallback: reduced motion
      // means a still sky, not no sky.
      active: document.documentElement.classList.contains("starscape-active"),
      starfield: getComputedStyle(document.querySelector(".starfield")!).visibility,
    })),
  ).toEqual({ frames: 0, sim: before, active: true, starfield: "hidden" });
  await context.close();
});

test("star labels name what is overhead and never land on the hero", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(site);
  await expect.poll(() => page.locator(".star-callout").count()).toBeGreaterThan(0);

  const overlaps = await page.evaluate(() => {
    const hero = document.querySelector(".home-card")!.getBoundingClientRect();
    return Array.from(document.querySelectorAll(".star-callout")).filter(label => {
      const box = label.getBoundingClientRect();
      return (
        box.left < hero.right &&
        box.right > hero.left &&
        box.top < hero.bottom &&
        box.bottom > hero.top
      );
    }).length;
  });
  expect(overlaps).toBe(0);

  const label = page.locator(".star-callout").first();
  await expect(label.locator("strong")).not.toBeEmpty();
  await expect(label.locator("span")).toHaveText(/· \d+ ly$/i);
});

test("a phone frame elides the labels rather than covering the page with them", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(site);
  await page.waitForTimeout(1_500);

  expect(
    await page.evaluate(() => ({
      labels: getComputedStyle(document.querySelector(".star-annotations")!).display,
      // Nothing may overflow: the page is exactly the frame.
      overflowX: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      overflowY: document.documentElement.scrollHeight - document.documentElement.clientHeight,
    })),
  ).toEqual({ labels: "none", overflowX: 0, overflowY: 0 });
  await expect(page.locator("#grounding")).not.toBeEmpty();
});

declare global {
  interface Window {
    __rnTelemetry: any;
    __rnStarscape: any;
  }
}
