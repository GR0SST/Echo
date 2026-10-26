#!/usr/bin/env python3
"""Bounded native PiP acceptance against an exact, isolated CEF distribution."""
import argparse
import ctypes as C
import hashlib
import http.server
import json
import os
from pathlib import Path
import plistlib
import re
import subprocess as S
import sys
import tempfile
import threading
import time
import urllib.parse

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--bundle', default=str(ROOT / 'target/bundle/Echo.app'))
p.add_argument('--cef-dist', default=str(Path.home() / 'Documents/Bot/cef-src/echo-extensions-dist'))
p.add_argument('--output', default=str(Path.home() / 'Library/Caches/EchoCEFQualification'))
p.add_argument('--locked', action='store_true')
a = p.parse_args()
if not a.locked:
    raise SystemExit(S.run(['lockf', '-k', '-t', '120', '/tmp/echo-screen.lock', 'gtimeout', '-k', '3', '180', sys.executable, __file__, *sys.argv[1:], '--locked']).returncode)
if sys.platform != 'darwin':
    raise SystemExit('This native acceptance runner requires macOS')
output = Path(a.output).resolve()
if output.is_relative_to(Path.home() / 'Documents'):
    p.error('Keep qualification outside ~/Documents; use ~/Library/Caches/EchoCEFQualification')
output.mkdir(parents=True, exist_ok=True)
evidence = Path(tempfile.mkdtemp(prefix='pip-', dir=output))
runtime = evidence / 'runtime'; runtime.mkdir()
profile = runtime / 'browser-drive-profile'; profile.mkdir()
# A local fixture must not start unrelated Internet pages from the demo session.
(profile/'session.json').write_text(json.dumps({
    'format_version':2,'next_id':4,'default_user_profile_id':1,'next_user_profile_id':2,
    'active_space':0,'archive':[],'profiles':{'1':{'id':1,'name':'Echo Test','archive_after':None}},
    'settings':{'theme_appearance':'Dark'},
    'spaces':[{'id':1,'user_profile_id':1,'name':'Echo Test','hue':250.0,'pinned':[],
               'today':[{'id':2,'title':'Fixture','url':'about:blank','last_active':{'secs_since_epoch':int(time.time()),'nanos_since_epoch':0}}],'active_tab':2}]}))
