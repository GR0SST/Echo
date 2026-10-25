// Adapt the official minimal distribution to cef-rs' flattened CEF_PATH layout.
import { mkdir, realpath, symlink, lstat } from "node:fs/promises";
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

if (import.meta.main) {
  const [mode, source, destination, ...extra] = Bun.argv.slice(2);
  try {
    if (mode === "stage" && source && destination && !extra.length) await stage(source, destination);
    else if (mode === "check" && source && !destination) console.log(await validate(source));
    else throw new Error("Usage: bun scripts/cef-dist.ts stage <minimal-dist> <cef-path> | check <cef-path>");
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
