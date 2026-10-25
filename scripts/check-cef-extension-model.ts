// Exercise the GPUI model through ordinary extension APIs in an owned profile.
import {cp, mkdir, mkdtemp, realpath} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {validate} from './cef-dist';
if (!Bun.argv.includes('--locked')) {
  const child=Bun.spawn(['lockf','-k','-t','120','/tmp/echo-screen.lock','gtimeout','-k','3','210','bun',import.meta.path,'--locked'],{stdout:'inherit',stderr:'inherit'});
  process.exit(await child.exited);
}
const root=resolve(import.meta.dir,'..');
const dist=await validate(process.env.ECHO_CEF_DIST ?? '');
const source=process.env.ECHO_CEF_SOURCE ?? join(process.env.HOME!,'Documents/Bot/cef-src');
const evidence=join(source,'qualification',`extension-model-${new Date().toISOString().replaceAll(':','-')}`);
await mkdir(evidence,{recursive:true});
const owned=await realpath(await mkdtemp(join(tmpdir(),'echo-model-check-')));
const profile=join(owned,'profile');await mkdir(profile);
await Bun.write(join(profile,'session.json'),JSON.stringify({format_version:2,next_id:4,default_user_profile_id:1,next_user_profile_id:2,
  active_space:0,archive:[],profiles:{'1':{id:1,name:'Echo Test',archive_after:null}},settings:{theme_appearance:'Dark'},
  spaces:[{id:1,user_profile_id:1,name:'Echo Test',hue:250,pinned:[],today:[{id:2,title:'Model acceptance',url:'https://www.rfc-editor.org/rfc/rfc2606.html',
    last_active:{secs_since_epoch:Math.floor(Date.now()/1000),nanos_since_epoch:0}}],active_tab:2}]}));
const app=join(owned,'Echo Model Check.app');
const copy=Bun.spawn(['cp','-cR',resolve(process.env.ECHO_CEF_TEST_BUNDLE ?? join(root,'target/bundle/Echo.app')),app]);if(await copy.exited)throw Error('Cannot freeze model acceptance app');
async function digest(path:string){const hash=new Bun.CryptoHasher('sha256');for await(const chunk of Bun.file(path).stream())hash.update(chunk);return hash.digest('hex');}
const framework='Chromium Embedded Framework.framework/Chromium Embedded Framework';
const frameworkHash=await digest(join(app,'Contents/Frameworks',framework));
if(frameworkHash!==await digest(join(dist,framework)))throw Error('Model acceptance bundle uses another framework');
let result:any;
const nonce=crypto.randomUUID();
const server=Bun.serve({hostname:'127.0.0.1',port:0,async fetch(request){
  if(new URL(request.url).pathname===`/result/${nonce}`&&request.method==='POST'){
    const body=await request.json();result=body.modelAcceptance;await Bun.write(join(evidence,'extension-result.json'),JSON.stringify(result,null,2)+'\n');return new Response(null,{status:204});
  }return new Response('not found',{status:404});
}});
const fixture=join(owned,'fixture');await cp(join(import.meta.dir,'fixtures/extension-model'),fixture,{recursive:true});
await Bun.write(join(fixture,'config.json'),JSON.stringify({reportUrl:`http://127.0.0.1:${server.port}/result/${nonce}`}));
const log=join(evidence,'native.log');
const browser=Bun.spawn([join(app,'Contents/MacOS/Echo'),'--in-process-gpu','--enable-logging',`--log-file=${join(evidence,'cef.log')}`],{
  env:{...process.env,BROWSER_DATA_DIR:profile,ECHO_EXTENSION_ACCEPTANCE:'1',ECHO_SHUTDOWN_TRACE:'1',
    BROWSER_SCRIPT:`wait 6; activate; resize 1000 760; position 100 100; require-focus; extension-load-fixture ${fixture}; wait 3; key cmd-r; wait 6; click 212 65; wait 2; click 129 186; wait 1; key cmd-h; wait 1; activate; wait 40; shotstack ${join(evidence,'model.png')}; dump; quit`},
  stdout:Bun.file(log),stderr:Bun.file(log)});
const timeout=setTimeout(()=>browser.kill('SIGKILL'),180000);
try {
  const exitCode=await browser.exited;const text=await Bun.file(log).text();
  const passed=exitCode===0&&result?.passed===true&&text.includes('[quit] CEF Complete')&&!text.includes('[automation] aborted:');
  const report={passed,exitCode,dist,framework_sha256:frameworkHash,executable_sha256:await digest(join(app,'Contents/MacOS/Echo')),evidence,owned,result};
  await Bun.write(join(evidence,'qualification.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report,null,2));if(!passed)process.exitCode=1;
} finally {clearTimeout(timeout);server.stop(true);if(browser.exitCode===null)browser.kill('SIGKILL');}
