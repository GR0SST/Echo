// Final native-only MV3 popup gate. Uses private CEF profiles and no owner data.
import { mkdir, mkdtemp, realpath } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

if (!Bun.argv.includes("--locked")) {
  const child = Bun.spawn(["lockf", "-k", "-t", "120", "/tmp/echo-screen.lock",
    "bun", import.meta.path, "--locked"], { stdout: "inherit", stderr: "inherit" });
  process.exit(await child.exited);
}
let app: ReturnType<typeof Bun.spawn> | undefined;
const deadline = setTimeout(() => {
  app?.kill("SIGKILL");
  console.error("MV3 popup gate exceeded its 90-second hard timeout");
  process.exit(124);
}, 90000);
deadline.unref();
const source = process.env.ECHO_CEF_SOURCE ?? join(process.env.HOME!, "Documents/Bot/cef-src");
const out = join(source, "chromium/src/out/Release_GN_arm64");
const bundle = join(out, "cefclient.app/Contents");
const framework = await realpath(join(bundle, "Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework"));
const profiles = await realpath(await mkdtemp(join(tmpdir(), "echo-mv3-gate-")));
const evidence = join(source, "qualification", `extensions-${new Date().toISOString().replaceAll(":", "-")}`);
await mkdir(evidence, { recursive: true });
const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch(request) {
  const profile = new URL(request.url).pathname === "/B" ? "B" : "A";
  return new Response(`<!doctype html><title>Alloy profile ${profile}</title>
    <style>body{background:#e9f0f3;color:#17374d;font:24px system-ui;padding:40px}</style>
    <h1>Alloy page — profile ${profile}</h1><p>The MV3 action popup is a separate ExtensionViewHost.</p>`,
    { headers: { "Content-Type": "text/html; charset=utf-8" } });
} });
const resultPath = join(evidence, "result.json");
const log = Bun.file(join(evidence, "native.log"));
app = Bun.spawn([join(bundle, "MacOS/cefclient"),
  "--use-native", "--use-alloy-style", "--in-process-gpu", "--use-mock-keychain",
  "--no-first-run", "--no-default-browser-check",
  "--echo-enable-extensions-bridge", "--enable-unsafe-extension-debugging",
  `--cache-path=${profiles}/A`, `--echo-extension-probe-root=${profiles}`,
  `--url=http://127.0.0.1:${server.port}/A`,
  `--echo-extension-probe-url-b=http://127.0.0.1:${server.port}/B`,
  `--echo-extension-probe-fixture=${resolve(import.meta.dir, "fixtures/extension-popup")}`,
  `--echo-extension-probe-output=${resultPath}`,
  `--echo-extension-probe-shot=${join(evidence, "popup.png")}`,
  `--echo-extension-probe-store-mode=${process.env.ECHO_CEF_TEST_STORE_MODE ?? "install"}`,
  `--echo-extension-probe-popup-only=${process.env.ECHO_CEF_TEST_POPUP_ONLY ?? "0"}`,
  ...(process.env.ECHO_CEF_TEST_STORE_ID ? [`--echo-extension-probe-store-id=${process.env.ECHO_CEF_TEST_STORE_ID}`] : [])], {
  env: { ...process.env, ECHO_EXTENSION_PROBE_FRAMEWORK: framework }, stdout: log, stderr: log,
});
try {
  const exitCode = await app.exited;
  const result = await Bun.file(resultPath).json();
  const passed = exitCode === 0 && result.passed && result.owner_alloy && result.other_alloy &&
    result.contexts_separate && result.has_a === 1 && result.has_b === 0 && result.invoke_b === 1 &&
    result.close_b === 4 && result.ready && result.popup_loaded && result.visible_native_view &&
    result.popup_closed && (result.management_ok || process.env.ECHO_CEF_TEST_POPUP_ONLY === "1") && result.title === "POPUP_GATE:1:fixture-profile-A" &&
    await Bun.file(join(evidence, "popup.png")).exists();
  console.log(JSON.stringify({ passed: Boolean(passed), exitCode, result, profiles, evidence }, null, 2));
  if (!passed) process.exitCode = 1;
} finally {
  clearTimeout(deadline);
  server.stop(true);
  if (app.exitCode === null) app.kill("SIGKILL");
}
