import { and, desc, eq, isNotNull, isNull } from 'drizzle-orm';
import { database, schema } from '@/db/client';
import { encodeId, tryDecodeId } from '@/lib/ids';

export type MemoRow = {
  id: string;
  title: string;
  state: string;
  durationSeconds: number | null;
  playableThroughSeconds: number;
  generation: number;
  processingMode: string;
  uploadComplete: boolean;
  playbackPositionSeconds: number;
  hls: string | null;
  fallback: string | null;
  waveform: string | null;
  failure: string | null;
  trashedAt: Date | null;
  createdAt: Date;
};

export async function listMemos(personId: number, trashed = false): Promise<MemoRow[]> {
  const rows = await database()
    .select()
    .from(schema.voiceMemo)
    .where(and(eq(schema.voiceMemo.personId, personId), trashed ? isNotNull(schema.voiceMemo.trashedAt) : isNull(schema.voiceMemo.trashedAt)))
    .orderBy(desc(schema.voiceMemo.createdAt));
  return rows.map((row) => {
    const id = encodeId('memo', row.id);
    const media = `/api/memos/${id}/media`;
    return {
      id,
      title: row.title,
      state: row.state,
      durationSeconds: row.durationMs === null ? null : row.durationMs / 1_000,
      playableThroughSeconds: row.playableThroughMs / 1_000,
      generation: row.generation,
      processingMode: row.processingMode,
      uploadComplete: row.uploadComplete,
      playbackPositionSeconds: row.playbackPositionMs / 1_000,
      hls: row.hlsMasterKey ? `${media}/g/${row.generation}/master.m3u8` : null,
      fallback: row.fallbackKey ? `${media}/g/${row.generation}/fallback.m4a` : null,
      waveform: row.waveformKey ? `${media}/g/${row.generation}/waveform.json?v=${row.publicationRevision}` : null,
      failure: row.failure ? 'Audio preparation failed. Retry to process the saved original.' : null,
      trashedAt: row.trashedAt,
      createdAt: row.createdAt,
    };
  });
}

export async function ownedMemo(publicId: string, personId: number) {
  const id = tryDecodeId('memo', publicId);
  if (id === null) return null;
  const rows = await database().select().from(schema.voiceMemo)
    .where(and(eq(schema.voiceMemo.id, id), eq(schema.voiceMemo.personId, personId))).limit(1);
  return rows[0] ?? null;
}
