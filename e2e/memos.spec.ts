import { execFile as execFileCallback, spawn, type ChildProcess } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { expect, test, type Page } from '@playwright/test';

test.describe.configure({ mode: 'serial' });
test.setTimeout(90_000);

const PASSWORD='a-long-enough-password';
const OPERATOR=`memo-operator-${Date.now()}@example.com`;
let worker:ChildProcess;
let fixtureRoot='';
let fixture='';
let fixtureBase64='';
const execFile=promisify(execFileCallback);

async function signIn(page:Page){
  await page.goto('/auth/sign-in');
  await page.locator('#sign-in-email').fill(OPERATOR);
  await page.locator('#sign-in-password').fill(PASSWORD);
  await page.getByRole('button',{name:'Sign in',exact:true}).click();
  await expect(page).toHaveURL(/\/(?:app|u\/p_[A-Za-z0-9_-]+)$/);
  await page.goto('/app/memos');
  await expect(page).toHaveURL(/\/u\/p_[A-Za-z0-9_-]+\/memos$/);
}

test.beforeAll(async()=>{
  await execFile('pnpm',['seed:operator','--email',OPERATOR,'--password',PASSWORD],{cwd:process.cwd()});
  fixtureRoot=await mkdtemp(join(tmpdir(),'memo-e2e-'));
  fixture=join(fixtureRoot,'capture.webm');
  await execFile('ffmpeg',['-nostdin','-v','error','-f','lavfi','-i','sine=frequency=440:duration=12','-c:a','libopus','-f','webm','-live','1','-cluster_time_limit','500',fixture]);
  fixtureBase64=(await readFile(fixture)).toString('base64');
  worker=spawn(process.execPath,['--import','tsx','scripts/memo-worker.mts'],{cwd:process.cwd(),env:process.env,stdio:['ignore','pipe','pipe']});
  await new Promise((resolve)=>setTimeout(resolve,500));
  if(worker.exitCode!==null)throw new Error(`memo worker exited ${worker.exitCode}`);
});

test.afterAll(async()=>{
  if(worker&&worker.exitCode===null){
    worker.kill('SIGTERM');
    await Promise.race([new Promise((resolve)=>worker.once('exit',resolve)),new Promise((resolve)=>setTimeout(resolve,2_000))]);
    if(worker.exitCode===null)worker.kill('SIGKILL');
  }
  if(fixtureRoot)await rm(fixtureRoot,{recursive:true,force:true});
});

test('imports, streams state, preserves other playback, switches the shared sink, and seeks on the waveform',async({page,browser})=>{
  page.on('console',(message)=>console.log('browser console',message.type(),message.text()));
  page.on('requestfailed',(request)=>console.log('request failed',request.method(),request.url(),request.failure()?.errorText));
  page.on('response',(response)=>{if(response.url().includes('/api/memos')&&response.status()>=400)void response.text().catch(()=>'<unavailable>').then((body)=>console.log('memo response',response.status(),response.url(),body));});
  await signIn(page);
  const importMemo=async(title:string)=>{
    await page.getByLabel('Title optional').fill(title);
    await page.locator('input[type=file]').setInputFiles(fixture);
    const memo=page.locator('.memo-card').filter({has:page.locator(`input[value="${title}"]`)});
    await expect(memo).toBeVisible({timeout:15_000});
    await expect(memo.getByRole('button',{name:'Play'})).toBeVisible({timeout:20_000});
    return memo;
  };
  const existing=await importMemo(`Baseline memo ${Date.now()}`);
  await existing.getByRole('button',{name:'Play'}).click();
  await expect(existing.getByRole('button',{name:'Pause'})).toBeVisible();
  const title=`Streaming memo ${Date.now()}`;
  const user=/\/u\/(p_[A-Za-z0-9_-]+)\/memos$/.exec(page.url())![1]!;
  const started=Date.now();
  const partial=await page.evaluate(async({encoded,title,user})=>{
    const binary=atob(encoded),bytes=Uint8Array.from(binary,(character)=>character.charCodeAt(0)),localId=crypto.randomUUID();
    const encode=(value:string)=>btoa(unescape(encodeURIComponent(value)));
    const metadata={localId,user,filetype:'audio/webm',filename:'streaming.webm',title};
    const created=await fetch('/api/memos/upload',{method:'POST',headers:{'Tus-Resumable':'1.0.0','Upload-Defer-Length':'1','Upload-Metadata':Object.entries(metadata).map(([key,value])=>`${key} ${encode(value)}`).join(',')}});
    if(!created.ok)throw new Error(`create failed ${created.status}`);
    const uploadUrl=new URL(created.headers.get('location')!,location.href).toString(),offset=Math.floor(bytes.length*0.7);
    const appended=await fetch(uploadUrl,{method:'PATCH',body:bytes.slice(0,offset),headers:{'Tus-Resumable':'1.0.0','Upload-Offset':'0','Content-Type':'application/offset+octet-stream'}});
    if(!appended.ok)throw new Error(`partial append failed ${appended.status}`);
    return {uploadUrl,offset,total:bytes.length,tail:Array.from(bytes.slice(offset))};
  },{encoded:fixtureBase64,title,user});
  const memo=page.locator('.memo-card').filter({has:page.locator(`input[value="${title}"]`)});
  await expect(memo.getByRole('button',{name:'Play'})).toBeVisible({timeout:5_000});
  expect(Date.now()-started).toBeLessThan(5_000);
  await expect(memo).toContainText('still saving');
  await expect(existing.getByRole('button',{name:'Pause'})).toBeVisible();
  await expect(memo.locator('.audio-waveform svg')).toBeVisible();
  await memo.getByRole('button',{name:'Play'}).click();
  await expect(memo.getByRole('button',{name:'Pause'})).toBeVisible();
  await expect(existing.getByRole('button',{name:'Play'})).toBeVisible();
  const slider=memo.getByRole('slider',{name:'Position'});
  await slider.fill('5');
  await slider.press('ArrowRight');
  await expect(slider).toHaveValue(/5/);
  const original=await memo.getByRole('link',{name:'Download original'}).getAttribute('href');
  expect(original).toBeTruthy();
  await page.evaluate(async({partial})=>{
    const completed=await fetch(partial.uploadUrl,{method:'PATCH',body:Uint8Array.from(partial.tail),headers:{'Tus-Resumable':'1.0.0','Upload-Offset':String(partial.offset),'Upload-Length':String(partial.total),'Content-Type':'application/offset+octet-stream'}});
    if(!completed.ok)throw new Error(`completion failed ${completed.status}`);
    for(let retry=0;retry<2;retry+=1){
      const finalized=await fetch('/api/memos/finalize',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({uploadUrl:partial.uploadUrl})});
      if(!finalized.ok)throw new Error(`finalization failed ${finalized.status}`);
    }
  },{partial});
  await expect(memo).toContainText('12 seconds',{timeout:20_000});
  const anonymous=await browser.newContext();
  expect((await anonymous.request.get(original!)).status()).toBe(401);
  await anonymous.close();
});
