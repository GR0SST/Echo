#!/usr/bin/env python3
"""Native regression: preload CEF before AppKit, reopen a copied test profile.

Usage: python3 scripts/check-cef-startup.py TEST_PROFILE [ATTEMPTS]
Run after `cargo run -p bundle`. Takes the shared screen lock itself. Never
uses or changes the owner's browser profile. Artifacts go under target/parity.
"""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile

if len(sys.argv) < 2:
    raise SystemExit(__doc__)
if sys.argv[-1] != '--locked':
    raise SystemExit(subprocess.run(['lockf', '-k', '/tmp/echo-screen.lock', sys.executable, __file__, *sys.argv[1:], '--locked']).returncode)
sys.argv.pop()
source = Path(sys.argv[1]).resolve()
assert (source / "session.json").is_file(), "An explicit Echo test profile is required"
attempts = int(sys.argv[2]) if len(sys.argv) > 2 else 3
assert 1 <= attempts <= 3, "Keep the shared screen session below 150 seconds"
root = Path(__file__).resolve().parents[1]
output = root / "target/parity/arc-max/startup-guard"
output.mkdir(parents=True, exist_ok=True)
results = []
for n in range(attempts):
    with tempfile.TemporaryDirectory(prefix='echo-cef-startup-') as tmp:
        work = Path(tmp)
        shutil.copytree(source, work / 'browser-drive-profile')
        env = dict(os.environ, TMPDIR=tmp, BROWSER_DRIVE_STRICT_EXIT='1')
        job = subprocess.Popen(['scripts/drive.sh', 'wait 1; dump; quit', '20'],
            cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            raw = job.communicate(timeout=28)[0]
        except subprocess.TimeoutExpired:
            os.killpg(job.pid, signal.SIGTERM)
            try: job.communicate(timeout=4)
            except subprocess.TimeoutExpired:
                os.killpg(job.pid, signal.SIGKILL)
                job.communicate()
            raise
        log = raw.decode(errors='replace')
        (output / f'launch-{n}.log').write_text(log)
        assert job.returncode == 0, log[-3000:]
        preload = log.index('[engine] framework preloaded before application')
        assert preload < log.index('[quit-native] Installed'), log[-3000:]
        assert '[automation]' in log and '[drive] application exited with status 0' in log
        results.append({'attempt': n, 'exit': job.returncode, 'preload_before_appkit': True})
(output / 'observations.json').write_text(json.dumps(results, indent=2))
print(json.dumps(results))
