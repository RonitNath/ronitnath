import { FileSystemMediaStorage } from '@isoastra/audio-node';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

export function memoMediaRoot(): string {
  return process.env.MEMO_MEDIA_ROOT ?? join(tmpdir(), 'ronitnath-memos');
}

export function memoUploadRoot(): string {
  return process.env.MEMO_UPLOAD_ROOT ?? join(tmpdir(), 'ronitnath-memo-uploads');
}

export function memoStorage(): FileSystemMediaStorage {
  return new FileSystemMediaStorage(memoMediaRoot());
}

export function memoPrefix(personId: number, localId: string): string {
  return `people/${personId}/memos/${localId}`;
}
