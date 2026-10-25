// Final native-only MV3 popup gate. Uses private CEF profiles and no owner data.
import { mkdir, mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { validate } from "./cef-dist";

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
const dist = await validate(process.env.ECHO_CEF_DIST ?? join(source, "echo-extensions-dist"));
const profiles = await realpath(await mkdtemp(join(tmpdir(), "echo-mv3-gate-")));
const evidence = join(source, "qualification", `extensions-${new Date().toISOString().replaceAll(":", "-")}`);
await mkdir(evidence, { recursive: true });
const privateApp = join(profiles, "cefclient.app");
const bundle = join(privateApp, "Contents");
const frameworkBinary = "Chromium Embedded Framework";
const frameworkName = `${frameworkBinary}.framework`;
async function copy(from: string, to: string) {
  const child = Bun.spawn(["cp", "-cR", from, to]);
  if (await child.exited !== 0) throw Error(`Cannot freeze native extension harness: ${from}`);
}
async function digest(path: string) {
  const hash = new Bun.CryptoHasher("sha256");
  for await (const chunk of Bun.file(path).stream()) hash.update(chunk);
  return hash.digest("hex");
}
await copy(join(out, "cefclient.app"), privateApp);
await rm(join(bundle, "Frameworks", frameworkName), { recursive: true });
await copy(await realpath(join(dist, frameworkName)), join(bundle, "Frameworks", frameworkName));
const framework = await realpath(join(bundle, "Frameworks", frameworkName, frameworkBinary));
const frameworkHash = await digest(framework);
if (frameworkHash !== await digest(join(dist, frameworkName, frameworkBinary))) throw Error("Extension harness does not contain the selected canonical CEF");
await Bun.write(join(evidence, "bundle.json"), JSON.stringify({ dist, framework_sha256: frameworkHash,
  test_executable_sha256: await digest(join(bundle, "MacOS/cefclient")) }, null, 2) + "\n");
const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch(request) {
  const profile = new URL(request.url).pathname === "/B" ? "B" : "A";
  return new Response(`<!doctype html><title>Alloy profile ${profile}</title>
    <style>body{background:#e9f0f3;color:#17374d;font:24px system-ui;padding:40px}</style>
    <h1>Alloy page — profile ${profile}</h1><p>The MV3 action popup is a separate ExtensionViewHost.</p>`,
    { headers: { "Content-Type": "text/html; charset=utf-8" } });
} });
const resultPath = join(evidence, "result.json");
let fixture = resolve(import.meta.dir, "fixtures/extension-popup");
if (process.env.ECHO_CEF_TEST_REDACTED === "1") {
  const redacted = join(profiles, "unprivileged-fixture");
  await copy(fixture, redacted);
  const manifest = await Bun.file(join(redacted, "manifest.json")).json();
  manifest.permissions = ["storage"];
  await Bun.write(join(redacted, "manifest.json"), JSON.stringify(manifest, null, 2));
  fixture = redacted;
}
const log = Bun.file(join(evidence, "native.log"));
app = Bun.spawn([join(bundle, "MacOS/cefclient"),
  "--use-native", "--use-alloy-style", "--in-process-gpu", "--use-mock-keychain",
  "--no-first-run", "--no-default-browser-check",
  "--echo-enable-extensions-bridge", "--enable-unsafe-extension-debugging",
  `--cache-path=${profiles}/A`, `--echo-extension-probe-root=${profiles}`,
  `--url=http://127.0.0.1:${server.port}/A`,
  `--echo-extension-probe-url-b=http://127.0.0.1:${server.port}/B`,
  `--echo-extension-probe-fixture=${fixture}`,
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
  const report = { passed: Boolean(passed), exitCode, result, profiles, evidence, dist, framework_sha256: frameworkHash,
    metadata_permissions: process.env.ECHO_CEF_TEST_REDACTED === "1" ? "redacted" : "activeTab" };
  await Bun.write(join(evidence, "qualification.json"), JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify(report, null, 2));
  if (!passed) process.exitCode = 1;
} finally {
  clearTimeout(deadline);
  server.stop(true);
  if (app.exitCode === null) app.kill("SIGKILL");
}
