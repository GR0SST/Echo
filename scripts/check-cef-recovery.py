#!/usr/bin/env python3
"""Force a bounded network timeout, then verify native cookie/storage recovery."""
import argparse
import hashlib
import http.cookies
import http.server
import json
import os
from pathlib import Path
import subprocess as S
import sys
import tempfile
import threading
import time
import urllib.parse

ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--cef-dist',required=True)
p.add_argument('--bundle',default=str(ROOT/'target/bundle/Echo.app'))
p.add_argument('--output',required=True)
p.add_argument('--locked',action='store_true')
a=p.parse_args()
if not a.locked:
    raise SystemExit(S.run(['lockf','-k','-t','120','/tmp/echo-screen.lock','gtimeout','-k','3','180',sys.executable,__file__,*sys.argv[1:],'--locked']).returncode)
output=Path(a.output).resolve();output.mkdir(parents=True,exist_ok=True)
evidence=Path(tempfile.mkdtemp(prefix='recovery-',dir=output));runtime=evidence/'runtime';runtime.mkdir()
profile=runtime/'browser-drive-profile';profile.mkdir()
root=profile/'Chromium'
(profile/'session.json').write_text(json.dumps({'format_version':2,'next_id':4,'default_user_profile_id':1,'next_user_profile_id':2,'active_space':0,'archive':[],
 'profiles':{'1':{'id':1,'name':'Echo Test','archive_after':None}},'settings':{'theme_appearance':'Dark'},
 'spaces':[{'id':1,'user_profile_id':1,'name':'Echo Test','hue':250.0,'pinned':[],'today':[{'id':2,'title':'Fixture','url':'about:blank','last_active':{'secs_since_epoch':int(time.time()),'nanos_since_epoch':0}}],'active_tab':2}]}))
app=evidence/'source.app';S.run(['cp','-cR',str(Path(a.bundle).resolve()),str(app)],check=True)
framework='Chromium Embedded Framework.framework/Chromium Embedded Framework'
expected=hashlib.sha256((Path(a.cef_dist).resolve()/framework).read_bytes()).hexdigest()
assert hashlib.sha256((app/'Contents/Frameworks'/framework).read_bytes()).hexdigest()==expected,'Recovery bundle differs from selected CEF'
observations=[];lock=threading.Lock()
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        url=urllib.parse.urlparse(self.path);query=urllib.parse.parse_qs(url.query)
        cookie=http.cookies.SimpleCookie(self.headers.get('Cookie',''))
        with lock: observations.append({'path':url.path,'cookie':cookie['echo_recovery'].value if 'echo_recovery' in cookie else '', 'storage':query.get('storage',[''])[0], 'indexeddb':query.get('idb',[''])[0]})
        self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Cache-Control','no-store')
        if url.path=='/seed':self.send_header('Set-Cookie','echo_recovery=sentinel; Max-Age=86400; Path=/; HttpOnly; SameSite=Lax')
        self.end_headers()
        if url.path=='/seed':self.wfile.write(b'<title>Recovery seeded</title><h1>Cookie and localStorage seeded</h1><script>localStorage.setItem("echo-recovery","sentinel");const q=indexedDB.open("echo-recovery",1);q.onupgradeneeded=()=>q.result.createObjectStore("values");q.onsuccess=()=>{const db=q.result;const tx=db.transaction("values","readwrite");tx.objectStore("values").put("sentinel","key");tx.oncomplete=()=>{db.close();fetch("/stored?idb=sentinel&storage="+localStorage.getItem("echo-recovery"))}}</script>')
        elif url.path=='/verify':self.wfile.write(b'<title>Recovery verified</title><h1 id=r>Verifying retained storage</h1><script>const v=localStorage.getItem("echo-recovery");document.querySelector("#r").textContent="Retained localStorage: "+v;const q=indexedDB.open("echo-recovery",1);q.onsuccess=()=>{const db=q.result;const r=db.transaction("values").objectStore("values").get("key");r.onsuccess=()=>{fetch("/observed?idb="+r.result+"&storage="+v);db.close()}}</script>')
        else:self.wfile.write(b'OK')
    def log_message(self,*_):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
origin=f'http://127.0.0.1:{server.server_port}'
environment={key:value for key,value in os.environ.items() if not key.startswith('ECHO_TEST_NETWORK_') and key!='ECHO_CEF_TEST_MODE'}
environment.update(TMPDIR=str(runtime),BROWSER_DRIVE_BUNDLE_PATH=str(app),BROWSER_DRIVE_STRICT_EXIT='1',ECHO_SHUTDOWN_TRACE='1')
def drive(phase,script,extra=None):
    env=dict(environment,BROWSER_DRIVE_LOG_PATH=str(evidence/f'{phase}-engine.log'),BROWSER_DRIVE_CEF_LOG_PATH=str(evidence/f'{phase}-cef.log'))
    env.update(extra or {})
    with (evidence/f'{phase}.log').open('w') as stream: result=S.run(['scripts/drive.sh',script,'110'],cwd=ROOT,env=env,stdout=stream,stderr=S.STDOUT,timeout=115)
    assert result.returncode==0,f'{phase} exited {result.returncode}'
    text=(evidence/f'{phase}-engine.log').read_text();assert '[quit] CEF Complete' in text,phase
    return text
