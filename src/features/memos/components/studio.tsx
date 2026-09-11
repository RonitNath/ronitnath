'use client';

import { AUDIO_LIMITS, PendingRecordingStore, appendToTus, uploadWithTus, withUploadLock } from '@isoastra/audio-core';
import { AudioPlayer, AudioRuntimeProvider, RecorderControls, useAudioRecorder } from '@isoastra/audio-react';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useActionState, useCallback, useEffect, useState } from 'react';
import type { FormState } from '@/features/auth/form-state';
import { userStreamPath } from '@/lib/paths';
import { useRefreshOn } from '@/features/realtime/use-events';
import { deleteMemoPermanently, renameMemo, restoreMemo, retryMemo, trashMemo } from '../actions';
import type { MemoRow } from '../queries';

const EMPTY: FormState = {};

async function upload(user: string, input: { localId: string; blob: Blob; mimeType: string; filename?: string; title?: string; onProgress?: (bytes: number) => void }) {
  if (input.blob.size > AUDIO_LIMITS.uploadBytes) throw new Error('This memo exceeds the 512 MiB upload limit.');
  await uploadWithTus({
    endpoint: '/api/memos/upload', blob: input.blob, localId: input.localId, mimeType: input.mimeType,
    metadata: { user, filename: input.filename ?? `recording-${input.localId}.${input.mimeType.includes('mp4') ? 'm4a' : 'webm'}`, ...(input.title ? { title: input.title } : {}) },
    onProgress: (bytes) => input.onProgress?.(bytes),
  });
}

function MemoCommand({ memo, user, kind }: { memo: MemoRow; user: string; kind: 'trash' | 'restore' | 'delete' | 'retry' }) {
  const action = kind === 'trash' ? trashMemo : kind === 'restore' ? restoreMemo : kind === 'retry' ? retryMemo : deleteMemoPermanently;
  const [state, run, pending] = useActionState<FormState, FormData>(action as (state: FormState, form: FormData) => Promise<FormState>, EMPTY);
  return <form action={run} onSubmit={(event) => { if (kind === 'delete' && !window.confirm('Permanently delete this memo and its original recording?')) event.preventDefault(); }}>
    <input type="hidden" name="user" value={user} /><input type="hidden" name="memo" value={memo.id} />
    <button type="submit" className="linkish" disabled={pending}>{kind === 'delete' ? 'Delete permanently' : `${kind[0]!.toUpperCase()}${kind.slice(1)}`}</button>
    {state.error ? <span className="note" role="alert"> {state.error}{'reauth' in state && state.reauth ? <> <Link href="/o/isoastra/reauth">Reauthenticate</Link></> : null}</span> : null}
  </form>;
}

function Rename({ memo, user }: { memo: MemoRow; user: string }) {
  const [state, run, pending] = useActionState<FormState, FormData>(renameMemo as (state: FormState, form: FormData) => Promise<FormState>, EMPTY);
  return <form action={run} className="memo-rename">
    <input type="hidden" name="user" value={user} /><input type="hidden" name="memo" value={memo.id} />
    <label className="sr-only" htmlFor={`title-${memo.id}`}>Title</label>
    <input id={`title-${memo.id}`} name="title" defaultValue={memo.title} maxLength={200} required />
    <button type="submit" className="linkish" disabled={pending}>Rename</button>
    {state.error ? <span className="note" role="alert"> {state.error}</span> : null}
  </form>;
}

