import { eq } from 'drizzle-orm';
import { z } from 'zod';
import { database, schema } from '@/db/client';
import { recordAudit } from '@/features/auth/audit';
import { currentPrincipal } from '@/features/auth/principal';
import { authorizedMemo } from '@/features/memos/media';

export async function PUT(request: Request, { params }: { params: Promise<{ memo: string }> }) {
  const principal = await currentPrincipal();
  if (!principal) return new Response('Sign in required', { status: 401 });
  const { memo: publicId } = await params;
  const memo = await authorizedMemo(publicId, principal);
  if (!memo) return new Response('Not found', { status: 404 });
  const parsed = z.object({ seconds: z.number().finite().min(0).max(60 * 60 * 24 * 30) }).safeParse(await request.json().catch(() => null));
  if (!parsed.success) return new Response('Invalid position', { status: 400 });
  const milliseconds = Math.round(parsed.data.seconds * 1_000);
  await database().transaction(async (tx) => {
    await tx.update(schema.voiceMemo).set({ playbackPositionMs: milliseconds, updatedAt: new Date() }).where(eq(schema.voiceMemo.id, memo.id));
    await recordAudit(tx, { actorPersonId: principal.personId, command: 'save-voice-memo-position', targetKind: 'voice-memo', targetId: memo.id, payload: { milliseconds } });
  });
  return new Response(null, { status: 204 });
}