app = evidence / 'source.app'
S.run(['cp', '-cR', str(Path(a.bundle).resolve()), str(app)], check=True)
framework = 'Chromium Embedded Framework.framework/Chromium Embedded Framework'
expected = hashlib.sha256((Path(a.cef_dist).resolve() / framework).read_bytes()).hexdigest()
assert hashlib.sha256((app / 'Contents/Frameworks' / framework).read_bytes()).hexdigest() == expected, 'PiP bundle differs from canonical CEF'
fixture = Path.home() / 'Documents/Bot/cef-src/fixtures/sintel-trailer.mp4'
video = fixture.read_bytes()
assert hashlib.sha256(video).hexdigest() == 'b670602fa00934ca27c4351bb0efe7ea7a07fae57284e44226025eeed7c51254'
events = []; events_lock = threading.Lock()
html = '''<!doctype html><title>Echo CEF PiP Fixture</title><style>html,body{margin:0;width:100%;height:100%;background:#222}video{width:100%;height:100%;object-fit:contain}button{position:fixed;left:20px;top:20px;width:100px;height:40px;z-index:2}</style>
<video id="v" controls loop src="/fixture.mp4"></video><button onclick="v.play()">Play video</button><button style="left:140px" onclick="v.pause()">Pause video</button><button style="left:260px" onclick="v.requestPictureInPicture()">Picture in Picture</button>
<script>v.volume=.05;navigator.mediaSession.metadata=new MediaMetadata({title:'Echo CEF PiP Fixture'});navigator.mediaSession.setActionHandler('play',()=>v.play());navigator.mediaSession.setActionHandler('pause',()=>v.pause());
const update=()=>{navigator.mediaSession.playbackState=v.paused?'paused':'playing';if(Number.isFinite(v.duration)&&v.duration>0)navigator.mediaSession.setPositionState({duration:v.duration,playbackRate:v.playbackRate,position:Math.min(v.currentTime,v.duration)});};
for(const name of ['play','pause','loadedmetadata','timeupdate'])v.addEventListener(name,update);
for(const name of ['play','pause','enterpictureinpicture','leavepictureinpicture'])v.addEventListener(name,()=>{if(name==='enterpictureinpicture')document.title='Echo CEF PiP Fixture — Entered PiP';if(name==='leavepictureinpicture')document.title='Echo CEF PiP Fixture — Left PiP';fetch('/event?name='+name).catch(()=>{});});
navigator.mediaSession.setActionHandler('seekbackward',()=>v.currentTime=Math.max(0,v.currentTime-15));navigator.mediaSession.setActionHandler('seekforward',()=>v.currentTime=Math.min(v.duration,v.currentTime+15));navigator.mediaSession.setActionHandler('seekto',d=>v.currentTime=d.seekTime);
</script>'''.encode()
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        url = urllib.parse.urlparse(self.path)
        if url.path == '/event':
            with events_lock: events.append({'name': urllib.parse.parse_qs(url.query).get('name', [''])[0], 'time': time.monotonic()})
            self.send_response(204); self.end_headers(); return
        body = video if url.path == '/fixture.mp4' else html if url.path == '/video' else b'<!doctype html><title>PiP background tab</title><h1>Other tab</h1>'
        first, last = 0, len(body)-1
        match = re.fullmatch(r'bytes=(\d+)-(\d*)', self.headers.get('Range', '')) if url.path == '/fixture.mp4' else None
        if match: first, last = int(match[1]), min(int(match[2]) if match[2] else last, last)
        assert 0 <= first <= last < len(body)
        self.send_response(206 if match else 200)
        self.send_header('Content-Type', 'video/mp4' if url.path == '/fixture.mp4' else 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(last-first+1)); self.send_header('Accept-Ranges', 'bytes')
        if match: self.send_header('Content-Range', f'bytes {first}-{last}/{len(body)}')
        self.end_headers(); self.wfile.write(body[first:last+1])
    def log_message(self, *_): pass
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
url = f'http://127.0.0.1:{server.server_port}'
# PiP intentionally becomes key outside GPUI; its own CGWindow guard below
# validates every interaction instead of requiring the browser root to stay key.
script = f'wait 2; activate; resize 1000 760; position 100 100; wait 1; key cmd-t; type {url}/video; key enter; wait 6; click 298 50; wait 3; dump; key cmd-t; type {url}/other; key enter; wait 5; dump; pointer 114 694; wait .8; shot {evidence}/mini.png; wait 32; dump; windows; click 538 50; wait 4; dump; wait 22; dump; quit'
env = dict(os.environ, TMPDIR=str(runtime), BROWSER_DRIVE_BUNDLE_PATH=str(app), BROWSER_DRIVE_STRICT_EXIT='1', ECHO_SHUTDOWN_TRACE='1', BROWSER_DRIVE_LOG_PATH=str(evidence/'engine.log'), BROWSER_DRIVE_CEF_LOG_PATH=str(evidence/'cef.log'))
log = (evidence/'driver.log').open('w')
driver = S.Popen(['scripts/drive.sh', script, '160'], cwd=ROOT, env=env, stdout=log, stderr=S.STDOUT)
CG = C.CDLL('/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics'); CF = C.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')
CG.CGWindowListCopyWindowInfo.argtypes=[C.c_uint32,C.c_uint32]; CG.CGWindowListCopyWindowInfo.restype=C.c_void_p
CF.CFPropertyListCreateData.argtypes=[C.c_void_p,C.c_void_p,C.c_long,C.c_ulong,C.c_void_p]; CF.CFPropertyListCreateData.restype=C.c_void_p
CF.CFDataGetLength.argtypes=[C.c_void_p]; CF.CFDataGetLength.restype=C.c_long
CF.CFDataGetBytePtr.argtypes=[C.c_void_p]; CF.CFDataGetBytePtr.restype=C.POINTER(C.c_ubyte); CF.CFRelease.argtypes=[C.c_void_p]
def windows():
    raw=CG.CGWindowListCopyWindowInfo(1|16,0); data=CF.CFPropertyListCreateData(None,raw,100,0,None)
    try: return plistlib.loads(C.string_at(CF.CFDataGetBytePtr(data),CF.CFDataGetLength(data)))
    finally: CF.CFRelease(data); CF.CFRelease(raw)
