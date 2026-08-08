/**
 * Mini-globe progressive enhancement.
 * Loads globe.gl UMD on demand, uses low-priority Earth textures, and
 * samples the shared __rnTrack API installed by the starscape WASM island.
 */

const DAY = "/textures/earth/day.jpg";
const BUMP = "/textures/earth/normal.jpg";
const SPECULAR = "/textures/earth/specular.jpg";

function loadScript(src) {
  return new Promise((resolve, reject) => {
    if (window.Globe) {
      resolve();
      return;
    }
    const s = document.createElement("script");
    s.src = src;
    s.async = true;
    s.fetchPriority = "low";
    s.onload = () => resolve();
    s.onerror = () => reject(new Error(`failed to load ${src}`));
    document.head.appendChild(s);
  });
}

function trackApi() {
  return window.__rnTrack || null;
}

function currentSimMs(epochMs, clientMountMs) {
  const api = trackApi();
  const now = Date.now();
  if (api && typeof api.syncedSimTimeMs === "function") {
    return api.syncedSimTimeMs(epochMs, clientMountMs, now);
  }
  return now;
}

function observerAt(simMs) {
  const api = trackApi();
  if (api && typeof api.observerAt === "function") {
    const pair = api.observerAt(simMs);
    return { lat: Number(pair[0]), lng: Number(pair[1]) };
  }
  return { lat: 37.7749, lng: -122.4194 };
}

/** Sample the recent track as consecutive arc segments on the surface. */
function buildArcs(simMs, periodMs) {
  const trail = periodMs * 0.2; // ~1/5 lap — readable on a 160px globe
  const steps = 48;
  const arcs = [];
  let prev = observerAt(simMs - trail);
  for (let i = 1; i <= steps; i++) {
    const t = simMs - trail + (trail * i) / steps;
    const next = observerAt(t);
    arcs.push({
      startLat: prev.lat,
      startLng: prev.lng,
      endLat: next.lat,
      endLng: next.lng,
    });
    prev = next;
  }
  return arcs;
}

function reveal(el) {
  el.classList.add("is-ready");
}

/** Keep globe.gl canvas in sync with the CSS box as the viewport reflows. */
function observeSize(el, globe) {
  let lastW = 0;
  let lastH = 0;
  let raf = 0;

  const apply = () => {
    raf = 0;
    const w = el.clientWidth;
    const h = el.clientHeight;
    if (w <= 0 || h <= 0) return;
    if (w === lastW && h === lastH) return;
    lastW = w;
    lastH = h;
    globe.width(w).height(h);
  };

  const schedule = () => {
    if (raf) return;
    raf = requestAnimationFrame(apply);
  };

  apply();
  if (typeof ResizeObserver === "function") {
    const ro = new ResizeObserver(schedule);
    ro.observe(el);
    return;
  }
  window.addEventListener("resize", schedule);
}

export async function mountMiniGlobe(el, epochMs) {
  const clientMountMs = Date.now();
  el.innerHTML = "";
  el.classList.remove("is-ready");

  await loadScript("/js/vendor/globe.gl.min.js");
  if (!window.Globe) {
    throw new Error("Globe global missing after script load");
  }

  await Promise.all(
    [DAY, BUMP, SPECULAR].map((url) =>
      fetch(url, { priority: "low" }).then((r) => {
        if (!r.ok) throw new Error(`${url} HTTP ${r.status}`);
        return r.blob();
      })
    )
  );

  const api = trackApi();
  const periodMs = (api && api.trackPeriodMs) || 43_082_045;
  let simMs = currentSimMs(epochMs, clientMountMs);
  let here = observerAt(simMs);

  const globe = window
    .Globe()(el)
    .width(el.clientWidth || 160)
    .height(el.clientHeight || 160)
    .backgroundColor("rgba(0,0,0,0)")
    .showGlobe(true)
    .showAtmosphere(true)
    .atmosphereColor("#9bb7ff")
    .atmosphereAltitude(0.15)
    .globeImageUrl(DAY)
    .bumpImageUrl(BUMP);

  if (typeof globe.specularImageUrl === "function") {
    globe.specularImageUrl(SPECULAR);
  }

  globe
    .arcsData(buildArcs(simMs, periodMs))
    .arcColor(() => "rgba(255, 214, 110, 0.95)")
    .arcAltitude(0.04)
    .arcStroke(1.15)
    .arcDashLength(1)
    .arcDashGap(0)
    .arcAltitudeAutoScale(0.3)
    .pointsData([{ lat: here.lat, lng: here.lng }])
    .pointColor(() => "#ffe08a")
    .pointAltitude(0.02)
    .pointRadius(2.2)
    .pointsMerge(false)
    .enablePointerInteraction(false);

  if (typeof globe.ringsData === "function") {
    globe
      .ringsData([{ lat: here.lat, lng: here.lng }])
      .ringColor(() => (t) => `rgba(255, 220, 120, ${1 - t})`)
      .ringMaxRadius(4)
      .ringPropagationSpeed(1.4)
      .ringRepeatPeriod(1400);
  }

  globe.pointOfView({ lat: here.lat, lng: here.lng, altitude: 1.85 }, 0);
  observeSize(el, globe);
  reveal(el);

  const tick = () => {
    simMs = currentSimMs(epochMs, clientMountMs);
    here = observerAt(simMs);
    globe.arcsData(buildArcs(simMs, periodMs));
    globe.pointsData([{ lat: here.lat, lng: here.lng }]);
    if (typeof globe.ringsData === "function") {
      globe.ringsData([{ lat: here.lat, lng: here.lng }]);
    }
    globe.pointOfView({ lat: here.lat, lng: here.lng, altitude: 1.85 }, 400);
  };
  setInterval(tick, 500);

  return globe;
}
