'use client';

import { AUDIO_LIMITS, PendingRecordingStore, appendToTus, uploadWithTus, withUploadLock } from '@isoastra/audio-core';
import { AudioPlayer, AudioRuntimeProvider, RecorderControls, useAudioRecorder } from '@isoastra/audio-react';
import Link from 'next/link';
import { useActionState, useCallback, useEffect, useState } from 'react';
import type { FormState } from '@/features/auth/form-state';
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

async function recoverUploadUrl(user: string, localId: string, signal?: AbortSignal) {
  const response = await fetch(`/api/memos/upload-recovery?user=${encodeURIComponent(user)}&localId=${encodeURIComponent(localId)}`, { signal });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`Could not recover upload identity (${response.status})`);
  return (await response.json() as { uploadUrl: string }).uploadUrl;
}

async function finalizeUpload(uploadUrl: string, _totalBytes: number, signal?: AbortSignal) {
  const response = await fetch('/api/memos/finalize', {
    method: 'POST', signal, headers: { 'content-type': 'application/json' }, body: JSON.stringify({ uploadUrl }),
  });
  if (!response.ok) throw new Error(`Could not finalize upload (${response.status})`);
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
  const [liveMemos,setLiveMemos]=useState(memos);
  const [title, setTitle] = useState('');
  const [recovery, setRecovery] = useState('');
  const [recoveryItems,setRecoveryItems]=useState<Array<{id:string;mimeType:string;bytes:number}>>([]);
  const [quality, setQuality] = useState<'automatic' | 'data-saver' | 'high'>('automatic');
  const [streamEpoch,setStreamEpoch]=useState(0);
  const uploader = useCallback(async (input: { localId: string; blob: Blob; mimeType: string; onProgress: (bytes: number) => void }) => {
    await upload(user, { ...input, title: title.trim() || undefined });
  }, [title, user]);
  const incrementalUploader = useCallback(async (input: { localId: string; blob: Blob; blobOffset: number; mimeType: string; complete: boolean; uploadUrl: string | null; acknowledgedBytes: number; onProgress: (bytes: number, uploadUrl: string) => void }) => {
    const result = await appendToTus({
      endpoint: '/api/memos/upload', blob: input.blob, uploadUrl: input.uploadUrl,
      acknowledgedBytes: input.acknowledgedBytes, blobOffset: input.blobOffset, complete: input.complete,
      metadata: { localId: input.localId, user, filetype: input.mimeType, filename: `recording-${input.localId}.${input.mimeType.includes('mp4') ? 'm4a' : 'webm'}`, ...(title.trim() ? { title: title.trim() } : {}) },
      recoverUploadUrl: (localId, signal) => recoverUploadUrl(user, localId, signal),
      finalize: finalizeUpload,
      onProgress: (bytes, _total, uploadUrl) => input.onProgress(bytes, uploadUrl),
    });
    return { serverFinalized: result.serverFinalized };
  }, [title, user]);
  const recorder = useAudioRecorder({ accountId:user, uploader, incrementalUploader, onUploaded: () => setTitle('') });

  useEffect(()=>{
    const source=new EventSource(`/api/memos/stream?user=${encodeURIComponent(user)}&trash=${trash?'1':'0'}`);
    const snapshot=(event:MessageEvent<string>)=>{
      const value=JSON.parse(event.data) as {memos:Array<Omit<MemoRow,'createdAt'|'trashedAt'>&{createdAt:string;trashedAt:string|null}>};
      setLiveMemos(value.memos.map((memo)=>({...memo,createdAt:new Date(memo.createdAt),trashedAt:memo.trashedAt?new Date(memo.trashedAt):null})));
    };
    source.addEventListener('snapshot',snapshot as EventListener);
    source.onerror=()=>setStreamEpoch((value)=>value+1);
    return()=>source.close();
  },[trash,user]);

  const recover = useCallback(async () => {
    const store = await PendingRecordingStore.open();
    try {
      const accountRows = await store.list(user);
      for(const row of accountRows.filter((candidate)=>!candidate.complete&&candidate.mimeType.includes('webm'))) await store.markComplete(row.id);
      const pending = (await store.list(user)).filter((row) => row.complete);
      const unrecoverable=accountRows.filter((row)=>!row.complete&&!row.mimeType.includes('webm')).length;
      setRecoveryItems(accountRows.filter((row)=>!row.complete&&!row.mimeType.includes('webm')).map(({id,mimeType,bytes})=>({id,mimeType,bytes})));
      if (!pending.length) { setRecovery(unrecoverable?`${unrecoverable} interrupted recording needs export or deletion.`:''); return; }
      setRecovery(`${pending.length} locally saved memo${pending.length === 1 ? '' : 's'} waiting to upload.${unrecoverable?` ${unrecoverable} interrupted recording needs export or deletion.`:''}`);
      if (!navigator.onLine) return;
      for (const row of pending) {
        const result = await withUploadLock(`${user}:${row.id}`, async () => {
          let persistence = Promise.resolve();
          let offset=row.acknowledgedBytes, uploadUrl=row.uploadUrl, finalized=row.serverFinalized;
          do {
            const blob=await store.readSlice(row.id,offset);
            const result=await appendToTus({endpoint:'/api/memos/upload',blob,blobOffset:offset,uploadUrl,acknowledgedBytes:offset,complete:row.complete&&offset+blob.size===row.bytes,metadata:{localId:row.id,user,filetype:row.mimeType,filename:row.filename??`recording-${row.id}.${row.mimeType.includes('mp4')?'m4a':'webm'}`,...(row.title?{title:row.title}:{})},recoverUploadUrl:(localId,signal)=>recoverUploadUrl(user,localId,signal),finalize:finalizeUpload,onProgress:(bytes,_total,url)=>{uploadUrl=url;offset=bytes;persistence=persistence.then(()=>store.setUploadState(row.id,url,bytes));}});
            finalized=result.serverFinalized;
            if (!blob.size && !finalized) break;
          } while(offset<row.bytes||!finalized);
          await persistence;
          if (!finalized) return false;
          await store.markServerFinalized(row.id);
          await store.remove(row.id);
          return true;
        });
        if (result) setRecovery('Recovered recording saved to the server.');
      }
      setRecovery('Recovered recordings were saved to the server.');
    } catch {
      setRecovery('Locally saved recordings will retry when the connection is available.');
    } finally { store.close(); }
  }, [user]);

  useEffect(() => { void recover(); window.addEventListener('online', recover); return () => window.removeEventListener('online', recover); }, [recover]);

  const exportPending=useCallback(async(id:string,mimeType:string)=>{
    const store=await PendingRecordingStore.open();
    try{
      const blob=await store.blob(id),url=URL.createObjectURL(blob),anchor=document.createElement('a');
      anchor.href=url;anchor.download=`interrupted-${id}.${mimeType.includes('mp4')?'m4a':'bin'}`;anchor.click();
      setTimeout(()=>URL.revokeObjectURL(url),1_000);
    }finally{store.close();}
  },[]);
  const removePending=useCallback(async(id:string)=>{
    if(!window.confirm('Delete this interrupted recording from this device?'))return;
    const store=await PendingRecordingStore.open();
    try{await store.remove(id);setRecoveryItems((items)=>items.filter((item)=>item.id!==id));}finally{store.close();}
  },[]);

  return <AudioRuntimeProvider stopKey={`${user}:${streamEpoch}`}>
    {canCreate && !trash ? <section className="memo-recorder">
      <h2>New memo</h2>
      <label htmlFor="memo-title">Title <span className="note">optional</span></label>
      <input id="memo-title" value={title} onChange={(event) => setTitle(event.target.value)} maxLength={200} placeholder="Voice memo" />
      <RecorderControls recorder={recorder} classes={{ root: 'recorder', status: 'memo-status', meter: 'level-meter', controls: 'memo-controls', error: 'note' }} />
      <label className="import-button">Import audio<input type="file" accept="audio/*" onChange={(event) => {
        const file = event.target.files?.[0];
        if (!file) return;
        event.currentTarget.value='';
        setRecovery(`Saving ${file.name} on this device…`);
        void (async()=>{
          const store=await PendingRecordingStore.open();
          try{
            const estimate=await navigator.storage?.estimate?.();
            const available=(estimate?.quota??Infinity)-(estimate?.usage??0);
            if(file.size>AUDIO_LIMITS.uploadBytes)throw new Error('This memo exceeds the 512 MiB upload limit.');
            if((await store.totalBytes())+file.size>AUDIO_LIMITS.pendingLocalBytes||available<file.size+1024*1024)throw new Error('There is not enough local space to save this import.');
            const row=await store.create(file.type||'application/octet-stream',crypto.randomUUID(),user,{title:title.trim()||undefined,filename:file.name});
            let sequence=0;
            for(let offset=0;offset<file.size;offset+=AUDIO_LIMITS.transferBufferBytes) await store.append(row.id,sequence++,file.slice(offset,Math.min(file.size,offset+AUDIO_LIMITS.transferBufferBytes)),0);
            await store.markComplete(row.id);
            setRecovery('Import saved on this device; uploading now.');
          }finally{store.close();}
          setTitle(''); await recover();
        })().catch((error:Error)=>setRecovery(error.message));
      }} /></label>
      {recovery ? <p className="note" role="status">{recovery}</p> : null}
      {recoveryItems.map((item)=><div className="memo-actions" key={item.id}>
        <span className="note">Interrupted recording, {Math.ceil(item.bytes/1024)} KiB</span>
        <button type="button" className="linkish" onClick={()=>void exportPending(item.id,item.mimeType)}>Export bytes</button>
        <button type="button" className="linkish" onClick={()=>void removePending(item.id)}>Delete local copy</button>
      </div>)}
    </section> : null}

    <section>
      <div className="memo-heading"><h2>{trash ? 'Trash' : 'Your memos'}</h2><Link href={trash ? `/u/${user}/memos` : `/u/${user}/memos?trash=1`}>{trash ? 'Back to memos' : 'View trash'}</Link></div>
      {!trash ? <label className="memo-quality">Playback quality <select value={quality} onChange={(event) => setQuality(event.target.value as typeof quality)}><option value="automatic">Automatic</option><option value="data-saver">Data saver</option><option value="high">High</option></select></label> : null}
      <div className="memo-list">
        {liveMemos.map((memo) => <article key={memo.id} className="memo-card">
          <Rename memo={memo} user={user} />
          <p className="note"><time dateTime={memo.createdAt.toISOString()}>{memo.createdAt.toISOString().slice(0, 16).replace('T', ' ')}</time>, {memo.state === 'ready' ? `${Math.round(memo.durationSeconds ?? 0)} seconds` : memo.state === 'playable' ? `playable through ${Math.round(memo.playableThroughSeconds)} seconds, still saving` : memo.processingMode === 'after-upload' && !memo.uploadComplete ? 'saved locally; replay starts when this format is finalized' : memo.state}</p>
          {(memo.state === 'ready'||memo.state === 'playable') && memo.hls && memo.playableThroughSeconds>0 ? <AudioPlayer id={memo.id} className="memo-player" quality={quality} initialPosition={memo.playbackPositionSeconds} source={{ generation:memo.generation, fallback:memo.fallback ?? memo.hls, hls:memo.hls, waveform:memo.waveform ?? undefined, duration:memo.durationSeconds, playableThrough:memo.playableThroughSeconds, publishedRanges:[{start:0,end:memo.playableThroughSeconds}] }} onPosition={(seconds) => void fetch(`/api/memos/${memo.id}/position`, { method: 'PUT', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ seconds }) })} /> : null}
          {memo.failure ? <p className="note" role="alert">{memo.failure}</p> : null}
          <div className="memo-actions">
            {!trash && memo.state !== 'deleting' ? <><a href={`/api/memos/${memo.id}/media/original`}>Download original</a><MemoCommand memo={memo} user={user} kind="trash" /></> : null}
            {trash && memo.state !== 'deleting' ? <><MemoCommand memo={memo} user={user} kind="restore" /><MemoCommand memo={memo} user={user} kind="delete" /></> : null}
            {memo.state === 'failed' ? <MemoCommand memo={memo} user={user} kind="retry" /> : null}
          </div>
        </article>)}
      </div>
      {liveMemos.length === 0 ? <p className="empty">{trash ? 'Trash is empty.' : 'No voice memos yet.'}</p> : null}
    </section>
  </AudioRuntimeProvider>;
}
