#!/usr/bin/env python3
"""Bounded, operator-driven native Developer and real-extension acceptance.

Run only inside check-cef-canonical.ts's long screen session. The operator uses
normal native input, inspects the screenshots, and supplies operator.json before
quitting each app. No evaluator, debug protocol, or synthetic bridge result is
used. A LaunchServices launch makes native sheets addressable by accessibility;
the canonical direct-exec gates separately enforce actual application exit codes.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import struct
import subprocess as sp
import time


def interrupted(signum, frame):
    raise SystemExit(128 + signum)


signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)

ROOT = Path(__file__).resolve().parents[1]
DIST = Path(os.environ['ECHO_CEF_DIST']).resolve()
BUNDLE = Path(os.environ['ECHO_CEF_TEST_BUNDLE']).resolve()
SOURCE = Path(os.environ.get('ECHO_CEF_SOURCE', Path.home() / 'Documents/Bot/cef-src'))
assert os.environ.get('ECHO_CEF_NATIVE_SESSION') == '1'
assert Path('/tmp/echo-screen-long').exists(), 'The caller must hold the long screen lock'
OUT = SOURCE / 'qualification' / time.strftime('developer-final-%Y%m%dT%H%M%SZ', time.gmtime())
OUT.mkdir()
RUNTIME = ROOT / 'target/parity/extensions-ui/qualify/runtime' / OUT.name
RUNTIME.mkdir()
# A LaunchServices app has its own macOS privacy identity. Keep its session in
# the same unprotected test area as drive.sh; never require Documents access.
FIXTURE = Path.home() / 'Library/Caches/EchoQualification' / OUT.name / 'fixture'
FIXTURE.parent.mkdir(parents=True)
FRAMEWORK = 'Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework'


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


hashes = {'framework_sha256': digest(BUNDLE / FRAMEWORK),
          'executable_sha256': digest(BUNDLE / 'Contents/MacOS/Echo')}
assert hashes['framework_sha256'] == json.loads((DIST / 'build.json').read_text())['framework_sha256']


def session(name, profile, seconds, checks, setup):
    case = OUT / name
    case.mkdir()
    app = RUNTIME / (name + '.app')
    sp.run(['cp', '-cR', str(BUNDLE), str(app)], check=True)
    identifier = 'dev.echo.browser.qualification.' + name
    sp.run(['plutil', '-replace', 'CFBundleIdentifier', '-string', identifier,
            str(app / 'Contents/Info.plist')], check=True)
    sp.run(['/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister',
            '-f', str(app)], check=True)
    log = case / 'native.log'
    lease = case / 'lease.json'
    script = ('activate; resize 1000 760; position 100 1300; require-focus; '
              + setup + f'; wait {seconds}; activate; require-focus; dump; quit')
    ready = {'identifier': identifier, 'profile': str(profile), 'case': str(case),
             'checks': checks, 'fixture': str(FIXTURE), **hashes}
    (case / 'ready.json').write_text(json.dumps(ready, indent=2) + '\n')
    lease.write_text(json.dumps({'active': False, 'expires_at': 0}))
    command = ['open', '-n', '-W', '-a', str(app), '--stdout', str(case / 'stdout.log'),
               '--stderr', str(log), '--env', 'BROWSER_DATA_DIR=' + str(profile),
               '--env', 'BROWSER_SCRIPT=' + script, '--env', 'ECHO_SHUTDOWN_TRACE=1',
               '--args', '--in-process-gpu']
    child = sp.Popen(command, start_new_session=True)
    try:
        startup = time.monotonic() + 30
        while f'[automation] wait {seconds}' not in (log.read_text() if log.exists() else ''):
            if child.poll() is not None or time.monotonic() >= startup:
                raise RuntimeError('The owned native app did not reach its input barrier within 30 seconds')
            time.sleep(.1)
        lease.write_text(json.dumps({'active': True, 'expires_at': int((time.time() + seconds - 10) * 1000)}))
        print(json.dumps({'ready': str(case / 'ready.json')}), flush=True)
        code = child.wait(timeout=seconds + 80)
    except BaseException:
        # Only this immutable private copy; never terminate a live user browser.
        for line in sp.check_output(['ps', '-axo', 'pid=,comm='], text=True).splitlines():
            parts = line.strip().split(None, 1)
            if len(parts) == 2 and parts[1] == str(app / 'Contents/MacOS/Echo'):
                os.kill(int(parts[0]), signal.SIGTERM)
        child.kill()
        child.wait()
        raise
    finally:
        lease.write_text(json.dumps({'active': False, 'expires_at': 0}))
    text = log.read_text()
    assert code == 0 and '[quit] CEF Complete' in text, 'Native teardown did not complete'
    assert not any(error in text for error in ('[automation] aborted:', '[automation] rejected:',
                                              '[automation] unknown command', 'Check failed:'))
    receipt = json.loads((case / 'operator.json').read_text())
    assert receipt['passed'] is True and all(receipt['checks'].get(key) is True for key in checks)
    assert receipt['artifacts'] and all(Path(path).is_file() for path in receipt['artifacts'])
    return {**receipt, **hashes, 'evidence': str(case), 'launcher_exit': code,
            'engine_complete': True, 'completed_utc': time.strftime('%FT%TZ', time.gmtime())}


fixture = FIXTURE
shutil.copytree(ROOT / 'scripts/fixtures/extension-developer', fixture)
profile = RUNTIME / 'profile'
profile.mkdir()
(profile / 'session.json').write_text(json.dumps({
    'format_version': 2, 'next_id': 5, 'default_user_profile_id': 1,
    'next_user_profile_id': 3, 'active_space': 0, 'archive': [],
    'profiles': {'1': {'id': 1, 'name': 'Echo Test', 'archive_after': None},
                 '2': {'id': 2, 'name': 'Echo Test Isolation', 'archive_after': None}},
    'settings': {'theme_appearance': 'Dark'},
    'spaces': [{'id': i, 'user_profile_id': i, 'name': name, 'hue': 250,
                'pinned': [], 'today': [{'id': i + 2, 'title': 'Developer acceptance',
                    'url': 'https://www.rfc-editor.org/rfc/rfc2606.html',
                    'last_active': {'secs_since_epoch': int(time.time()), 'nanos_since_epoch': 0}}],
                'active_tab': i + 2}
               for i, name in [(1, 'Echo Test'), (2, 'Echo Test Isolation')]]}))
developer = session('developer', profile, 250,
    ['load_unpacked', 'update', 'worker_inspection', 'permissions', 'pack',
     'overwrite_confirmation', 'profile_isolation'],
    'key cmd-t; type browser://extensions; key enter; wait 2')
crx = fixture.with_suffix('.crx')
pem = fixture.with_suffix('.pem')
magic, version = struct.unpack('<4sI', crx.read_bytes()[:8])
assert magic == b'Cr24' and version == 3 and pem.stat().st_size > 100
developer['packed_crx_sha256'] = digest(crx)
(DIST / 'developer-qualification.json').write_text(json.dumps(developer, indent=2) + '\n')
sp.run(['python3', str(ROOT / 'scripts/check-cef-real-extensions.py')], check=True)
print(json.dumps({'passed': True, 'evidence': str(OUT), **hashes}), flush=True)
