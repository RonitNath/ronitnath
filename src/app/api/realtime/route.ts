import { z } from 'zod';
import { acknowledgeView } from '@/features/realtime/acknowledgment';
import { disconnectView, registerView } from '@/features/realtime/server';

const envelope = z.discriminatedUnion('action', [
  z.object({ action: z.enum(['register', 'heartbeat']), view: z.unknown() }),
  z.object({
    action: z.literal('acknowledge'),
    visitorId: z.string().uuid(),
    viewId: z.string().uuid(),
    acknowledgment: z.unknown(),
  }),
  z.object({
    action: z.literal('disconnect'),
    visitorId: z.string().uuid(),
    viewId: z.string().uuid(),
  }),
]);

export async function POST(request: Request): Promise<Response> {
  const parsed = envelope.safeParse(await request.json().catch(() => null));
  if (!parsed.success)
    return Response.json({ error: 'invalid realtime report' }, { status: 400 });
  if (parsed.data.action === 'register' || parsed.data.action === 'heartbeat') {
    try {
      return Response.json(await registerView(parsed.data.view));
    } catch {
      return Response.json(
        { error: 'invalid or unauthorized realtime report' },
        { status: 400 },
      );
    }
  }
  if (parsed.data.action === 'acknowledge') {
    const ok = await acknowledgeView(
      parsed.data.visitorId,
      parsed.data.viewId,
      parsed.data.acknowledgment,
    ).catch((error: unknown) => {
      console.error(
        JSON.stringify({
          level: 'error',
          event: 'realtime.acknowledgment.failed',
          message: error instanceof Error ? error.message : String(error),
        }),
      );
      return false;
    });
    return new Response(null, { status: ok ? 204 : 404 });
  }
  if (parsed.data.action === 'disconnect') {
    await disconnectView(parsed.data.visitorId, parsed.data.viewId);
    return new Response(null, { status: 204 });
  }
  return new Response(null, { status: 400 });
}
