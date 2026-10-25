import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { validate, version } from "./cef-dist";

const directories: string[] = [];
afterEach(async () => {
  await Promise.all(directories.splice(0).map(path => rm(path, { recursive: true, force: true })));
});

async function fixture() {
  const path = await mkdtemp(join(tmpdir(), "echo-cef-dist-test-"));
  directories.push(path);
  for (const name of ["include", "cmake", "libcef_dll", "Chromium Embedded Framework.framework"]) {
    await mkdir(join(path, name));
  }
  await Bun.write(join(path, "CMakeLists.txt"), "# fixture\n");
  await Bun.write(join(path, "Chromium Embedded Framework.framework/Chromium Embedded Framework"), "fixture binary");
  await Bun.write(join(path, "include/cef_version.h"), `#define CEF_VERSION "${version}"\n`);
  await Bun.write(join(path, "archive.json"), JSON.stringify({
    type: "minimal", name: `cef_binary_${version}_macosarm64_minimal.tar.bz2`, sha1: "0".repeat(40),
  }));
  return path;
}

test("custom selection is passed to child commands as an absolute CEF_PATH", async () => {
  const path = await fixture();
  const child = Bun.spawn(["bash", new URL("./with-cef.sh", import.meta.url).pathname,
    "bun", "-e", "console.log(process.env.CEF_PATH)"], {
    env: { ...process.env, ECHO_CEF_DIST: path }, stdout: "pipe", stderr: "pipe",
  });
  expect(await child.exited).toBe(0);
  expect((await new Response(child.stdout).text()).trim()).toBe(await validate(path));
});

test("a different runtime version cannot be selected", async () => {
  const path = await fixture();
  await Bun.write(join(path, "include/cef_version.h"), '#define CEF_VERSION "155.0.0"\n');
  await expect(validate(path)).rejects.toThrow("must match CEF");
});

test("invalid selection stops before running Cargo or triggering a fallback download", async () => {
  const path = await fixture();
  await rm(join(path, "archive.json"));
  const marker = join(path, "child-was-run");
  const child = Bun.spawn(["bash", new URL("./with-cef.sh", import.meta.url).pathname,
    "touch", marker], {
    env: { ...process.env, ECHO_CEF_DIST: path }, stdout: "pipe", stderr: "pipe",
  });
  expect(await child.exited).not.toBe(0);
  expect(await Bun.file(marker).exists()).toBe(false);
});

test("incomplete archive metadata is rejected before cef-rs can fall back", async () => {
  const path = await fixture();
  await Bun.write(join(path, "archive.json"), JSON.stringify({
    name: `cef_binary_${version}_macosarm64_minimal.tar.bz2`,
  }));
  await expect(validate(path)).rejects.toThrow("archive metadata");
});

test("canonical publication retains the old generation and switches one symlink", async () => {
  const { publish } = await import('./cef-dist');
  const { realpath, lstat } = await import('node:fs/promises');
  const first = await fixture(), second = await fixture();
  const parent = await mkdtemp(join(tmpdir(), 'echo-cef-publish-')); directories.push(parent);
  const canonical = join(parent, 'echo-extensions-dist');
  const initial = await publish(first, canonical);
  expect(initial.previous).toBeNull();
  const updated = await publish(second, canonical);
  expect(updated.previous).toBe(await realpath(first));
  expect(await realpath(canonical)).toBe(await realpath(second));
  expect((await lstat(canonical)).isSymbolicLink()).toBe(true);
  expect(await validate(first)).toBe(await realpath(first));
});

test("the original directory adapter is preserved during one-time migration", async () => {
  const { publish } = await import('./cef-dist');
  const { realpath } = await import('node:fs/promises');
  const original = await fixture(), next = await fixture();
  const result = await publish(next, original);
  directories.push(result.previous!);
  expect(await validate(result.previous!)).toBe(await realpath(result.previous!));
  expect(await realpath(original)).toBe(await realpath(next));
});

test("publication cannot replace an unrelated directory", async () => {
  const { publish } = await import('./cef-dist');
  const source = await fixture(), destination = await mkdtemp(join(tmpdir(), 'echo-cef-unrelated-'));
  directories.push(destination);
  await Bun.write(join(destination, 'unrelated'), 'preserve me');
  await expect(publish(source, destination)).rejects.toThrow();
  expect(await Bun.file(join(destination, 'unrelated')).text()).toBe('preserve me');
});

test('the publication CLI refuses an unqualified candidate before changing the default', async () => {
  const { publish } = await import('./cef-dist');
  const { realpath } = await import('node:fs/promises');
  const current = await fixture(), candidate = await fixture();
  const parent = await mkdtemp(join(tmpdir(), 'echo-cef-qualified-publish-')); directories.push(parent);
  const canonical = join(parent, 'echo-extensions-dist');
  await publish(current, canonical);
  const child = Bun.spawn(['bun', new URL('./cef-dist.ts', import.meta.url).pathname, 'publish', candidate, canonical], { stdout: 'pipe', stderr: 'pipe' });
  expect(await child.exited).not.toBe(0);
  expect(await realpath(canonical)).toBe(await realpath(current));
});
