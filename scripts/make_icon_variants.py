"""Render material variants of the unmodified Echo mark (requires rsvg-convert).

The source mark, its placement, and the Dock canvas match make_icon.py.
Only material, lighting, and background change. Original is never written.
Run: python3 scripts/make_icon_variants.py
"""
import base64
import math
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
MARK = base64.b64encode((ROOT / 'crates/bundle/assets/echo-mark.png').read_bytes()).decode()
OUT = ROOT / 'crates/browser/assets/app-icons'
OUT.mkdir(exist_ok=True)
# Same 100/1024 inset and continuous superellipse as the Original.
points = []
for i in range(2000):
    angle = math.tau * i / 2000
    c, s = math.cos(angle), math.sin(angle)
    points.append((96 + 77.25 * math.copysign(abs(c) ** .4, c),
                   96 + 77.25 * math.copysign(abs(s) ** .4, s)))
TILE = 'M' + ' L'.join(f'{x:.4f},{y:.4f}' for x, y in points) + 'Z'


def gradient(name, stops, x1='0', y1='0', x2='1', y2='1'):
    return f'<linearGradient id="{name}" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}">' + ''.join(
        f'<stop offset="{i / (len(stops) - 1):.3f}" stop-color="{color}"/>'
        for i, color in enumerate(stops)) + '</linearGradient>'


for name, background, colors in [
    ('candy', ['#f3d6e4', '#e8bfce'], ['#7562ed', '#bb91ef', '#ffb4ce', '#ff829f']),
    ('neon', ['#08072e', '#34043e'], ['#d28bff', '#ffa4e5', '#f579d3']),
    ('glass', ['#f4e2e9', '#f2e8d8'], ['#302441', '#6b537d', '#34314e', '#b5835b', '#dca86e']),
    ('schoolbook', ['#ffecb5', '#fae5aa'], ['#ffbe32', '#f36487', '#884cdd', '#3e72e8', '#36bcb5']),
    ('colorful', ['#080531', '#270642'], ['#ffb336', '#ff4d6e', '#ed24b8', '#9227e8', '#4931ed']),
]:
    strength = {'candy': 4.5, 'neon': 1.5, 'glass': 1.4, 'schoolbook': 4.5, 'colorful': 3.4}[name]
    defs = gradient('background', background, x2='0', y2='1') + gradient('ink', colors, y1='1', y2='0')
    defs += f'''<clipPath id="tile"><path d="{TILE}"/></clipPath>
    <filter id="density"><feComponentTransfer><feFuncA type="linear" slope="{strength}"/></feComponentTransfer></filter>
    <mask id="mark" maskUnits="userSpaceOnUse" x="0" y="0" width="192" height="192">
      <image x="31.125" y="31.125" width="129.75" height="129.75" filter="url(#density)" href="data:image/png;base64,{MARK}"/>
    </mask>
    <filter id="blur" x="-.5" y="-.5" width="2" height="2"><feGaussianBlur stdDeviation="2.5"/></filter>
    <filter id="glow" x="-.5" y="-.5" width="2" height="2"><feGaussianBlur stdDeviation="4.5"/></filter>
    <linearGradient id="rib"><stop stop-color="#fff" stop-opacity=".6"/><stop offset=".35" stop-color="#fff" stop-opacity="0"/><stop offset=".8" stop-color="#463448" stop-opacity=".16"/><stop offset="1" stop-color="#fff" stop-opacity=".5"/></linearGradient>'''

    def mark(fill='url(#ink)', extra=''):
        return f'<g {extra}><rect width="192" height="192" fill="{fill}" mask="url(#mark)"/></g>'

    body = f'<path d="{TILE}" fill="url(#background)"/>'
    if name == 'candy':
        body += mark('#b774a0', 'transform="translate(0 2)" filter="url(#blur)" opacity=".45"')
        body += mark()
    elif name == 'neon':
        body += mark('#bc3cff', 'filter="url(#glow)"')
        body += mark()
        body += mark('#ffe6fa', 'opacity=".28"')
    elif name == 'glass':
        body += mark(extra='filter="url(#blur)"')
        body += mark(extra='opacity=".75"')
        for x in range(19, 174, 4):
            body += f'<rect x="{x}" y="19" width="4" height="154" fill="url(#rib)"/>'
    elif name == 'schoolbook':
        body += mark()
    else:
        body += mark(extra='filter="url(#glow)" opacity=".65"')
        body += mark()
    body += f'<path d="{TILE}" fill="none" stroke="white" stroke-opacity=".14" stroke-width=".4"/>'
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 192 192"><defs>{defs}</defs><g clip-path="url(#tile)">{body}</g></svg>'
    with tempfile.NamedTemporaryFile(suffix='.svg', mode='w') as source:
        source.write(svg)
        source.flush()
        subprocess.run(['rsvg-convert', source.name, '-o', str(OUT / f'{name}.png')], check=True)
