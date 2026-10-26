#!/usr/bin/env python3
"""Real Store extensions: native event routing and own-window stack captures.

The canonical caller owns the long screen lock. The established native event
driver runs both actions; inspect the PNGs before writing operator.json.
"""
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess as sp
import time

ROOT = Path(__file__).resolve().parents[1]
DIST = Path(os.environ['ECHO_CEF_DIST']).resolve()
BUNDLE = Path(os.environ['ECHO_CEF_TEST_BUNDLE']).resolve()
PROFILE = Path(os.environ['ECHO_CEF_REAL_EXTENSION_PROFILE']).resolve()
assert os.environ.get('ECHO_CEF_NATIVE_SESSION') == '1' and Path('/tmp/echo-screen-long').exists()
assert (PROFILE / 'session.json').is_file()
name = time.strftime('real-extensions-%Y%m%dT%H%M%SZ', time.gmtime())
OUT = Path.home() / 'Library/Caches/EchoCEFQualification' / name
OUT.mkdir()
RUNTIME = OUT / 'runtime'
RUNTIME.mkdir()
app = RUNTIME / 'Echo-real-native.app'
sp.run(['cp', '-cR', str(BUNDLE), str(app)], check=True)
identifier = 'dev.echo.browser.qualification.' + name
sp.run(['plutil', '-replace', 'CFBundleIdentifier', '-string', identifier, str(app / 'Contents/Info.plist')], check=True)
sp.run(['/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister', '-f', str(app)], check=True)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


hashes = {'framework_sha256': digest(app / 'Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework'),
          'executable_sha256': digest(app / 'Contents/MacOS/Echo')}
assert hashes['framework_sha256'] == json.loads((DIST / 'build.json').read_text())['framework_sha256']
shot = lambda label: f'shotstack {RUNTIME / (label + ".png")}'
action = lambda title: f'key cmd-t; type Extension {title}; wait 1; key enter; wait 3'
script = '; '.join([
    'activate; resize 1000 760; position 100 1300; require-focus; key ctrl-1',
    'key cmd-t; type https://www.rfc-editor.org/rfc/rfc2606.html; key enter; wait 6',
    action('Dark Reader'), shot('reader-popup'), 'click 272 106; wait 2; key cmd-w; wait 1', shot('reader-light'),
    action('Dark Reader'), 'click 220 106; wait 2; key cmd-w; wait 1', shot('reader-dark'),
    action('uBlock Origin Lite'), shot('ubol-popup'),
    'click 145 279; wait 2; pointer 500 210; wait 1', shot('ubol-hover'),
    'click 500 210; wait 1', shot('ubol-zapped'),
    'key escape; key cmd-r; wait 4; key ctrl-2; wait 2; key cmd-e; wait 2',
    shot('profile-isolated'), 'dump; key escape; key ctrl-1; wait 1; dump; quit'])
log = OUT / 'native.log'
ready = {'app': str(app), 'identifier': identifier, 'case': str(OUT), 'screenshots': str(RUNTIME), **hashes}
(OUT / 'ready.json').write_text(json.dumps(ready, indent=2) + '\n')


def interrupted(signum, frame):
    raise SystemExit(128 + signum)


signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)
child = sp.Popen(['open', '-n', '-W', '-a', str(app), '--stdout', str(OUT / 'stdout.log'), '--stderr', str(log),
    '--env', 'BROWSER_DATA_DIR=' + str(PROFILE), '--env', 'BROWSER_SCRIPT=' + script,
    '--env', 'ECHO_SHUTDOWN_TRACE=1', '--args', '--in-process-gpu'], start_new_session=True)
try:
    end = time.monotonic() + 150
    print(json.dumps({'running': str(OUT / 'ready.json')}), flush=True)
    code = child.wait(timeout=max(1, end - time.monotonic()))
    text = log.read_text()
    assert code == 0 and '[quit] CEF Complete' in text
    assert not any(error in text for error in ('[automation] aborted:', '[automation] rejected:', '[automation] unknown command', 'Check failed:'))
    print(json.dumps({'awaiting_operator_receipt': str(OUT / 'operator.json')}), flush=True)
    receipt_end = time.monotonic() + 90
    while not (OUT / 'operator.json').exists() and time.monotonic() < receipt_end:
        time.sleep(.1)
    receipt = json.loads((OUT / 'operator.json').read_text())
    assert receipt['passed'] is True and all(receipt['checks'].get(key) is True
        for key in ('dark_reader_https', 'ubol_https', 'profile_isolation'))
    assert receipt['artifacts'] and all(Path(path).is_file() for path in receipt['artifacts'])
    report = {**receipt, **hashes, 'launcher_exit': code, 'engine_complete': True, 'evidence': str(OUT)}
    (DIST / 'real-extension-qualification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)
except BaseException:
    for line in sp.check_output(['ps', '-axo', 'pid=,comm='], text=True).splitlines():
        parts = line.strip().split(None, 1)
        if len(parts) == 2 and parts[1] == str(app / 'Contents/MacOS/Echo'):
            os.kill(int(parts[0]), signal.SIGTERM)
    if child.poll() is None:
        child.kill()
    child.wait()
    raise
