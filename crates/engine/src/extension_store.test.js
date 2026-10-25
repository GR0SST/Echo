import { test, expect } from 'bun:test';
import { runInNewContext } from 'node:vm';
const source = await Bun.file(new URL('./extension_store.js', import.meta.url)).text();
function host(native = true) {
  const window = native ? { chrome: { management: {}, webstorePrivate: { beginInstallWithManifest3() { throw Error('Compatibility must never install'); } } } } : {};
  runInNewContext(source, { window });
  return window;
}
test('native support changes only the presentation capability and preserves factory calls', () => {
  const window = host();
  const permissions = ['Read and change site data'];
  const receiver = { token: 42 };
  window._F_getIjData = function (value) { return { Kk: false, permissions, token: this.token, value }; };
  const result = window._F_getIjData.call(receiver, 'original argument');
  expect(result).toEqual({ Kk: true, permissions, token: 42, value: 'original argument' });
  expect(result.permissions).toBe(permissions);
});
test('missing native Store support keeps the page unsupported', () => {
  const window = host(false);
  window._F_getIjData = () => ({ Kk: false });
  expect(window._F_getIjData().Kk).toBe(false);
});
test('unknown site contracts fail closed', () => {
  const window = host();
  window._F_getIjData = () => ({ other: false });
  expect(window._F_getIjData()).toEqual({ other: false });
  window._F_getIjData = null;
  expect(window._F_getIjData).toBeNull();
});
test('a locked site factory is left intact', () => {
  const factory = () => ({ Kk: false });
  const window = {};
  Object.defineProperty(window, '_F_getIjData', { value: factory });
  runInNewContext(source, { window });
  expect(window._F_getIjData).toBe(factory);
});