export function MemoStudio({ user, memos, trash, canCreate }: { user: string; memos: MemoRow[]; trash: boolean; canCreate: boolean }) {
  const router = useRouter();
  const [title, setTitle] = useState('');
  const [recovery, setRecovery] = useState('');
  const [quality, setQuality] = useState<'automatic' | 'data-saver' | 'high'>('automatic');
  const uploader = useCallback(async (input: { localId: string; blob: Blob; mimeType: string; onProgress: (bytes: number) => void }) => {
    await upload(user, { ...input, title: title.trim() || undefined });
  }, [title, user]);
  const incrementalUploader = useCallback(async (input: { localId: string; blob: Blob; blobOffset: number; mimeType: string; complete: boolean; uploadUrl: string | null; acknowledgedBytes: number; onProgress: (bytes: number, uploadUrl: string) => void }) => {
    await appendToTus({
      endpoint: '/api/memos/upload', blob: input.blob, uploadUrl: input.uploadUrl,
      acknowledgedBytes: input.acknowledgedBytes, blobOffset: input.blobOffset, complete: input.complete,
      metadata: { localId: input.localId, user, filetype: input.mimeType, filename: `recording-${input.localId}.${input.mimeType.includes('mp4') ? 'm4a' : 'webm'}`, ...(title.trim() ? { title: title.trim() } : {}) },
      onProgress: (bytes, _total, uploadUrl) => input.onProgress(bytes, uploadUrl),
    });
  }, [title, user]);
  const recorder = useAudioRecorder({ uploader, incrementalUploader, onUploaded: () => { setTitle(''); router.refresh(); } });
  useRefreshOn(userStreamPath(user), ['voice-memo']);

  const recover = useCallback(async () => {
    const store = await PendingRecordingStore.open();
    try {
      const pending = (await store.list()).filter((row) => row.complete);
      if (!pending.length) { setRecovery(''); return; }
      setRecovery(`${pending.length} locally saved memo${pending.length === 1 ? '' : 's'} waiting to upload.`);
      if (!navigator.onLine) return;
      for (const row of pending) {
        const result = await withUploadLock(row.id, async () => {
          const blob = await store.blob(row.id, row.acknowledgedBytes);
          let persistence = Promise.resolve();
          await appendToTus({
            endpoint: '/api/memos/upload', blob, blobOffset: row.acknowledgedBytes, uploadUrl: row.uploadUrl, acknowledgedBytes: row.acknowledgedBytes,
            complete: true, metadata: { localId: row.id, user, filetype: row.mimeType, filename: `recording-${row.id}.${row.mimeType.includes('mp4') ? 'm4a' : 'webm'}` },
            onProgress: (bytes, _total, uploadUrl) => { persistence = persistence.then(() => store.setUploadState(row.id, uploadUrl, bytes)); },
          });
          await persistence;
          await store.remove(row.id);
          return true;
        });
        if (result) router.refresh();
      }
      setRecovery('Recovered recordings were saved to the server.');
    } catch {
      setRecovery('Locally saved recordings will retry when the connection is available.');
    } finally { store.close(); }
  }, [router, user]);

  useEffect(() => { void recover(); window.addEventListener('online', recover); return () => window.removeEventListener('online', recover); }, [recover]);

  return <AudioRuntimeProvider>
    {canCreate && !trash ? <section className="memo-recorder">
      <h2>New memo</h2>
      <label htmlFor="memo-title">Title <span className="note">optional</span></label>
      <input id="memo-title" value={title} onChange={(event) => setTitle(event.target.value)} maxLength={200} placeholder="Voice memo" />
      <RecorderControls recorder={recorder} classes={{ root: 'recorder', status: 'memo-status', meter: 'level-meter', controls: 'memo-controls', error: 'note' }} />
      <label className="import-button">Import audio<input type="file" accept="audio/*" onChange={(event) => {
        const file = event.target.files?.[0];
        if (!file) return;
        setRecovery(`Uploading ${file.name}…`);
        void upload(user, { localId: crypto.randomUUID(), blob: file, mimeType: file.type || 'application/octet-stream', filename: file.name, title: title.trim() || file.name })
          .then(() => { setRecovery('Import saved to server; preparing playback.'); setTitle(''); router.refresh(); })
          .catch((error: Error) => setRecovery(error.message));
      }} /></label>
      {recovery ? <p className="note" role="status">{recovery}</p> : null}
    </section> : null}

    <section>
      <div className="memo-heading"><h2>{trash ? 'Trash' : 'Your memos'}</h2><Link href={trash ? `/u/${user}/memos` : `/u/${user}/memos?trash=1`}>{trash ? 'Back to memos' : 'View trash'}</Link></div>
      {!trash ? <label className="memo-quality">Playback quality <select value={quality} onChange={(event) => setQuality(event.target.value as typeof quality)}><option value="automatic">Automatic</option><option value="data-saver">Data saver</option><option value="high">High</option></select></label> : null}
      <div className="memo-list">
        {memos.map((memo) => <article key={memo.id} className="memo-card">
          <Rename memo={memo} user={user} />
          <p className="note"><time dateTime={memo.createdAt.toISOString()}>{memo.createdAt.toISOString().slice(0, 16).replace('T', ' ')}</time>, {memo.state === 'ready' ? `${Math.round(memo.durationSeconds ?? 0)} seconds` : memo.state}</p>
          {memo.state === 'ready' && memo.fallback && memo.durationSeconds ? <AudioPlayer id={memo.id} className="memo-player" quality={quality} initialPosition={memo.playbackPositionSeconds} source={{ fallback: memo.fallback, hls: memo.hls ?? undefined, waveform: memo.waveform ?? undefined, duration: memo.durationSeconds }} onPosition={(seconds) => void fetch(`/api/memos/${memo.id}/position`, { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ seconds }) })} /> : null}
          {memo.failure ? <p className="note" role="alert">{memo.failure}</p> : null}
          <div className="memo-actions">
            {!trash && memo.state !== 'deleting' ? <><a href={`/api/memos/${memo.id}/media/original`}>Download original</a><MemoCommand memo={memo} user={user} kind="trash" /></> : null}
            {trash && memo.state !== 'deleting' ? <><MemoCommand memo={memo} user={user} kind="restore" /><MemoCommand memo={memo} user={user} kind="delete" /></> : null}
            {memo.state === 'failed' ? <MemoCommand memo={memo} user={user} kind="retry" /> : null}
          </div>
        </article>)}
      </div>
      {memos.length === 0 ? <p className="empty">{trash ? 'Trash is empty.' : 'No voice memos yet.'}</p> : null}
    </section>
  </AudioRuntimeProvider>;
}
