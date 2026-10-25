import { afterEach, expect, test } from 'bun:test';
import { mkdir, mkdtemp, realpath, rm, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { version } from './cef-dist';
const roots: string[] = [];
afterEach(async () => { for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true }); });
async function fixture() {
  const root = await realpath(await mkdtemp(join(tmpdir(), 'echo-cef-clean-test-'))); roots.push(root);
  const generation = '20261025T000000Z-1';
  const adapter = join(root, 'canonical-adapters', generation);
  const binary = join(root, 'canonical-binary-distrib', generation, 'Chromium Embedded Framework.framework');
  for (const path of [binary, ...['include', 'cmake', 'libcef_dll'].map(name => join(adapter, name))]) await mkdir(path, { recursive: true });
  await Bun.write(join(binary, 'Chromium Embedded Framework'), 'canonical binary');
  await symlink(binary, join(adapter, 'Chromium Embedded Framework.framework'));
  await symlink(adapter, join(root, 'echo-extensions-dist'));
  await Bun.write(join(adapter, 'CMakeLists.txt'), '# fixture');
  await Bun.write(join(adapter, 'include/cef_version.h'), `#define CEF_VERSION "${version}"\n`);
  await Bun.write(join(adapter, 'archive.json'), JSON.stringify({ type: 'minimal', name: `cef_binary_${version}_macosarm64_minimal.tar.bz2`, sha1: '0'.repeat(40) }));
  const hash = new Bun.CryptoHasher('sha256').update('canonical binary').digest('hex');
  await Bun.write(join(adapter, 'build.json'), JSON.stringify({ framework_sha256: hash }));
  const qualification = { passed: true, framework_sha256: hash, codecs: { passed: true }, extensions: { passed: true }, pip: { passed: true }, browsing_data: { passed: true }, shutdown_recovery: { passed: true },
    shutdown: { clean_exits: 16, matrix: Array.from({ length: 8 }, () => ({ passed: true, cookies_verified: true })) } };
  const save = () => Bun.write(join(adapter, 'qualification.json'), JSON.stringify(qualification)); await save();
  for (const name of ['echo-pip-dist/old', 'pip-final-binary-distrib/old', 'qualification/retain', 'chromium/src/retain', 'unrelated/retain', 'releases/phase2-413c771.tar.bz2']) await Bun.write(join(root, name), 'retain unless obsolete');
  async function run() {
    const child = Bun.spawn(['bun', join(import.meta.dir, 'clean-cef-dists.ts'), '--locked'], { env: { ...process.env, ECHO_CEF_SOURCE: root }, stdout: 'pipe', stderr: 'pipe' });
    const [code, stdout, stderr] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
    return { code, stdout, stderr };
  }
  return { root, adapter, binary, qualification, save, run };
}
test('cleanup refuses an incomplete native gate before deleting any artifact', async () => {
  const f = await fixture(); f.qualification.pip.passed = false; await f.save();
  expect((await f.run()).code).not.toBe(0);
  expect(await Bun.file(join(f.root, 'echo-pip-dist/old')).exists()).toBe(true);
  expect(await Bun.file(join(f.adapter, 'cleanup.json')).exists()).toBe(false);
});
test('cleanup refuses a framework that changed after qualification', async () => {
  const f = await fixture(); await Bun.write(join(f.binary, 'Chromium Embedded Framework'), 'changed');
  expect((await f.run()).code).not.toBe(0);
  expect(await Bun.file(join(f.root, 'pip-final-binary-distrib/old')).exists()).toBe(true);
});
test('qualified cleanup removes obsolete packages while retaining canonical binaries, sources and evidence', async () => {
  const f = await fixture(), result = await f.run();
  expect(result.code).toBe(0);
  for (const path of ['echo-pip-dist/old', 'pip-final-binary-distrib/old', 'releases/phase2-413c771.tar.bz2']) expect(await Bun.file(join(f.root, path)).exists()).toBe(false);
  for (const path of ['qualification/retain', 'chromium/src/retain', 'unrelated/retain']) expect(await Bun.file(join(f.root, path)).exists()).toBe(true);
  expect(await Bun.file(join(f.binary, 'Chromium Embedded Framework')).text()).toBe('canonical binary');
  expect(await realpath(join(f.root, 'echo-extensions-dist'))).toBe(f.adapter);
  expect((await Bun.file(join(f.adapter, 'cleanup.json')).json()).removed.length).toBe(3);
});
