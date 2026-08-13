/** Lightweight named-star overlay. The explorer itself is a separate module. */

const FOCAL = 0.8391;
const STRIDE = 20;
const HEADER = 8;

function emit(name, values = {}) {
  const telemetry = window.__rnTelemetry;
  if (!telemetry?.enabled) return;
  telemetry.annotations ||= {};
  Object.assign(telemetry.annotations, values);
  telemetry.events ||= [];
  telemetry.events.push({ atMs: Date.now(), name });
  if (telemetry.events.length > 100) telemetry.events.splice(0, telemetry.events.length - 100);
}

function simNow(epochMs, mountMs) {
  const api = window.__rnTrack;
  return api?.syncedSimTimeMs?.(epochMs, mountMs, Date.now()) || Date.now();
}

function project(pos, matrix, width, height, clipped = true) {
  const [x,y,z] = pos;
  const vx = matrix[0]*x + matrix[3]*y + matrix[6]*z;
  const vy = matrix[1]*x + matrix[4]*y + matrix[7]*z;
  const vz = matrix[2]*x + matrix[5]*y + matrix[8]*z;
  if (vz <= 0.12) return null;
  const aspect = width / height;
  const nx = vx / vz * FOCAL / aspect;
  const ny = vy / vz * FOCAL;
  if (clipped && (Math.abs(nx) > 0.88 || Math.abs(ny) > 0.82)) return null;
  return { x: (nx + 1) * width / 2, y: (1 - ny) * height / 2, nx, ny, vz };
}

async function loadNamedStars() {
  const metadataResponse = await fetch("/stars/named.json");
  if (!metadataResponse.ok) throw new Error(`named stars HTTP ${metadataResponse.status}`);
  const metadata = await metadataResponse.json();
  if (metadata?.version !== 1 || !Array.isArray(metadata.stars)) {
    throw new Error("invalid named-star catalog");
  }
  for (let waited = 0; waited < 1000 && !(window.__rnBrightCatalog instanceof Uint8Array); waited += 25) {
    await new Promise(resolve => setTimeout(resolve, 25));
  }
  let buffer;
  if (window.__rnBrightCatalog instanceof Uint8Array) {
    buffer = window.__rnBrightCatalog.buffer.slice(
      window.__rnBrightCatalog.byteOffset,
      window.__rnBrightCatalog.byteOffset + window.__rnBrightCatalog.byteLength,
    );
  } else {
    const response = await fetch("/stars/bright.bin");
    if (!response.ok) throw new Error(`bright stars HTTP ${response.status}`);
    buffer = await response.arrayBuffer();
  }
  const view = new DataView(buffer);
  return metadata.stars.filter(star => star.brightIndex < view.getUint32(4, true)).map(meta => {
    const offset = HEADER + meta.brightIndex * STRIDE;
    return {
      meta,
      magnitude: view.getFloat32(offset + 12, true),
      pos: [view.getFloat32(offset,true), view.getFloat32(offset+4,true), view.getFloat32(offset+8,true)],
    };
  });
}

function overlaps(a, b, pad = 12) {
  return a.left < b.right + pad && a.right > b.left - pad && a.top < b.bottom + pad && a.bottom > b.top - pad;
}

