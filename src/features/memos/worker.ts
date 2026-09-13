import { createReadStream, createWriteStream } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { pipeline } from 'node:stream/promises';
import { FileSystemUploadReader, processAudio, processGrowingAudio } from '@isoastra/audio-node';
import { and, eq, ne, sql } from 'drizzle-orm';
import { database, schema } from '@/db/client';
import { emit, type Transaction } from '@/lib/fleet/events';
import { encodeId } from '@/lib/ids';
import { memoPrefix, memoStorage, memoUploadRoot } from './storage';
import { memoTusServer } from './upload';
import { canSwitchGeneration, commonPlayableBoundary } from './publication';

type ClaimedJob = { memoId: number; kind: 'ingest' | 'process' | 'delete'; attempts: number; leaseToken: string; generation: number | null };
const LIVE_CONCURRENCY = 2;
class LiveInputIdle extends Error {}

function delay(milliseconds: number, signal?: AbortSignal) {
  if (signal?.aborted) return Promise.reject(signal.reason);
  return new Promise<void>((resolve, reject) => {
    const timer = setTimeout(done, milliseconds);
    function done() {
      signal?.removeEventListener('abort', cancel);
      resolve();
    }
    function cancel() {
      clearTimeout(timer);
      signal?.removeEventListener('abort', cancel);
      reject(signal?.reason);
    }
    signal?.addEventListener('abort', cancel, { once: true });
  });
}

async function claim(kind: ClaimedJob['kind']): Promise<ClaimedJob | null> {
  const token = randomUUID();
  return database().transaction(async (tx) => {
    const result = await tx.execute(sql`
      WITH candidate AS (SELECT memo_id FROM voice_memo_job
        WHERE kind=${kind} AND ((state='queued' AND available_at<=now()) OR (state='running' AND lease_until<now()))
        ORDER BY available_at,memo_id FOR UPDATE SKIP LOCKED LIMIT 1)
      UPDATE voice_memo_job job SET state='running',attempts=job.attempts+1,lease_token=${token}::uuid,
        lease_until=now()+interval '2 minutes',updated_at=now()
      FROM candidate WHERE job.memo_id=candidate.memo_id
      RETURNING job.memo_id AS "memoId",job.kind,job.attempts,job.lease_token AS "leaseToken",job.generation`);
    const claimed = result.rows[0] as ClaimedJob | undefined;
    if (!claimed || kind === 'delete') return claimed ?? null;
    const allocated = await tx.execute(sql`
      UPDATE voice_memo SET processing_generation=next_generation,next_generation=next_generation+1,updated_at=now()
      WHERE id=${claimed.memoId} AND state<>'deleting'
      RETURNING processing_generation AS generation`);
    const generation = Number(allocated.rows[0]?.generation ?? 0);
    if (!generation) {
      await tx.update(schema.voiceMemoJob).set({ state: 'queued', leaseToken: null, leaseUntil: null }).where(eq(schema.voiceMemoJob.memoId, claimed.memoId));
      return null;
    }
    await tx.update(schema.voiceMemoJob).set({ generation }).where(and(eq(schema.voiceMemoJob.memoId, claimed.memoId), eq(schema.voiceMemoJob.leaseToken, claimed.leaseToken)));
    return { ...claimed, generation };
  });
}

async function fenced(job: ClaimedJob) {
  const row = await database().select({ token: schema.voiceMemoJob.leaseToken, leaseUntil: schema.voiceMemoJob.leaseUntil, generation: schema.voiceMemoJob.generation }).from(schema.voiceMemoJob)
    .where(and(eq(schema.voiceMemoJob.memoId, job.memoId), eq(schema.voiceMemoJob.state, 'running'))).limit(1);
  return row[0]?.token === job.leaseToken && row[0].generation === job.generation && !!row[0].leaseUntil && row[0].leaseUntil.getTime() > Date.now();
}

