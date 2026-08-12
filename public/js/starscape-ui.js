/** Lightweight named-star overlay. The explorer itself is a separate module. */

const FOCAL = 0.8391;
const STRIDE = 20;
const HEADER = 8;

// Indexes refer to the magnitude-sorted bright.bin catalog. Distances and
// classifications are curated from SIMBAD/Hipparcos; duplicate component
// records in the first 50 catalog rows are intentionally omitted.
const NAMED = [
  [0,"Sirius","Canis Major","A-type main-sequence star","8.6"],
  [1,"Canopus","Carina","yellow-white bright giant","310"],
  [2,"Vega","Lyra","A-type main-sequence star","25"],
  [3,"Arcturus","Boötes","orange giant","36.7"],
  [4,"Alpha Centauri","Centaurus","triple-star system","4.4"],
  [5,"Rigel","Orion","blue supergiant","860"],
  [6,"Capella","Auriga","four-star system","42.9"],
  [7,"Achernar","Eridanus","rapidly rotating B star","139"],
  [8,"Procyon","Canis Minor","binary star system","11.5"],
  [9,"Betelgeuse","Orion","red supergiant","548"],
  [10,"Hadar","Centaurus","blue-giant binary","390"],
  [11,"Acrux","Crux","triple-star system","320"],
  [12,"Altair","Aquila","A-type main-sequence star","16.7"],
  [13,"Spica","Virgo","blue-giant binary","250"],
  [14,"Antares","Scorpius","red supergiant","550"],
  [15,"Aldebaran","Taurus","orange giant","65"],
  [16,"Mimosa","Crux","blue giant","280"],
  [17,"Fomalhaut","Piscis Austrinus","A-type main-sequence star","25.1"],
  [19,"Pollux","Gemini","orange giant","33.7"],
  [20,"Deneb","Cygnus","blue-white supergiant","2,615"],
  [21,"Regulus","Leo","four-star system","79"],
  [22,"Adhara","Canis Major","blue bright giant","405"],
  [23,"Shaula","Scorpius","triple-star system","570"],
  [24,"Bellatrix","Orion","blue giant","250"],
  [25,"Castor","Gemini","six-star system","51"],
  [26,"Elnath","Taurus","blue giant","134"],
  [27,"Alnilam","Orion","blue supergiant","2,000"],
  [28,"Gacrux","Crux","red giant","89"],
  [29,"Miaplacidus","Carina","A-type giant","113"],
  [30,"Alnitak","Orion","triple-star system","1,260"],
  [31,"Alnair","Grus","B-type main-sequence star","101"],
  [32,"Regor","Vela","Wolf–Rayet binary","1,090"],
  [33,"Alioth","Ursa Major","chemically peculiar giant","83"],
  [35,"Kaus Australis","Sagittarius","blue subgiant","143"],
  [36,"Alkaid","Ursa Major","B-type main-sequence star","104"],
  [37,"Peacock","Pavo","blue subgiant","179"],
  [39,"Tejat","Gemini","red giant","230"],
  [40,"Mirzam","Canis Major","blue giant","500"],
  [41,"Mirfak","Perseus","yellow-white supergiant","510"],
  [42,"Menkalinan","Auriga","binary subgiants","81"],
  [43,"Sargas","Scorpius","yellow-white giant","300"],
  [44,"Alhena","Gemini","binary star system","109"],
  [45,"Delta Velorum","Vela","eclipsing triple system","80"],
  [46,"Dubhe","Ursa Major","giant binary","123"],
  [47,"R Doradus","Dorado","red giant","178"],
  [48,"Wezen","Canis Major","yellow supergiant","1,600"],
  [49,"Larawag","Scorpius","orange giant","64"],
  [50,"Avior","Carina","eclipsing binary","630"],
  [51,"Saiph","Orion","blue supergiant","650"],
  [52,"Nunki","Sagittarius","B-type main-sequence star","228"],
];

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

function project(pos, matrix, width, height) {
  const [x,y,z] = pos;
  const vx = matrix[0]*x + matrix[3]*y + matrix[6]*z;
  const vy = matrix[1]*x + matrix[4]*y + matrix[7]*z;
  const vz = matrix[2]*x + matrix[5]*y + matrix[8]*z;
  if (vz <= 0.12) return null;
  const aspect = width / height;
  const nx = vx / vz * FOCAL / aspect;
  const ny = vy / vz * FOCAL;
  if (Math.abs(nx) > 0.88 || Math.abs(ny) > 0.82) return null;
  return { x: (nx + 1) * width / 2, y: (1 - ny) * height / 2 };
}

