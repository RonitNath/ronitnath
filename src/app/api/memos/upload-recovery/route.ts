import { and, eq } from 'drizzle-orm';
import { z } from 'zod';
import { database, schema } from '@/db/client';
import { currentPrincipal } from '@/features/auth/principal';
import { tryDecodeId } from '@/lib/ids';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(request: Request) {
  const principal = await currentPrincipal();
  if (!principal) return new Response('Sign in required', { status: 401 });
  const url = new URL(request.url);
  const personId = tryDecodeId('person', url.searchParams.get('user') ?? '');
  const localId = z.string().uuid().safeParse(url.searchParams.get('localId'));
  if (personId === null || !localId.success || (personId !== principal.personId && !principal.isOperator)) return new Response('Not found', { status: 404 });
  const memo = (await database().select({
    uploadId: schema.voiceMemo.uploadId, durableBytes: schema.voiceMemo.durableBytes,
    finalizedAt: schema.voiceMemo.uploadFinalizedAt, state: schema.voiceMemo.state,
  }).from(schema.voiceMemo).where(and(eq(schema.voiceMemo.personId, personId), eq(schema.voiceMemo.localId, localId.data))).limit(1))[0];
  if (!memo?.uploadId || memo.state === 'deleting') return new Response(null, { status: 404 });
  return Response.json({
    uploadUrl: new URL(`/api/memos/upload/${memo.uploadId}`, request.url).toString(),
    acknowledgedBytes: memo.durableBytes,
    serverFinalized: memo.finalizedAt !== null,
  }, { headers: { 'cache-control': 'private, no-store' } });
}
