#!/usr/bin/env python3
"""Generate the one release configuration using CEF's official GN helpers.

The automate-git sync prepares standard CEF patches before our custom series.
Do not rerun its patcher over that series: overlapping custom changes make an
already-applied base patch fail its textual reverse check.
"""
import pathlib
import subprocess
import sys

source = pathlib.Path(sys.argv[1]).resolve()
sys.path.insert(0, str(source / 'cef/tools'))
from gn_args import GetAllPlatformConfigs, GetConfigFileContents

configs = GetAllPlatformConfigs({})
if set(configs) != {'Release_GN_arm64'}:
    raise SystemExit(f'Expected only Release_GN_arm64, got {list(configs)}')
for name, args in configs.items():
    output = source / 'out' / name
    output.mkdir(parents=True, exist_ok=True)
    path = output / 'args.gn'
    content = GetConfigFileContents(args)
    if not path.exists() or path.read_text() != content:
        path.write_text(content)
    subprocess.run(['gn', 'gen', str(output)], cwd=source, check=True)
