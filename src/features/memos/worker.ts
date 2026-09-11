import { createReadStream, createWriteStream } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { mkdtemp, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { pipeline } from 'node:stream/promises';
import { FileSystemUploadReader, processAudio, processGrowingAudio } from '@isoastra/audio-node';
import { and, eq, inArray, sql } from 'drizzle-orm';
import { database, schema } from '@/db/client';
import { emit } from '@/lib/fleet/events';
import { encodeId } from '@/lib/ids';
import { memoPrefix, memoStorage, memoUploadRoot } from './storage';

type ClaimedJob = { memoId: number; kind: 'process' | 'delete'; attempts: number; leaseToken: string };
const active = new Map<number, Promise<void>>();
const LIVE_CONCURRENCY = 2;

async function claim(): Promise<ClaimedJob | null> {
  const token = randomUUID();
  return database().transaction(async (tx) => {
    const result = await tx.execute(sql`
      WITH candidate AS (SELECT memo_id FROM voice_memo_job
        WHERE (state='queued' AND available_at<=now()) OR (state='running' AND lease_until<now())
        ORDER BY available_at,memo_id FOR UPDATE SKIP LOCKED LIMIT 1)
      UPDATE voice_memo_job job SET state='running',attempts=job.attempts+1,lease_token=${token}::uuid,
        lease_until=now()+interval '2 minutes',updated_at=now()
      FROM candidate WHERE job.memo_id=candidate.memo_id
      RETURNING job.memo_id AS "memoId",job.kind,job.attempts,job.lease_token AS "leaseToken"`);
    return (result.rows[0] as ClaimedJob | undefined) ?? null;
  });
}

async function fenced(job: ClaimedJob) {
  const row = await database().select({ token: schema.voiceMemoJob.leaseToken }).from(schema.voiceMemoJob)
    .where(and(eq(schema.voiceMemoJob.memoId, job.memoId), eq(schema.voiceMemoJob.state, 'running'))).limit(1);
  return row[0]?.token === job.leaseToken;
}

function renew(job: ClaimedJob) {
  const timer = setInterval(() => void database().update(schema.voiceMemoJob)
    .set({ leaseUntil: new Date(Date.now() + 120_000), updatedAt: new Date() })
    .where(and(eq(schema.voiceMemoJob.memoId, job.memoId), eq(schema.voiceMemoJob.leaseToken, job.leaseToken))), 30_000);
  return () => clearInterval(timer);
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
  if (!(await fenced(job))) return;
  await database().transaction(async (tx) => {
    await tx.delete(schema.voiceMemo).where(eq(schema.voiceMemo.id, job.memoId));
    await emit(tx, { orgId: encodeId('person', memo.personId), resourceKind: 'voice-memo', resourceId: encodeId('memo', job.memoId), kind: 'deleted' });
  });
}

async function processMemo(job: ClaimedJob) {
  const memo = (await database().select().from(schema.voiceMemo).where(eq(schema.voiceMemo.id, job.memoId)).limit(1))[0];
  if (!memo || memo.state === 'deleting' || !memo.uploadComplete) return;
  const storage = memoStorage();
  const root = await mkdtemp(join(tmpdir(), `memo-${job.memoId}-`));
  try {
    const source = join(root, 'source');
    await pipeline(await storage.read(memo.sourceKey), createWriteStream(source));
    const out = join(root, 'out');
    const processed = await processAudio(source, out);
    const generation = memo.generation + 1;
    const prefix = `${memoPrefix(memo.personId, memo.localId)}/derived/g${generation}`;
    await writeFile(join(out, 'master.m3u8'), ['#EXTM3U','#EXT-X-VERSION:7','#EXT-X-STREAM-INF:BANDWIDTH=36000,CODECS="mp4a.40.2"','hls-32/index.m3u8','#EXT-X-STREAM-INF:BANDWIDTH=70000,CODECS="mp4a.40.2"','hls-64/index.m3u8',''].join('\n'));
    for (const path of await filesBelow(out)) await storage.put(`${prefix}/${relative(out, path)}`, createReadStream(path));
    if (!(await fenced(job))) return;
    await database().transaction(async (tx) => {
      const ready = await tx.update(schema.voiceMemo).set({ state: 'ready', generation, processingMode: 'complete', durationMs: Math.round(processed.durationSeconds*1000), playableThroughMs: Math.round(processed.durationSeconds*1000), sourceFormat: processed.sourceFormat, sourceCodec: processed.sourceCodec, hlsMasterKey: `${prefix}/master.m3u8`, fallbackKey: `${prefix}/fallback.m4a`, waveformKey: `${prefix}/waveform.json`, failure: null, updatedAt: new Date() })
        .where(and(eq(schema.voiceMemo.id, job.memoId), eq(schema.voiceMemo.state, 'processing'))).returning({ id: schema.voiceMemo.id });
      if (!ready[0]) return;
      await tx.delete(schema.voiceMemoJob).where(and(eq(schema.voiceMemoJob.memoId, job.memoId), eq(schema.voiceMemoJob.leaseToken, job.leaseToken)));
      await emit(tx, { orgId: encodeId('person', memo.personId), resourceKind: 'voice-memo', resourceId: encodeId('memo', job.memoId), kind: 'ready' });
    });
  } finally { await rm(root, { recursive: true, force: true }); }
}

async function* liveBytes(memoId: number, uploadId: string, signal: AbortSignal) {
  const reader = new FileSystemUploadReader(memoUploadRoot());
  let offset = 0;
  while (!signal.aborted) {
    const memo = (await database().select({ durable: schema.voiceMemo.durableBytes, complete: schema.voiceMemo.uploadComplete, state: schema.voiceMemo.state }).from(schema.voiceMemo).where(eq(schema.voiceMemo.id, memoId)).limit(1))[0];
    if (!memo || memo.state === 'deleting') throw new Error('Live processing was cancelled');
    if (memo.durable > offset) {
      const end = Math.min(memo.durable, offset + 1024*1024)-1;
      for await (const chunk of await reader.read(uploadId, offset, end)) yield Buffer.from(chunk);
      offset = end+1;
    } else if (memo.complete) return;
    else await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

function waveformPayload(peaks: readonly number[]) {
  const reduce = (step: number) => Array.from({ length: Math.ceil(peaks.length/step) }, (_, i) => Math.max(...peaks.slice(i*step,(i+1)*step)));
  return { samplesPerSecond: 50, levels: [{ step: 1, peaks }, { step: 10, peaks: reduce(10) }, { step: 100, peaks: reduce(100) }] };
}

async function startLive(memoId: number) {
  const memo = (await database().select().from(schema.voiceMemo).where(eq(schema.voiceMemo.id, memoId)).limit(1))[0];
  if (!memo?.uploadId || !memo.sourceMimeType.includes('webm') || memo.uploadComplete || memo.state === 'deleting') return;
  const generation = memo.generation+1;
  const claimed = await database().update(schema.voiceMemo).set({ generation, processingMode: 'streaming', updatedAt: new Date() })
    .where(and(eq(schema.voiceMemo.id,memoId),eq(schema.voiceMemo.generation,memo.generation),eq(schema.voiceMemo.uploadComplete,false))).returning({id:schema.voiceMemo.id});
  if (!claimed[0]) return;
  const abort = new AbortController();
  const root = await mkdtemp(join(tmpdir(), `memo-live-${memoId}-`));
  const storage = memoStorage();
  const prefix = `${memoPrefix(memo.personId,memo.localId)}/derived/g${generation}`;
  const init = new Set<number>();
  let lastWaveform = 0;
  try {
    const result = await processGrowingAudio(liveBytes(memoId,memo.uploadId,abort.signal),root,{
      signal: abort.signal,
      onSegment: async (segment) => {
        const current = (await database().select({generation:schema.voiceMemo.generation,state:schema.voiceMemo.state}).from(schema.voiceMemo).where(eq(schema.voiceMemo.id,memoId)).limit(1))[0];
        if (!current || current.generation!==generation || current.state==='deleting') { abort.abort(); return; }
        const dir=`hls-${segment.rendition}`;
        if (!init.has(segment.rendition)) { await storage.put(`${prefix}/${dir}/init.mp4`,createReadStream(segment.initPath)); init.add(segment.rendition); }
        const key=`${prefix}/${dir}/s${String(segment.sequence).padStart(5,'0')}.m4s`;
        if (!(await storage.stat(key))) await storage.put(key,createReadStream(segment.path));
        await database().transaction(async (tx) => {
          await tx.insert(schema.voiceMemoSegment).values({memoId,generation,rendition:segment.rendition,sequence:segment.sequence,durationMs:segment.durationMs,key}).onConflictDoNothing();
          const boundary=await tx.execute(sql`SELECT min(total)::int AS ms FROM (SELECT rendition,sum(duration_ms)::int total FROM voice_memo_segment WHERE memo_id=${memoId} AND generation=${generation} GROUP BY rendition) r HAVING count(*)=2`);
          const through=Number(boundary.rows[0]?.ms??0);
          if (through>0) {
            await tx.update(schema.voiceMemo).set({state:'playable',playableThroughMs:through,hlsMasterKey:`${prefix}/master.m3u8`,updatedAt:new Date()}).where(and(eq(schema.voiceMemo.id,memoId),eq(schema.voiceMemo.generation,generation)));
            await emit(tx,{orgId:encodeId('person',memo.personId),resourceKind:'voice-memo',resourceId:encodeId('memo',memoId),kind:'playable'});
          }
        });
      },
      onWaveform: async (peaks) => {
        if (peaks.length-lastWaveform<50) return;
        lastWaveform=peaks.length;
        const path=join(root,`waveform-${peaks.length}.json`); await writeFile(path,JSON.stringify(waveformPayload(peaks)));
        const key=`${prefix}/waveform-${peaks.length}.json`; await storage.put(key,createReadStream(path));
        await database().update(schema.voiceMemo).set({waveformKey:key,updatedAt:new Date()}).where(and(eq(schema.voiceMemo.id,memoId),eq(schema.voiceMemo.generation,generation)));
      }
    });
    const fallbackKey=`${prefix}/fallback.m4a`, waveformKey=`${prefix}/waveform.json`;
    await storage.put(fallbackKey,createReadStream(result.fallbackPath));
    await writeFile(result.waveformPath,JSON.stringify(waveformPayload(result.peaks)));
    await storage.put(waveformKey,createReadStream(result.waveformPath));
    await database().transaction(async (tx) => {
      const ready=await tx.update(schema.voiceMemo).set({state:'ready',processingMode:'complete',durationMs:Math.round(result.peaks.length/50*1000),playableThroughMs:Math.round(result.peaks.length/50*1000),fallbackKey,waveformKey,failure:null,updatedAt:new Date()}).where(and(eq(schema.voiceMemo.id,memoId),eq(schema.voiceMemo.generation,generation),eq(schema.voiceMemo.uploadComplete,true))).returning({id:schema.voiceMemo.id});
      if (ready[0]) { await tx.delete(schema.voiceMemoJob).where(eq(schema.voiceMemoJob.memoId,memoId)); await emit(tx,{orgId:encodeId('person',memo.personId),resourceKind:'voice-memo',resourceId:encodeId('memo',memoId),kind:'ready'}); }
    });
  } catch (error) {
    const current=(await database().select({complete:schema.voiceMemo.uploadComplete}).from(schema.voiceMemo).where(eq(schema.voiceMemo.id,memoId)).limit(1))[0];
    if (current&&!current.complete) await database().update(schema.voiceMemo).set({state:'uploading',processingMode:'after-upload',updatedAt:new Date()}).where(and(eq(schema.voiceMemo.id,memoId),eq(schema.voiceMemo.generation,generation)));
    else throw error;
  } finally { abort.abort(); await rm(root,{recursive:true,force:true}); }
}

async function startLiveIngests() {
  if (active.size>=LIVE_CONCURRENCY) return;
  const rows=await database().select({id:schema.voiceMemo.id}).from(schema.voiceMemo).where(and(inArray(schema.voiceMemo.state,['uploading','playable']),eq(schema.voiceMemo.uploadComplete,false),sql`${schema.voiceMemo.durableBytes}>0`,sql`${schema.voiceMemo.sourceMimeType} LIKE '%webm%'`,sql`${schema.voiceMemo.processingMode} IN ('probing','streaming')`)).limit(LIVE_CONCURRENCY-active.size);
  for (const {id} of rows) if (!active.has(id)) { const task=startLive(id).catch((error)=>console.error('live memo processing failed',{memoId:id,error})).finally(()=>active.delete(id)); active.set(id,task); }
}

async function fail(job: ClaimedJob,error:unknown) {
  const message=error instanceof Error?error.message.slice(-2000):String(error).slice(-2000), exhausted=job.attempts>=5;
  await database().transaction(async(tx)=>{
    await tx.update(schema.voiceMemoJob).set({state:exhausted?'failed':'queued',error:message,leaseUntil:null,leaseToken:null,availableAt:new Date(Date.now()+Math.min(60000,2**job.attempts*1000)),updatedAt:new Date()}).where(and(eq(schema.voiceMemoJob.memoId,job.memoId),eq(schema.voiceMemoJob.leaseToken,job.leaseToken)));
    await tx.update(schema.voiceMemo).set({failure:'Audio preparation failed. Retry to process the saved original.',...(exhausted?{state:'failed'}:{}),updatedAt:new Date()}).where(eq(schema.voiceMemo.id,job.memoId));
  });
}

export async function workOne() {
  const job=await claim(); if(!job)return false; const stop=renew(job);
  try { if(job.kind==='delete')await removeMemo(job);else await processMemo(job); } catch(error){await fail(job,error);} finally{stop();}
  return true;
}

export async function runMemoWorker(signal?:AbortSignal){
  while(!signal?.aborted){await startLiveIngests();if(!(await workOne()))await new Promise((resolve)=>setTimeout(resolve,250));}
  await Promise.allSettled(active.values());
}
