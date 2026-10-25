#!/usr/bin/env python3
"""Measure owned Echo windows disappearing after Quit; never interrupt CEF."""
import argparse
import hashlib
import ctypes as C
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--bundle", type=Path, default=Path(__file__).resolve().parent.parent / "target/bundle/Echo.app")
parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parent.parent / "target/parity/quit-latency")
parser.add_argument("--locked", action="store_true")
args = parser.parse_args()
if not args.locked:
    raise SystemExit(subprocess.run(["lockf", "-k", "-t", "120", "/tmp/echo-screen.lock",
        "gtimeout", "-k", "3", "300", sys.executable, __file__, *sys.argv[1:], "--locked"]).returncode)
CG = C.CDLL("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
CF = C.CDLL("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
CG.CGWindowListCopyWindowInfo.argtypes = [C.c_uint32, C.c_uint32]
CG.CGWindowListCopyWindowInfo.restype = C.c_void_p
CF.CFPropertyListCreateData.argtypes = [C.c_void_p, C.c_void_p, C.c_long, C.c_ulong, C.c_void_p]
CF.CFPropertyListCreateData.restype = C.c_void_p
CF.CFDataGetLength.argtypes = [C.c_void_p]
CF.CFDataGetLength.restype = C.c_long
CF.CFDataGetBytePtr.argtypes = [C.c_void_p]
CF.CFDataGetBytePtr.restype = C.POINTER(C.c_ubyte)
CF.CFRelease.argtypes = [C.c_void_p]
def visible(pid):
    raw = CG.CGWindowListCopyWindowInfo(1 | 16, 0)
    data = CF.CFPropertyListCreateData(None, raw, 100, 0, None)
    try:
        rows = plistlib.loads(C.string_at(CF.CFDataGetBytePtr(data), CF.CFDataGetLength(data)))
    finally:
        CF.CFRelease(data)
        CF.CFRelease(raw)
    return any(w.get("kCGWindowOwnerPID") == pid and w.get("kCGWindowLayer") == 0 for w in rows)

root = Path(__file__).resolve().parent.parent
output = args.output.resolve()
bundle = args.bundle.resolve()
framework_hash = hashlib.sha256((bundle / "Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework").read_bytes()).hexdigest()
executable_hash = hashlib.sha256((bundle / "Contents/MacOS/Echo").read_bytes()).hexdigest()
output.mkdir(parents=True, exist_ok=True)
results = []
for run in range(3):
    directory = Path(tempfile.mkdtemp(prefix="echo-quit-latency-"))
    app = directory / "Echo.app"
    subprocess.run(["cp", "-cR", str(bundle), str(app)], check=True)
    profile = directory / "profile"
    profile.mkdir()
    (profile / "session.json").write_text(json.dumps({"format_version": 2, "next_id": 4,
        "default_user_profile_id": 1, "next_user_profile_id": 2, "active_space": 0, "archive": [],
        "profiles": {"1": {"id": 1, "name": "Echo Test", "archive_after": None}},
        "settings": {"theme_appearance": "Dark"}, "spaces": [{"id": 1, "user_profile_id": 1,
        "name": "Echo Test", "hue": 250, "pinned": [], "today": [], "active_tab": None}]}))
    log = output / f"run-{run}.log"
    with log.open("w") as stream:
        process = subprocess.Popen([str(app / "Contents/MacOS/Echo"), "--in-process-gpu"],
            env={**os.environ, "BROWSER_DATA_DIR": str(directory / "profile"),
                 "BROWSER_SCRIPT": f"activate; wait 3; require-focus; shot {output}/before-{run}.png; quit"},
            stdout=stream, stderr=subprocess.STDOUT)
        seen = False
        gone = None
        requested = None
        deadline = time.monotonic() + 90
        try:
            while time.monotonic() < deadline and process.poll() is None:
                on_screen = visible(process.pid)
                observed_us = time.time_ns() // 1000
                seen |= on_screen
                text = log.read_text(errors="replace")
                for line in text.splitlines():
                    if "request_unix_us=" in line:
                        requested = int(line.split("request_unix_us=")[1].split()[0])
                if seen and not on_screen and requested is not None and gone is None:
                    gone = observed_us
                time.sleep(.005)
            status = process.wait(timeout=max(1, deadline - time.monotonic()))
            assert status == 0, (status, log)
            assert seen and gone is not None and requested is not None, log
            assert "CEF Complete" in log.read_text(), log
            assert "[automation] aborted:" not in log.read_text(), log
            result = {"run": run, "quit_to_window_gone_ms": max(0, gone - requested) / 1000,
                      "exit": status, "poll_ms": 5, "log": str(log),
                      "framework_sha256": framework_hash, "executable_sha256": executable_hash}
            results.append(result)
            print(json.dumps(result), flush=True)
        finally:
            if process.poll() is None:
                # Failure-only bounded cleanup of this owned test process.
                process.kill()
                process.wait()
(output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
