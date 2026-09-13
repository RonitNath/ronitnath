'use client';

import { useState, useTransition } from 'react';
import { Banner, Card } from '@isoastra/ui';
import { LayoutSurface, type LayoutDocument, type LayoutRegistry } from '@isoastra/ui-layout';
import { LayoutStudio } from '@isoastra/ui-layout/authoring';
import { saveOperatorLayout, publishOperatorLayout } from './actions';
import { operatorLayoutContract } from './contract';

const registry:LayoutRegistry={
  copy:{...operatorLayoutContract.copy!,render:({props})=><Card title={String(props.title)}>{String(props.body??'')}</Card>},
};

export function OperatorLayoutStudio({initial,version,history}:{initial:LayoutDocument;version:number;history:{version:number;at:string;by:string|null}[]}) {
  const [document,setDocument]=useState(initial),[draftVersion,setDraftVersion]=useState(version),[selected,setSelected]=useState(initial.nodes[0]?.id),[message,setMessage]=useState('No local changes.'),[sourceOpen,setSourceOpen]=useState(false),[pending,startTransition]=useTransition();
  const save=()=>startTransition(async()=>{const next={...document,revision:draftVersion+1};const result=await saveOperatorLayout(JSON.stringify(next),draftVersion);setMessage(result.error??result.notice??'Save completed.');if(result.version){setDocument(next);setDraftVersion(result.version)}});
  const publish=()=>startTransition(async()=>{const result=await publishOperatorLayout(draftVersion);setMessage(result.error??result.notice??'Publication completed.')});
  return <LayoutStudio
    document={document}
    registry={registry}
    {...(selected?{selected}:{})}
    onSelect={setSelected}
    onChange={(next)=>{setDocument(next);setMessage('Unsaved structural or copy changes.')}}
    onInsert={(component)=>{const id=`${component}-${document.nodes.length+1}`;setDocument({...document,nodes:[...document.nodes,{id,component,placement:{region:'primary'},props:{title:'New section',body:''}}]});setSelected(id);setMessage('Unsaved structural change.')}}
    onRemove={(id)=>{setDocument({...document,nodes:document.nodes.filter(node=>node.id!==id)});setMessage('Removal is staged locally. Save or reload before publication.')}}
    onSave={save}
    onPublish={publish}
    savePending={pending}
    publishPending={pending}
    sourceOpen={sourceOpen}
    onSourceOpenChange={setSourceOpen}
    diff={<Banner tone={message.includes('changed')||message.includes('moved')?'warning':'info'} title="Draft status">{message}</Banner>}
    history={<Card title="Published history">{history.length?history.map(item=><p key={item.version}>Revision {item.version} · {item.at} · {item.by??'system'}</p>):<p>No published revisions yet.</p>}</Card>}
    preview={<LayoutSurface document={document} registry={registry}/>}
  />;
}
