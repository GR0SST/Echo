#!/usr/bin/env python3
"""Real profile-scoped browsing-data ABI smoke, using a debug acceptance bundle."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess as S
import sys
import tempfile
import threading
import time

ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--cef-dist',required=True)
p.add_argument('--bundle',default=os.environ.get('ECHO_CEF_BROWSING_DATA_BUNDLE',str(ROOT/'target/bundle/Echo.app')))
p.add_argument('--output',required=True)
p.add_argument('--locked',action='store_true')
a=p.parse_args()
if not a.locked:
    raise SystemExit(S.run(['lockf','-k','-t','120','/tmp/echo-screen.lock','gtimeout','-k','3','120',sys.executable,__file__,*sys.argv[1:],'--locked']).returncode)
output=Path(a.output).resolve();output.mkdir(parents=True,exist_ok=True)
evidence=Path(tempfile.mkdtemp(prefix='browsing-data-',dir=output));runtime=evidence/'runtime';runtime.mkdir()
profile=runtime/'browser-drive-profile';profile.mkdir()
(profile/'session.json').write_text(json.dumps({'format_version':2,'next_id':4,'default_user_profile_id':1,'next_user_profile_id':2,'active_space':0,'archive':[],
 'profiles':{'1':{'id':1,'name':'Echo Test','archive_after':None}},'settings':{'theme_appearance':'Dark','air_traffic_control':None},
 'spaces':[{'id':1,'user_profile_id':1,'name':'Echo Test','hue':250.0,'pinned':[],'today':[{'id':2,'title':'Fixture','url':'about:blank','last_active':{'secs_since_epoch':int(time.time()),'nanos_since_epoch':0}}],'active_tab':2}]}))
app=evidence/'source.app';S.run(['cp','-cR',str(Path(a.bundle).resolve()),str(app)],check=True)
framework_name='Chromium Embedded Framework.framework';binary='Chromium Embedded Framework'
dist=Path(a.cef_dist).resolve();framework=(dist/framework_name).resolve()
# The acceptance executable may precede this source build; always freeze the
# entire selected framework, never rely on its older packaged library.
target=app/'Contents/Frameworks'/framework_name
if target.is_symlink():target.unlink()
else:shutil.rmtree(target)
S.run(['cp','-cR',str(framework),str(app/'Contents/Frameworks'/framework_name)],check=True)
expected=hashlib.sha256((framework/binary).read_bytes()).hexdigest()
assert hashlib.sha256((app/'Contents/Frameworks'/framework_name/binary).read_bytes()).hexdigest()==expected
reports=[]
html='''<!doctype html><title>Echo browsing-data ABI smoke</title><style>body{background:#fff;color:#111;font:18px system-ui;padding:40px}</style><h1>Echo browsing-data ABI smoke</h1><pre id="state">Working…</pre><script>
(async()=>{try{const phase=new URL(location).searchParams.get('phase')||'seed';if(phase==='seed'){document.cookie='echo_abi=seed; Max-Age=600; SameSite=Lax; Path=/';localStorage.setItem('echo-abi','seed');await new Promise((ok,no)=>{let r=indexedDB.open('echo-abi',1);r.onupgradeneeded=()=>r.result.createObjectStore('values');r.onsuccess=()=>{r.result.close();ok()};r.onerror=()=>no(r.error)});await(await caches.open('echo-abi')).put('/local-fixture',new Response('seed'));}let state={phase,cookie:document.cookie.includes('echo_abi=seed'),local:localStorage.getItem('echo-abi'),databases:(await indexedDB.databases()).map(x=>x.name).filter(x=>x==='echo-abi'),caches:(await caches.keys()).filter(x=>x==='echo-abi')};document.getElementById('state').textContent=JSON.stringify(state,null,2);await fetch('/report',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(state)});}catch(e){await fetch('/report',{method:'POST',body:JSON.stringify({error:String(e)})})}})();</script>'''
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body=html.encode();self.send_response(200);self.send_header('Content-Type','text/html');self.send_header('Cache-Control','no-store');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    def do_POST(self):
        reports.append(json.loads(self.rfile.read(int(self.headers.get('Content-Length','0')))));self.send_response(204);self.end_headers()
    def log_message(self,*_):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);server.daemon_threads=True;threading.Thread(target=server.serve_forever,daemon=True).start()
origin=f'http://127.0.0.1:{server.server_port}/'
environment=dict(os.environ,TMPDIR=str(runtime),BROWSER_DRIVE_BUNDLE_PATH=str(app),BROWSER_DRIVE_STRICT_EXIT='1',BROWSER_BROWSING_DATA_FIXTURE='1',ECHO_SHUTDOWN_TRACE='1',BROWSER_DRIVE_CEF_LOG_PATH=str(evidence/'cef.log'))
script=f'wait 2; activate; resize 1000 760; position 100 100; require-focus; key cmd-t; type {origin}?phase=seed; key enter; wait 2; shot {evidence}/seed.png; browsing-data-probe 1 2 -1; key cmd-l; type {origin}?phase=after; key enter; wait 2; shot {evidence}/after.png; quit'
result={'passed':False,'framework_sha256':expected,'dist':str(dist),'fixture_bundle':str(Path(a.bundle).resolve()),'evidence':str(evidence),
        'fixture_executable_sha256':hashlib.sha256((app/'Contents/MacOS/Echo').read_bytes()).hexdigest()}
try:
    with (evidence/'native.log').open('w') as log:code=S.run(['scripts/drive.sh',script,'110'],cwd=ROOT,env=environment,stdout=log,stderr=S.STDOUT).returncode
    assert code==0,f'Browsing-data acceptance drive failed: {code}'
    text=(evidence/'native.log').read_text()
    assert '[browsing-data-probe] profile=1 categories=2 since=-1 failed_bits=0' in text,'No successful real ABI callback; use a debug bundle with the browsing-data-probe acceptance command'
    assert '[quit] CEF Complete' in text and '[drive] application exited with status 0' in text,'No clean CEF shutdown'
    assert reports==[{'phase':'seed','cookie':True,'local':'seed','databases':['echo-abi'],'caches':['echo-abi']},{'phase':'after','cookie':False,'local':None,'databases':[],'caches':[]}],reports
    copied=runtime/'browser-drive.app/Contents/Frameworks'/framework_name/binary
    assert hashlib.sha256(copied.read_bytes()).hexdigest()==expected,'Drive substituted the framework'
    result.update(passed=True,failed_bits=0,strict_exit=0)
finally:
    server.shutdown();server.server_close()
    result['reports']=reports
    (evidence/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2),flush=True)