async function loadNamedStars() {
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
  return NAMED.filter(([index]) => index < view.getUint32(4, true)).map(meta => {
    const offset = HEADER + meta[0] * STRIDE;
    return { meta, pos: [view.getFloat32(offset,true), view.getFloat32(offset+4,true), view.getFloat32(offset+8,true)] };
  });
}

function overlaps(a, b, pad = 12) {
  return a.left < b.right + pad && a.right > b.left - pad && a.top < b.bottom + pad && a.bottom > b.top - pad;
}

export async function mountStarScapeControls(root, epochMs) {
  const mountMs = Date.now();
  const layer = root.querySelector(".star-annotations");
  const launch = root.querySelector(".starscape-launch");
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.classList.add("star-leaders");
  svg.setAttribute("aria-hidden", "true");
  layer.append(svg);
  const stars = await loadNamedStars();
  const labels = new Map();
  let disposed = false, lastLayout = 0, lastSignature = "";

  const open = async (star = null) => {
    if (document.documentElement.classList.contains("starscape-explorer-open")) return;
    emit("starscape-explorer-requested", { selected: star?.meta?.[1] || null, requestedAtMs: Date.now() });
    launch.disabled = true;
    launch.textContent = "Loading StarScape…";
    const trigger = document.activeElement;
    try {
      const module = await import("/js/starscape-explorer.js");
      await module.openStarScape({ epochMs, clientMountMs: mountMs, selected: star, onClose: () => {
        launch.disabled = false;
        launch.textContent = "Load StarScape";
        trigger?.focus?.();
      }});
    } catch (error) {
      console.warn("StarScape explorer unavailable", error);
      launch.disabled = false;
      launch.textContent = "Try StarScape again";
    }
  };
  launch.addEventListener("click", () => open());

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
    for (const star of stars) {
      const point = project(star.pos, matrix, width, height);
      if (!point || placed.length >= limit) continue;
      const right = point.x < width * .55;
      const labelW = Math.min(278, width - 32);
      const left = right ? point.x + 54 : point.x - labelW - 54;
      const top = point.y - 46;
      const rect = { left, top, right: left + labelW, bottom: top + 58 };
      if (left < 12 || rect.right > width - 12 || top < 72 || rect.bottom > height - 18 || blocked.some(b => overlaps(rect,b)) || placed.some(p => overlaps(rect,p.rect,18))) continue;
      placed.push({ star, point, rect, right });
    }
    const active = new Set(placed.map(p => p.star.meta[0]));
    for (const [key, node] of labels) if (!active.has(key)) { node.button.remove(); node.path.remove(); labels.delete(key); }
    for (const item of placed) {
      const [index,name,constellation,nature,distance] = item.star.meta;
      let entry = labels.get(index);
      if (!entry) {
        const button = document.createElement("button");
        button.type = "button"; button.className = "star-callout";
        button.innerHTML = `<strong>${name} · ${constellation}</strong><span>${nature} · ${distance} ly</span>`;
        button.setAttribute("aria-label", `Explore ${name}, ${nature}, ${distance} light-years away in ${constellation}`);
        button.addEventListener("click", () => { emit("star-annotation-clicked", { selected:name }); open(item.star); });
        const path = document.createElementNS(svg.namespaceURI,"path");
        svg.append(path); layer.append(button); entry = {button,path}; labels.set(index,entry);
      }
      Object.assign(entry.button.style,{left:`${item.rect.left}px`,top:`${item.rect.top}px`,width:`${item.rect.right-item.rect.left}px`});
      const endX = item.right ? item.rect.left : item.rect.right;
      const elbowX = item.right ? item.point.x + 25 : item.point.x - 25;
      const lineY = item.rect.bottom - 8;
      entry.path.setAttribute("d",`M ${item.point.x.toFixed(1)} ${item.point.y.toFixed(1)} L ${elbowX.toFixed(1)} ${lineY.toFixed(1)} L ${endX.toFixed(1)} ${lineY.toFixed(1)}`);
    }
    const signature = placed.map(item => item.star.meta[1]).join("|");
    if (signature !== lastSignature) {
      lastSignature = signature;
      emit("star-annotations-changed", { visible: placed.length, names: signature });
    } else if (window.__rnTelemetry?.enabled) {
      window.__rnTelemetry.annotations.visible = placed.length;
    }
  };
  requestAnimationFrame(layout);
  window.addEventListener("pagehide", () => { disposed = true; }, {once:true});
}
