// Final native qualification only: opens an isolated Echo and takes a screenshot.
import { mkdir, mkdtemp, realpath } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { validate } from "./cef-dist";

if (!Bun.argv.includes("--locked")) {
  const lock = Bun.spawn(["lockf", "-k", "-t", "120", "/tmp/echo-screen.lock",
    "bun", import.meta.path, "--locked"], { stdout: "inherit", stderr: "inherit" });
  process.exit(await lock.exited);
}

let ownedBrowser: ReturnType<typeof Bun.spawn> | undefined;
// Bound the complete locked check, including bundle copying and hashing.
const hardTimeout = setTimeout(() => {
  console.error("Native codec check exceeded its 120-second hard timeout");
  ownedBrowser?.kill("SIGKILL");
  process.exit(124);
}, 120000);
hardTimeout.unref();

const dist = await validate(process.env.ECHO_CEF_DIST ?? "");
const root = resolve(import.meta.dir, "..");
const sourceRoot = process.env.ECHO_CEF_SOURCE ?? join(process.env.HOME!, "Documents/Bot/cef-src");
const fixture = join(sourceRoot, "fixtures/sintel-trailer.mp4");
if (!await Bun.file(fixture).exists()) throw new Error(`Download the documented W3C fixture first: ${fixture}`);
const evidence = join(sourceRoot, "qualification", new Date().toISOString().replaceAll(":", "-"));
await mkdir(evidence, { recursive: true });
const privateRoot = await mkdtemp(join(tmpdir(), "echo-codec-check-"));
await mkdir(join(privateRoot, "profile"));
await Bun.write(join(privateRoot, "profile/session.json"), JSON.stringify({
  format_version: 2, next_id: 4, default_user_profile_id: 1, next_user_profile_id: 2,
  active_space: 0, archive: [], profiles: { "1": { id: 1, name: "Echo Test", archive_after: null } },
  settings: { theme_appearance: "Dark" },
  spaces: [{ id: 1, user_profile_id: 1, name: "Echo Test", hue: 250, pinned: [],
    today: [{ id: 2, title: "Fixture", url: "about:blank",
      last_active: { secs_since_epoch: Math.floor(Date.now() / 1000), nanos_since_epoch: 0 } }], active_tab: 2 }],
}));
const appPath = join(privateRoot, "Echo Codec Check.app");
const copy = Bun.spawn(["cp", "-cR", resolve(process.env.ECHO_CEF_TEST_BUNDLE ?? join(root, "target/bundle/Echo.app")), appPath]);
if (await copy.exited !== 0) throw new Error("Cannot copy the Echo bundle");
const framework = "Chromium Embedded Framework.framework/Chromium Embedded Framework";
async function digest(path: string) {
  const hash = new Bun.CryptoHasher("sha256");
  for await (const chunk of Bun.file(await realpath(path)).stream()) hash.update(chunk);
  return hash.digest("hex");
}
const expectedHash = await digest(join(dist, framework));
const bundledHash = await digest(join(appPath, "Contents/Frameworks", framework));
const executableHash = await digest(join(appPath, "Contents/MacOS/Echo"));
if (bundledHash !== expectedHash) throw new Error("Bundle does not contain the selected custom CEF");
const fixtureHash = await digest(fixture);
if (fixtureHash !== "b670602fa00934ca27c4351bb0efe7ea7a07fae57284e44226025eeed7c51254") {
  throw new Error("The W3C fixture differs from the documented H.264/AAC sample");
}

