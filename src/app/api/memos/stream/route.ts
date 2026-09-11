import { currentPrincipal } from '@/features/auth/principal';
import { listMemos } from '@/features/memos/queries';
import { subscribe } from '@/lib/fleet/stream';
import { encodeId, tryDecodeId } from '@/lib/ids';

export const runtime='nodejs';
export const dynamic='force-dynamic';

export async function GET(request:Request){
  const principal=await currentPrincipal();
  if(!principal)return new Response('Sign in required',{status:401});
  const url=new URL(request.url), user=url.searchParams.get('user')??'', personId=tryDecodeId('person',user);
  if(personId===null||(personId!==principal.personId&&!principal.isOperator))return new Response('Not found',{status:404});
  const trash=url.searchParams.get('trash')==='1', orgId=encodeId('person',personId), encoder=new TextEncoder();
  let unsubscribe:(()=>void)|null=null, ping:ReturnType<typeof setInterval>|null=null, lifetime:ReturnType<typeof setTimeout>|null=null, closed=false, writing=false, pending=true, version=0;
  const stream=new ReadableStream<Uint8Array>({
    async start(controller){
      const write=async()=>{
        if(closed||writing){pending=true;return;}
        writing=true;
        try{
          do{
            pending=false;
            const memos=await listMemos(personId,trash);
            controller.enqueue(encoder.encode(`event: snapshot\nid: ${++version}\ndata: ${JSON.stringify({version,memos})}\n\n`));
          }while(pending&&!closed);
        }catch{if(!closed)controller.enqueue(encoder.encode('event: reset\ndata: {}\n\n'));}
        finally{writing=false;}
      };
      unsubscribe=subscribe(orgId,()=>{pending=true;void write();});
      await write();
      ping=setInterval(()=>{if(!closed)controller.enqueue(encoder.encode(': ping\n\n'));},15_000);
      lifetime=setTimeout(()=>{closed=true;unsubscribe?.();if(ping)clearInterval(ping);controller.close();},55_000);
    },
    cancel(){closed=true;unsubscribe?.();if(ping)clearInterval(ping);if(lifetime)clearTimeout(lifetime);}
  });
  return new Response(stream,{headers:{'content-type':'text/event-stream','cache-control':'private, no-cache, no-transform','connection':'keep-alive'}});
}
