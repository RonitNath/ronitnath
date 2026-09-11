import { createReadStream } from 'node:fs';
import { basename } from 'node:path';
import { AUDIO_LIMITS } from '@isoastra/audio-core';
import { createFileTusServer } from '@isoastra/audio-node';
import { and, eq } from 'drizzle-orm';
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

export function memoTusServer() {
  if (globalTus.memoTus) return globalTus.memoTus;
  globalTus.memoTus = createFileTusServer({
    path: '/api/memos/upload',
    directory: memoUploadRoot(),
    maxBytes: AUDIO_LIMITS.uploadBytes,
    onIncomingRequest: async (_request, uploadId) => {
      if (uploadId) await authorizeUpload(uploadId);
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
          state: 'uploading', sourceKey, sourceMimeType: values.filetype, sourceBytes: 0,
        }).onConflictDoNothing({ target: [schema.voiceMemo.personId, schema.voiceMemo.localId] })
          .returning({ id: schema.voiceMemo.id });
        if (!inserted[0]) {
          const existing = await tx.select({ uploadId: schema.voiceMemo.uploadId }).from(schema.voiceMemo)
            .where(and(eq(schema.voiceMemo.personId, subject), eq(schema.voiceMemo.localId, values.localId))).limit(1);
          if (existing[0]?.uploadId !== upload.id) throw { status_code: 409, body: 'Recording identifier is already in use' };
          return;
        }
        await recordAudit(tx, { actorPersonId: principal.personId, command: 'create-voice-memo', targetKind: 'voice-memo', targetId: inserted[0].id });
        await emit(tx, { orgId: encodeId('person', subject), resourceKind: 'voice-memo', resourceId: encodeId('memo', inserted[0].id), kind: 'uploading' });
      });
      return { metadata: upload.metadata };
    },
    onUploadFinish: async (_request, upload) => {
      const { subject, values } = await subjectFrom(upload.metadata);
      if (!upload.storage?.path || upload.size === undefined || upload.offset !== upload.size) {
        throw { status_code: 409, body: 'Upload is incomplete' };
      }
      const prefix = memoPrefix(subject, values.localId);
      const sourceKey = `${prefix}/source/${basename(values.filename ?? 'recording')}`;
      const storage = memoStorage();
      if (!(await storage.stat(sourceKey))) {
        await storage.put(sourceKey, createReadStream(upload.storage.path), { contentType: values.filetype });
      }
      const memo = await database().transaction(async (tx) => {
        const row = (await tx.update(schema.voiceMemo).set({
          sourceKey, sourceBytes: upload.size!, uploadComplete: true,
          durableBytes: upload.size!, state: 'processing', updatedAt: new Date(),
        }).where(and(eq(schema.voiceMemo.personId, subject), eq(schema.voiceMemo.localId, values.localId), eq(schema.voiceMemo.uploadId, upload.id)))
          .returning({ id: schema.voiceMemo.id }))[0];
        if (!row) throw new Error('Memo finalization failed');
        await tx.insert(schema.voiceMemoJob).values({ memoId: row.id, kind: 'process' })
          .onConflictDoNothing({ target: schema.voiceMemoJob.memoId });
        await emit(tx, { orgId: encodeId('person', subject), resourceKind: 'voice-memo', resourceId: encodeId('memo', row.id), kind: 'processing' });
        return row;
      });
      return { headers: { 'x-memo-id': encodeId('memo', memo.id) } };
    },
    onDurableOffset: async (uploadId, offset) => {
      await database().transaction(async (tx) => {
        const memo = (await tx.update(schema.voiceMemo).set({ durableBytes: offset, updatedAt: new Date() })
          .where(and(eq(schema.voiceMemo.uploadId, uploadId), eq(schema.voiceMemo.uploadComplete, false)))
          .returning({ id: schema.voiceMemo.id, personId: schema.voiceMemo.personId }))[0];
        if (!memo) return;
        await emit(tx, { orgId: encodeId('person', memo.personId), resourceKind: 'voice-memo', resourceId: encodeId('memo', memo.id), kind: 'upload-progress' });
      });
    },
  });
  return globalTus.memoTus;
}