const nonce = crypto.randomUUID();
let result: Record<string, any> | undefined;
const html = `<!doctype html><meta charset="utf-8"><title>Echo H.264 / AAC qualification</title>
<style>body{background:#121722;color:#fff;font:20px system-ui;padding:24px}video{width:854px;max-width:90vw}pre{white-space:pre-wrap}</style>
<h1>Echo H.264 / AAC native check</h1><video controls autoplay src="/fixture.mp4"></video><pre>Decoding…</pre>
<script>
const video = document.querySelector('video');
video.volume = 0.05;
(async () => {
  try {
    await Promise.race([video.play(), new Promise((_, reject) => setTimeout(() => reject(Error('video playback timed out')), 20000))]);
    const audio = new AudioContext();
    const buffer = await audio.decodeAudioData(await (await fetch('/fixture.mp4')).arrayBuffer());
    const before = video.currentTime;
    await new Promise(resolve => setTimeout(resolve, 4000));
    const quality = video.getVideoPlaybackQuality();
    const sample = buffer.getChannelData(0);
    let peak = 0;
    for (const value of sample) peak = Math.max(peak, Math.abs(value));
    const report = {timeBefore:before,timeAfter:video.currentTime,frames:quality.totalVideoFrames - quality.droppedVideoFrames,
      droppedFrames:quality.droppedVideoFrames,muted:video.muted,volume:video.volume,audioDecodedBytes:video.webkitAudioDecodedByteCount ?? null,
      width:video.videoWidth,height:video.videoHeight,videoError:video.error?.message ?? null,
      audioSamples:buffer.length,audioChannels:buffer.numberOfChannels,audioRate:buffer.sampleRate,audioPeak:peak};
    document.querySelector('pre').textContent = JSON.stringify(report, null, 2);
    await fetch('/result/${nonce}', {method:'POST',body:JSON.stringify(report)});
    await audio.close();
  } catch (error) {
    const report = {error:String(error)};
    document.querySelector('pre').textContent = JSON.stringify(report);
    await fetch('/result/${nonce}', {method:'POST',body:JSON.stringify(report)});
  }
})();
</script>`;
const server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
  const url = new URL(request.url);
  if (url.pathname === `/result/${nonce}` && request.method === "POST") {
    result = await request.json();
    return new Response("recorded");
  }
  if (url.pathname === "/fixture.mp4") {
    const file = Bun.file(fixture);
    const range = request.headers.get("range")?.match(/^bytes=(\d+)-(\d*)$/);
    if (range) {
      const start = Number(range[1]);
      const end = range[2] ? Math.min(Number(range[2]), file.size - 1) : file.size - 1;
      if (start > end || start >= file.size) return new Response(null, { status: 416 });
      return new Response(file.slice(start, end + 1), { status: 206, headers: {
        "Content-Type": "video/mp4", "Accept-Ranges": "bytes", "Content-Range": `bytes ${start}-${end}/${file.size}`,
      } });
    }
    return new Response(file, { headers: { "Content-Type": "video/mp4", "Accept-Ranges": "bytes" } });
  }
  if (url.pathname === `/check/${nonce}`) return new Response(html, { headers: { "Content-Type": "text/html" } });
  return new Response("not found", { status: 404 });
} });
const appLog = Bun.file(join(evidence, "native.log"));
const browser = Bun.spawn([join(appPath, "Contents/MacOS/Echo"), "--in-process-gpu", "--autoplay-policy=no-user-gesture-required"], {
  env: { ...process.env, BROWSER_DATA_DIR: join(privateRoot, "profile"),
    BROWSER_SCRIPT: `wait 3; activate; resize 1000 760; position 100 100; require-focus; key escape; key cmd-t; type http://127.0.0.1:${server.port}/check/${nonce}; key enter; wait 25; activate; wait 1; require-focus; windows; shotstack ${join(evidence, "playback.png")}; dump; quit` },
  stdout: appLog, stderr: appLog,
});
ownedBrowser = browser;
try {
  const exitCode = await browser.exited;
  const nativeLog = await appLog.text();
  const passed = exitCode === 0 && !nativeLog.includes('[automation] aborted:') &&
    !nativeLog.includes('[automation] unknown command') && result && !result.error && !result.videoError &&
    result.timeAfter - result.timeBefore > 1 && result.frames > 0 && result.width > 0 && !result.muted && result.volume > 0 &&
    result.audioSamples > 0 && result.audioChannels > 0 && result.audioPeak > 0;
  const report = { passed: Boolean(passed), exitCode, result, bundledHash, executableHash,
    fixtureReported: Boolean(result), fixtureHash,
    fixtureSource: "https://media.w3.org/2010/05/sintel/trailer.mp4", privateRoot, evidence };
  await Bun.write(join(evidence, "result.json"), JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify(report, null, 2));
  if (!passed) process.exitCode = 1;
} finally {
  clearTimeout(hardTimeout);
  server.stop(true);
  if (browser.exitCode === null) browser.kill("SIGKILL");
}
