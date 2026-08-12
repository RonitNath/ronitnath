/** Lazily imported full-screen celestial atlas. No code in this file is fetched
 * until a named star or the explicit Load StarScape control is activated. */

const TAU = Math.PI * 2;
const STRIDE = 20, HEADER = 8;
let active = null;

function octDecode(x, y) {
  let px=x/32767, py=y/32767, pz=1-Math.abs(px)-Math.abs(py);
  if(pz<0){const ox=px;px=(1-Math.abs(py))*Math.sign(ox||1);py=(1-Math.abs(ox))*Math.sign(py||1);}
  const n=Math.hypot(px,py,pz); return [px/n,py/n,pz/n];
}

function bpRpColor(value) {
  // Compact perceptual approximation: blue at negative BP-RP, warm at 2+.
  const t=Math.max(0,Math.min(1,(value+0.4)/2.8));
  return [Math.round(190+65*t),Math.round(216+26*t),Math.round(255-115*t)];
}

function decodeLod(buffer, start=0) {
  const view=new DataView(buffer), count=Math.floor((buffer.byteLength-start)/16), out=new Float32Array(count*7);
  for(let i=0;i<count;i++){const o=start+i*16,p=octDecode(view.getInt16(o+8,true),view.getInt16(o+10,true)),c=bpRpColor(view.getInt16(o+14,true)/1000),q=i*7;out[q]=p[0];out[q+1]=p[1];out[q+2]=p[2];out[q+3]=view.getInt16(o+12,true)/1000;out[q+4]=c[0];out[q+5]=c[1];out[q+6]=c[2];}
  return out;
}

function validLodAsset(buffer) {
  if(buffer.byteLength<12)return false;const bytes=new Uint8Array(buffer,0,8);
  return String.fromCharCode(...bytes)==="GDR3LOD1";
}

function telemetry(name, fields = {}) {
  const t = window.__rnTelemetry;
  if (!t?.enabled) return;
  t.explorer ||= {}; Object.assign(t.explorer, fields);
  t.events ||= []; t.events.push({atMs:Date.now(),name});
  if (t.events.length > 100) t.events.splice(0,t.events.length-100);
}

function vectorToAngles([x,y,z]) { return {ra:Math.atan2(y,x), dec:Math.asin(Math.max(-1,Math.min(1,z)))}; }
function anglesToVector(ra,dec) { const c=Math.cos(dec); return [c*Math.cos(ra),c*Math.sin(ra),Math.sin(dec)]; }

async function catalog() {
  let buffer;
  if(window.__rnBrightCatalog instanceof Uint8Array){buffer=window.__rnBrightCatalog.buffer.slice(window.__rnBrightCatalog.byteOffset,window.__rnBrightCatalog.byteOffset+window.__rnBrightCatalog.byteLength);}
  else {const response=await fetch("/stars/bright.bin");buffer=await response.arrayBuffer();}
  const view = new DataView(buffer);
  const count=view.getUint32(4,true), stars=new Array(count);
  for(let i=0;i<count;i++){const o=HEADER+i*STRIDE;stars[i]={p:[view.getFloat32(o,true),view.getFloat32(o+4,true),view.getFloat32(o+8,true)],m:view.getFloat32(o+12,true),c:[view.getUint8(o+16),view.getUint8(o+17),view.getUint8(o+18)]};}
  return stars;
}

function shell() {
  const node=document.createElement("section"); node.className="starscape-explorer"; node.setAttribute("role","dialog"); node.setAttribute("aria-modal","true"); node.setAttribute("aria-label","StarScape celestial atlas");
  node.innerHTML=`<canvas aria-label="Interactive star map"></canvas><div class="explorer-loading">Preparing the sky…</div>
  <header class="explorer-bar"><strong>StarScape</strong><span class="explorer-target"></span><span class="explorer-spacer"></span>
  <button data-action="zoom-out" aria-label="Zoom out">−</button><output class="explorer-fov">115°</output><button data-action="zoom-in" aria-label="Zoom in">+</button>
  <button data-action="pause">Pause</button><button data-action="close" aria-label="Close StarScape">Close</button></header>
  <nav class="explorer-links" aria-label="Ronit Nath links"><strong>Ronit Nath</strong><a href="https://isoastra.com" target="_blank" rel="noopener">Isoastra</a><a href="https://github.com/RonitNath" target="_blank" rel="me noopener">GitHub</a><a href="https://instagram.com/ronit_nath" target="_blank" rel="me noopener">Instagram</a><a href="https://linkedin.com/in/ronitn" target="_blank" rel="me noopener">LinkedIn</a><a href="mailto:ronit@isoastra.com">Email</a></nav>
  <p class="explorer-help">Drag to explore · scroll or pinch to zoom · arrow keys pan · Space pauses</p>`;
  document.body.append(node); return node;
}

