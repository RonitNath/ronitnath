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

// ---------------------------------------------------------------- atlas ---
//
// The deep-zoom atlas. Every case below is the counterpart of one in the
// pre-rebuild suite, which tested these behaviours against a lazily-imported
// JavaScript module.
//
// Two of that suite's cases have no counterpart, and the reason is the same for
// both: they tested the module *import* — a retry after a failed import, and
// suppression of a second launch while an import was in flight. There is no
// import. The atlas is in the bundle the landing has already loaded, so neither
// failure can occur. What the second was really protecting — one dialog,
// however many launches — is proved below without it.
//
// A third, "StarScape is home-only", is likewise absent: the bundle mounts only
// on a page carrying `#starscape`, and the landing is the only page this leg
// serves. It becomes provable once there is a second page to prove it against.

/** Which regional-catalog files the page has actually asked the network for. */
const lodRequests = (page: import("@playwright/test").Page) =>
  page.evaluate(() =>
    performance
      .getEntriesByType("resource")
      .filter(entry => entry.name.includes("/static/stars/lod/"))
      .map(entry => ({
        name: entry.name,
        bytes: (entry as PerformanceResourceTiming).decodedBodySize,
      })),
  );

test("the atlas is interaction-gated, and zooming pays for tiles, not the file", async ({
  page,
}) => {
  await page.goto(withTelemetry);
  await page.waitForFunction(() => window.__rnTelemetry?.stars?.complete === true);

  // Nothing of the 49 MB regional catalog is touched by the landing itself.
  expect(await lodRequests(page)).toEqual([]);
  await expect(page.getByRole("button", { name: "Open atlas" })).toBeEnabled();

  // Where the landing is looking, sampled just before the atlas opens on it.
  const zenith = await page.evaluate(() => {
    const matrix = window.__rnStarscape.viewMatrix();
    return [matrix[2], matrix[5], matrix[8]];
  });

  await page.getByRole("button", { name: "Open atlas" }).click();
  const atlas = page.getByRole("dialog", { name: "Celestial atlas" });
  await expect(atlas).toBeVisible();
  await page.waitForFunction(() => window.__rnStarscape.atlas()?.firstFrameAtMs > 0);
  // The sky behind it stops drawing a picture nobody can see.
  expect(await page.evaluate(() => window.__rnTelemetry.sky.pausedForExplorer)).toBe(true);
  // The launcher is behind the dialog, so it leaves the tab order entirely.
  await expect(page.getByRole("button", { name: "Open atlas" })).toBeHidden();

  // It opens on the zenith the landing was drawing at its centre. The sky runs
  // at sixty times real time, so this is an angle between two directions and
  // not an equality between two samples of a moving one.
  const opened: number[] = await page.evaluate(
    () => window.__rnStarscape.atlas().initialForward,
  );
  const cosine = opened.reduce((sum, value, index) => sum + value * zenith[index], 0);
  expect(cosine).toBeGreaterThan(0.999);

  // Zoom in to the regional threshold and prove the tiles arrive by range.
  for (let step = 0; step < 5; step += 1) {
    await page.getByRole("button", { name: "Zoom in" }).click();
  }
  await expect.poll(() => page.evaluate(() => window.__rnStarscape.atlas().fov)).toBeLessThan(25);
  await page.waitForFunction(() => window.__rnTelemetry.explorer?.residentTiles > 0);
  // The whole view fills in, not just the six tiles one gesture can have in
  // flight: the frame keeps asking until nothing it can see is missing.
  await expect
    .poll(() => page.evaluate(() => window.__rnTelemetry.explorer.residentTiles), {
      timeout: 15_000,
    })
    .toBeGreaterThan(6);

  const requests = await lodRequests(page);
  const deep = requests.filter(entry => entry.name.endsWith("g12.bin"));
  expect(deep.length).toBeGreaterThan(0);
  // The whole file is 49 MB. No single response may be anywhere near it.
  expect(Math.max(...deep.map(entry => entry.bytes))).toBeLessThan(2 * 1024 * 1024);
  expect(await page.evaluate(() => window.__rnTelemetry.explorer.residentBytes)).toBeLessThan(
    64 * 1024 * 1024,
  );
});

