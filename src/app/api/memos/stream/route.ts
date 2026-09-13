import { currentPrincipal } from '@/features/auth/principal';
import { database } from '@/db/client';
import { listMemos } from '@/features/memos/queries';
import { subscribe } from '@/lib/fleet/stream';
import { latestSeq } from '@/lib/fleet/events';
import { encodeId, tryDecodeId } from '@/lib/ids';

export const runtime='nodejs';
export const dynamic='force-dynamic';

export async function GET(request:Request){
  const principal=await currentPrincipal();
  if(!principal)return new Response('Sign in required',{status:401});
  const url=new URL(request.url), user=url.searchParams.get('user')??'', personId=tryDecodeId('person',user);
  if(personId===null||(personId!==principal.personId&&!principal.isOperator))return new Response('Not found',{status:404});
  const trash=url.searchParams.get('trash')==='1', orgId=encodeId('person',personId), encoder=new TextEncoder();
  let unsubscribe:(()=>void)|null=null, ping:ReturnType<typeof setInterval>|null=null, closed=false, writing=false, pending=true;
  const stream=new ReadableStream<Uint8Array>({
    async start(controller){
      const close=()=>{if(closed)return;closed=true;unsubscribe?.();unsubscribe=null;if(ping)clearInterval(ping);try{controller.close();}catch{}};
      const write=async()=>{
        if(closed||writing){pending=true;return;}
        writing=true;
        try{
          do{
            pending=false;
            const refreshed=await currentPrincipal();
            if(!refreshed||(refreshed.personId!==personId&&!refreshed.isOperator)){close();return;}
            let before=0,after=0,memos:Awaited<ReturnType<typeof listMemos>>=[];
            do{before=await latestSeq(database(),orgId);memos=await listMemos(personId,trash);after=await latestSeq(database(),orgId);}while(before!==after&&!closed);
            if(controller.desiredSize!==null&&controller.desiredSize<=0){close();return;}
            controller.enqueue(encoder.encode(`event: snapshot\nid: ${after}\ndata: ${JSON.stringify({version:after,memos})}\n\n`));
          }while(pending&&!closed);
        }catch{close();}
        finally{writing=false;}
      };
      unsubscribe=subscribe(orgId,()=>{pending=true;void write();});
      await write();
      ping=setInterval(()=>{if(!closed)void (async()=>{
        const refreshed=await currentPrincipal().catch(()=>null);
        if(!refreshed||(refreshed.personId!==personId&&!refreshed.isOperator)){close();return;}
        if(controller.desiredSize!==null&&controller.desiredSize<=0){close();return;}
        controller.enqueue(encoder.encode(': ping\n\n'));
      })();},15_000);
    },
    cancel(){closed=true;unsubscribe?.();if(ping)clearInterval(ping);}
  });
  return new Response(stream,{headers:{'content-type':'text/event-stream','cache-control':'private, no-cache, no-transform','connection':'keep-alive'}});
}