export async function openStarScape(options) {
  if(active) return; const openedAt=Date.now(), node=shell(), canvas=node.querySelector("canvas"), ctx=canvas.getContext("2d",{alpha:false});
  document.documentElement.classList.add("starscape-explorer-open"); document.body.style.overflow="hidden";
  window.dispatchEvent(new Event("starscape-explorer-state"));
  const api=window.__rnTrack, selected=options.selected;
  let viewerSim=api?.syncedSimTimeMs?.(options.epochMs,options.clientMountMs,Date.now())||Date.now(), lastFrame=performance.now(), lastPaint=0;
  const currentMatrix=api?.viewMatrix?Array.from(api.viewMatrix(viewerSim)):null;
  let fov=selected?20:115, center=selected?vectorToAngles(selected.pos):currentMatrix?vectorToAngles([currentMatrix[2],currentMatrix[5],currentMatrix[8]]):{ra:0,dec:Math.PI/2}, tracking=!!selected, paused=false, dragging=false, lastPointer=null, pinch=null, raf=0, disposed=false;
  const pointers=new Map();
  const state=api.viewerState={open:true,paused:false,simMs:viewerSim,rate:60};
  active={node}; telemetry("starscape-explorer-loading",{chunkReadyAtMs:Date.now(),selected:selected?.meta?.[1]||null});
  const stars=await catalog(); node.querySelector(".explorer-loading").remove(); node.querySelector(".explorer-target").textContent=selected?`Tracking ${selected.meta[1]}`:"Celestial atlas";
  let manifest=null, mid=null, midLoadedAt=0, loadingMid=false;
  const tileCache=new Map(), tileControllers=new Map(), tileFailures=new Map();
  fetch("/stars/lod/manifest.json").then(r=>r.ok?r.json():null).then(value=>{if(value?.version!==1||!Array.isArray(value.tiles))return;manifest=value;requestLod();telemetry("starscape-lod-manifest",{tileCount:value.tiles.length});}).catch(()=>{});

  const wantedTiles=()=>{
    if(!manifest||fov>25)return[];const ra=((center.ra%TAU)+TAU)%TAU/TAU*manifest.raBins,dec=(center.dec+Math.PI/2)/Math.PI*manifest.decBins;
    const halfV=fov/2,halfH=Math.atan(Math.tan(halfV*Math.PI/180)*innerWidth/innerHeight)*180/Math.PI,cosDec=Math.max(.2,Math.cos(center.dec));
    const rx=Math.max(1,Math.ceil(halfH/(360/manifest.raBins)/cosDec)+1),ry=Math.max(1,Math.ceil(halfV/(180/manifest.decBins))+1),ids=[];
    for(let y=Math.floor(dec)-ry;y<=Math.floor(dec)+ry;y++){if(y<0||y>=manifest.decBins)continue;for(let x=Math.floor(ra)-rx;x<=Math.floor(ra)+rx;x++)ids.push(y*manifest.raBins+((x%manifest.raBins)+manifest.raBins)%manifest.raBins);}
    return [...new Set(ids)];
  };
  const requestLod=()=>{
    if(fov<=60&&!mid&&!loadingMid){loadingMid=true;fetch("/stars/lod/g9.bin").then(r=>r.arrayBuffer()).then(b=>{if(!validLodAsset(b))throw new Error("invalid G9 asset");mid=decodeLod(b,12);midLoadedAt=performance.now();telemetry("starscape-mid-lod-ready",{midStars:mid.length/7});}).catch(()=>{}).finally(()=>loadingMid=false);}
    const wanted=wantedTiles();
    for(const [id,controller] of tileControllers)if(!wanted.includes(id)){controller.abort();tileControllers.delete(id);}
    const missing=wanted.filter(id=>!tileCache.has(id)&&!tileControllers.has(id)&&Date.now()-(tileFailures.get(id)||0)>30000).slice(0,Math.max(0,6-tileControllers.size));
    for(const id of missing){const info=manifest.tiles[id];if(!info?.bytes)continue;const controller=new AbortController();tileControllers.set(id,controller);
      fetch("/stars/lod/g12.bin",{headers:{Range:`bytes=${info.offset}-${info.offset+info.bytes-1}`},signal:controller.signal}).then(async r=>{let b=await r.arrayBuffer();if(r.status===200&&b.byteLength>info.bytes)b=b.slice(info.offset,info.offset+info.bytes);tileCache.delete(id);tileCache.set(id,{data:decodeLod(b),used:performance.now(),bytes:b.byteLength,loadedAt:performance.now()});let bytes=0;for(const tile of tileCache.values())bytes+=tile.bytes;while(tileCache.size>128||bytes>64*1024*1024){const oldest=tileCache.keys().next().value;bytes-=tileCache.get(oldest).bytes;tileCache.delete(oldest);}telemetry("starscape-deep-tile-ready",{residentTiles:tileCache.size,residentBytes:bytes});}).catch(error=>{if(error?.name!=="AbortError")tileFailures.set(id,Date.now());}).finally(()=>{tileControllers.delete(id);if(!disposed)requestLod();});
    }
  };

  const resize=()=>{const d=Math.max(1,devicePixelRatio),w=innerWidth,h=innerHeight;canvas.width=w*d;canvas.height=h*d;canvas.style.width=`${w}px`;canvas.style.height=`${h}px`;ctx.setTransform(d,0,0,d,0,0);}; resize(); addEventListener("resize",resize);
  const setFov=value=>{fov=Math.max(1,Math.min(115,value));state.fov=fov;node.querySelector(".explorer-fov").value=`${fov.toFixed(fov<10?1:0)}°`;state.rate=Math.max(1,60*fov/115);requestLod();telemetry("starscape-explorer-zoom",{fov,rate:state.rate});};
  const pan=(dx,dy)=>{tracking=false;node.querySelector(".explorer-target").textContent="Celestial atlas";center.ra=(center.ra-dx*fov*Math.PI/180/innerWidth*1.6)%TAU;center.dec=Math.max(-Math.PI/2,Math.min(Math.PI/2,center.dec+dy*fov*Math.PI/180/innerHeight*1.6));requestLod();};
  const close=()=>{if(disposed)return;disposed=true;cancelAnimationFrame(raf);for(const controller of tileControllers.values())controller.abort();tileCache.clear();removeEventListener("resize",resize);node.remove();document.documentElement.classList.remove("starscape-explorer-open");window.dispatchEvent(new Event("starscape-explorer-state"));document.body.style.overflow="";delete api.viewerState;active=null;telemetry("starscape-explorer-closed",{closedAtMs:Date.now(),residentTiles:0});options.onClose?.();};
  const togglePause=()=>{paused=!paused;state.paused=paused;node.querySelector('[data-action="pause"]').textContent=paused?"Resume Time":"Pause";telemetry(paused?"starscape-explorer-paused":"starscape-explorer-resumed",{paused});};
  node.addEventListener("click",e=>{const a=e.target.closest("[data-action]")?.dataset.action;if(a==="close")close();if(a==="pause")togglePause();if(a==="zoom-in")setFov(fov/1.5);if(a==="zoom-out")setFov(fov*1.5);});
  canvas.addEventListener("wheel",e=>{e.preventDefault();setFov(fov*Math.exp(e.deltaY*.001));},{passive:false});
  canvas.addEventListener("pointerdown",e=>{canvas.setPointerCapture(e.pointerId);pointers.set(e.pointerId,[e.clientX,e.clientY]);dragging=true;lastPointer=[e.clientX,e.clientY];if(pointers.size===2){const [a,b]=[...pointers.values()];pinch={distance:Math.hypot(a[0]-b[0],a[1]-b[1]),fov};}});
  canvas.addEventListener("pointermove",e=>{if(!pointers.has(e.pointerId))return;pointers.set(e.pointerId,[e.clientX,e.clientY]);if(pointers.size===2&&pinch){const [a,b]=[...pointers.values()],distance=Math.hypot(a[0]-b[0],a[1]-b[1]);setFov(pinch.fov*pinch.distance/Math.max(1,distance));return;}if(dragging&&lastPointer){pan(e.clientX-lastPointer[0],e.clientY-lastPointer[1]);lastPointer=[e.clientX,e.clientY];}});
  const release=e=>{pointers.delete(e.pointerId);pinch=null;if(!pointers.size){dragging=false;lastPointer=null;}};
  canvas.addEventListener("pointerup",release);canvas.addEventListener("pointercancel",release);
  node.addEventListener("keydown",e=>{if(e.key==="Escape")close();if(e.key===" "){e.preventDefault();togglePause();}if(e.key.startsWith("Arrow")){e.preventDefault();pan(e.key==="ArrowLeft"?-30:e.key==="ArrowRight"?30:0,e.key==="ArrowUp"?-30:30);}});
  node.tabIndex=-1; node.focus(); setFov(fov);

  const draw=now=>{if(disposed)return;const dt=Math.min(100,now-lastFrame);lastFrame=now;if(!paused){viewerSim+=dt*state.rate;state.simMs=viewerSim;if(!tracking)center.ra=(center.ra+dt*state.rate*TAU/86164090.5)%TAU;}
    if(now-lastPaint<50){raf=requestAnimationFrame(draw);return;}lastPaint=now;
    const w=innerWidth,h=innerHeight,light=document.documentElement.dataset.theme==="light";ctx.fillStyle=light?"#e5ddd0":"#030509";ctx.fillRect(0,0,w,h);
    const forward=anglesToVector(center.ra,center.dec),right=[-Math.sin(center.ra),Math.cos(center.ra),0],up=[-Math.sin(center.dec)*Math.cos(center.ra),-Math.sin(center.dec)*Math.sin(center.ra),Math.cos(center.dec)];
    const focal=1/Math.tan(fov*Math.PI/360),aspect=w/h;let count=0;
    const paint=(p,m,c)=>{const z=p[0]*forward[0]+p[1]*forward[1]+p[2]*forward[2];if(z<=0)return;const x=(p[0]*right[0]+p[1]*right[1]+p[2]*right[2])/z*focal/aspect,y=(p[0]*up[0]+p[1]*up[1]+p[2]*up[2])/z*focal;if(Math.abs(x)>1.05||Math.abs(y)>1.05)return;const px=(x+1)*w/2,py=(1-y)*h/2,r=Math.max(.4,Math.min(5,1.2*Math.pow(10,-.12*m)*Math.sqrt(115/fov)));ctx.beginPath();ctx.fillStyle=`rgba(${c[0]},${c[1]},${c[2]},${Math.min(1,.22+1.1*Math.pow(10,-.4*m))})`;ctx.arc(px,py,r,0,TAU);ctx.fill();count++;};
    for(const s of stars)paint(s.p,s.m,s.c);
    const paintLod=(data,minMagnitude)=>{for(let i=0;i<data.length;i+=7)if(data[i+3]>minMagnitude)paint([data[i],data[i+1],data[i+2]],data[i+3],[data[i+4],data[i+5],data[i+6]]);};
    // Magnitude bands are disjoint, preventing Gaia/base cross-match stars
    // from drawing on top of themselves: base <=6.5, mid (6.5,9], deep >9.
    const reduced=matchMedia("(prefers-reduced-motion: reduce)").matches;if(fov<=60&&mid){ctx.globalAlpha=reduced?1:Math.min(1,(now-midLoadedAt)/350);paintLod(mid,6.5);ctx.globalAlpha=1;}if(fov<=25)for(const id of wantedTiles()){const tile=tileCache.get(id);if(tile){tile.used=now;ctx.globalAlpha=reduced?1:Math.min(1,(now-tile.loadedAt)/350);paintLod(tile.data,9);ctx.globalAlpha=1;}}
    state.visibleStars=count;if(!state.firstFrameAtMs){state.firstFrameAtMs=Date.now();telemetry("starscape-explorer-first-frame",{firstFrameAtMs:state.firstFrameAtMs,visibleStars:count,fov,rate:state.rate,paused});}else if(window.__rnTelemetry?.enabled){Object.assign(window.__rnTelemetry.explorer,{visibleStars:count,fov,rate:state.rate,paused});}raf=requestAnimationFrame(draw);};
  raf=requestAnimationFrame(draw); telemetry("starscape-explorer-ready",{readyAtMs:Date.now(),loadMs:Date.now()-openedAt,starCount:stars.length});
}