async function fencedInTransaction(tx: Transaction, job: ClaimedJob) {
  const result = await tx.execute(sql`SELECT 1 FROM voice_memo_job
    WHERE memo_id=${job.memoId} AND state='running' AND lease_token=${job.leaseToken}::uuid
      AND lease_until>now() AND generation IS NOT DISTINCT FROM ${job.generation}
    FOR UPDATE`);
  return result.rows.length === 1;
}

function renew(job: ClaimedJob, abort: AbortController) {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const tick = async () => {
    if (stopped) return;
    try {
      const renewed = await database().update(schema.voiceMemoJob)
        .set({ leaseUntil: new Date(Date.now() + 120_000), updatedAt: new Date() })
        .where(and(eq(schema.voiceMemoJob.memoId, job.memoId), eq(schema.voiceMemoJob.leaseToken, job.leaseToken), eq(schema.voiceMemoJob.state, 'running'), sql`${schema.voiceMemoJob.leaseUntil}>now()`))
        .returning({ id: schema.voiceMemoJob.memoId });
      if (!renewed[0]) abort.abort(new Error('Processing lease was lost'));
    } catch (error) { abort.abort(error); }
    if (!stopped) timer = setTimeout(() => void tick(), 30_000);
  };
  timer = setTimeout(() => void tick(), 30_000);
  return () => { stopped = true; if (timer) clearTimeout(timer); };
}

async function filesBelow(root: string, directory = root): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) found.push(...await filesBelow(root, path)); else found.push(path);
  }
  return found;
}

async function removeMemo(job: ClaimedJob) {
  if (!(await fenced(job))) return;
  const memo = (await database().select().from(schema.voiceMemo).where(eq(schema.voiceMemo.id, job.memoId)).limit(1))[0];
  if (!memo) return;
  await memoStorage().deletePrefix(memoPrefix(memo.personId, memo.localId));
  if (memo.uploadId) await memoTusServer().staging.remove(memo.uploadId).catch(() => {});
  if (!(await fenced(job))) return;
  await database().transaction(async (tx) => {
    if (!(await fencedInTransaction(tx, job))) return;
    await tx.delete(schema.voiceMemo).where(eq(schema.voiceMemo.id, job.memoId));
    await emit(tx, { orgId: encodeId('person', memo.personId), resourceKind: 'voice-memo', resourceId: encodeId('memo', job.memoId), kind: 'deleted' });
  });
}

