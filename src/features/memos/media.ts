import { and, eq } from 'drizzle-orm';
import { database, schema } from '@/db/client';
import type { Principal } from '@/features/auth/principal';
import { tryDecodeId } from '@/lib/ids';

export async function authorizedMemo(publicId: string, principal: Principal) {
  const id = tryDecodeId('memo', publicId);
  if (id === null) return null;
  const rows = await database().select().from(schema.voiceMemo).where(
    principal.isOperator ? eq(schema.voiceMemo.id, id) : and(eq(schema.voiceMemo.id, id), eq(schema.voiceMemo.personId, principal.personId)),
  ).limit(1);
  return rows[0] ?? null;
}

export function parseRange(value: string | null, size: number): { start: number; end: number } | null | 'invalid' {
  if (!value) return null;
  const match = /^bytes=(\d*)-(\d*)$/.exec(value);
  if (!match || (!match[1] && !match[2])) return 'invalid';
  if (!match[1]) {
    const suffix = Number(match[2]);
    if (!Number.isSafeInteger(suffix) || suffix <= 0) return 'invalid';
    return { start: Math.max(0, size - suffix), end: size - 1 };
  }
  const start = Number(match[1]);
  const end = match[2] ? Math.min(Number(match[2]), size - 1) : size - 1;
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || start > end || start >= size) return 'invalid';
  return { start, end };
}
