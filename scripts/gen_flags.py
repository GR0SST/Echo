#!/usr/bin/env python3
"""Builds crates/browser/assets/flags.json from the Chromium fork's flag list.

Keeps the desktop (macOS, Windows, Linux) entries that a plain feature or a
command-line switch backs; multi-value and origin-list flags are left out.
Run from the repository root after updating src/: python3 -I scripts/gen_flags.py
"""
import json, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src"
OUT = ROOT / "crates/browser/assets/flags.json"

def strings(text):
    """The concatenated C string literals in `text`."""
    unescape = lambda s: re.sub(r'\\(.)', lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), s)
    return "".join(unescape(s) for s in re.findall(r'"((?:[^"\\]|\\.)*)"', text))

def descriptions():
    text = (SRC / "chrome/browser/flag_descriptions.h").read_text()
    table = {}
    for match in re.finditer(r'inline constexpr char (k\w+)\[\]\s*=\s*((?:\s*"(?:[^"\\]|\\.)*")+)\s*;', text):
        table[match.group(1)] = strings(match.group(2))
    return table

def git_grep(pattern, *options):
    # One pathspec per run: some git versions return nothing for several.
    return "".join(
        subprocess.run(["git", "grep", "-h", *options, "-E", pattern, "--", spec], cwd=SRC, capture_output=True, text=True).stdout
        for spec in ("*.cc", "*.h"))

def feature_names():
    names = {}
    out = git_grep(r"BASE_FEATURE\(", "-A2")
    for match in re.finditer(r'BASE_FEATURE\(\s*(k\w+)\s*,\s*(?:"([^"]+)"\s*,)?', out):
        symbol, name = match.group(1), match.group(2)
        names.setdefault(symbol, set()).add(name or symbol[1:])
    return names

def switch_values():
    values = {}
    for match in re.finditer(r'(k\w+)\[\]\s*=\s*"([^"]*)"', git_grep(r'char k[A-Za-z0-9_]+\[\] *= *"', "-A0")):
        values.setdefault(match.group(1), set()).add(match.group(2))
    return values

OS = {"kOsMac": "mac", "kOsWin": "windows", "kOsLinux": "linux", "kOsDesktop": "mac windows linux", "kOsAll": "mac windows linux"}

def main():
    about = (SRC / "chrome/browser/about_flags.cc").read_text()
    start = about.index("const FeatureEntry kFeatureEntries[] = {")
    body = about[start:]
    body = body[: body.index("\n};\n")]
    texts, features, switches = descriptions(), feature_names(), switch_values()
    entries, seen = [], set()
    pattern = re.compile(
        r'\{\s*"([\w-]+)"\s*,\s*flag_descriptions::(k\w+)\s*,\s*flag_descriptions::(k\w+)\s*,\s*([^,]+?)\s*,\s*'
        r'(FEATURE_VALUE_TYPE|FEATURE_WITH_PARAMS_VALUE_TYPE|SINGLE_VALUE_TYPE|SINGLE_VALUE_TYPE_AND_VALUE|SINGLE_DISABLE_VALUE_TYPE)\(\s*([^)]*?)\)', re.S)
    for match in pattern.finditer(body):
        flag, name, description, os_mask, kind, args = match.groups()
        platforms = sorted({p for token, value in OS.items() if token in os_mask for p in value.split()})
        if not platforms or flag in seen or name not in texts:
            continue
        parts = [part.strip() for part in args.split(",")]
        symbol = parts[0].split("::")[-1]
        entry = {"id": flag, "name": texts[name], "description": texts.get(description, ""), "platforms": platforms}
        if kind.startswith("FEATURE"):
            found = features.get(symbol)
            if not found or len(found) != 1:
                continue
            entry["feature"] = next(iter(found))
        else:
            found = switches.get(symbol)
            if not found or len(found) != 1:
                continue
            entry["switch"] = next(iter(found))
            if kind == "SINGLE_VALUE_TYPE_AND_VALUE" and len(parts) > 1:
                entry["value"] = strings(parts[1])
            if kind == "SINGLE_DISABLE_VALUE_TYPE":
                entry["on_disable"] = True
        seen.add(flag)
        entries.append(entry)
    entries.sort(key=lambda entry: entry["name"].lower())
    OUT.write_text(json.dumps(entries, indent=0, ensure_ascii=False) + "\n")
    print(f"{len(entries)} flags -> {OUT.relative_to(ROOT)}", file=sys.stderr)

if __name__ == "__main__":
    main()
