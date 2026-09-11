import { markSent, pendingDeliveries } from '@/features/realtime/server';
import { subscribeDeliveries } from '@/features/realtime/delivery-stream';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

export async function GET(request: Request): Promise<Response> {
  const url = new URL(request.url);
  const visitorId = url.searchParams.get('visitor');
  const viewId = url.searchParams.get('view');
  if (!visitorId || !viewId) return new Response(null, { status: 404 });
  const encoder = new TextEncoder();
  let unsubscribe: (() => void) | null = null;
  let ping: ReturnType<typeof setInterval> | null = null;
  let draining = false;
  let again = false;
  let closed = false;
  const stream = new ReadableStream<Uint8Array>({
    async start(controller) {
      const write = (text: string) => {
        if (!closed) controller.enqueue(encoder.encode(text));
      };
      const drain = async () => {
        if (draining) {
          again = true;
          return;
        }
        draining = true;
        try {
          do {
            again = false;
            const rows = await pendingDeliveries(visitorId, viewId);
            for (const row of rows) write(`id: ${row.id}\ndata: ${JSON.stringify(row)}\n\n`);
            await markSent(rows.map((row) => row.id));
          } while (again && !closed);
        } catch {
          /* the next notification retries without marking rows sent */
        } finally {
          draining = false;
        }
      };
      write('retry: 3000\n\n');
      await drain();
      unsubscribe = subscribeDeliveries(() => {
        void drain();
      });
      ping = setInterval(() => {
        write(': ping\n\n');
        void drain();
      }, 20_000);
    },
    cancel() {
      closed = true;
      unsubscribe?.();
      if (ping) clearInterval(ping);
    },
  });
  return new Response(stream, {
    headers: {
      'Content-Type': 'text/event-stream; charset=utf-8',
      'Cache-Control': 'no-cache, no-transform',
      'X-Accel-Buffering': 'no',
    },
  });
}
