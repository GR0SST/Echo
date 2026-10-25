#!/usr/bin/env python3
"""Measure owned Echo windows disappearing after Quit; never interrupt CEF."""
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

if "--locked" not in sys.argv:
    raise SystemExit(subprocess.run(["lockf", "-k", "-t", "120", "/tmp/echo-screen.lock",
        "gtimeout", "-k", "3", "90", sys.executable, __file__, "--locked"]).returncode)
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
output = root / "target/parity/quit-latency"
output.mkdir(parents=True, exist_ok=True)
results = []
for run in range(3):
    directory = Path(tempfile.mkdtemp(prefix="echo-quit-latency-"))
    app = directory / "Echo.app"
    subprocess.run(["cp", "-cR", str(root / "target/bundle/Echo.app"), str(app)], check=True)
    log = output / f"run-{run}.log"
    with log.open("w") as stream:
        process = subprocess.Popen([str(app / "Contents/MacOS/Echo"), "--in-process-gpu"],
            env={**os.environ, "BROWSER_DATA_DIR": str(directory / "profile"),
                 "BROWSER_SCRIPT": f"activate; wait 3; shot {output}/before-{run}.png; quit"},
            stdout=stream, stderr=subprocess.STDOUT)
        seen = False
        gone = None
        requested = None
        deadline = time.monotonic() + 25
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
            result = {"run": run, "quit_to_window_gone_ms": max(0, gone - requested) / 1000,
                      "exit": status, "poll_ms": 5, "log": str(log)}
            results.append(result)
            print(json.dumps(result), flush=True)
        finally:
            if process.poll() is None:
                # Failure-only bounded cleanup of this owned test process.
                process.kill()
                process.wait()
(output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
