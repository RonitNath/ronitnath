import { Readable } from 'node:stream';
import { currentPrincipal } from '@/features/auth/principal';
import { authorizedMemo, parseRange } from '@/features/memos/media';
import { memoStorage } from '@/features/memos/storage';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const types: Record<string, string> = {
  m3u8: 'application/vnd.apple.mpegurl', m4a: 'audio/mp4', m4s: 'video/iso.segment', json: 'application/json', mp4: 'audio/mp4', webm: 'audio/webm', ogg: 'audio/ogg',
};

export async function GET(request: Request, { params }: { params: Promise<{ memo: string; file: string[] }> }) {
  const principal = await currentPrincipal();
  if (!principal) return new Response('Sign in required', { status: 401 });
  const { memo: publicId, file } = await params;
  const memo = await authorizedMemo(publicId, principal);
  if (!memo) return new Response('Not found', { status: 404 });
  const name = file.join('/');
  let key: string | null = null;
  if (name === 'original') key = memo.sourceKey;
  else if (name === 'fallback.m4a') key = memo.fallbackKey;
  else if (name === 'waveform.json') key = memo.waveformKey;
  else if (name === 'master.m3u8') key = memo.hlsMasterKey;
  else if (/^hls-(32|64)\/(index\.m3u8|init\.mp4|s\d{5}\.m4s)$/.test(name) && memo.hlsMasterKey) key = `${memo.hlsMasterKey.slice(0, -'master.m3u8'.length)}${name}`;
  if (!key) return new Response('Not found', { status: 404 });
  const storage = memoStorage();
  const object = await storage.stat(key);
  if (!object) return new Response('Not found', { status: 404 });
  const range = parseRange(request.headers.get('range'), object.size);
  if (range === 'invalid') return new Response(null, { status: 416, headers: { 'content-range': `bytes */${object.size}` } });
  const stream = Readable.toWeb(await storage.read(key, range ?? undefined) as Readable) as ReadableStream;
  const extension = (name === 'original' ? key : name).split('.').at(-1) ?? '';
  const headers = new Headers({
    'accept-ranges': 'bytes',
    'cache-control': 'private, no-store',
    'content-type': types[extension] ?? memo.sourceMimeType,
    'content-length': String(range ? range.end - range.start + 1 : object.size),
  });
  if (range) headers.set('content-range', `bytes ${range.start}-${range.end}/${object.size}`);
  if (name === 'original') headers.set('content-disposition', `attachment; filename="voice-memo.${extension || 'bin'}"`);
  return new Response(stream, { status: range ? 206 : 200, headers });
}
