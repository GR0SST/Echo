// Delete only superseded CEF build artifacts after the canonical gates pass.
import { lstat, readdir, realpath, rm } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join, resolve, sep } from 'node:path';
import { validate } from './cef-dist';
if (!Bun.argv.includes('--locked')) {
  const child = Bun.spawn(['lockf', '-k', '-t', '120', '/tmp/echo-cef-build.lock', 'bun', import.meta.path, '--locked'], { stdout: 'inherit', stderr: 'inherit' });
  process.exit(await child.exited);
}
const root = resolve(process.env.ECHO_CEF_SOURCE ?? join(homedir(), 'Documents/Bot/cef-src'));
const repository = resolve(import.meta.dir, '..');
if (root === repository || root.startsWith(repository + sep)) throw Error('CEF artifacts must be outside the repository');
const canonicalPath = join(root, 'echo-extensions-dist');
const canonical = await validate(canonicalPath);
const build = await Bun.file(join(canonical, 'build.json')).json();
const qualification = await Bun.file(join(canonical, 'qualification.json')).json();
if (!qualification.passed || qualification.framework_sha256 !== build.framework_sha256 ||
    !qualification.codecs?.passed || !qualification.extensions?.passed || !qualification.pip?.passed || !qualification.browsing_data?.passed || !qualification.shutdown_recovery?.passed ||
    qualification.shutdown?.clean_exits !== 16 || qualification.shutdown.matrix.length !== 8 ||
    qualification.shutdown.matrix.some((r: any) => !r.passed || !r.cookies_verified)) throw Error('All canonical gates must pass before deleting superseded distributions');
const framework = await realpath(join(canonical, 'Chromium Embedded Framework.framework/Chromium Embedded Framework'));
const hash = new Bun.CryptoHasher('sha256');
for await (const chunk of Bun.file(framework).stream()) hash.update(chunk);
if (hash.digest('hex') !== build.framework_sha256) throw Error('Canonical framework changed after qualification');
const binaryRoot = join(root, 'canonical-binary-distrib');
if (!framework.startsWith(binaryRoot + sep) || !canonical.startsWith(join(root, 'canonical-adapters') + sep)) throw Error('Canonical CEF is not an immutable generation');
const keptBinary = join(binaryRoot, framework.slice(binaryRoot.length + 1).split(sep)[0]);
const oldPhaseTwoArchive = join(root, 'releases/phase2-413c771.tar.bz2');
const candidates = new Set([
  ...['echo-dist', 'echo-pip-dist', 'echo-pip-final-dist', 'bridge-binary-distrib', 'pip-binary-distrib', 'pip-final-binary-distrib'].map(name => join(root, name)),
  join(root, 'chromium/src/cef/binary_distrib'),
  oldPhaseTwoArchive,
]);
for (const name of await readdir(root)) if (name.startsWith('echo-extensions-dist.previous-')) candidates.add(join(root, name));
for (const parent of [binaryRoot, join(root, 'canonical-adapters')]) {
  for (const name of await readdir(parent)) {
    const path = join(parent, name);
    if (/^\d{8}T\d{6}Z-\d+$/.test(name) && path !== keptBinary && path !== canonical) candidates.add(path);
  }
}
const removed = [];
for (const path of candidates) {
  let stat;
  try { stat = await lstat(path); } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') continue; throw error; }
  if (!stat.isDirectory() && !stat.isSymbolicLink() && !(path === oldPhaseTwoArchive && stat.isFile())) throw Error(`Unexpected distribution artifact: ${path}`);
  if (stat.isDirectory()) {
    const actual = await realpath(path);
    if (actual === canonical || framework.startsWith(actual + sep)) throw Error(`Refusing to remove the canonical distribution: ${path}`);
  }
  const size = Bun.spawn(['du', '-sk', path], { stdout: 'pipe', stderr: 'pipe' });
  const text = await new Response(size.stdout).text();
  if (await size.exited !== 0) throw Error(`Cannot measure ${path}`);
  const logicalKiB = Number(text.split(/\s/)[0]);
  await rm(path, { recursive: true, force: true });
  removed.push({ path, logical_kib: logicalKiB });
}
const result = { canonical: canonicalPath, generation: canonical, framework_sha256: build.framework_sha256,
  removed, logical_kib_removed: removed.reduce((sum, item) => sum + item.logical_kib, 0), completed_utc: new Date().toISOString() };
await Bun.write(join(canonical, 'cleanup.json'), JSON.stringify(result, null, 2) + '\n');
console.log(JSON.stringify(result, null, 2));