async function processMemo(job: ClaimedJob, signal: AbortSignal) {
  const memo = (await database().select().from(schema.voiceMemo).where(eq(schema.voiceMemo.id, job.memoId)).limit(1))[0];
  if (!memo || memo.state === 'deleting') return;
  if (!memo.uploadComplete) throw new LiveInputIdle('The upload is not complete');
  const storage = memoStorage();
  const root = await mkdtemp(join(tmpdir(), `memo-${job.memoId}-`));
  try {
    const source = join(root, 'source');
    await pipeline(await storage.read(memo.sourceKey), createWriteStream(source));
    const out = join(root, 'out');
    const processed = await processAudio(source, out, { signal });
    const generation = job.generation;
    if (!generation) throw new Error('Processing generation was not allocated');
    const prefix = `${memoPrefix(memo.personId, memo.localId)}/derived/g${generation}`;
    await writeFile(join(out, 'master.m3u8'), ['#EXTM3U','#EXT-X-VERSION:7','#EXT-X-STREAM-INF:BANDWIDTH=36000,CODECS="mp4a.40.2"','hls-32/index.m3u8','#EXT-X-STREAM-INF:BANDWIDTH=70000,CODECS="mp4a.40.2"','hls-64/index.m3u8',''].join('\n'));
    for (const path of await filesBelow(out)) await storage.put(`${prefix}/${relative(out, path)}`, createReadStream(path));
    const segments:Array<{memoId:number;generation:number;rendition:32|64;sequence:number;durationMs:number;key:string}>=[];
    for(const rendition of [32,64] as const){
      const lines=(await readFile(join(out,`hls-${rendition}/index.m3u8`),'utf8')).split('\n');
      for(let index=0;index<lines.length-1;index+=1){
        if(!lines[index]?.startsWith('#EXTINF:'))continue;
        const match=/^s(\d+)\.m4s$/.exec(lines[index+1]?.trim()??'');
        if(match)segments.push({memoId:job.memoId,generation,rendition,sequence:Number(match[1]),durationMs:Math.round(Number(lines[index]!.slice(8).replace(',',''))*1000),key:`${prefix}/hls-${rendition}/${lines[index+1]!.trim()}`});
      }
    }
    const waveform=JSON.parse(await readFile(join(out,'waveform.json'),'utf8')) as {peaks:number[]};
    const waveformTiles:Array<{memoId:number;generation:number;level:number;startPeak:number;peakCount:number;key:string}>=[];
    for(const level of [1,10,100]){
      const levelPeaks=level===1?waveform.peaks:Array.from({length:Math.ceil(waveform.peaks.length/level)},(_,index)=>Math.max(0,...waveform.peaks.slice(index*level,(index+1)*level)));
      for(let startPeak=0;startPeak<levelPeaks.length;startPeak+=3200){
        const peaks=levelPeaks.slice(startPeak,startPeak+3200),path=join(out,`tile-${level}-${startPeak}.json`),key=`${prefix}/waveform/l${level}-${String(startPeak).padStart(9,'0')}.json`;
        await writeFile(path,JSON.stringify({samplesPerSecond:50/level,startPeak,peaks}));
        await storage.put(key,createReadStream(path));
        waveformTiles.push({memoId:job.memoId,generation,level,startPeak,peakCount:peaks.length,key});
      }
    }
    if (!(await fenced(job))) return;
    await database().transaction(async (tx) => {
      if (!(await fencedInTransaction(tx, job))) return;
      if(segments.length)await tx.insert(schema.voiceMemoSegment).values(segments).onConflictDoNothing();
      if(waveformTiles.length)await tx.insert(schema.voiceMemoWaveformTile).values(waveformTiles).onConflictDoNothing();
      const ready = await tx.update(schema.voiceMemo).set({ state: 'ready', generation, processingGeneration: null, publicationRevision: sql`${schema.voiceMemo.publicationRevision}+1`, processingMode: 'complete', durationMs: Math.round(processed.durationSeconds*1000), playableThroughMs: Math.round(processed.durationSeconds*1000), sourceFormat: processed.sourceFormat, sourceCodec: processed.sourceCodec, hlsMasterKey: `${prefix}/master.m3u8`, fallbackKey: `${prefix}/fallback.m4a`, waveformKey: `${prefix}/waveform.json`, failure: null, updatedAt: new Date() })
        .where(and(eq(schema.voiceMemo.id, job.memoId), eq(schema.voiceMemo.processingGeneration, generation), ne(schema.voiceMemo.state, 'deleting'), eq(schema.voiceMemo.uploadComplete, true))).returning({ id: schema.voiceMemo.id });
      if (!ready[0]) return;
      await tx.delete(schema.voiceMemoJob).where(and(eq(schema.voiceMemoJob.memoId, job.memoId), eq(schema.voiceMemoJob.leaseToken, job.leaseToken)));
      await emit(tx, { orgId: encodeId('person', memo.personId), resourceKind: 'voice-memo', resourceId: encodeId('memo', job.memoId), kind: 'ready' });
    });
  } finally { await rm(root, { recursive: true, force: true }); }
}

async function* liveBytes(memoId: number, uploadId: string, signal: AbortSignal) {
  const reader = new FileSystemUploadReader(memoUploadRoot());
  let offset = 0;
  let lastProgress = Date.now();
  while (!signal.aborted) {
    const memo = (await database().select({ durable: schema.voiceMemo.durableBytes, complete: schema.voiceMemo.uploadComplete, state: schema.voiceMemo.state }).from(schema.voiceMemo).where(eq(schema.voiceMemo.id, memoId)).limit(1))[0];
    if (!memo || memo.state === 'deleting') throw new Error('Live processing was cancelled');
    if (memo.durable > offset) {
      const end = Math.min(memo.durable, offset + 1024*1024)-1;
      for await (const chunk of await reader.read(uploadId, offset, end)) yield Buffer.from(chunk);
      offset = end+1;
      lastProgress = Date.now();
    } else if (memo.complete) return;
    else {
      if (Date.now()-lastProgress>=60_000) throw new LiveInputIdle('Live upload is idle');
      await delay(100, signal);
    }
  }
}