test("the regional catalog answers ranges, and refuses the ones it cannot", async ({ page }) => {
  await page.goto(site);
  const responses = await page.evaluate(async () => {
    const url = "/static/stars/lod/g12.bin";
    const whole = await fetch("/static/stars/lod/manifest.json");
    const tile = await fetch(url, { headers: { Range: "bytes=12-4459" } });
    const body = await tile.arrayBuffer();
    const head = await fetch(url, { headers: { Range: "bytes=0-11" } });
    const header = await head.arrayBuffer();
    const magic = new TextDecoder().decode(header.slice(0, 8));
    const past = await fetch(url, { headers: { Range: "bytes=99999999999-" } });
    return {
      wholeStatus: whole.status,
      wholeAcceptsRanges: whole.headers.get("accept-ranges"),
      tileStatus: tile.status,
      tileRange: tile.headers.get("content-range"),
      tileBytes: body.byteLength,
      magic,
      pastStatus: past.status,
      pastRange: past.headers.get("content-range"),
    };
  });

  expect(responses).toMatchObject({
    wholeStatus: 200,
    wholeAcceptsRanges: "bytes",
    tileStatus: 206,
    tileBytes: 4_448,
    // The 12-byte file header is inside the range that was asked for, and is
    // *not* inside the tile above it — the tiles start after it.
    magic: "GDR3LOD1",
    pastStatus: 416,
  });
  expect(responses.tileRange).toMatch(/^bytes 12-4459\/\d+$/);
  expect(responses.pastRange).toMatch(/^bytes \*\/\d+$/);
});

test("a star label opens the atlas on that star and holds it there", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(withTelemetry);
  await expect.poll(() => page.locator(".star-callout").count()).toBeGreaterThan(0);

  const label = page.locator(".star-callout").first();
  const name = await label.getAttribute("data-name");
  expect(name).toBeTruthy();
  await label.click();

  await expect(page.getByRole("dialog", { name: "Celestial atlas" })).toBeVisible();
  await page.waitForFunction(() => window.__rnStarscape.atlas()?.firstFrameAtMs > 0);
  const state = await page.evaluate(() => window.__rnStarscape.atlas());
  expect(state.target).toBe(name);
  expect(state.tracking).toBe(true);
  // A star is worth a close look, so it opens narrow rather than at the sky.
  expect(state.fov).toBeLessThan(25);
  await expect(page.locator(".atlas-target")).toHaveText(name!);

  // Tracking holds: the sky drifts past, the star does not.
  const first = state.ra;
  await page.waitForTimeout(600);
  expect(await page.evaluate(() => window.__rnStarscape.atlas().ra)).toBe(first);
});

test("pausing stops the clock, and closing gives the page back unchanged", async ({ page }) => {
  await page.goto(withTelemetry);
  await page.waitForFunction(() => window.__rnTelemetry?.stars?.complete === true);

  // A viewpoint the visitor chose before opening the atlas is theirs to keep.
  await page.evaluate(() => window.__rnStarscape.setObserver(-33.8688, 151.2093));
  await expect(page.locator("#resume-orbit")).toBeVisible();

  await page.getByRole("button", { name: "Open atlas" }).click();
  await page.waitForFunction(() => window.__rnStarscape.atlas()?.firstFrameAtMs > 0);

  await page.getByRole("button", { name: "Pause", exact: true }).click();
  const pausedAt = await page.evaluate(() => window.__rnStarscape.atlas().simMs);
  await page.waitForTimeout(300);
  expect(await page.evaluate(() => window.__rnStarscape.atlas().simMs)).toBe(pausedAt);
  await expect(page.getByRole("button", { name: "Resume", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Close the atlas" }).click();
  await expect(page.getByRole("dialog", { name: "Celestial atlas" })).toHaveCount(0);
  expect(
    await page.evaluate(() => ({
      atlas: window.__rnStarscape.atlas(),
      openClass: document.documentElement.classList.contains("starscape-explorer-open"),
      // Closing hands focus back to the control that opened it.
      focus: document.activeElement?.id,
      pausedForExplorer: window.__rnTelemetry.sky.pausedForExplorer,
      observer: window.__rnStarscape.manualObserver()?.lat,
    })),
  ).toMatchObject({
    atlas: null,
    openClass: false,
    focus: "starscape-launch",
    pausedForExplorer: false,
  });
  await expect(page.getByRole("button", { name: "Open atlas" })).toBeEnabled();
  // And the sky is drawing again.
  const frames = await page.evaluate(() => window.__rnTelemetry.sky.animationFrames);
  await expect
    .poll(() => page.evaluate(() => window.__rnTelemetry.sky.animationFrames))
    .toBeGreaterThan(frames);

  // Resume orbit still clears the observer the atlas was opened over.
  await page.getByRole("button", { name: "Resume orbit" }).click();
  await expect.poll(() => page.evaluate(() => window.__rnStarscape.manualObserver())).toBeNull();
});

test("escape closes the atlas, and two launches make one dialog", async ({ page }) => {
  await page.goto(withTelemetry);
  await page.waitForFunction(() => window.__rnTelemetry?.stars?.complete === true);

  await page.evaluate(() => {
    window.__rnStarscape.openAtlas();
    window.__rnStarscape.openAtlas();
  });
  await expect(page.getByRole("dialog", { name: "Celestial atlas" })).toHaveCount(1);
  await page.waitForFunction(() => window.__rnStarscape.atlas()?.firstFrameAtMs > 0);
  await page.evaluate(() => window.__rnStarscape.openAtlas());
  await expect(page.getByRole("dialog", { name: "Celestial atlas" })).toHaveCount(1);

  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "Celestial atlas" })).toHaveCount(0);
});

