#!/usr/bin/env python3
"""Extract real Debian bookworm amd64 packages without executing target code."""
import hashlib
import lzma
import os
from pathlib import Path
import re
import subprocess
import urllib.request

base = 'https://deb.debian.org/debian/'
cache = Path(os.environ.get('ECHO_CROSS_CACHE', Path.home() / 'Library/Caches/Echo/cross'))
sdk = Path(os.environ.get('ECHO_LINUX_SYSROOT', cache / 'linux-bookworm-amd64'))
archives = cache / 'debs'
archives.mkdir(parents=True, exist_ok=True)
sdk.mkdir(parents=True, exist_ok=True)
index = cache / 'bookworm-amd64-Packages.xz'
urllib.request.urlretrieve(base + 'dists/bookworm/main/binary-amd64/Packages.xz', index)
packages = {}
for stanza in lzma.decompress(index.read_bytes()).decode().split('\n\n'):
    fields = {}
    for line in stanza.splitlines():
        if line and not line[0].isspace() and ': ' in line:
            key, value = line.split(': ', 1)
            fields[key] = value
    if 'Package' in fields:
        packages[fields['Package']] = fields
seeds = '''libasound2-dev libfontconfig1-dev libfreetype6-dev libxkbcommon-dev
libxkbcommon-x11-dev libwayland-dev libegl1-mesa-dev libgl1-mesa-dev
libx11-dev libx11-xcb-dev libxcb1-dev libxcb-shape0-dev libxcb-xfixes0-dev
libxcb-randr0-dev libxcb-render0-dev libxcb-shm0-dev libxcb-xinput-dev
libxcb-xkb-dev libxcursor-dev libxi-dev libxrandr-dev libudev-dev'''.split()
selected = {}
def visit(name):
    if name in selected:
        return
    item = packages[name]
    selected[name] = item
    for dep in (item.get('Pre-Depends', '') + ',' + item.get('Depends', '')).split(','):
        alternatives = [re.split(r'[ :(<\[]', part.strip())[0] for part in dep.split('|')]
        concrete = next((candidate for candidate in alternatives if candidate in packages), None)
        if concrete:
            visit(concrete)
for seed in seeds:
    visit(seed)
manifest = []
for name, item in sorted(selected.items()):
    archive = archives / Path(item['Filename']).name
    if not archive.exists() or hashlib.sha256(archive.read_bytes()).hexdigest() != item['SHA256']:
        print('Downloading', name, item['Version'], flush=True)
        urllib.request.urlretrieve(base + item['Filename'], archive)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != item['SHA256']:
        raise RuntimeError(f'Checksum mismatch: {archive}')
    # macOS bsdtar understands both ar (.deb) and its nested data tar archive.
    members = subprocess.check_output(['/usr/bin/tar', '-tf', str(archive)], text=True).splitlines()
    data = next(member for member in members if member.startswith('data.tar'))
    payload = subprocess.check_output(['/usr/bin/tar', '-xOf', str(archive), data])
    subprocess.run(['/usr/bin/tar', '-xf', '-', '-C', str(sdk)], input=payload, check=True)
    manifest.append(f"{name}\t{item['Version']}\t{item['SHA256']}\t{base}{item['Filename']}")
(sdk / 'echo-packages.tsv').write_text('\n'.join(manifest) + '\n')
print(f'Extracted {len(selected)} packages into {sdk}')