function waveformPayload(peaks: readonly number[]) {
  const reduce = (step: number) => Array.from({ length: Math.ceil(peaks.length/step) }, (_, i) => Math.max(...peaks.slice(i*step,(i+1)*step)));
  return { samplesPerSecond: 50, levels: [{ step: 1, peaks }, { step: 10, peaks: reduce(10) }, { step: 100, peaks: reduce(100) }] };
}

async function startLive(job: ClaimedJob, abort: AbortController) {
  const memoId = job.memoId;
  const memo = (await database().select().from(schema.voiceMemo).where(eq(schema.voiceMemo.id, memoId)).limit(1))[0];
  if (!memo?.uploadId || !memo.sourceMimeType.includes('webm') || memo.state === 'deleting' || !job.generation) throw new Error('Live processing input is unavailable');
  const generation = job.generation;
  await database().update(schema.voiceMemo).set({ processingMode: 'streaming', updatedAt: new Date() })
    .where(and(eq(schema.voiceMemo.id,memoId),eq(schema.voiceMemo.processingGeneration,generation),ne(schema.voiceMemo.state,'deleting')));
  const root = await mkdtemp(join(tmpdir(), `memo-live-${memoId}-`));
  const storage = memoStorage();
  const prefix = `${memoPrefix(memo.personId,memo.localId)}/derived/g${generation}`;
  const init = new Set<number>();
  try {
    const result = await processGrowingAudio(liveBytes(memoId,memo.uploadId,abort.signal),root,{
      signal: abort.signal,
      onSegment: async (segment) => {
        if (!(await fenced(job))) { abort.abort(new Error('Processing lease was lost')); return; }
        const dir=`hls-${segment.rendition}`;
        if (!init.has(segment.rendition)) { await storage.put(`${prefix}/${dir}/init.mp4`,createReadStream(segment.initPath)); init.add(segment.rendition); }
        const key=`${prefix}/${dir}/s${String(segment.sequence).padStart(5,'0')}.m4s`;
        if (!(await storage.stat(key))) await storage.put(key,createReadStream(segment.path));
        await database().transaction(async (tx) => {
          if (!(await fencedInTransaction(tx, job))) { abort.abort(new Error('Processing lease was lost')); return; }
          await tx.insert(schema.voiceMemoSegment).values({memoId,generation,rendition:segment.rendition,sequence:segment.sequence,durationMs:segment.durationMs,key}).onConflictDoNothing();
          const rows=await tx.select({rendition:schema.voiceMemoSegment.rendition,sequence:schema.voiceMemoSegment.sequence,durationMs:schema.voiceMemoSegment.durationMs}).from(schema.voiceMemoSegment).where(and(eq(schema.voiceMemoSegment.memoId,memoId),eq(schema.voiceMemoSegment.generation,generation))).orderBy(schema.voiceMemoSegment.sequence);
          const through=commonPlayableBoundary(rows).durationMs;
          if (canSwitchGeneration(memo.generation,generation,memo.playableThroughMs,through)) {
            await tx.update(schema.voiceMemo).set({state:'playable',generation,publicationRevision:sql`${schema.voiceMemo.publicationRevision}+1`,playableThroughMs:through,hlsMasterKey:`${prefix}/master.m3u8`,updatedAt:new Date()}).where(and(eq(schema.voiceMemo.id,memoId),eq(schema.voiceMemo.processingGeneration,generation),ne(schema.voiceMemo.state,'deleting')));
            await emit(tx,{orgId:encodeId('person',memo.personId),resourceKind:'voice-memo',resourceId:encodeId('memo',memoId),kind:'playable'});
          }
        });
      },
      onWaveformTile: async (startPeak, peaks) => {
        if (!(await fenced(job)) || peaks.length===0) return;
        const path=join(root,`waveform-${startPeak}.json`); await writeFile(path,JSON.stringify({samplesPerSecond:50,startPeak,peaks}));
        const key=`${prefix}/waveform/l1-${String(startPeak).padStart(9,'0')}.json`; await storage.put(key,createReadStream(path));
        await database().transaction(async(tx)=>{
          if (!(await fencedInTransaction(tx,job))) return;
          await tx.insert(schema.voiceMemoWaveformTile).values({memoId,generation,level:1,startPeak,peakCount:peaks.length,key}).onConflictDoNothing();
          await tx.update(schema.voiceMemo).set({waveformKey:`${prefix}/waveform`,publicationRevision:sql`${schema.voiceMemo.publicationRevision}+1`,updatedAt:new Date()}).where(and(eq(schema.voiceMemo.id,memoId),eq(schema.voiceMemo.processingGeneration,generation),eq(schema.voiceMemo.generation,generation),ne(schema.voiceMemo.state,'deleting')));
        });
      }
    });
    const fallbackKey=`${prefix}/fallback.m4a`, waveformKey=`${prefix}/waveform.json`;
    await storage.put(fallbackKey,createReadStream(result.fallbackPath));
    await writeFile(result.waveformPath,JSON.stringify(waveformPayload(result.peaks)));
    await storage.put(waveformKey,createReadStream(result.waveformPath));
    await database().transaction(async (tx) => {
      if (!(await fencedInTransaction(tx,job))) return;
      const ready=await tx.update(schema.voiceMemo).set({state:'ready',generation,processingGeneration:null,publicationRevision:sql`${schema.voiceMemo.publicationRevision}+1`,processingMode:'complete',durationMs:Math.round(result.peaks.length/50*1000),playableThroughMs:Math.round(result.peaks.length/50*1000),fallbackKey,waveformKey,failure:null,sourceFormat:'webm',sourceCodec:'opus',updatedAt:new Date()}).where(and(eq(schema.voiceMemo.id,memoId),eq(schema.voiceMemo.processingGeneration,generation),ne(schema.voiceMemo.state,'deleting'),eq(schema.voiceMemo.uploadComplete,true))).returning({id:schema.voiceMemo.id});
      if (ready[0]) { await tx.delete(schema.voiceMemoJob).where(and(eq(schema.voiceMemoJob.memoId,memoId),eq(schema.voiceMemoJob.leaseToken,job.leaseToken))); await emit(tx,{orgId:encodeId('person',memo.personId),resourceKind:'voice-memo',resourceId:encodeId('memo',memoId),kind:'ready'}); }
    });
  } finally { abort.abort(); await rm(root,{recursive:true,force:true}); }
}

