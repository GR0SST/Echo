"""Export MIT-licensed Ionicons glyph outlines; runtime needs no font or Python."""
import argparse
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.pens.basePen import BasePen
import json

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('font', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
source = args.font
font = TTFont(source)
if font['head'].unitsPerEm != 512 or font['hhea'].ascent != 511:
    raise ValueError('Expected the reference Ionicons font grid (512 units, 511 ascent)')
glyphs = font.getGlyphSet()
class GeometryPen(BasePen):
    def __init__(self):
        super().__init__(glyphs)
        self.commands = []
    def add(self, kind, *points):
        self.commands.append([kind, *[int(v) if float(v).is_integer() else v for point in points for v in point]])
    def _moveTo(self, p): self.add(0, p)
    def _lineTo(self, p): self.add(1, p)
    def _qCurveToOne(self, control, end): self.add(2, control, end)
    def _curveToOne(self, c1, c2, end): self.add(3, c1, c2, end)
    def _closePath(self): self.add(4)
    def _endPath(self): pass
catalog = {}
for name in sorted(set(font.getBestCmap().values())):
    if name.startswith('logo-'): continue
    pen = GeometryPen()
    glyphs[name].draw(pen)
    catalog[name] = pen.commands
args.output.write_text(json.dumps(catalog, separators=(',', ':')) + '\n')
print(len(catalog), 'icons,', args.output.stat().st_size, 'bytes')
