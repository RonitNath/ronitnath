import { spawn, type ChildProcess } from 'node:child_process';
import { expect, test, type Page } from '@playwright/test';

const fixture=process.env.MEMO_LONG_FIXTURE;
const password='a-long-enough-password';
const operator=`memo-long-${Date.now()}@example.com`;
let worker:ChildProcess;

test.skip(!fixture,'Set MEMO_LONG_FIXTURE to run the hour-long media acceptance test.');
test.setTimeout(10*60_000);

async function signIn(page:Page){
  await page.goto('/auth/sign-in');
  await page.locator('#sign-in-email').fill(operator);
  await page.locator('#sign-in-password').fill(password);
  await page.getByRole('button',{name:'Sign in',exact:true}).click();
  await expect(page).toHaveURL(/\/(?:app|u\/p_[A-Za-z0-9_-]+)$/);
  await page.goto('/app/memos');
  await expect(page).toHaveURL(/\/u\/p_[A-Za-z0-9_-]+\/memos$/);
}

test.beforeAll(async()=>{
  const {execFile}=await import('node:child_process');
  await new Promise<void>((resolve,reject)=>execFile('pnpm',['seed:operator','--email',operator,'--password',password],{cwd:process.cwd()},(error)=>error?reject(error):resolve()));
  worker=spawn(process.execPath,['--import','tsx','scripts/memo-worker.mts'],{cwd:process.cwd(),env:process.env,stdio:'ignore'});
  await new Promise((resolve)=>setTimeout(resolve,500));
  if(worker.exitCode!==null)throw new Error(`memo worker exited ${worker.exitCode}`);
});

test.afterAll(async()=>{
  if(!worker||worker.exitCode!==null)return;
  worker.kill('SIGTERM');
  await Promise.race([new Promise((resolve)=>worker.once('exit',resolve)),new Promise((resolve)=>setTimeout(resolve,2_000))]);
  if(worker.exitCode===null)worker.kill('SIGKILL');
});

test('keeps hour-long playback segmented and meets repeated local start and seek targets',async({page})=>{
  await signIn(page);
  const title=`Hour memo ${Date.now()}`,mediaRequests:string[]=[];
  page.on('request',(request)=>{if(request.url().includes('/api/memos/')&&request.url().includes('/media/'))mediaRequests.push(request.url());});
  await page.getByLabel('Title optional').fill(title);
  await page.locator('input[type=file]').setInputFiles(fixture!);
  const memo=page.locator('.memo-card').filter({has:page.locator(`input[value="${title}"]`)});
  await expect(memo).toBeVisible({timeout:30_000});
  await expect(memo.getByRole('button',{name:'Play'})).toBeVisible({timeout:5*60_000});
  const waveform=memo.locator('.audio-waveform');
  await expect(waveform.locator('svg')).toBeVisible();
  const geometry=await waveform.evaluate((element)=>({samples:Number(element.getAttribute('data-samples')),columns:Number(element.getAttribute('data-columns')),width:element.getBoundingClientRect().width}));
  expect(geometry.samples).toBeLessThanOrEqual(Math.ceil(geometry.width)+1);
  expect(geometry.columns).toBeLessThanOrEqual(Math.ceil(geometry.width)+1);

  const samples:number[]=[];
  for(let trial=0;trial<30;trial+=1){
    const target=30+trial*115;
    const slider=memo.getByRole('slider',{name:'Position'});
    await slider.fill(String(target));
    await slider.press('ArrowRight');
    const started=performance.now();
    await memo.getByRole('button',{name:'Play'}).click();
    await expect(memo.getByRole('button',{name:'Pause'})).toBeVisible({timeout:2_000});
    await expect.poll(()=>page.locator('audio').evaluate((audio)=>(audio as HTMLAudioElement).currentTime),{timeout:2_000}).toBeGreaterThan(target-1);
    samples.push(performance.now()-started);
    await memo.getByRole('button',{name:'Pause'}).click();
  }
  samples.sort((a,b)=>a-b);
  const p95=samples[Math.ceil(samples.length*0.95)-1]!;
  expect(p95).toBeLessThan(2_000);
  expect(mediaRequests.some((url)=>url.endsWith('/media/original')||url.includes('/fallback.m4a'))).toBe(false);
  console.log(JSON.stringify({trials:samples.length,p95StartAndSeekMs:Math.round(p95),maximumMs:Math.round(samples.at(-1)!),waveform:geometry,mediaRequests:mediaRequests.length}));
});
