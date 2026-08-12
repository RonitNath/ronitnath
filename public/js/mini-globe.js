/**
 * Mini-globe progressive enhancement.
 * Loads globe.gl UMD on demand, uses low-priority Earth textures, and
 * samples the shared __rnTrack API installed by the starscape WASM island.
 */

const DAY = "/textures/earth/day.jpg";
const BUMP = "/textures/earth/normal.jpg";
const SPECULAR = "/textures/earth/specular.jpg";

function telemetry() {
  const root = window.__rnTelemetry;
  return root && root.enabled ? root : null;
}

function telemetryEvent(name) {
  const root = telemetry();
  if (!root) return;
  root.events ||= [];
  root.events.push({ atMs: Date.now(), name });
  if (root.events.length > 100) root.events.splice(0, root.events.length - 100);
}

function telemetrySet(field, value) {
  const root = telemetry();
  if (!root) return;
  root.globe ||= {};
  root.globe[field] = value;
}

function telemetryIncrement(field) {
  const root = telemetry();
  if (!root) return;
  root.globe ||= {};
  root.globe[field] = (root.globe[field] || 0) + 1;
}

function sampleRuntime(globe) {
  const root = telemetry();
  if (!root) return;
  const now = Date.now();
  const previous = root.globe.lastTickAtMs;
  if (previous) {
    root.globe.maxTickGapMs = Math.max(root.globe.maxTickGapMs || 0, now - previous);
  }
  root.globe.lastTickAtMs = now;

  const info = globe.renderer?.().info;
  if (info) {
    root.globe.geometries = info.memory?.geometries;
    root.globe.textures = info.memory?.textures;
    root.globe.programs = info.programs?.length;
    root.globe.renderCalls = info.render?.calls;
    root.globe.triangles = info.render?.triangles;
  }
  if (performance.memory) {
    root.globe.usedJSHeapBytes = performance.memory.usedJSHeapSize;
    root.globe.totalJSHeapBytes = performance.memory.totalJSHeapSize;
  }
}

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
  if (api?.viewerState?.open && Number.isFinite(api.viewerState.simMs)) {
    return api.viewerState.simMs;
  }
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
function buildArcs(simMs, periodMs, arcs = []) {
  const trail = periodMs * 0.2; // ~1/5 lap — readable on a 160px globe
  const steps = 48;
  let prev = observerAt(simMs - trail);
  for (let i = 1; i <= steps; i++) {
    const t = simMs - trail + (trail * i) / steps;
    const next = observerAt(t);
    const arc = arcs[i - 1] || {};
    arc.startLat = prev.lat;
    arc.startLng = prev.lng;
    arc.endLat = next.lat;
    arc.endLng = next.lng;
    arcs[i - 1] = arc;
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
    return () => {
      ro.disconnect();
      if (raf) cancelAnimationFrame(raf);
    };
  }
  window.addEventListener("resize", schedule);
  return () => {
    window.removeEventListener("resize", schedule);
    if (raf) cancelAnimationFrame(raf);
  };
}