async function fail(job: ClaimedJob,error:unknown) {
  const message=error instanceof Error?error.message.slice(-2000):String(error).slice(-2000), exhausted=job.attempts>=5;
  await database().transaction(async(tx)=>{
    const idle=error instanceof LiveInputIdle;
    const changed=await tx.update(schema.voiceMemoJob).set({state:idle?'queued':exhausted?'failed':'queued',attempts:idle?sql`greatest(${schema.voiceMemoJob.attempts}-1,0)`:job.attempts,error:idle?null:message,leaseUntil:null,leaseToken:null,generation:null,availableAt:new Date(Date.now()+(idle?5_000:Math.min(60_000,2**job.attempts*1000))),updatedAt:new Date()}).where(and(eq(schema.voiceMemoJob.memoId,job.memoId),eq(schema.voiceMemoJob.leaseToken,job.leaseToken))).returning({id:schema.voiceMemoJob.memoId});
    if(!changed[0])return;
    const memo=(await tx.select({hls:schema.voiceMemo.hlsMasterKey,state:schema.voiceMemo.state}).from(schema.voiceMemo).where(eq(schema.voiceMemo.id,job.memoId)).limit(1))[0];
    if(!memo||memo.state==='deleting')return;
    await tx.update(schema.voiceMemo).set({processingGeneration:null,...(!idle?{failure:'Audio preparation failed. Retry to process the saved original.'}:{}),...(!idle&&exhausted&&!memo.hls?{state:'failed'}:{}),updatedAt:new Date()}).where(and(eq(schema.voiceMemo.id,job.memoId),eq(schema.voiceMemo.processingGeneration,job.generation!)));
  });
}