def until(predicate, seconds, message):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        result=predicate()
        if result: return result
        if driver.poll() is not None: break
        time.sleep(.2)
    raise AssertionError(message)
def owned_pid():
    exe=str(runtime/'browser-drive.app/Contents/MacOS/Echo'); found=[]
    for row in S.check_output(['ps','-axo','pid=,command='],text=True).splitlines():
        parts=row.strip().split(None,1)
        if len(parts)==2 and os.path.normpath(parts[1].split()[0])==exe: found.append(int(parts[0]))
    assert len(found)<=1,found
    return found[0] if found else None
result={'passed':False,'framework_sha256':expected,'executable_sha256':hashlib.sha256((app/'Contents/MacOS/Echo').read_bytes()).hexdigest(),'cef_dist':str(Path(a.cef_dist).resolve()),'evidence':str(evidence)}
try:
    pid=until(owned_pid,20,'Private Echo did not start')
    def pip_windows(): return [w for w in windows() if w.get('kCGWindowOwnerPID')==pid and w.get('kCGWindowLayer')==3 and w.get('kCGWindowName')=='Picture in Picture']
    pip=until(pip_windows,30,'Automatic PiP did not open'); assert len(pip)==1,pip
    window=pip[0]
    # The driver moves the pointer away before its long idle wait. Do not race
    # that move with a held control click: MouseExited correctly hides controls.
    until(lambda:(evidence/'mini.png').exists(),15,'Driver did not reach its idle input barrier')
    window=until(lambda:next((w for w in pip_windows() if w['kCGWindowBounds']['Width']==224 and w['kCGWindowBounds']['Height']==126),None),3,'Automatic PiP did not settle at 224x126')
    def geometry():
        b=window['kCGWindowBounds']; assert (b['Width'],b['Height'])==(224,126),b
        return int(b['X']),int(b['Y'])
    x,y=geometry(); center=(x+112,y+63)
    def check(point=None):
        px,py=point or center
        for row in windows():
            b=row['kCGWindowBounds']
            if row.get('kCGWindowAlpha',1)<=0: continue
            if row.get('kCGWindowOwnerName')=='Notification Center' and b['Width']==3840 and b['Height']==2160: continue
            if b['X']<=px<b['X']+b['Width'] and b['Y']<=py<b['Y']+b['Height']:
                assert row['kCGWindowNumber']==window['kCGWindowNumber'],f'Owned PiP covered: {row}'
                return
        raise AssertionError('Owned PiP is no longer visible')
    def mouse(*args): S.run(['cliclick',*args],check=True,timeout=5)
    def shot(name):
        check(); S.run(['screencapture','-x',f'-R{x},{y},224,126',str(evidence/name)],check=True,timeout=10)
    def key(code):
        focused=S.check_output(['osascript','-e','with timeout of 5 seconds','-e','tell application "System Events"',
            '-e',f'set owned to first process whose unix id is {pid}',
            '-e','set frontmost of owned to true',
            '-e','perform action "AXRaise" of (first window of owned whose name is "Picture in Picture")',
            '-e','return name of (value of attribute "AXFocusedWindow" of owned)',
            '-e','end tell','-e','end timeout'],text=True,timeout=8).strip()
        result.setdefault('keyboard_focused_windows',[]).append(focused)
        assert focused=='Picture in Picture',f'Owned PiP is not the keyboard target: {focused}'
        # AXRaise updates NSWindow key state before Views' activation callback
        # necessarily runs. Let its focus manager receive that activation.
        time.sleep(.3)
        foreground=int(S.check_output(['osascript','-e','tell application "System Events" to get unix id of first process whose frontmost is true'],text=True,timeout=5).strip())
        result.setdefault('keyboard_foreground_pids',[]).append(foreground)
        assert foreground==pid,f'PiP keyboard precondition failed: foreground PID {foreground}, expected owned Echo {pid}'
        check(); S.run(['osascript','-e','with timeout of 5 seconds','-e',f'tell application "System Events" to tell (first process whose unix id is {pid}) to key code {code}','-e','end timeout'],check=True,timeout=8)
    result['automatic_bounds']=window['kCGWindowBounds']; shot('idle.png')
    check(); mouse(f'm:{center[0]},{center[1]}'); time.sleep(.8); shot('hover.png')
    check(); mouse(f'c:{center[0]},{center[1]}'); time.sleep(.3)
    before=time.monotonic(); key(49)
    until(lambda:any(event['name']=='pause' and event['time']>before for event in events),3,'Space did not pause the actual video')
    time.sleep(.5); shot('keyboard.png')
    check(); mouse('m:700,600'); time.sleep(.3); check(); mouse(f'm:{center[0]},{center[1]}'); time.sleep(.5); shot('pointer-restored.png')
    def reveal():
        check(); mouse(f'c:{center[0]},{center[1]}'); key(48); time.sleep(.4); shot('tab-reveal.png')
    def press(offset, name):
        point=(x+offset,y+17); check(point); mouse(f'm:{point[0]},{point[1]}'); time.sleep(.3); shot(f'before-{name}.png')
        # Capturing a screenshot can outlast Chromium's controls idle timer on
        # a busy machine. Move immediately before the click so the photographed
        # control is still revealed when the actual input arrives. These are
        # Chromium Views buttons, so send a normal click rather than a drag.
        # A move to the unchanged coordinate need not produce MouseMoved.
        # Stay inside PiP and give Chromium one event-loop turn to reveal again.
        check(point); mouse(f'm:{point[0]+2},{point[1]+2}','w:50',f'm:{point[0]},{point[1]}',f'c:{point[0]},{point[1]}')
        until(lambda:not any(w.get('kCGWindowNumber')==window['kCGWindowNumber'] for w in windows()),3,f'{name} did not close owned PiP')
    reveal(); press(184,'return'); result['return_passed']=True
    manual=until(pip_windows,40,'Manual HTML PiP did not reopen'); assert len(manual)==1,manual
    time.sleep(.7); window=next(w for w in windows() if w['kCGWindowNumber']==manual[0]['kCGWindowNumber'])
    x,y=geometry(); center=(x+112,y+63); result['manual_bounds']=window['kCGWindowBounds']
    assert result['manual_bounds']==result['automatic_bounds'],'PiP re-entry moved or resized the window'
    reveal(); press(208,'close'); result['close_passed']=True
    assert driver.wait(timeout=100)==0,'Echo did not exit cleanly'
    text=(evidence/'engine.log').read_text(); assert '[quit] CEF Complete' in text
    cef_text=(evidence/'cef.log').read_text(errors='replace')
    assert '[echo-shutdown] network-cleanup-complete' in cef_text and 'all_exited=1' in cef_text and 'all_exited=0' not in cef_text and '[echo-shutdown] dirty-shutdown' not in cef_text,'PiP quit used an incomplete network shutdown'
    assert any('*shown "Echo CEF PiP Fixture' in row and url+'/video' in row for row in text.splitlines()),'Back to Tab did not select retained source'
    assert sum(event['name']=='enterpictureinpicture' for event in events)>=2,events
    assert sum(event['name']=='leavepictureinpicture' for event in events)>=2,events
    result.update(passed=True,space_paused=True,strict_exit=0,graceful_network_exit=True)
finally:
    if driver.poll() is None: driver.wait(timeout=170)
    server.shutdown();server.server_close();log.close()
    result['events']=events
    (evidence/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2),flush=True)
