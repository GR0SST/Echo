import { lstat, readFile, realpath } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

type Image = { path: string; sha256: string | null };
export type Manifest = {
  schema: number;
  repositories: { path: string; commit: string }[];
  patch_files: Image[];
  standard_patch_files: Image[];
  standard_sources: Image[];
  authored_sources: {
    path: string;
    base_sha256: string | null;
    result_sha256: string;
  }[];
  qualification_inputs: Image[];
};
export type Mode = "base" | "patched" | "qualified-inputs";

function relativePath(path: string, allowRoot = false): string {
  if (allowRoot && path === "") return path;
  if (!path || path.includes("\\") || path.split("/").some((p) => !p || p === "." || p === "..") || path.includes(":")) {
    throw new Error(`Invalid manifest path: ${path}`);
  }
  return path;
}

async function verifyImage(root: string, image: Image): Promise<void> {
  const path = resolve(root, relativePath(image.path));
  let stat;
  try {
    stat = await lstat(path);
  } catch (error) {
    if ((error as { code?: string }).code !== "ENOENT") throw error;
    if (image.sha256 === null) return;
    throw new Error(`Missing source: ${image.path}`);
  }
  if (image.sha256 === null) throw new Error(`Expected absent source: ${image.path}`);
  if (!/^[0-9a-f]{64}$/.test(image.sha256)) throw new Error(`Invalid digest: ${image.path}`);
  if (!stat.isFile() || stat.isSymbolicLink()) throw new Error(`Expected regular file: ${image.path}`);
  const actual = new Bun.CryptoHasher("sha256").update(await readFile(path)).digest("hex");
  if (actual !== image.sha256) throw new Error(`Digest mismatch: ${image.path}`);
}

async function gitHead(directory: string): Promise<string> {
  const child = Bun.spawn(["git", "-C", directory, "rev-parse", "HEAD"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [output, error, code] = await Promise.all([
    new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
  ]);
  if (code !== 0) throw new Error(`Cannot read repository at ${directory}: ${error.trim()}`);
  return output.trim();
}

export async function verifyStack(
  mode: Mode,
  checkout: string,
  manifest: Manifest,
  patchDirectory: string,
  readHead: (directory: string) => Promise<string> = gitHead,
) {
  if (!["base", "patched", "qualified-inputs"].includes(mode)) throw new Error("Unknown verification mode");
  if (manifest.schema !== 1) throw new Error("Unsupported manifest schema");
  const root = await realpath(checkout);
  for (const repo of manifest.repositories) {
    const path = relativePath(repo.path, true);
    if (!/^[0-9a-f]{40}$/.test(repo.commit)) throw new Error(`Invalid repository pin: ${path}`);
    if (await readHead(resolve(root, path)) !== repo.commit) throw new Error(`Repository pin mismatch: ${path || "Chromium"}`);
  }
  for (const image of manifest.patch_files) await verifyImage(patchDirectory, image);
  for (const image of manifest.standard_patch_files) await verifyImage(root, image);
  const images = new Map<string, string | null>();
  for (const image of manifest.standard_sources) images.set(relativePath(image.path), image.sha256);
  for (const image of manifest.authored_sources) {
    const path = relativePath(image.path);
    if (mode === "base" && images.has(path) && images.get(path) !== image.base_sha256) {
      throw new Error(`Conflicting baseline image: ${path}`);
    }
    images.set(path, mode === "base" ? image.base_sha256 : image.result_sha256);
  }
  if (mode === "qualified-inputs") {
    for (const image of manifest.qualification_inputs) {
      const path = relativePath(image.path);
      if (images.has(path) && images.get(path) !== image.sha256) throw new Error(`Conflicting qualified input: ${path}`);
      images.set(path, image.sha256);
    }
  }
  for (const [path, sha256] of images) await verifyImage(root, { path, sha256 });
  return { mode, repositories: manifest.repositories.length, sourceImages: images.size, patchFiles: manifest.patch_files.length };
}

if (import.meta.main) {
  try {
    const [mode, checkout, ...extra] = Bun.argv.slice(2);
    if (!checkout || extra.length) throw new Error("Usage: bun scripts/cef154-patches.ts <base|patched|qualified-inputs> <CEF154 chromium/src>");
    const directory = fileURLToPath(new URL("../patches/cef154/", import.meta.url));
    const manifest = await Bun.file(resolve(directory, "manifest.json")).json();
    console.log(JSON.stringify(await verifyStack(mode as Mode, checkout, manifest, directory)));
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
