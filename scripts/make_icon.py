"""Builds the Echo app icon from the logo artwork.

Usage: python3 scripts/make_icon.py <logo.png> crates/bundle/assets/echo-mark.png \
           crates/bundle/assets/echo-icon-1024.png
then build Echo.icns from the 1024 px icon with `iconutil` (see the bundle crate).
The logo's gray rings become white with matching opacity, so the center is the
most opaque, and sit on a dark squircle with a soft glow.
"""
import sys
from PIL import Image, ImageDraw, ImageFilter, ImageChops
src = Image.open(sys.argv[1]).convert('RGBA')
# Rings become white; their gray level sets opacity, so the center is the most opaque.
r, g, b, a = src.split()
lum = src.convert('L')
alpha = ImageChops.multiply(a, lum)
alpha = alpha.point(lambda v: 0 if v < 12 else v)
bbox = alpha.getbbox()
mark = Image.merge('RGBA', (Image.new('L', src.size, 255),) * 3 + (alpha,)).crop(bbox)
side = max(mark.size)
sq = Image.new('RGBA', (side, side), (255, 255, 255, 0))
sq.paste(mark, ((side - mark.width) // 2, (side - mark.height) // 2))
sq.save(sys.argv[2])

def squircle_mask(size, inset, n=5.0):
    S = 4
    big = size * S
    m = Image.new('L', (big, big), 0)
    px = m.load()
    half = (big - 2 * inset * S) / 2
    c = big / 2
    import math
    pts = []
    for i in range(2000):
        t = 2 * math.pi * i / 2000
        ct, st = math.cos(t), math.sin(t)
        x = c + half * (abs(ct) ** (2 / n)) * (1 if ct >= 0 else -1)
        y = c + half * (abs(st) ** (2 / n)) * (1 if st >= 0 else -1)
        pts.append((x, y))
    ImageDraw.Draw(m).polygon(pts, fill=255)
    return m.resize((size, size), Image.LANCZOS)

N = 1024
inset = 100
mask = squircle_mask(N, inset)
bg = Image.new('RGBA', (N, N))
top, bottom = (46, 46, 52), (12, 12, 15)
draw = ImageDraw.Draw(bg)
for y in range(N):
    t = y / (N - 1)
    draw.line([(0, y), (N, y)], fill=tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3)) + (255,))
icon = Image.new('RGBA', (N, N), (0, 0, 0, 0))
icon.paste(bg, (0, 0), mask)
# Soft glow under the mark, then the mark itself.
size = int((N - 2 * inset) * 0.84)
m = sq.resize((size, size), Image.LANCZOS)
glow = Image.new('RGBA', (N, N), (0, 0, 0, 0))
off = ((N - size) // 2, (N - size) // 2)
glow.paste(m, off, m)
glow = glow.filter(ImageFilter.GaussianBlur(28))
ga = glow.split()[3].point(lambda v: int(v * 0.35))
glow.putalpha(ImageChops.multiply(ga, mask))
icon = Image.alpha_composite(icon, glow)
layer = Image.new('RGBA', (N, N), (0, 0, 0, 0)); layer.paste(m, off, m)
icon = Image.alpha_composite(icon, layer)
# Hairline rim like system icons.
rim = ImageChops.subtract(mask, squircle_mask(N, inset + 2))
rim_layer = Image.new('RGBA', (N, N), (255, 255, 255, 0)); rim_layer.putalpha(rim.point(lambda v: int(v * 0.12)))
icon = Image.alpha_composite(icon, rim_layer)
icon.save(sys.argv[3])