async function release(job: ClaimedJob) {
  await database().transaction(async(tx)=>{
    const changed=await tx.update(schema.voiceMemoJob).set({state:'queued',attempts:sql`greatest(${schema.voiceMemoJob.attempts}-1,0)`,leaseUntil:null,leaseToken:null,generation:null,availableAt:new Date(),updatedAt:new Date()})
      .where(and(eq(schema.voiceMemoJob.memoId,job.memoId),eq(schema.voiceMemoJob.leaseToken,job.leaseToken))).returning({id:schema.voiceMemoJob.memoId});
    if(changed[0]&&job.generation)await tx.update(schema.voiceMemo).set({processingGeneration:null,updatedAt:new Date()})
      .where(and(eq(schema.voiceMemo.id,job.memoId),eq(schema.voiceMemo.processingGeneration,job.generation)));
  });
}

export async function reconcileMemoMedia(now = Date.now()) {
  const storage=memoStorage(),objects=await storage.list('people'),memos=await database().select({personId:schema.voiceMemo.personId,localId:schema.voiceMemo.localId,generation:schema.voiceMemo.generation,processingGeneration:schema.voiceMemo.processingGeneration}).from(schema.voiceMemo);
  const known=new Map(memos.map((memo)=>[`${memo.personId}:${memo.localId}`,memo]));
  for(const object of objects){
    if(now-object.modifiedAt.getTime()<10*60_000)continue;
    const match=/^people\/(\d+)\/memos\/([^/]+)(?:\/derived\/g(\d+))?\//.exec(object.key);
    if(!match)continue;
    const memo=known.get(`${Number(match[1])}:${match[2]}`),generation=match[3]?Number(match[3]):null;
    if(!memo||generation!==null&&(generation!==memo.generation&&generation!==memo.processingGeneration))await storage.delete(object.key);
  }
}

async function execute(job: ClaimedJob, outerSignal?: AbortSignal) {
  const abort=new AbortController();
  const forward=()=>abort.abort(outerSignal?.reason);
  outerSignal?.addEventListener('abort',forward,{once:true});
  const stop=renew(job,abort);
  try {
    if(job.kind==='delete')await removeMemo(job);
    else if(job.kind==='ingest')await startLive(job,abort);
    else await processMemo(job,abort.signal);
  } catch(error){if(outerSignal?.aborted)await release(job);else await fail(job,error);}
  finally{stop();outerSignal?.removeEventListener('abort',forward);abort.abort();}
}

export async function workOne() {
  const job=await claim('delete')??await claim('process'); if(!job)return false;
  await execute(job);
  return true;
}

export async function runMemoWorker(signal?:AbortSignal){
  const loop=async(kind:'ingest'|'batch')=>{
    while(!signal?.aborted){
      const job=kind==='ingest'?await claim('ingest'):await claim('delete')??await claim('process');
      if(job)await execute(job,signal);else await delay(250,signal).catch(()=>{});
    }
  };
  const reconcile=async()=>{while(!signal?.aborted){await reconcileMemoMedia().catch((error)=>console.error('memo media reconciliation failed',{error}));await delay(60_000,signal).catch(()=>{});}};
  await Promise.allSettled([...Array.from({length:LIVE_CONCURRENCY},()=>loop('ingest')),loop('batch'),reconcile()]);
}
