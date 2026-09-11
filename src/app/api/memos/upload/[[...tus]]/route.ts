import { memoTusServer } from '@/features/memos/upload';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

async function handle(request: Request) {
  return memoTusServer().handleWeb(request);
}

export { handle as DELETE, handle as HEAD, handle as OPTIONS, handle as PATCH, handle as POST };