export async function mountStarScapeControls(root, epochMs) {
  if (root.dataset.starscapeControlsMounted === "true") return;
  root.dataset.starscapeControlsMounted = "true";
  const mountMs = Date.now();
  const layer = root.querySelector(".star-annotations");
  const launch = root.querySelector(".starscape-launch");
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.classList.add("star-leaders");
  svg.setAttribute("aria-hidden", "true");
  layer.append(svg);
  const labels = new Map();
  let disposed = false, lastLayout = 0, lastSignature = "", launchState = "idle";

  const setLaunchState = state => {
    launchState = state;
    const states = {
      idle: ["Load StarScape", false],
      loading: ["Loading...", true],
      ready: ["StarScape ready", true],
      error: ["Try StarScape again", false],
    };
    [launch.textContent, launch.disabled] = states[state];
    launch.dataset.state = state;
  };

  const open = async (request = { source: "button" }) => {
    if (launchState === "loading" || document.documentElement.classList.contains("starscape-explorer-open")) return;
    const retry = launchState === "error";
    const star = request.star || null;
    if (request.observer) {
      window.__rnTrack?.setObserverImmediate?.(request.observer.lat, request.observer.lon);
    }
    emit("starscape-explorer-requested", { source: request.source, selected: star?.meta?.name || null, requestedAtMs: Date.now() });
    setLaunchState("loading");
    launch.disabled = true;
    const trigger = document.activeElement;
    try {
      const module = await import(`/js/starscape-explorer.js${retry ? `?retry=${Date.now()}` : ""}`);
      await module.openStarScape({
        epochMs,
        clientMountMs: mountMs,
        selected: star,
        observer: request.observer || null,
        onReady: () => setLaunchState("ready"),
        onClose: () => {
        setLaunchState("idle");
        trigger?.focus?.();
      }});
    } catch (error) {
      console.warn("StarScape explorer unavailable", error);
      setLaunchState("error");
    }
  };
  setLaunchState("idle");
  launch.addEventListener("click", () => open({ source: "button" }));
  const openRequested = event => open(event.detail || { source: "button" });
  window.addEventListener("starscape-open-request", openRequested);
  window.__rnLaunchStarScape = open;
  if (window.__rnPendingStarScapeRequest) {
    const pending = window.__rnPendingStarScapeRequest;
    delete window.__rnPendingStarScapeRequest;
    queueMicrotask(() => open(pending));
  }

  let stars = [];
  try {
    stars = await loadNamedStars();
    root.dataset.annotationsReady = "true";
  } catch (error) {
    // Catalog annotations are progressive enhancement. A failed annotation
    // fetch must never disable the primary explorer launcher.
    root.dataset.annotationsReady = "false";
    console.warn("Named-star annotations unavailable", error);
  }

  const layout = now => {
    if (disposed) return;
    requestAnimationFrame(layout);
    if (now - lastLayout < 100 || document.hidden || document.documentElement.classList.contains("starscape-explorer-open")) return;
    lastLayout = now;
    const api = window.__rnTrack;
    if (!api?.viewMatrix) return;
    const width = innerWidth, height = innerHeight;
    svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
    const matrix = Array.from(api.viewMatrix(simNow(epochMs, mountMs)));
    const blocked = [".topbar", ".home-card", ".sky-chrome", ".starscape-launch"]
      .map(s => document.querySelector(s)?.getBoundingClientRect()).filter(Boolean);
    const placed = [];
    const limit = matchMedia("(max-width: 768px)").matches ? 1 : 3;
    const ranked = stars.map(star => ({ star, point: project(star.pos, matrix, width, height) }))
      .filter(item => item.point)
      .sort((a,b) => (a.point.nx*a.point.nx+a.point.ny*a.point.ny+a.star.magnitude*.035) - (b.point.nx*b.point.nx+b.point.ny*b.point.ny+b.star.magnitude*.035));
    for (const {star, point} of ranked) {
      if (!point || placed.length >= limit) continue;
      const right = point.x < width * .55;
      const labelW = Math.min(278, width - 32);
      const left = right ? point.x + 54 : point.x - labelW - 54;
      const top = point.y - 46;
      const rect = { left, top, right: left + labelW, bottom: top + 58 };
      if (left < 12 || rect.right > width - 12 || top < 72 || rect.bottom > height - 18 || blocked.some(b => overlaps(rect,b)) || placed.some(p => overlaps(rect,p.rect,18))) continue;
      placed.push({ star, point, rect, right });
    }
    if (!placed.length && stars.length) {
      const fallback = stars.map(star => ({star, point: project(star.pos,matrix,width,height,false)}))
        .filter(item => item.point)
        .sort((a,b) => (b.point.vz-b.star.magnitude*.01)-(a.point.vz-a.star.magnitude*.01))[0];
      const slots = [
        {left:16,top:88}, {left:Math.max(16,width-294),top:88},
        {left:Math.max(16,width-294),top:Math.max(88,height-150)},
      ];
      const labelW=Math.min(278,width-32);
      const slot=slots.map(value => ({...value,right:value.left+labelW,bottom:value.top+58}))
        .find(rect => !blocked.some(item => overlaps(rect,item))) || {left:16,top:88,right:16+labelW,bottom:146};
      const point={...fallback.point,x:Math.max(10,Math.min(width-10,fallback.point.x)),y:Math.max(68,Math.min(height-10,fallback.point.y))};
      placed.push({star:fallback.star,point,rect:slot,right:slot.left>width/2,forced:true});
    }
    const active = new Set(placed.map(p => p.star.meta.brightIndex));
    for (const [key, node] of labels) if (!active.has(key)) { node.button.remove(); node.path.remove(); labels.delete(key); }
    for (const item of placed) {
      const {brightIndex:index,name,constellation,classification:nature,distanceLy:distance} = item.star.meta;
      let entry = labels.get(index);
      if (!entry) {
        const button = document.createElement("button");
        button.type = "button"; button.className = "star-callout";
        button.innerHTML = `<strong>${name} · ${constellation}</strong><span>${nature} · ${distance} ly</span>`;
        button.setAttribute("aria-label", `Explore ${name}, ${nature}, ${distance} light-years away in ${constellation}`);
        button.addEventListener("click", () => { emit("star-annotation-clicked", { selected:name }); open({source:"annotation",star:item.star}); });
        const path = document.createElementNS(svg.namespaceURI,"path");
        svg.append(path); layer.append(button); entry = {button,path}; labels.set(index,entry);
      }
      Object.assign(entry.button.style,{left:`${item.rect.left}px`,top:`${item.rect.top}px`,width:`${item.rect.right-item.rect.left}px`});
      entry.button.classList.toggle("is-forced", !!item.forced);
      const endX = item.right ? item.rect.left : item.rect.right;
      const elbowX = item.right ? item.point.x + 25 : item.point.x - 25;
      const lineY = item.rect.bottom - 8;
      entry.path.setAttribute("d",`M ${item.point.x.toFixed(1)} ${item.point.y.toFixed(1)} L ${elbowX.toFixed(1)} ${lineY.toFixed(1)} L ${endX.toFixed(1)} ${lineY.toFixed(1)}`);
    }
    const signature = placed.map(item => item.star.meta.name).join("|");
    if (signature !== lastSignature) {
      lastSignature = signature;
      emit("star-annotations-changed", { visible: placed.length, names: signature });
    } else if (window.__rnTelemetry?.enabled) {
      window.__rnTelemetry.annotations.visible = placed.length;
    }
  };
  requestAnimationFrame(layout);
  window.addEventListener("pagehide", () => {
    disposed = true;
    window.removeEventListener("starscape-open-request", openRequested);
    if (window.__rnLaunchStarScape === open) delete window.__rnLaunchStarScape;
  }, {once:true});
}
