import { createHash } from 'node:crypto';
import { basename } from 'node:path';
import { AUDIO_LIMITS } from '@isoastra/audio-core';
import { FileSystemUploadReader, createFileTusServer } from '@isoastra/audio-node';
import { and, eq, ne } from 'drizzle-orm';
import { z } from 'zod';
import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import { currentPrincipal } from '@/features/auth/principal';
import { emit } from '@/lib/fleet/events';
import { encodeId, tryDecodeId } from '@/lib/ids';
import { memoPrefix, memoStorage, memoUploadRoot } from './storage';

const metadata = z.object({
  localId: z.string().uuid(),
  user: z.string().min(1),
  filename: z.string().max(300).optional(),
  filetype: z.string().min(1).max(150),
  title: z.string().trim().min(1).max(200).optional(),
});

function denied(status = 404): never {
  throw { status_code: status, body: status === 401 ? 'Sign in required' : 'Upload not found' };
}

async function subjectFrom(values: Record<string, string | null> | undefined) {
  const parsed = metadata.safeParse(values);
  if (!parsed.success) throw { status_code: 400, body: 'Invalid upload metadata' };
  const principal = await currentPrincipal();
  if (!principal) denied(401);
  const subject = tryDecodeId('person', parsed.data.user);
  if (subject === null || (subject !== principal.personId && !principal.isOperator)) denied();
  return { principal, subject, values: parsed.data };
}

async function authorizeUpload(id: string) {
  if (!/^[A-Za-z0-9_-]+$/.test(id)) denied();
  const principal = await currentPrincipal();
  if (!principal) denied(401);
  const memo = (await database().select({ personId: schema.voiceMemo.personId }).from(schema.voiceMemo).where(eq(schema.voiceMemo.uploadId, id)).limit(1))[0];
  if (!memo || (memo.personId !== principal.personId && !principal.isOperator)) denied();
}

const globalTus = globalThis as unknown as { memoTus?: ReturnType<typeof createFileTusServer> };

async function publishSource(uploadId: string, size: number) {
  const memo = (await database().select().from(schema.voiceMemo).where(eq(schema.voiceMemo.uploadId, uploadId)).limit(1))[0];
  if (!memo || memo.state === 'deleting') throw { status_code: 404, body: 'Upload not found' };
  if (memo.durableBytes !== size) throw { status_code: 409, body: 'Upload receipt does not match its declared length' };
  const storage = memoStorage();
  const reader = new FileSystemUploadReader(memoUploadRoot());
  let sha256 = memo.sourceSha256;
  if (!(await storage.stat(memo.sourceKey))) {
    const hash = createHash('sha256');
    const source = await reader.read(uploadId, 0, size - 1);
    async function* hashed() {
      for await (const chunk of source) { const bytes = Buffer.from(chunk); hash.update(bytes); yield bytes; }
    }
    await storage.put(memo.sourceKey, hashed(), { contentType: memo.sourceMimeType });
    sha256 = hash.digest('hex');
  } else if (!sha256) {
    const hash = createHash('sha256');
    for await (const chunk of await storage.read(memo.sourceKey)) hash.update(Buffer.from(chunk));
    sha256 = hash.digest('hex');
  }
  await database().transaction(async (tx) => {
    const current = (await tx.select().from(schema.voiceMemo).where(eq(schema.voiceMemo.id, memo.id)).limit(1))[0];
    if (!current || current.state === 'deleting' || current.durableBytes !== size) throw new Error('Memo finalization was fenced');
    if (current.uploadComplete && current.uploadFinalizedAt) return;
    const streaming = current.processingMode === 'streaming';
    const jobKind = current.sourceMimeType.includes('webm') ? 'ingest' : 'process';
    await tx.update(schema.voiceMemo).set({
      sourceBytes: size, sourceSha256: sha256, uploadComplete: true, uploadFinalizedAt: new Date(),
      state: streaming ? current.state : 'processing',
      processingMode: streaming ? 'streaming' : current.sourceMimeType.includes('webm') ? 'probing' : 'after-upload',
      updatedAt: new Date(),
    }).where(and(eq(schema.voiceMemo.id, memo.id), ne(schema.voiceMemo.state, 'deleting')));
    await tx.insert(schema.voiceMemoJob).values({ memoId: memo.id, kind: jobKind, state: 'queued' })
      .onConflictDoUpdate({ target: schema.voiceMemoJob.memoId, set: streaming
        ? { updatedAt: new Date() }
        : { kind: jobKind, state: 'queued', availableAt: new Date(), leaseUntil: null, leaseToken: null, generation: null, updatedAt: new Date() } });
    await emit(tx, { orgId: encodeId('person', memo.personId), resourceKind: 'voice-memo', resourceId: encodeId('memo', memo.id), kind: streaming ? 'upload-complete' : 'processing' });
  });
  return memo;
}

