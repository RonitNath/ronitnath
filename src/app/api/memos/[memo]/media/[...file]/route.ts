import { Readable } from 'node:stream';
import { currentPrincipal } from '@/features/auth/principal';
import { authorizedMemo, parseRange } from '@/features/memos/media';
import { memoStorage } from '@/features/memos/storage';
import { database, schema } from '@/db/client';
import { and, asc, eq } from 'drizzle-orm';
import { commonPlayableBoundary, reducePeaks, selectWaveformLevel } from '@/features/memos/publication';

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
  const requested = file.join('/');
  const generationPath = /^g\/(\d+)\/(.+)$/.exec(requested);
  const requestedGeneration = generationPath ? Number(generationPath[1]) : null;
  const name = generationPath?.[2] ?? requested;
  let servingGeneration=memo.generation;
  if(name!=='original'){
    if(requestedGeneration===null||requestedGeneration<=0)return new Response('Not found',{status:404});
    if(requestedGeneration!==memo.generation){
      const retired=requestedGeneration<memo.generation&&(await database().select({sequence:schema.voiceMemoSegment.sequence}).from(schema.voiceMemoSegment)
        .where(and(eq(schema.voiceMemoSegment.memoId,memo.id),eq(schema.voiceMemoSegment.generation,requestedGeneration))).limit(1))[0];
      if(!retired)return new Response('Not found',{status:404});
    }
    servingGeneration=requestedGeneration;
  }
  const currentBase=memo.hlsMasterKey?.slice(0,-'master.m3u8'.length)??null;
  const generationBase=currentBase?.replace(/\/g\d+\/$/,`/g${servingGeneration}/`)??null;
  if (name === 'master.m3u8' && memo.hlsMasterKey && memo.generation > 0) {
    const live=(await database().select({sequence:schema.voiceMemoSegment.sequence}).from(schema.voiceMemoSegment).where(and(eq(schema.voiceMemoSegment.memoId,memo.id),eq(schema.voiceMemoSegment.generation,servingGeneration))).limit(1))[0];
    if(live)return playlistResponse(['#EXTM3U','#EXT-X-VERSION:7','#EXT-X-STREAM-INF:BANDWIDTH=36000,CODECS="mp4a.40.2"','hls-32/index.m3u8','#EXT-X-STREAM-INF:BANDWIDTH=70000,CODECS="mp4a.40.2"','hls-64/index.m3u8',''].join('\n'));
  }
  const playlist = /^hls-(32|64)\/index\.m3u8$/.exec(name);
  if (playlist && memo.hlsMasterKey && memo.generation > 0) {
    const rendition = Number(playlist[1]);
    const allSegments = await database().select().from(schema.voiceMemoSegment).where(and(eq(schema.voiceMemoSegment.memoId,memo.id),eq(schema.voiceMemoSegment.generation,servingGeneration))).orderBy(asc(schema.voiceMemoSegment.sequence));
    const commonThrough=commonPlayableBoundary(allSegments).sequence;
    const segments = allSegments.filter((segment)=>segment.rendition===rendition&&segment.sequence<=commonThrough);
    if (segments.length) {
      const target=Math.max(1,Math.ceil(Math.max(...segments.map((segment)=>segment.durationMs))/1000));
      const lines=['#EXTM3U','#EXT-X-VERSION:7',`#EXT-X-TARGETDURATION:${target}`,`#EXT-X-MEDIA-SEQUENCE:${segments[0]!.sequence}`,'#EXT-X-PLAYLIST-TYPE:EVENT','#EXT-X-MAP:URI="init.mp4"'];
      for (const segment of segments) lines.push(`#EXTINF:${(segment.durationMs/1000).toFixed(3)},`,`s${String(segment.sequence).padStart(5,'0')}.m4s`);
      if (servingGeneration!==memo.generation||memo.state==='ready') lines.push('#EXT-X-ENDLIST');
      return playlistResponse(`${lines.join('\n')}\n`);
    }
  }
  let key: string | null = null;
  if (name === 'original') key = memo.sourceKey;
  else if (name === 'fallback.m4a') key = generationBase ? `${generationBase}fallback.m4a` : memo.fallbackKey;
  else if (name === 'waveform.json') key = generationBase ? `${generationBase}waveform.json` : memo.waveformKey;
  else if (name === 'master.m3u8') key = generationBase ? `${generationBase}master.m3u8` : memo.hlsMasterKey;
  else if (/^hls-(32|64)\/init\.mp4$/.test(name) && generationBase) {
    const rendition=Number(/^hls-(32|64)/.exec(name)?.[1]);
    const committed=(await database().select({sequence:schema.voiceMemoSegment.sequence}).from(schema.voiceMemoSegment).where(and(eq(schema.voiceMemoSegment.memoId,memo.id),eq(schema.voiceMemoSegment.generation,servingGeneration),eq(schema.voiceMemoSegment.rendition,rendition))).limit(1))[0];
    if(committed)key=`${generationBase}${name}`;
  }
  else if (/^hls-(32|64)\/s\d{5}\.m4s$/.test(name)) {
    const match=/^hls-(32|64)\/s(\d{5})\.m4s$/.exec(name)!;
    key=(await database().select({key:schema.voiceMemoSegment.key}).from(schema.voiceMemoSegment).where(and(eq(schema.voiceMemoSegment.memoId,memo.id),eq(schema.voiceMemoSegment.generation,servingGeneration),eq(schema.voiceMemoSegment.rendition,Number(match[1])),eq(schema.voiceMemoSegment.sequence,Number(match[2])))).limit(1))[0]?.key??null;
  }
  else if (/^hls-(32|64)\/index\.m3u8$/.test(name) && memo.state==='ready' && generationBase) key = `${generationBase}${name}`;
  if (!key) return new Response('Not found', { status: 404 });
  const storage = memoStorage();
  if (name === 'waveform.json') {
    const tileRows=await database().select().from(schema.voiceMemoWaveformTile).where(and(eq(schema.voiceMemoWaveformTile.memoId,memo.id),eq(schema.voiceMemoWaveformTile.generation,servingGeneration))).orderBy(asc(schema.voiceMemoWaveformTile.startPeak));
    if(tileRows.length){
      const requested=Math.max(1,Math.min(5000,Number(new URL(request.url).searchParams.get('columns'))||1000));
      const levels=[...new Set(tileRows.map((tile)=>tile.level))].sort((a,b)=>a-b);
      const sourcePeaks=Math.max(1,Math.ceil(memo.playableThroughMs/1000*50));
      const level=selectWaveformLevel(levels,sourcePeaks,requested);
      const peaks:number[]=[];
      for(const tile of tileRows.filter((candidate)=>candidate.level===level)){
        const chunks:Buffer[]=[];
        for await(const chunk of await storage.read(tile.key))chunks.push(Buffer.from(chunk));
        const payload=JSON.parse(Buffer.concat(chunks).toString('utf8')) as {peaks:number[]};
        peaks.push(...payload.peaks);
      }
      const reduced=reducePeaks(peaks,requested);
      return Response.json({samplesPerSecond:50/level/reduced.step,peaks:reduced.peaks},{headers:{'cache-control':memo.state==='ready'?'private, max-age=3600':'private, no-store'}});
    }
    const object = await storage.stat(key);
    if (!object) return new Response('Not found',{status:404});
    const chunks: Buffer[]=[];
    for await (const chunk of await storage.read(key)) chunks.push(Buffer.from(chunk));
    const payload=JSON.parse(Buffer.concat(chunks).toString('utf8')) as {samplesPerSecond:number;peaks?:number[];levels?:Array<{step:number;peaks:number[]}>};
    const requested=Math.max(1,Math.min(5000,Number(new URL(request.url).searchParams.get('columns'))||1000));
    const levels=payload.levels??[{step:1,peaks:payload.peaks??[]}];
    const level=[...levels].sort((a,b)=>a.step-b.step).find((candidate)=>candidate.peaks.length<=requested*2)??levels.at(-1)!;
    return Response.json({samplesPerSecond:payload.samplesPerSecond/level.step,peaks:level.peaks},{headers:{'cache-control':memo.state==='ready'?'private, max-age=3600':'private, no-store'}});
  }
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

function playlistResponse(body:string){return new Response(body,{headers:{'content-type':'application/vnd.apple.mpegurl','cache-control':'private, no-store'}});}
