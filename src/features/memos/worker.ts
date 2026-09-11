import { createReadStream } from 'node:fs';
import { mkdtemp, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { processAudio } from '@isoastra/audio-node';
import { and, eq, sql } from 'drizzle-orm';
import { database, schema } from '@/db/client';
import { emit } from '@/lib/fleet/events';
import { encodeId } from '@/lib/ids';
import { memoPrefix, memoStorage } from './storage';

type ClaimedJob = { memoId: number; kind: 'process' | 'delete'; attempts: number };

async function claim(): Promise<ClaimedJob | null> {
  return database().transaction(async (tx) => {
    const result = await tx.execute(sql`
      WITH candidate AS (
        SELECT memo_id FROM voice_memo_job
        WHERE (state = 'queued' AND available_at <= now())
           OR (state = 'running' AND lease_until < now())
        ORDER BY available_at, memo_id
        FOR UPDATE SKIP LOCKED LIMIT 1
      )
      UPDATE voice_memo_job AS job
      SET state = 'running', attempts = job.attempts + 1,
          lease_until = now() + interval '10 minutes', updated_at = now()
      FROM candidate WHERE job.memo_id = candidate.memo_id
      RETURNING job.memo_id AS "memoId", job.kind, job.attempts
    `);
    return (result.rows[0] as ClaimedJob | undefined) ?? null;
  });
}

async function filesBelow(root: string, directory = root): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const found: string[] = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) found.push(...await filesBelow(root, path));
    else found.push(path);
  }
  return found;
}

async function removeMemo(memoId: number) {
  const rows = await database().select().from(schema.voiceMemo).where(eq(schema.voiceMemo.id, memoId)).limit(1);
  const memo = rows[0];
  if (!memo) return;
  await memoStorage().deletePrefix(memoPrefix(memo.personId, memo.localId));
  await database().transaction(async (tx) => {
    await tx.delete(schema.voiceMemo).where(eq(schema.voiceMemo.id, memoId));
    await emit(tx, { orgId: encodeId('person', memo.personId), resourceKind: 'voice-memo', resourceId: encodeId('memo', memoId), kind: 'deleted' });
  });
}

async function processMemo(memoId: number) {
  const rows = await database().select().from(schema.voiceMemo).where(eq(schema.voiceMemo.id, memoId)).limit(1);
  const memo = rows[0];
  if (!memo || memo.state === 'deleting') return;
  const storage = memoStorage();
  const output = await mkdtemp(join(tmpdir(), `memo-${memoId}-`));
  try {
    const processed = await processAudio(storage.localPath(memo.sourceKey), output);
    await writeFile(join(output, 'master.m3u8'), [
      '#EXTM3U', '#EXT-X-VERSION:7',
      '#EXT-X-STREAM-INF:BANDWIDTH=36000,CODECS="mp4a.40.2"', 'hls-32/index.m3u8',
      '#EXT-X-STREAM-INF:BANDWIDTH=70000,CODECS="mp4a.40.2"', 'hls-64/index.m3u8', '',
    ].join('\n'));
    const prefix = `${memoPrefix(memo.personId, memo.localId)}/derived`;
    for (const path of await filesBelow(output)) {
      const key = `${prefix}/${relative(output, path)}`;
      if (!(await storage.stat(key))) await storage.put(key, createReadStream(path));
    }
    await database().transaction(async (tx) => {
      const ready = await tx.update(schema.voiceMemo).set({
        state: 'ready', durationMs: Math.round(processed.durationSeconds * 1_000),
        sourceFormat: processed.sourceFormat, sourceCodec: processed.sourceCodec,
        hlsMasterKey: `${prefix}/master.m3u8`, fallbackKey: `${prefix}/fallback.m4a`,
        waveformKey: `${prefix}/waveform.json`, failure: null, updatedAt: new Date(),
      }).where(and(eq(schema.voiceMemo.id, memoId), eq(schema.voiceMemo.state, 'processing')))
        .returning({ id: schema.voiceMemo.id });
      if (!ready[0]) return;
      await tx.delete(schema.voiceMemoJob).where(and(eq(schema.voiceMemoJob.memoId, memoId), eq(schema.voiceMemoJob.kind, 'process')));
      await emit(tx, { orgId: encodeId('person', memo.personId), resourceKind: 'voice-memo', resourceId: encodeId('memo', memoId), kind: 'ready' });
    });
  } finally {
    await rm(output, { recursive: true, force: true });
  }
}

async function fail(job: ClaimedJob, error: unknown) {
  const message = error instanceof Error ? error.message.slice(-2_000) : String(error).slice(-2_000);
  const exhausted = job.attempts >= 5;
  await database().transaction(async (tx) => {
    await tx.update(schema.voiceMemoJob).set({
      state: exhausted ? 'failed' : 'queued', error: message, leaseUntil: null,
      availableAt: new Date(Date.now() + Math.min(60_000, 2 ** job.attempts * 1_000)), updatedAt: new Date(),
    }).where(eq(schema.voiceMemoJob.memoId, job.memoId));
    await tx.update(schema.voiceMemo).set({ failure: message, ...(exhausted ? { state: 'failed' } : {}), updatedAt: new Date() })
      .where(eq(schema.voiceMemo.id, job.memoId));
  });
}

export async function workOne(): Promise<boolean> {
  const job = await claim();
  if (!job) return false;
  try {
    if (job.kind === 'delete') await removeMemo(job.memoId);
    else await processMemo(job.memoId);
  } catch (error) {
    await fail(job, error);
  }
  return true;
}

export async function runMemoWorker(signal?: AbortSignal) {
  while (!signal?.aborted) {
    if (!(await workOne())) await new Promise((resolve) => setTimeout(resolve, 1_000));
  }
}
