// Adapt the official minimal distribution to cef-rs' flattened CEF_PATH layout.
import { mkdir, realpath, symlink, lstat, rename, rm } from "node:fs/promises";
import { join, resolve } from "node:path";

export const version = "154.0.34+g14c5a08+chromium-154.0.8037.98";
export async function validate(directory: string) {
  const path = await realpath(directory);
  const header = await Bun.file(join(path, "include/cef_version.h")).text();
  if (!header.includes(`#define CEF_VERSION "${version}"`)) {
    throw new Error(`Custom distribution must match CEF ${version}: ${path}`);
  }
  for (const entry of ["CMakeLists.txt", "cmake", "libcef_dll", "Chromium Embedded Framework.framework"]) {
    await realpath(join(path, entry));
  }
  const archive = await Bun.file(join(path, "archive.json")).json();
  if (archive.type !== "minimal" || !/^[0-9a-f]{40}$/.test(archive.sha1 ?? "") ||
    archive.name !== `cef_binary_${version}_macosarm64_minimal.tar.bz2`) {
    throw new Error(`Unexpected custom CEF archive metadata: ${path}`);
  }
  const executable = Bun.file(join(path, "Chromium Embedded Framework.framework/Chromium Embedded Framework"));
  if (!await executable.exists() || executable.size === 0) throw new Error(`Missing custom CEF binary: ${path}`);
  return path;
}

async function stage(source: string, destination: string) {
  const src = await realpath(source);
  const dst = resolve(destination);
  const header = await Bun.file(join(src, "include/cef_version.h")).text();
  if (!header.includes(`#define CEF_VERSION "${version}"`)) throw new Error("CEF version mismatch");
  const archivePath = `${src}.tar.bz2`;
  // This is the actual locally built archive's digest, never the CDN archive's.
  const hash = new Bun.CryptoHasher("sha1");
  for await (const chunk of Bun.file(archivePath).stream()) hash.update(chunk);
  await mkdir(dst, { recursive: true });
  for (const [name, target] of [
    ["Chromium Embedded Framework.framework", join(src, "Release/Chromium Embedded Framework.framework")],
    ...["CMakeLists.txt", "CREDITS.html", "LICENSE.txt", "cmake", "include", "libcef_dll"].map(name => [name, join(src, name)]),
  ]) {
    await realpath(target);
    const link = join(dst, name);
    try {
      const stat = await lstat(link);
      if (!stat.isSymbolicLink() || await realpath(link) !== await realpath(target)) {
        throw new Error(`Refusing to replace ${link}`);
      }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      await symlink(target, link);
    }
  }
  await Bun.write(join(dst, "archive.json"), JSON.stringify({
    type: "minimal", name: `cef_binary_${version}_macosarm64_minimal.tar.bz2`, sha1: hash.digest("hex"),
  }, null, 2) + "\n");
  console.log(await validate(dst));
}

// Publish immutable adapters. Future updates replace only this symlink; Cargo
// readers keep their resolved generation until the qualification/cleanup step.
export async function publish(source: string, destination: string) {
  const src = await validate(source), dst = resolve(destination);
  if (src === dst) throw new Error("Canonical distribution cannot point to itself");
  const next = `${dst}.next-${crypto.randomUUID()}`;
  let previous: string | null = null;
  let migrated = false;
  await symlink(src, next);
  try {
    let stat;
    try { stat = await lstat(dst); } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
    }
    if (stat?.isSymbolicLink()) {
      previous = await realpath(dst);
    } else if (stat) {
      if (!stat.isDirectory()) throw new Error(`Refusing to replace ${dst}`);
      await validate(dst);
      previous = `${dst}.previous-${crypto.randomUUID()}`;
      await rename(dst, previous);
      migrated = true;
    }
    try { await rename(next, dst); }
    catch (error) { if (migrated) await rename(previous!, dst); throw error; }
    return { path: dst, generation: src, previous };
  } finally { await rm(next, { force: true }); }
}

async function qualified(source: string) {
  const dist = await validate(source);
  const build = await Bun.file(join(dist, 'build.json')).json();
  const q = await Bun.file(join(dist, 'qualification.json')).json();
  const expected = build.framework_sha256;
  if (!q.native_passed || !q.rust_tests?.passed || !q.candidate_bundle?.passed ||
      !q.codecs?.passed || !q.extensions?.passed || !q.pip?.passed || !q.browsing_data?.passed || !q.shutdown_recovery?.passed ||
      q.shutdown?.clean_exits !== 16 || q.shutdown.matrix.length !== 8 ||
      q.shutdown.matrix.some((r: any) => !r.passed || !r.cookies_verified) ||
      q.framework_sha256 !== expected || q.codecs.bundledHash !== expected ||
      [q.extensions, q.pip, q.browsing_data, q.shutdown_recovery].some((r: any) => r.framework_sha256 !== expected)) throw Error('All five native gates, shutdown recovery and Rust validation must pass before canonical publication');
  const hash = new Bun.CryptoHasher('sha256');
  for await (const bytes of Bun.file(join(dist, 'Chromium Embedded Framework.framework/Chromium Embedded Framework')).stream()) hash.update(bytes);
  if (hash.digest('hex') !== expected) throw Error('CEF framework changed after native qualification');
  return dist;
}

if (import.meta.main) {
  const [mode, source, destination, ...extra] = Bun.argv.slice(2);
  try {
    if (mode === "stage" && source && destination && !extra.length) await stage(source, destination);
    else if (mode === "publish" && source && destination && !extra.length) console.log(JSON.stringify(await publish(await qualified(source), destination)));
    else if (mode === "check" && source && !destination) console.log(await validate(source));
    else throw new Error("Usage: bun scripts/cef-dist.ts stage <minimal-dist> <cef-path> | publish <generation> <canonical-path> | check <cef-path>");
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
