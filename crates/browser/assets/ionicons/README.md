# Ionicons geometry

1108 non-brand icons from the MIT-licensed Ionicons font shipped with the
installed Arc 1.169.0 reference. Brand glyphs (`logo-*`) are excluded. The
proprietary IcoMoon bundle, Arc logos and app artwork are not included.

Upstream: https://github.com/ionic-team/ionicons
License source: https://raw.githubusercontent.com/ionic-team/ionicons/main/LICENSE

Source font SHA-256:
`97087b7a242d8a246ee02c556da9ee0a904e04004b203ff91c4013294604709a`

The font uses 512 units per em and a 511-unit ascent. `paths.json` preserves
its filled contours, including holes and quadratic curves. Command arrays:
`[0,x,y]` move, `[1,x,y]` line, `[2,cx,cy,x,y]` quadratic,
`[3,c1x,c1y,c2x,c2y,x,y]` cubic, `[4]` close. Runtime scales these contours
to the icon box and flips the font's Y axis. No system icon font is needed.

To regenerate, with Python fontTools installed:

```sh
python3 scripts/import-ionicons.py /path/to/ionicons.ttf crates/browser/assets/ionicons/paths.json
```

The complete set is embedded for browser controls and future graphical icon
selection. The current custom Space/tab/folder editor still accepts user
symbols and emoji; importing this catalog does not replace that picker UI.