test("star names failing to load leaves the atlas reachable from the launcher", async ({
  page,
}) => {
  await page.route("**/static/stars/named.json", route => route.abort());
  await page.goto(withTelemetry);
  await page.waitForFunction(() => window.__rnTelemetry?.stars?.complete === true);

  await expect(page.locator("#star-annotations")).toHaveAttribute(
    "data-annotations-ready",
    "false",
  );
  expect(await page.locator(".star-callout").count()).toBe(0);

  await page.getByRole("button", { name: "Open atlas" }).click();
  await expect(page.getByRole("dialog", { name: "Celestial atlas" })).toBeVisible();
  await page.waitForFunction(() => window.__rnStarscape.atlas()?.visibleStars > 0);
});

test("reduced motion opens the atlas as a still sky", async ({ browser }) => {
  const context = await browser.newContext({ reducedMotion: "reduce" });
  const page = await context.newPage();
  await page.goto(withTelemetry);
  await page.waitForFunction(() => window.__rnTelemetry?.stars?.complete === true);

  await page.getByRole("button", { name: "Open atlas" }).click();
  await page.waitForFunction(() => window.__rnStarscape.atlas()?.firstFrameAtMs > 0);

  const before = await page.evaluate(() => window.__rnStarscape.atlas());
  expect(before.paused).toBe(true);
  await page.waitForTimeout(700);
  const after = await page.evaluate(() => window.__rnStarscape.atlas());
  // Still a sky — stars are drawn — but not a moving one.
  expect(after.visibleStars).toBeGreaterThan(0);
  expect(after.simMs).toBe(before.simMs);
  expect(after.ra).toBe(before.ra);
  expect(
    await page.evaluate(
      () => getComputedStyle(document.querySelector(".starscape-explorer")!).animationName,
    ),
  ).toBe("none");
  await context.close();
});

test("a phone frame fits the atlas rather than letting it overflow", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(withTelemetry);
  await page.waitForFunction(() => window.__rnTelemetry?.stars?.complete === true);

  await page.getByRole("button", { name: "Open atlas" }).click();
  await page.waitForFunction(() => window.__rnStarscape.atlas()?.firstFrameAtMs > 0);

  expect(
    await page.evaluate(() => {
      const bar = document.querySelector(".atlas-bar")!.getBoundingClientRect();
      return {
        barFits: bar.right <= window.innerWidth + 0.5 && bar.left >= -0.5,
        // The keyboard legend is a desktop affordance; a phone has no keys.
        keys: getComputedStyle(document.querySelector(".atlas-keys")!).display,
        overflowX: document.documentElement.scrollWidth - document.documentElement.clientWidth,
        overflowY: document.documentElement.scrollHeight - document.documentElement.clientHeight,
      };
    }),
  ).toEqual({ barFits: true, keys: "none", overflowX: 0, overflowY: 0 });

  // Every control in the bar is still reachable and still works.
  await page.getByRole("button", { name: "Zoom in" }).click();
  await expect.poll(() => page.evaluate(() => window.__rnStarscape.atlas().fov)).toBeLessThan(115);
  await page.getByRole("button", { name: "Close the atlas" }).click();
  await expect(page.getByRole("dialog", { name: "Celestial atlas" })).toHaveCount(0);
});

test("the atlas takes its ground from the theme it is opened in", async ({ page }) => {
  await page.goto(site);
  const ground = async (theme: "dark" | "light") => {
    await page.evaluate(name => document.documentElement.setAttribute("data-theme", name), theme);
    await page.getByRole("button", { name: "Open atlas" }).click();
    await page.waitForFunction(() => window.__rnStarscape.atlas()?.firstFrameAtMs > 0);
    const colours = await page.evaluate(() => ({
      atlas: getComputedStyle(document.querySelector(".starscape-explorer")!).backgroundColor,
      page: getComputedStyle(document.documentElement).backgroundColor,
    }));
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog", { name: "Celestial atlas" })).toHaveCount(0);
    return colours;
  };

  const night = await ground("dark");
  const dusk = await ground("light");

  // Both themes are authored, and they are not the same picture.
  expect(night.atlas).not.toBe(dusk.atlas);
  // At night the atlas is the page's own ground, continuing the sky behind it.
  expect(night.atlas).toBe(night.page);
  // By day the page runs to near-white, and a star map on paper is not a star
  // map — so the atlas takes the dark end of the dusk gradient instead.
  expect(dusk.atlas).not.toBe(dusk.page);
  expect(dusk.atlas).not.toBe(night.page);
});

declare global {
  interface Window {
    __rnTelemetry: any;
    __rnStarscape: any;
  }
}
