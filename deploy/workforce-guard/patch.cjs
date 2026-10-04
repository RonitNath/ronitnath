'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const root=process.argv[2]||'/app/.next/server';
const guardPath='/app/isoastra-workforce-guard.cjs';let profiles=0,principals=0;const changed=[];
function scan(folder) {
  for(const entry of fs.readdirSync(folder,{withFileTypes:true})) {
    const file=path.join(folder,entry.name);if(entry.isDirectory()){scan(file);continue;}if(!file.endsWith('.js'))continue;
    let source=fs.readFileSync(file,'utf8'),original=source;
    source=source.replace(/mapProfileToUser:([a-z])=>\{/g,(match,p)=>{profiles++;return `${match}if(!require(${JSON.stringify(guardPath)}).profile(${p}))throw require(${JSON.stringify(guardPath)}).denied();`;});
    const pattern=/\[([a-z]),([a-z])\]=await Promise\.all\(\[([a-z])\.select\(\{id:([a-z])\.Qr\.account\.id\}\)\.from\(\4\.Qr\.account\)\.where\(\(0,([a-z])\.Uo\)\(\(0,\5\.eq\)\(\4\.Qr\.account\.userId,([a-z])\.user\.id\),\(0,\5\.eq\)\(\4\.Qr\.account\.providerId,"zitadel"\)\)\)\.limit\(1\),/g;
    const match=pattern.exec(source);
    if(match) {
      if(pattern.exec(source))throw new Error(`Ambiguous principal copy in ${file}`);
      const query=match[0],db=match[4],accounts=match[1],user=match[6];
      source=source.slice(0,match.index)+source.slice(match.index).replace(query,query.replace(`id:${db}.Qr.account.id`,`id:${db}.Qr.account.id,subject:${db}.Qr.account.accountId`));
      const end=source.indexOf('.limit(1)]);return',match.index);if(end<0)throw new Error('Unknown principal query boundary');
      const offset=end+'.limit(1)]);'.length;
      source=source.slice(0,offset)+`if(!require(${JSON.stringify(guardPath)}).principal(${accounts},${user}.user))return null;`+source.slice(offset);principals++;
    }
    if(source!==original){new vm.Script(source,{filename:file});changed.push([file,source]);}
  }
}
scan(root);
if(profiles!==2||principals!==5)throw new Error(`Frozen native auth layout mismatch: profile=${profiles},principal=${principals}`);
for(const [file,source] of changed)fs.writeFileSync(file,source);
console.log(JSON.stringify({profiles,principals,changed:changed.map(([file])=>file)}));
