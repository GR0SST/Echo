#!/usr/bin/env python3
"""Inspect Arc panels while the caller holds the shared screen lock.

The qualification task explicitly authorizes restarting Arc to clear leftover
panels. Never edit Arc session or preference files.
"""
import ctypes as C
import json
import plistlib
import subprocess
import time

CG = C.CDLL('/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics')
CF = C.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')
CG.CGWindowListCopyWindowInfo.argtypes = [C.c_uint32, C.c_uint32]
CG.CGWindowListCopyWindowInfo.restype = C.c_void_p
CF.CFPropertyListCreateData.argtypes = [C.c_void_p, C.c_void_p, C.c_long, C.c_ulong, C.c_void_p]
CF.CFPropertyListCreateData.restype = C.c_void_p
CF.CFDataGetLength.argtypes = [C.c_void_p]
CF.CFDataGetLength.restype = C.c_long
CF.CFDataGetBytePtr.argtypes = [C.c_void_p]
CF.CFDataGetBytePtr.restype = C.POINTER(C.c_ubyte)
CF.CFRelease.argtypes = [C.c_void_p]


def elevated_arc_windows():
    raw = CG.CGWindowListCopyWindowInfo(1 | 16, 0)
    data = CF.CFPropertyListCreateData(None, raw, 100, 0, None)
    try:
        rows = plistlib.loads(C.string_at(CF.CFDataGetBytePtr(data), CF.CFDataGetLength(data)))
    finally:
        CF.CFRelease(data)
        CF.CFRelease(raw)
    return [row for row in rows
            if row.get('kCGWindowOwnerName') == 'Arc' and row.get('kCGWindowLayer', 0) > 0]


before = elevated_arc_windows()
print(json.dumps({'arc_elevated_before': before}), flush=True)
for attempt in range(2):
    if not before:
        break
    time.sleep(3)
    before = elevated_arc_windows()
    print(json.dumps({'arc_retry': attempt + 1, 'arc_elevated': before}), flush=True)
if before:
    subprocess.run(['pkill', '-x', 'Arc'], check=True)
    time.sleep(3)
    subprocess.run(['open', '-a', 'Arc'], check=True)
    time.sleep(3)
remaining = elevated_arc_windows()
print(json.dumps({'arc_elevated_after': remaining}), flush=True)
if remaining:
    raise SystemExit('Arc elevated panel persists after authorized restart')
