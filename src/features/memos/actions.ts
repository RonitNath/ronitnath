'use server';

import { and, eq, sql } from 'drizzle-orm';
import { revalidatePath } from 'next/cache';
import { z } from 'zod';
import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import { currentPrincipal } from '@/features/auth/principal';
import { needsReauth, REAUTH_REQUIRED } from '@/features/platform/reauth';
import { emit } from '@/lib/fleet/events';
import { encodeId, tryDecodeId } from '@/lib/ids';
import { userPath } from '@/lib/paths';

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

async function context(form: FormData) {
  const principal = await currentPrincipal();
  const user = field(form, 'user');
  if (!principal || !user) return null;
  const subject = tryDecodeId('person', user);
  if (subject === null) return null;
  if (subject !== principal.personId && !principal.isOperator) return null;
  return { principal, subject, user, path: userPath(user, 'memos') };
}

async function mutate(form: FormData, command: string, values: Record<string, unknown>) {
  const ctx = await context(form);
  const memoId = tryDecodeId('memo', field(form, 'memo'));
  if (!ctx || memoId === null) return { error: 'That memo is not here.' };
  const changed = await database().transaction(async (tx) => {
    const rows = await tx.update(schema.voiceMemo).set({ ...values, updatedAt: new Date() })
      .where(and(eq(schema.voiceMemo.id, memoId), eq(schema.voiceMemo.personId, ctx.subject)))
      .returning({ id: schema.voiceMemo.id });
    if (!rows[0]) return false;
    await recordAudit(tx, { actorPersonId: ctx.principal.personId, command, targetKind: 'voice-memo', targetId: memoId });
    await emit(tx, { orgId: encodeId('person', ctx.subject), resourceKind: 'voice-memo', resourceId: encodeId('memo', memoId), kind: command });
    return true;
  });
  if (!changed) return { error: 'That memo is not here.' };
  revalidatePath(ctx.path);
  return { notice: 'Done.' };
}

export async function renameMemo(_previous: unknown, form: FormData) {
  const title = z.string().trim().min(1).max(200).safeParse(field(form, 'title'));
  if (!title.success) return { error: 'Give the memo a title.' };
  return mutate(form, 'rename-voice-memo', { title: title.data });
}

export async function trashMemo(_previous: unknown, form: FormData) {
  return mutate(form, 'trash-voice-memo', { trashedAt: new Date() });
}

export async function restoreMemo(_previous: unknown, form: FormData) {
  return mutate(form, 'restore-voice-memo', { trashedAt: null });
}

export async function retryMemo(_previous: unknown, form: FormData) {
  const ctx = await context(form);
  const memoId = tryDecodeId('memo', field(form, 'memo'));
  if (!ctx || memoId === null) return { error: 'That memo is not here.' };
  const retried = await database().transaction(async (tx) => {
    const rows = await tx.update(schema.voiceMemo).set({ state: 'processing', failure: null, updatedAt: new Date() })
      .where(and(eq(schema.voiceMemo.id, memoId), eq(schema.voiceMemo.personId, ctx.subject), eq(schema.voiceMemo.state, 'failed')))
      .returning({ id: schema.voiceMemo.id });
    if (!rows[0]) return false;
    await tx.insert(schema.voiceMemoJob).values({ memoId, kind: 'process', state: 'queued' })
      .onConflictDoUpdate({ target: schema.voiceMemoJob.memoId, set: { kind: 'process', state: 'queued', attempts: 0, availableAt: new Date(), leaseUntil: null, error: null, updatedAt: new Date() } });
    await recordAudit(tx, { actorPersonId: ctx.principal.personId, command: 'retry-voice-memo', targetKind: 'voice-memo', targetId: memoId });
    await emit(tx, { orgId: encodeId('person', ctx.subject), resourceKind: 'voice-memo', resourceId: encodeId('memo', memoId), kind: 'processing' });
    return true;
  });
  if (!retried) return { error: 'That memo is not retryable.' };
  revalidatePath(ctx.path);
  return { notice: 'Processing queued.' };
}

export async function deleteMemoPermanently(_previous: unknown, form: FormData) {
  const ctx = await context(form);
  const memoId = tryDecodeId('memo', field(form, 'memo'));
  if (!ctx || memoId === null) return { error: 'That memo is not here.' };
  if (needsReauth(ctx.principal)) return { error: REAUTH_REQUIRED, reauth: true };
  const deleted = await database().transaction(async (tx) => {
    const rows = await tx.update(schema.voiceMemo).set({ state: 'deleting', updatedAt: new Date() })
      .where(and(eq(schema.voiceMemo.id, memoId), eq(schema.voiceMemo.personId, ctx.subject), sql`${schema.voiceMemo.trashedAt} IS NOT NULL`))
      .returning({ id: schema.voiceMemo.id });
    if (!rows[0]) return false;
    await tx.insert(schema.voiceMemoJob).values({ memoId, kind: 'delete', state: 'queued' })
      .onConflictDoUpdate({ target: schema.voiceMemoJob.memoId, set: { kind: 'delete', state: 'queued', availableAt: new Date(), leaseUntil: null, error: null, updatedAt: new Date() } });
    await recordAudit(tx, { actorPersonId: ctx.principal.personId, command: 'delete-voice-memo', targetKind: 'voice-memo', targetId: memoId });
    await emit(tx, { orgId: encodeId('person', ctx.subject), resourceKind: 'voice-memo', resourceId: encodeId('memo', memoId), kind: 'deleting' });
    return true;
  });
  if (!deleted) return { error: 'Trash the memo before deleting it.' };
  revalidatePath(ctx.path);
  return { notice: 'Deletion queued.' };
}
