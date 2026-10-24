import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, readFile, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Manifest, type Mode, verifyStack } from "./cef154-patches";

const temporary: string[] = [];
const digest = (value: string) => new Bun.CryptoHasher("sha256").update(value).digest("hex");
const chromium = "1".repeat(40);
const cef = "2".repeat(40);

afterEach(async () => {
  await Promise.all(temporary.splice(0).map((path) => rm(path, { recursive: true, force: true })));
});

async function fixture() {
  const parent = await mkdtemp(join(tmpdir(), "echo-cef-patch-check-"));
  temporary.push(parent);
  let root = join(parent, "checkout");
  const patches = join(parent, "patches");
  await mkdir(join(root, "cef/patch"), { recursive: true });
  root = await realpath(root);
  await mkdir(patches);
  for (const [path, value] of [["base.cc", "before"], ["standard.cc", "standard"], ["cef/patch/patch.cfg", "config"]]) {
    await writeFile(join(root, path), value);
  }
  await writeFile(join(patches, "chromium.patch"), "patch");
  const manifest: Manifest = {
    schema: 1,
    repositories: [{ path: "", commit: chromium }, { path: "cef", commit: cef }],
    patch_files: [{ path: "chromium.patch", sha256: digest("patch") }],
    standard_patch_files: [{ path: "cef/patch/patch.cfg", sha256: digest("config") }],
    standard_sources: [{ path: "base.cc", sha256: digest("before") }, { path: "standard.cc", sha256: digest("standard") }],
    authored_sources: [
      { path: "base.cc", base_sha256: digest("before"), result_sha256: digest("after") },
      { path: "new.cc", base_sha256: null, result_sha256: digest("new") },
    ],
    qualification_inputs: [{ path: "generated.cc", sha256: digest("generated") }],
  };
  const heads = async (path: string) => path === root ? chromium : cef;
  const verify = (mode: Mode, readHead = heads) => verifyStack(mode, root, manifest, patches, readHead);
  const patch = async () => {
    await writeFile(join(root, "base.cc"), "after");
    await writeFile(join(root, "new.cc"), "new");
  };
  return { root, patches, manifest, verify, patch };
}

test("base verification checks standard images and keeps new files absent", async () => {
  const f = await fixture();
  expect(await f.verify("base")).toEqual({ mode: "base", repositories: 2, sourceImages: 3, patchFiles: 1 });
  expect(await readFile(join(f.root, "base.cc"), "utf8")).toBe("before");
  expect(await Bun.file(join(f.root, "new.cc")).exists()).toBe(false);
});

test("patched and qualified-input modes require their distinct images", async () => {
  const f = await fixture();
  await f.patch();
  expect((await f.verify("patched")).sourceImages).toBe(3);
  await expect(f.verify("qualified-inputs")).rejects.toThrow("Missing source: generated.cc");
  await writeFile(join(f.root, "generated.cc"), "generated");
  expect((await f.verify("qualified-inputs")).sourceImages).toBe(4);
});

test("a wrong Chromium or CEF repository pin fails before source acceptance", async () => {
  const f = await fixture();
  await expect(f.verify("base", async () => "3".repeat(40))).rejects.toThrow("Repository pin mismatch: Chromium");
  await expect(f.verify("base", async (path) => path === f.root ? chromium : "3".repeat(40))).rejects.toThrow("Repository pin mismatch: cef");
});

test("modified patch artifacts and standard patch inputs fail", async () => {
  const f = await fixture();
  await writeFile(join(f.patches, "chromium.patch"), "other patch");
  await expect(f.verify("base")).rejects.toThrow("Digest mismatch: chromium.patch");
  await writeFile(join(f.patches, "chromium.patch"), "patch");
  await writeFile(join(f.root, "cef/patch/patch.cfg"), "other config");
  await expect(f.verify("base")).rejects.toThrow("Digest mismatch: cef/patch/patch.cfg");
});

test("partial application and unexpected new files cannot pass base verification", async () => {
  const f = await fixture();
  await writeFile(join(f.root, "new.cc"), "new");
  await expect(f.verify("base")).rejects.toThrow("Expected absent source: new.cc");
  await rm(join(f.root, "new.cc"));
  await writeFile(join(f.root, "base.cc"), "after");
  await expect(f.verify("base")).rejects.toThrow("Digest mismatch: base.cc");
});

test("missing or changed patched and standard files fail", async () => {
  const f = await fixture();
  await f.patch();
  await rm(join(f.root, "new.cc"));
  await expect(f.verify("patched")).rejects.toThrow("Missing source: new.cc");
  await writeFile(join(f.root, "new.cc"), "new");
  await writeFile(join(f.root, "standard.cc"), "changed");
  await expect(f.verify("patched")).rejects.toThrow("Digest mismatch: standard.cc");
});

test("a symlink cannot substitute for a selected regular source", async () => {
  const f = await fixture();
  await f.patch();
  await rm(join(f.root, "new.cc"));
  await writeFile(join(f.root, "target.cc"), "new");
  await symlink(join(f.root, "target.cc"), join(f.root, "new.cc"));
  await expect(f.verify("patched")).rejects.toThrow("Expected regular file: new.cc");
});

test("invalid paths and inconsistent baseline metadata fail", async () => {
  const f = await fixture();
  f.manifest.authored_sources[0].base_sha256 = digest("conflict");
  await expect(f.verify("base")).rejects.toThrow("Conflicting baseline image: base.cc");
  f.manifest.authored_sources[0].path = "../outside.cc";
  await expect(f.verify("base")).rejects.toThrow("Invalid manifest path");
});