report={'passed':False,'framework_sha256':expected,'cef_dist':str(Path(a.cef_dist).resolve()),'evidence':str(evidence)}
try:
    drive('forced',f'wait 2; activate; resize 1000 760; position 100 100; require-focus; key cmd-t; type {origin}/seed; key enter; wait 3; shot {evidence}/seeded.png; dump; quit',
          {'ECHO_CEF_TEST_MODE':'1','ECHO_TEST_NETWORK_WAIT_MS':'500','ECHO_TEST_NETWORK_DELAY_MS':'5000'})
    marker=root/'echo-dirty-shutdown.json';dirty=json.loads(marker.read_text())
    assert dirty['schema']==1 and dirty['helper_terminated'] is True,dirty
    helper=S.run(['ps','-p',str(dirty['network_pid']),'-o','command='],text=True,capture_output=True).stdout
    assert not helper or str(runtime) not in helper,'Timed-out owned helper survived its parent'
    cef=(evidence/'forced-cef.log').read_text(errors='replace')
    assert '[echo-shutdown] network-test-delay' in cef and 'recorded=1' in cef and 'terminated=1' in cef,cef[-2000:]
    assert any(r['path']=='/stored' and r['cookie']=='sentinel' and r['storage']=='sentinel' and r['indexeddb']=='sentinel' for r in observations),observations
    report['forced_shutdown']={'exit':0,'marker':dirty,'helper_terminated':True}
    drive('restart',f'wait 3; activate; require-focus; key cmd-t; type {origin}/verify; key enter; wait 3; shot {evidence}/verified.png; dump; quit')
    receipt=json.loads((root/'echo-shutdown-recovery.json').read_text())
    assert receipt['integrity']['passed'] and receipt['integrity']['sqlite_databases']>=1 and receipt['integrity']['leveldb_databases']>=1,receipt
    assert not marker.exists(),'Verified startup did not clear its dirty marker'
    assert any(r['path']=='/observed' and r['cookie']=='sentinel' and r['storage']=='sentinel' and r['indexeddb']=='sentinel' for r in observations),observations
    report['recovery']=receipt;report['cookie_preserved']=True;report['local_storage_preserved']=True;report['indexeddb_preserved']=True
    # Chromium uses an origin-prefixed filename for first-party IndexedDB and
    # this plain filename for bucket/third-party storage (file_path_util.cc).
    # Clone a closed, real database to exercise both engine comparator layouts.
    bucket_root=evidence/'bucket-layout'
    bucket=bucket_root/'Default/WebStorage/42/IndexedDB/indexeddb.leveldb'
    bucket.parent.mkdir(parents=True)
    original=next(root.rglob('*.indexeddb.leveldb'))
    S.run(['cp','-cR',str(original),str(bucket)],check=True)
    checked=S.run([str(app/'Contents/MacOS/Echo'),'--echo-verify-storage',str(bucket_root)],text=True,capture_output=True,timeout=60)
    (evidence/'bucket-verifier.log').write_text(checked.stdout+checked.stderr)
    bucket_result=json.loads(checked.stdout)
    assert checked.returncode==0 and bucket_result['passed'] and bucket_result['leveldb_databases']==1 and bucket_result['entries_checked']>0,bucket_result
    report['bucket_indexeddb']=bucket_result
    corrupt=evidence/'corrupt-storage';S.run(['cp','-cR',str(root),str(corrupt)],check=True)
    cookies=corrupt/'Default/Network/Cookies'
    if not cookies.exists():cookies=corrupt/'Default/Cookies'
    assert cookies.exists()
    with cookies.open('r+b') as stream:stream.write(b'corrupt fixture!')
    damaged_hash=hashlib.sha256(cookies.read_bytes()).hexdigest()
    rejected=S.run([str(app/'Contents/MacOS/Echo'),'--echo-verify-storage',str(corrupt)],text=True,capture_output=True,timeout=60)
    (evidence/'corrupt-verifier.log').write_text(rejected.stdout+rejected.stderr)
    rejection=json.loads(rejected.stdout)
    assert rejected.returncode!=0 and not rejection['passed'],rejection
    assert hashlib.sha256(cookies.read_bytes()).hexdigest()==damaged_hash,'Verifier modified a corrupt database'
    report.update(passed=True,corruption_rejected_without_modification=True,corrupt_result=rejection)
finally:
    server.shutdown();server.server_close()
    report['requests']=observations
    (evidence/'result.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2),flush=True)
