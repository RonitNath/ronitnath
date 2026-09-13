'use server';

import { revalidatePath } from 'next/cache';
import { database } from '@/db/client';
import { ConfiguredConflict, publish, saveDraft } from '@/lib/fleet/configured';
import { requireOperator } from '@/lib/tiers';
import { validateLayout } from '@isoastra/ui-layout/core';
import { OPERATOR_LAYOUT_KEY, operatorLayoutContract } from './contract';

export type LayoutActionResult = { version?:number; notice?:string; error?:string; conflict?:boolean };

export async function saveOperatorLayout(source:string,expectedVersion:number):Promise<LayoutActionResult> {
  await requireOperator();
  try {
    const document=validateLayout(JSON.parse(source) as unknown,operatorLayoutContract);
    if(document.revision!==expectedVersion+1)return {error:'The draft revision is not the expected successor.'};
    const version=await database().transaction((tx)=>saveDraft(tx,'isoastra',OPERATOR_LAYOUT_KEY,document,expectedVersion));
    revalidatePath('/o/isoastra/layouts');
    return {version,notice:`Draft ${version} saved.`};
  } catch(error) {
    if(error instanceof ConfiguredConflict)return {error:'The layout changed in another session. Reload and review the current draft.',conflict:true};
    return {error:'The layout draft is invalid or could not be saved.'};
  }
}

export async function publishOperatorLayout(expectedVersion:number):Promise<LayoutActionResult> {
  await requireOperator();
  try {
    const version=await database().transaction((tx)=>publish(tx,'isoastra',OPERATOR_LAYOUT_KEY,expectedVersion));
    revalidatePath('/o/isoastra/layouts');
    return {version,notice:`Published layout revision ${version}.`};
  } catch(error) {
    return {error:error instanceof ConfiguredConflict?'The draft moved. Reload and review it before publishing.':'The layout could not be published.',conflict:error instanceof ConfiguredConflict};
  }
}
