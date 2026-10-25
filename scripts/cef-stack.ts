// Rehearse the complete ordered stack on selected source images before writing.
import { chmod, copyFile, lstat, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
export type Patch = { path: string; directory: string };
const digest = (bytes: Uint8Array | string) => new Bun.CryptoHasher('sha256').update(bytes).digest('hex');
function relative(path: string, root = false) {
  if (root && path === '') return path;
  if (!path || path.startsWith('/') || path.includes('\\') || path.split('/').some(p => !p || p === '.' || p === '..')) throw Error(`Unsafe patch path: ${path}`);
  return path;
}
async function git(cwd: string, args: string[]) {
  const child = Bun.spawn(['git', 'apply', ...args], { cwd, stdout: 'pipe', stderr: 'pipe' });
  const [code, stdout, stderr] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
  return { code, stdout, stderr };
}
type Image = { bytes: Uint8Array; mode: number } | null;
async function read(path: string): Promise<Image> {
  try {
    const stat = await lstat(path);
    if (!stat.isFile() || stat.isSymbolicLink()) throw Error(`Expected a regular patch target: ${path}`);
    return { bytes: await readFile(path), mode: stat.mode & 0o777 };
  } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null; throw error; }
}
const same = (a: Image, b: Image) => a === null || b === null ? a === b : a.mode === b.mode && digest(a.bytes) === digest(b.bytes);
async function atomic(path: string, image: Image) {
  if (!image) { await rm(path, { force: true }); return; }
  await mkdir(dirname(path), { recursive: true });
  const temporary = `${path}.echo-cef-stack-${crypto.randomUUID()}`;
  try { await writeFile(temporary, image.bytes); await chmod(temporary, image.mode); await rename(temporary, path); }
  finally { await rm(temporary, { force: true }); }
}
export async function applyStack(mode: 'apply' | 'check', source: string, patchDirectory: string) {
  const root = resolve(source), directory = resolve(patchDirectory);
  const series = JSON.parse(await readFile(join(directory, 'series.json'), 'utf8')) as { schema: number; patches: Patch[] };
  if (series.schema !== 1 || !Array.isArray(series.patches) || !series.patches.length) throw Error('Invalid CEF series');
  const declared = series.patches.map(p => relative(p.path));
  const actual = (await readdir(directory)).filter(p => p.endsWith('.patch')).sort();
  if (new Set(declared).size !== declared.length || JSON.stringify([...declared].sort()) !== JSON.stringify(actual)) throw Error('Every top-level CEF patch must occur exactly once in series.json');
  const paths = new Set<string>();
  const patches = [];
  for (const patch of series.patches) {
    relative(patch.directory, true);
    const file = join(directory, patch.path), bytes = await readFile(file);
    const stats = await git(root, ['--numstat', file]);
    if (stats.code) throw Error(`Invalid ${patch.path}: ${stats.stderr}`);
    for (const row of stats.stdout.trim().split('\n')) {
      const path = relative(row.split('\t').slice(2).join('\t'));
      paths.add(relative(patch.directory ? `${patch.directory}/${path}` : path));
    }
    patches.push({ ...patch, sha256: digest(bytes) });
  }
  const original = new Map<string, Image>();
  for (const path of [...paths].sort()) original.set(path, await read(join(root, path)));
  const temporary = await mkdtemp(join(tmpdir(), 'echo-cef-stack-'));
  let attempt = 0;
  async function shadow() {
    const folder = join(temporary, String(attempt++)); await mkdir(folder);
    for (const [path, image] of original) {
      if (image) { await mkdir(dirname(join(folder, path)), { recursive: true }); await copyFile(join(root, path), join(folder, path)); await chmod(join(folder, path), image.mode); }
    }
    return folder;
  }
  async function run(folder: string, reverse: boolean) {
    for (const patch of reverse ? [...patches].reverse() : patches) {
      const result = await git(folder, [...(reverse ? ['--reverse'] : []), ...(patch.directory ? [`--directory=${patch.directory}`] : []), join(directory, patch.path)]);
      if (result.code) return `${patch.path}: ${result.stderr.trim()}`;
    }
    return null;
  }
  try {
    let folder = await shadow();
    const reverseError = await run(folder, true);
    let state: 'already-applied' | 'applied' = 'already-applied';
    if (reverseError) {
      if (mode === 'check') throw Error(`CEF stack is incomplete or modified: ${reverseError}`);
      folder = await shadow();
      const forwardError = await run(folder, false);
      if (forwardError) throw Error(`CEF source is neither the baseline nor the complete stack; no source changed. ${forwardError}`);
      state = 'applied';
    } else {
      const roundtrip = await run(folder, false);
      if (roundtrip) throw Error(`CEF stack cannot round-trip: ${roundtrip}`);
    }
    const final = new Map<string, Image>();
    for (const path of original.keys()) {
      const image = await read(join(folder, path));
      if (state === 'already-applied' && !same(image, original.get(path)!)) throw Error(`CEF round-trip changed ${path}`);
      final.set(path, image);
      if (!same(await read(join(root, path)), original.get(path)!)) throw Error(`CEF source changed during rehearsal: ${path}`);
    }
    const changed: string[] = [];
    if (state === 'applied') {
      try {
        for (const [path, image] of final) if (!same(image, original.get(path)!)) { changed.push(path); await atomic(join(root, path), image); }
      } catch (error) {
        for (const path of changed.reverse()) await atomic(join(root, path), original.get(path)!);
        throw error;
      }
    }
    return { state, patches, stack_sha256: digest(JSON.stringify(patches)), source_images: final.size,
      source_sha256: digest(JSON.stringify([...final].map(([path, image]) => ({ path, sha256: image && digest(image.bytes), mode: image?.mode })))) };
  } finally { await rm(temporary, { recursive: true, force: true }); }
}
if (import.meta.main) {
  const [mode, source, ...extra] = Bun.argv.slice(2);
  try {
    if (!['apply', 'check'].includes(mode) || !source || extra.length) throw Error('Usage: bun scripts/cef-stack.ts <apply|check> <CEF chromium/src>');
    console.log(JSON.stringify(await applyStack(mode as 'apply' | 'check', source, resolve(import.meta.dir, '../patches/cef154')), null, 2));
  } catch (error) { console.error(error instanceof Error ? error.message : error); process.exitCode = 1; }
}
