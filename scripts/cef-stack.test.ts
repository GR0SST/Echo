import { afterEach, expect, test } from 'bun:test';
import { mkdtemp, mkdir, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { applyStack } from './cef-stack';
const folders: string[] = [];
afterEach(async () => { await Promise.all(folders.splice(0).map(path => rm(path, { recursive: true, force: true }))); });
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'echo-cef-series-test-')); folders.push(root);
  const source = join(root, 'source'), patches = join(root, 'patches');
  await mkdir(join(source, 'cef'), { recursive: true }); await mkdir(patches);
  await Bun.write(join(source, 'cef/file.cc'), 'before\n');
  await Bun.write(join(patches, 'first.patch'), '--- a/cef/file.cc\n+++ b/cef/file.cc\n@@ -1 +1 @@\n-before\n+middle\n');
  await Bun.write(join(patches, 'second.patch'), '--- a/file.cc\n+++ b/file.cc\n@@ -1 +1 @@\n-middle\n+after\n');
  const series = { schema: 1, patches: [{ path: 'first.patch', directory: '' }, { path: 'second.patch', directory: 'cef' }] };
  const save = () => Bun.write(join(patches, 'series.json'), JSON.stringify(series)); await save();
  return { source, patches, series, save, text: () => readFile(join(source, 'cef/file.cc'), 'utf8') };
}
test('ordered cross-repository stack applies once and verifies the complete result', async () => {
  const f = await fixture();
  expect((await applyStack('apply', f.source, f.patches)).state).toBe('applied');
  expect(await f.text()).toBe('after\n');
  const checked = await applyStack('check', f.source, f.patches);
  expect(checked.state).toBe('already-applied');
  expect((await applyStack('apply', f.source, f.patches)).source_sha256).toBe(checked.source_sha256);
});
test('an incorrect order leaves the checkout untouched', async () => {
  const f = await fixture(); f.series.patches.reverse(); await f.save();
  await expect(applyStack('apply', f.source, f.patches)).rejects.toThrow('no source changed');
  expect(await f.text()).toBe('before\n');
});
test('partial and edited source cannot silently pass or be overwritten', async () => {
  const f = await fixture();
  for (const text of ['middle\n', 'hand-edited\n']) {
    await Bun.write(join(f.source, 'cef/file.cc'), text);
    await expect(applyStack('apply', f.source, f.patches)).rejects.toThrow('no source changed');
    await expect(applyStack('check', f.source, f.patches)).rejects.toThrow('incomplete or modified');
    expect(await f.text()).toBe(text);
  }
});
test('every patch needs one explicit position in the series', async () => {
  const f = await fixture(); await Bun.write(join(f.patches, 'forgotten.patch'), 'unlisted');
  await expect(applyStack('apply', f.source, f.patches)).rejects.toThrow('exactly once');
});