export async function finalizeMemoUpload(uploadId: string) {
  await authorizeUpload(uploadId);
  const upload = await memoTusServer().staging.getUpload(uploadId);
  if (upload.size === undefined || upload.offset !== upload.size || upload.size <= 0) throw { status_code: 409, body: 'Upload is incomplete' };
  return publishSource(uploadId, upload.size);
}

export function memoTusServer() {
  if (globalTus.memoTus) return globalTus.memoTus;
  globalTus.memoTus = createFileTusServer({
    path: '/api/memos/upload',
    directory: memoUploadRoot(),
    maxBytes: AUDIO_LIMITS.uploadBytes,
    onIncomingRequest: async (request, uploadId) => {
      if (request.method !== 'POST' && uploadId) await authorizeUpload(uploadId);
      else if (!(await currentPrincipal())) denied(401);
    },
    onUploadCreate: async (_request, upload) => {
      const { principal, subject, values } = await subjectFrom(upload.metadata);
      const prefix = memoPrefix(subject, values.localId);
      const sourceKey = `${prefix}/source/${basename(values.filename ?? 'recording')}`;
      await database().transaction(async (tx) => {
        const inserted = await tx.insert(schema.voiceMemo).values({
          personId: subject, localId: values.localId, uploadId: upload.id,
          title: values.title ?? `Voice memo ${new Date().toLocaleDateString('en-US')}`,
          state: 'uploading', processingMode: values.filetype.includes('webm') ? 'probing' : 'after-upload', sourceKey, sourceMimeType: values.filetype, sourceBytes: 0,
        }).onConflictDoNothing({ target: [schema.voiceMemo.personId, schema.voiceMemo.localId] })
          .returning({ id: schema.voiceMemo.id });
        if (!inserted[0]) {
          const existing = await tx.select({ uploadId: schema.voiceMemo.uploadId }).from(schema.voiceMemo)
            .where(and(eq(schema.voiceMemo.personId, subject), eq(schema.voiceMemo.localId, values.localId))).limit(1);
          if (existing[0]?.uploadId !== upload.id) throw { status_code: 409, body: 'Recording identifier is already in use' };
          return;
        }
        await recordAudit(tx, { actorPersonId: principal.personId, command: 'create-voice-memo', targetKind: 'voice-memo', targetId: inserted[0].id });
        if (values.filetype.includes('webm')) await tx.insert(schema.voiceMemoJob).values({ memoId: inserted[0].id, kind: 'ingest', state: 'queued' });
        await emit(tx, { orgId: encodeId('person', subject), resourceKind: 'voice-memo', resourceId: encodeId('memo', inserted[0].id), kind: 'uploading' });
      });
      return { metadata: upload.metadata };
    },
    onUploadFinish: async (_request, upload) => {
      await subjectFrom(upload.metadata);
      if (upload.size === undefined || upload.offset !== upload.size) {
        throw { status_code: 409, body: 'Upload is incomplete' };
      }
      const memo = await publishSource(upload.id, upload.size);
      return { headers: { 'x-memo-id': encodeId('memo', memo.id) } };
    },
    getDurableOffset: async (uploadId) => {
      const row = (await database().select({ offset: schema.voiceMemo.durableBytes }).from(schema.voiceMemo).where(eq(schema.voiceMemo.uploadId, uploadId)).limit(1))[0];
      return row?.offset ?? null;
    },
    onDurableOffset: async (uploadId, expectedOffset, offset) => {
      await database().transaction(async (tx) => {
        const memo = (await tx.update(schema.voiceMemo).set({ durableBytes: offset, updatedAt: new Date() })
          .where(and(eq(schema.voiceMemo.uploadId, uploadId), eq(schema.voiceMemo.durableBytes, expectedOffset), eq(schema.voiceMemo.uploadComplete, false), ne(schema.voiceMemo.state, 'deleting')))
          .returning({ id: schema.voiceMemo.id, personId: schema.voiceMemo.personId }))[0];
        if (!memo) throw Object.assign(new Error('Upload offset changed'), { expectedOffset });
        await emit(tx, { orgId: encodeId('person', memo.personId), resourceKind: 'voice-memo', resourceId: encodeId('memo', memo.id), kind: 'upload-progress' });
      });
      return true;
    },
  });
  return globalTus.memoTus;
}
