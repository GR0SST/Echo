#!/usr/bin/env python3
"""Native shutdown and restart persistence matrix; build with cargo run -p bundle.

Each drive run acquires the shared screen lock unless --screen-locked is passed
by the whole-matrix holder. Only freshly created fixture profiles are used.
"""
import argparse
import http.cookies
import http.server
import json
import os
import re
import pathlib
import shutil
import hashlib
import subprocess
import tempfile
import threading
import struct
import zlib
import time
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent.parent


def workspace(count):
    return {
        "format_version": 2, "next_id": count * 2 + 2,
        "default_user_profile_id": 1, "next_user_profile_id": count + 1,
        "active_space": 0, "archive": [],
        "profiles": {str(i): {"id": i, "name": f"Profile {i}", "archive_after": None}
                     for i in range(1, count + 1)},
        "spaces": [{"id": i, "user_profile_id": i, "name": f"Shutdown {i}",
                    "hue": 250.0, "pinned": [],
                    "today": [{"id": count + i, "title": "Seed", "url": "about:blank",
                               "last_active": {"secs_since_epoch": int(time.time()), "nanos_since_epoch": 0}}],
                    "active_tab": count + i}
                   for i in range(1, count + 1)],
    }


def main():
    if not __debug__:
        raise SystemExit("Run acceptance without -O or PYTHONOPTIMIZE; cookie assertions must be enabled")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", default="target/parity/shutdown-watchdog/acceptance")
    parser.add_argument("--case", choices=["normal", "profiles", "private", "reset", "gpu", "combined", "media-pip", "media-archived", "library-media"], action="append")
    parser.add_argument("--keep-going", action="store_true", help="collect all scenarios even after a failing quit")
    parser.add_argument("--quit-command", choices=["quit", "native-quit"], default="quit",
                        help="exercise the coordinator or native application termination entry point")
    parser.add_argument("--screen-locked", action="store_true", help="caller owns the screen lock for the complete matrix")
    parser.add_argument("--bundle", default=str(ROOT / "target/bundle/Echo.app"))
    parser.add_argument("--cef-dist", help="require this distribution's exact framework before the matrix")
    parser.add_argument("--appearance", choices=["Light", "Dark"], default="Dark")
    parser.add_argument("--profiles", type=int, default=3, help="persistent profile count for profiles/combined cases")
    parser.add_argument("--repeat", type=int, default=1)
    parser.add_argument("--require-network-exit", action="store_true",
                        help="require source-built CEF to confirm graceful network-helper exits")
    args = parser.parse_args()
    assert 1 <= args.repeat <= 20
    assert 1 <= args.profiles <= 8
    output = pathlib.Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    evidence = pathlib.Path(tempfile.mkdtemp(prefix="run-", dir=output))
    media = evidence / "video.webm"
    if not args.case or any(case.startswith("media-") for case in args.case):
        subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i",
                        "testsrc=size=320x180:rate=15", "-f", "lavfi", "-i",
                        "sine=frequency=440:sample_rate=48000", "-t", "2", "-vf", "format=yuv420p",
                        "-af", "volume=0.02", "-c:v", "libvpx", "-deadline", "realtime", "-cpu-used", "8",
                        "-threads", "1", "-c:a", "libopus", str(media)], check=True)
    # Freeze one coherent bundle before any native run; later rebuilds must not
    # replace files midway through the matrix or a driver copy.
    frozen_bundle = evidence / "source.app"
    shutil.copytree(pathlib.Path(args.bundle).resolve(), frozen_bundle, symlinks=True)
    framework_hash = hashlib.sha256((frozen_bundle / "Contents/Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework").read_bytes()).hexdigest()
    if args.cef_dist:
        dist_binary = pathlib.Path(args.cef_dist).resolve() / "Chromium Embedded Framework.framework/Chromium Embedded Framework"
        assert framework_hash == hashlib.sha256(dist_binary.read_bytes()).hexdigest(), "Shutdown bundle differs from the selected canonical CEF"
    (evidence / "bundle.json").write_text(json.dumps({
        "source": str(pathlib.Path(args.bundle).resolve()),
        "main_sha256": hashlib.sha256((frozen_bundle / "Contents/MacOS/Echo").read_bytes()).hexdigest(),
        "framework_sha256": framework_hash,
        "cef_dist": str(pathlib.Path(args.cef_dist).resolve()) if args.cef_dist else None,
        "quit_command": args.quit_command,
    }, indent=2))
    observations = []
    observations_lock = threading.Lock()
    def png_chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    pixels = b''.join(b'\0' + bytes((y % 256, 90, 180)) * 512 for y in range(512))
    png = (b'\x89PNG\r\n\x1a\n' + png_chunk(b'IHDR', struct.pack('>IIBBBBB', 512, 512, 8, 2, 0, 0, 0))
           + png_chunk(b'IDAT', zlib.compress(pixels)) + png_chunk(b'IEND', b''))

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            parsed = urllib.parse.urlparse(self.path)
            query = urllib.parse.parse_qs(parsed.query)
            label = query.get("label", [""])[0]
            cookie = http.cookies.SimpleCookie(self.headers.get("Cookie", ""))
            value = cookie["echo_shutdown"].value if "echo_shutdown" in cookie else ""
            with observations_lock:
                observations.append({"path": parsed.path, "label": label, "cookie": value})
            if parsed.path == "/video.webm":
                data = media.read_bytes()
                self.send_response(200)
                self.send_header("Content-Type", "video/webm")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
                return
            self.send_response(200)
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Type", "text/html")
            if parsed.path == "/set":
                self.send_header("Set-Cookie", f"echo_shutdown={label}; Max-Age=86400; Path=/; HttpOnly; SameSite=Lax")
            self.end_headers()
            if parsed.path == "/media":
                self.wfile.write(("<title>Shutdown media</title><style>button{position:absolute;top:60px;width:100px}"
                    "video{position:absolute;left:60px;top:110px;width:360px;height:240px}</style>"
                    "<button style='left:60px' onclick='v.play()'>Play</button>"
                    "<button style='left:200px' onclick='v.requestPictureInPicture()'>PiP</button>"
                    "<video id=v controls loop src='/video.webm'></video><script>"
                    "v.addEventListener('enterpictureinpicture',()=>fetch('/pip-enter?label=" + label + "'));"
                    "v.addEventListener('playing',()=>{document.title='Shutdown media playing';fetch('/playing?label=" + label + "')});"
                    "</script>").encode())
            else:
                self.wfile.write(f"<title>Shutdown fixture {label}</title><h1>Shutdown fixture</h1>".encode())

        def log_message(self, *_):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    origin = f"http://127.0.0.1:{server.server_port}"

    def navigate(path, label):
        return f"key escape; key cmd-t; type {origin}/{path}?label={label}; key enter; wait 2; dump; shot {directory}/screen-{label}.png; "

    native_failures = []

    def drive(directory, phase, script, gpu, library_media=False):
        directory.mkdir(parents=True, exist_ok=True)
        log = directory / f"{phase}.log"
        command = ["zsh",
                   str(ROOT / "scripts/check-shutdown-run.sh"), str(directory), phase, gpu, script]
        if not args.screen_locked:
            command = ["lockf", "-k", "/tmp/echo-screen.lock", *command]
        stop = threading.Event()
        writes = []
        def write_watched_fixture():
            while not stop.wait(.3):
                (directory / "media-fixtures/Watcher added.png").write_bytes(png)
                writes.append(time.monotonic())
        writer = threading.Thread(target=write_watched_fixture, daemon=True) if library_media else None
        if writer: writer.start()
        try:
            with log.open("w") as stream:
                environment = dict(os.environ, BROWSER_DRIVE_BUNDLE_PATH=str(frozen_bundle))
                result = subprocess.run(command, cwd=ROOT, env=environment, stdout=stream, stderr=subprocess.STDOUT)
        finally:
            stop.set()
            if writer: writer.join(timeout=1)
        text = log.read_text()
        checks = {
            "zero exit": result.returncode == 0,
            "complete barrier": "[quit] CEF Complete; owners=0; browsers=[]; cookie_flushes=0; failures=[]" in text,
            "one native shutdown": text.count("cef-shutdown-enter") == text.count("cef-shutdown-return") == 1,
            "one final terminate": text.count("invoking preserved original terminate exactly once") == 1,
            "strict exit": "[drive] application exited with status 0" in text,
            "no watchdog": "Teardown watchdog" not in text,
            "automation finished": "[automation] aborted:" not in text,
            "no destructive diagnostic": "[cef-helper local-fixture-probe]" not in text,
        }
        if args.quit_command == "native-quit":
            checks["native termination requested"] = "[automation] native-quit sending terminate:nil" in text
            checks["native termination captured"] = "[quit-native] terminate captured;" in text
        if library_media:
            retired = re.search(r"\[media-shutdown\] producers-cancelled discovery=true thumbnails=true ready_images=(\d+)", text)
            checks["populated Media retired before CEF"] = bool(retired and int(retired[1]) > 0 and retired.start() < text.find("cef-shutdown-enter"))
            checks["external folder changes during run"] = bool(writes)
        if args.require_network_exit:
            cef_log = directory / f"{phase}-cef.log"
            cef_text = cef_log.read_text(errors="replace") if cef_log.exists() else ""
            checks["graceful network cleanup"] = (
                "[echo-shutdown] network-cleanup-complete" in cef_text
                and "all_exited=1" in cef_text
                and "all_exited=0" not in cef_text
            )
        failed = [name for name, passed in checks.items() if not passed]
        if failed:
            native_failures.append({"log": str(log), "exit": result.returncode, "checks": failed})
        # Even a watchdog failure must be restarted to measure actual cookie
        # durability. It still fails qualification after the HTTP checks.
        return text

    results = []
    try:
        for repeat in range(args.repeat):
            for case in args.case or ["normal", "profiles", "private", "reset", "gpu", "combined", "media-pip", "media-archived"]:
                failure_start = len(native_failures)
                cookies_verified = False
                try:
                    directory = evidence / f"{repeat + 1}-{case}"
                    profile = directory / "browser-drive-profile"
                    profile.mkdir(parents=True)
                    count = args.profiles if case in ("profiles", "combined") else 1
                    seed = workspace(count)
                    # Measure accepted quit; confirmation has its own native suite.
                    seed["settings"] = {"theme_appearance": args.appearance,
                                        "toggles": {"warn_before_quitting": False}}
                    library_media = case in ("library-media", "media-archived")
                    if library_media:
                        fixtures = directory / "media-fixtures"
                        fixtures.mkdir()
                        for i in range(8): (fixtures / f"Fixture {i}.png").write_bytes(png)
                        seed["profiles"]["1"]["settings"] = {"media": {"folders": {
                            "Documents": "Denied", "Desktop": "Denied", "Downloads": {"Granted": str(fixtures)}}, "revision": 1}}
                    (profile / "session.json").write_text(json.dumps(seed))
                    gpu = "separate" if case in ("gpu", "combined", "media-pip", "media-archived") else "in-process"
                    prefix = f"r{repeat + 1}-{case}"
                    start = len(observations)
                    script = f"wait 2; activate; resize 1000 760; position 100 100; require-focus; shot {directory}/initial.png; "
                    for i in range(1, count + 1):
                        if i > 1:
                            script += "menu Spaces > Next Space; wait 2; "
                        script += navigate("set", f"{prefix}-p{i}")
                    if case == "reset":
                        script += navigate("verify", f"{prefix}-reset")
                        script += "key cmd-t; type Clear Cookies and Refresh; key enter; wait 3; dump; "
                        script += navigate("set", f"{prefix}-p1")
                    if case in ("private", "combined"):
                        script += "key cmd-shift-n; wait 2; window last; key escape; "
                        script += navigate("set", f"{prefix}-private")
                    if case.startswith("media-"):
                        script += navigate("media", prefix)
                        script += "click 330 80 200; wait 2; click 480 80 200; wait 2; window 0; activate; dump; "
                        if case == "media-archived":
                            script += "key cmd-w; wait 2; dump; "
                        else:
                            script += navigate("verify", f"{prefix}-background")
                            script += "wait 1; dump; "
                    if library_media:
                        script += f"menu Window > View Downloads…; wait .3; click 50 125 150; wait 2; shot {directory}/library-write.png; dump; "
                    text = drive(directory, "write", script + args.quit_command, gpu, library_media)
                    if case.startswith("media-"):
                        assert any(r["path"] == "/playing" and r["label"] == prefix for r in observations[start:]), directory
                        assert any(r["path"] == "/pip-enter" and r["label"] == prefix for r in observations[start:]), directory
                        assert "media_pip=true" in text, directory
                        if case == "media-pip":
                            assert "audible=true visible=false" in text, directory
                    requested = text.count("cookie-flush-request id=")
                    completed = text.count("cookie-flushed-collected id=")
                    assert requested == completed == count, (directory, requested, completed, count)
                    if case == "reset":
                        records = [r for r in observations[start:] if r["label"] == f"{prefix}-reset"]
                        assert any(r["cookie"] == "" for r in records), (directory, records)
                    # Reopen the identical workspace/profile, then verify by real HTTP
                    # Cookie headers rather than trusting an empty SQLite/WAL file.
                    script = "wait 2; activate; key escape; key ctrl-1; wait 1; "
                    for i in range(1, count + 1):
                        if i > 1:
                            script += "menu Spaces > Next Space; wait 2; "
                        script += navigate("verify", f"{prefix}-verify{i}")
                    if case in ("private", "combined"):
                        script += "key cmd-shift-n; wait 2; window last; key escape; "
                        script += navigate("verify", f"{prefix}-private-empty")
                    if library_media:
                        script += f"menu Window > View Downloads…; wait .3; click 50 125 150; wait 2; shot {directory}/library-read.png; dump; "
                    drive(directory, "read", script + args.quit_command, gpu, library_media)
                    for i in range(1, count + 1):
                        records = [r for r in observations[start:] if r["label"] == f"{prefix}-verify{i}"]
                        assert records and all(r["cookie"] == f"{prefix}-p{i}" for r in records), (directory, records)
                    if case in ("private", "combined"):
                        records = [r for r in observations[start:] if r["label"] == f"{prefix}-private-empty"]
                        assert records and all(r["cookie"] == "" for r in records), (directory, records)
                    cookies_verified = True
                    assert len(native_failures) == failure_start, native_failures[failure_start:]
                    results.append({"case": case, "repeat": repeat + 1, "contexts": count, "gpu": gpu, "cookies_verified": True, "passed": True})
                    print(f"{case} #{repeat + 1}: clean write/restart, {count} profile cookie(s) persisted", flush=True)
                except (AssertionError, OSError, subprocess.SubprocessError) as error:
                    results.append({"case": case, "repeat": repeat + 1, "passed": False, "cookies_verified": cookies_verified, "error": str(error)})
                    print(f"{case} #{repeat + 1}: FAILED: {error}", flush=True)
                    if not args.keep_going:
                        raise

    finally:
        (evidence / "requests.json").write_text(json.dumps(observations, indent=2))
        (evidence / "native-failures.json").write_text(json.dumps(native_failures, indent=2))
        (evidence / "results.json").write_text(json.dumps(results, indent=2))
        server.shutdown()
        server.server_close()
    print(f"Evidence: {evidence}", flush=True)
    if any(not result["passed"] for result in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
