import { z } from 'zod';
import { finalizeMemoUpload } from '@/features/memos/upload';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const input = z.object({ uploadUrl: z.string().min(1) });

export async function POST(request: Request) {
  const parsed = input.safeParse(await request.json().catch(() => null));
  if (!parsed.success) return new Response('Invalid finalization request', { status: 400 });
  const pathname = new URL(parsed.data.uploadUrl, request.url).pathname;
  const uploadId = pathname.split('/').filter(Boolean).at(-1) ?? '';
  try {
    const memo = await finalizeMemoUpload(uploadId);
    return Response.json({ memoId: memo.id, finalized: true });
  } catch (error) {
    const status = typeof error === 'object' && error && 'status_code' in error ? Number(error.status_code) : 500;
    const body = typeof error === 'object' && error && 'body' in error ? String(error.body) : 'Finalization failed';
    return new Response(body, { status });
  }
}