export async function mountMiniGlobe(el, epochMs) {
  telemetryEvent("globe-mount-started");
  const clientMountMs = Date.now();
  el.innerHTML = "";
  el.classList.remove("is-ready");

  await loadScript("/js/vendor/globe.gl.min.js");
  if (!window.Globe) {
    throw new Error("Globe global missing after script load");
  }

  const api = trackApi();
  const periodMs = (api && api.trackPeriodMs) || 43_082_045;
  let simMs = currentSimMs(epochMs, clientMountMs);
  let here = observerAt(simMs);
  const arcs = buildArcs(simMs, periodMs);
  const point = { lat: here.lat, lng: here.lng };
  const ring = { lat: here.lat, lng: here.lng };
  const resumeButton = document.createElement("button");
  resumeButton.type = "button";
  resumeButton.className = "resume-orbit";
  resumeButton.textContent = "Resume Orbit";
  resumeButton.hidden = !api?.manualObserver;
  resumeButton.addEventListener("click", () => {
    resumeButton.hidden = true;
    api?.resumeOrbit?.();
  });
  el.after(resumeButton);

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
    .arcsData(arcs)
    .arcColor(() => "rgba(255, 214, 110, 0.95)")
    .arcAltitude(0.04)
    .arcStroke(1.15)
    .arcDashLength(1)
    .arcDashGap(0)
    .arcAltitudeAutoScale(0.3)
    .arcsTransitionDuration(0)
    .pointsData([point])
    .pointColor(() => "#ffe08a")
    .pointAltitude(0.02)
    .pointRadius(2.2)
    .pointsTransitionDuration(0)
    .pointsMerge(false)
    .enablePointerInteraction(false);

  if (typeof globe.ringsData === "function") {
    globe
      .ringsData([ring])
      .ringColor(() => (t) => `rgba(255, 220, 120, ${1 - t})`)
      .ringMaxRadius(4)
      .ringPropagationSpeed(1.4)
      .ringRepeatPeriod(1400);
  }
  telemetryEvent("globe-textures-requested");

  globe.pointOfView({ lat: here.lat, lng: here.lng, altitude: 1.85 }, 0);
  const stopObservingSize = observeSize(el, globe);
  reveal(el);
  telemetrySet("arcCount", 48);
  telemetrySet("pointCount", 1);
  telemetrySet("ringCount", 1);
  telemetrySet("tickIntervalMs", 500);
  telemetrySet("trailFractionOfLap", 0.2);
  telemetryEvent("globe-ready");
  sampleRuntime(globe);

  const tick = () => {
    telemetryIncrement("ticks");
    if (document.hidden) telemetryIncrement("ticksWhileHidden");
    sampleRuntime(globe);
    simMs = currentSimMs(epochMs, clientMountMs);
    here = observerAt(simMs);
    buildArcs(simMs, periodMs, arcs);
    point.lat = here.lat;
    point.lng = here.lng;
    ring.lat = here.lat;
    ring.lng = here.lng;
    globe.arcsData(arcs);
    globe.pointsData([point]);
    if (typeof globe.ringsData === "function") {
      globe.ringsData([ring]);
    }
    if (!document.documentElement.classList.contains("starscape-explorer-open")) {
      globe.pointOfView({ lat: here.lat, lng: here.lng, altitude: 1.85 }, 400);
    }
    const currentApi = trackApi();
    resumeButton.hidden = !currentApi?.manualObserver || currentApi?.observerTransition?.clearAtEnd;
  };

  const setInteractive = () => {
    const open = document.documentElement.classList.contains("starscape-explorer-open");
    globe.enablePointerInteraction(open);
    el.setAttribute("aria-hidden", open ? "false" : "true");
    telemetrySet("interactive", open);
  };
  const controls = globe.controls?.();
  const selectGlobeCenter = () => {
    if (!document.documentElement.classList.contains("starscape-explorer-open")) return;
    const pov = globe.pointOfView();
    trackApi()?.setManualObserver?.(Number(pov.lat), Number(pov.lng));
    resumeButton.hidden = false;
    telemetryEvent("globe-manual-observer-selected");
  };
  controls?.addEventListener?.("end", selectGlobeCenter);
  window.addEventListener("starscape-explorer-state", setInteractive);
  window.addEventListener("starscape-observer-changed", () => {
    const currentApi = trackApi();
    resumeButton.hidden = !currentApi?.manualObserver || currentApi?.observerTransition?.clearAtEnd;
    tick();
  });

  let intervalId = 0;
  let disposed = false;
  const pause = () => {
    if (intervalId) clearInterval(intervalId);
    intervalId = 0;
    globe.pauseAnimation();
    telemetrySet("intervalId", 0);
    telemetrySet("paused", true);
  };
  const resume = (updateNow = true) => {
    if (disposed || document.hidden || intervalId) return;
    globe.resumeAnimation();
    if (updateNow) tick();
    intervalId = setInterval(tick, 500);
    telemetrySet("intervalId", intervalId);
    telemetrySet("paused", false);
  };
  const visibilityChanged = () => (document.hidden ? pause() : resume());
  const pageShown = (event) => {
    if (event.persisted) resume();
  };
  const pageHidden = (event) => {
    pause();
    if (!event.persisted) {
      disposed = true;
      stopObservingSize();
      document.removeEventListener("visibilitychange", visibilityChanged);
      window.removeEventListener("starscape-explorer-state", setInteractive);
      controls?.removeEventListener?.("end", selectGlobeCenter);
      window.removeEventListener("pageshow", pageShown);
      window.removeEventListener("pagehide", pageHidden);
      globe._destructor?.();
      telemetryEvent("globe-disposed");
    }
  };
  document.addEventListener("visibilitychange", visibilityChanged);
  window.addEventListener("pageshow", pageShown);
  window.addEventListener("pagehide", pageHidden);
  if (document.hidden) pause();
  else resume(false);
  setInteractive();

  return globe;
}
