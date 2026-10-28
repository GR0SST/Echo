import {test, expect} from 'bun:test';
import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {brotliDecompressSync} from 'node:zlib';
import {createContext, runInContext, runInNewContext} from 'node:vm';

const source = readFileSync(new URL('../src/devtools_host.js', import.meta.url), 'utf8');
function setup() {
  const calls = [], dispatched = [], panels = [], menus = [];
  const global = {navigator: {userAgent: 'test Linux'}, InspectorFrontendAPI: {
    dispatchMessage: message => dispatched.push(message),
    showPanel: panel => panels.push(panel), setUseSoftMenu: value => menus.push(value),
    enterInspectElementMode: () => calls.push(['inspect']), fileSystemsLoaded() {},
  }};
  const receive = runInNewContext(source, global)((...args) => calls.push(args));
  return {global, host: global.InspectorFrontendHost, receive, calls, dispatched, panels, menus};
}
test('host callbacks unblock bootstrap, preferences stay in document memory, menus are soft', () => {
  const a = setup(), b = setup();
  expect(a.host.isHostedMode()).toBe(false);
  let config, preferences;
  a.host.getHostConfig(value => config = value);
  a.host.setPreference('secret', 'private');
  a.host.getPreferences(value => preferences = value);
  expect(config.devToolsVeLogging.enabled).toBe(false);
  // Bootstrap waits for the parent's dock placement before reading preferences.
  expect(preferences).toBeUndefined();
  a.receive('preferences', 'right');
  expect(preferences.secret).toBe('private');
  b.receive('preferences', 'right');
  b.host.getPreferences(value => expect(value.secret).toBeUndefined());
  a.host.clearPreferences();
  a.host.getPreferences(value => expect(Object.keys(value)).toHaveLength(0));
  a.host.loadCompleted();
  expect(a.menus).toEqual([true]);
  expect(a.calls).toContainEqual(['loaded', '']);
  expect(() => a.host.showContextMenuAtPoint()).toThrow('soft menus');
});
test('dock side is restored once from the parent and menu choices are reported', () => {
  const s = setup();
  let preferences, single;
  s.host.getPreferences(value => preferences = value);
  s.host.getPreference('currentDockState', value => single = value);
  s.receive('preferences', 'evil');
  expect(preferences).toBeUndefined();
  s.receive('preferences', 'bottom');
  expect(preferences.currentDockState).toBe('"bottom"');
  expect(single).toBe('"bottom"');
  s.receive('preferences', 'left');
  s.host.getPreferences(value => preferences = value);
  expect(preferences.currentDockState).toBe('"bottom"');
  // Restoring the same side is not a request; a menu choice is.
  s.host.setPreference('currentDockState', '"bottom"');
  s.host.setPreference('currentDockState', '"undocked"');
  s.host.setPreference('currentDockState', '"top"');
  expect(s.calls.filter(([kind]) => kind === 'dock')).toEqual([['dock', 'undocked']]);
});
test('backend data remains data, buffers until first bound send, and reports successful delivery', () => {
  const s = setup();
  const payload = JSON.stringify({id: 1, result: {value: '";globalThis.pwned = true;//\u2028'}});
  s.receive('protocol', payload);
  expect(s.dispatched).toHaveLength(0);
  s.host.sendMessageToBackend('{"id":2,"method":"Runtime.enable"}');
  expect(s.dispatched).toEqual([payload]);
  expect(s.global.pwned).toBeUndefined();
  expect(s.calls).toContainEqual(['roundtrip', '[1,null]']);
  s.receive('protocol', '{"id":2,"error":{"code":-1}}');
  expect(s.calls.filter(([kind]) => kind === 'roundtrip')).toHaveLength(1);
  s.receive('panel', 'console'); s.receive('panel', 'javascript:bad');
  expect(s.panels).toEqual(['console']);
  s.receive('inspect', ''); expect(s.calls).toContainEqual(['inspect']);
  s.receive('revoke', '');
  const count = s.calls.length;
  s.receive('protocol', payload); s.receive('inspect', ''); s.host.connectionReady();
  expect(s.dispatched).toHaveLength(2); expect(s.calls).toHaveLength(count);
});

// Optional exact installed-bundle contract checks. No network or CEF process.
const cef = process.env.CEF_PATH;
function resource(id) {
  const pak = readFileSync(join(cef, 'Chromium Embedded Framework.framework/Resources/resources.pak'));
  expect(pak.readUInt32LE(0)).toBe(5);
  const count = pak.readUInt16LE(8);
  for (let i = 0; i < count; i++) {
    const entry = 12 + i * 6;
    if (pak.readUInt16LE(entry) !== id) continue;
    const bytes = pak.subarray(pak.readUInt32LE(entry + 2), pak.readUInt32LE(entry + 8));
    return (bytes[0] === 0x1e && bytes[1] === 0x9b ? brotliDecompressSync(bytes.subarray(8)) : bytes).toString();
  }
  throw new Error(`Missing bundled resource ${id}`);
}
test.skipIf(!cef)('native host survives exact compatibility then original host-module bootstrap order', () => {
  const compatibility = resource(51410), bundledHost = resource(51395);
  expect(compatibility).toContain('window.InspectorFrontendHost = new InspectorFrontendHostImpl();');
  const stubStart = bundledHost.indexOf('k=class{');
  const bootStart = bundledHost.indexOf(';var c,ze=class', stubStart);
  const bootEnd = bundledHost.indexOf('function lr(', bootStart);
  const descriptorsStart = bundledHost.indexOf('We=[[');
  const descriptorsEnd = bundledHost.indexOf(',Xe=', descriptorsStart);
  expect(stubStart).toBeGreaterThan(0); expect(bootStart).toBeGreaterThan(stubStart);
  expect(bootEnd).toBeGreaterThan(bootStart);
  expect(descriptorsStart).toBeGreaterThan(0); expect(descriptorsEnd).toBeGreaterThan(descriptorsStart);
  const calls = [], embedderCalls = [], events = [];
  class ObjectWrapper {
    listeners = new Map();
    addEventListener(name, callback, receiver) {
      const listeners = this.listeners.get(name) ?? [];
      listeners.push(event => callback.call(receiver, event));
      this.listeners.set(name, listeners);
      return {};
    }
    dispatchEventToListeners(name, data) {
      events.push([name, data]);
      for (const listener of this.listeners.get(name) ?? []) listener({data});
    }
  }
  const global = {navigator: {userAgent: 'test Linux'}, location: {search: ''}, URLSearchParams,
    DevToolsHost: {sendMessageToEmbedder: message => embedderCalls.push(message)},
    X: {ObjectWrapper: {ObjectWrapper}},
    k: runInNewContext(`(${bundledHost.slice(stubStart + 2, bootStart)})`),
  };
  global.window = global;
  const context = createContext(global);
  const receive = runInContext(source, context)((...args) => calls.push(args));
  const nativeHost = global.InspectorFrontendHost;
  expect(global.InspectorFrontendAPI).toBeUndefined();
  // Full classic script injected by Blink, after the native context callback.
  runInContext(compatibility, context);
  expect(global.DevToolsAPI).toBeDefined();
  expect(global.InspectorFrontendHost).toBe(nativeHost);
  expect(global.InspectorFrontendAPI).toBeUndefined();
  // Execute the original API descriptors, API class, installer and startup IIFE.
  // Only imported ObjectWrapper infrastructure is supplied by the harness.
  const original = runInContext(`var ${bundledHost.slice(descriptorsStart, descriptorsEnd)};${bundledHost.slice(bootStart, bootEnd)};({host:c, API:ze})`, context);
  expect(original.host).toBe(nativeHost);
  expect(global.InspectorFrontendAPI).toBeInstanceOf(original.API);
  expect(nativeHost.events).toBeInstanceOf(ObjectWrapper);
  expect(nativeHost.isHostedMode()).toBe(false);
  const sdk = resource(51405), connectionStart = sdk.indexOf('As=class{');
  const connectionEnd = sdk.indexOf(',Ls=class{', connectionStart);
  expect(connectionStart).toBeGreaterThan(0); expect(connectionEnd).toBeGreaterThan(connectionStart);
  global.Ft = {InspectorFrontendHost: {InspectorFrontendHostInstance: original.host},
    InspectorFrontendHostAPI: {Events: {DispatchMessage: 'dispatchMessage', DispatchMessageChunk: 'dispatchMessageChunk'}}};
  global.Oh = {EventTarget: {removeEventListeners() {}}};
  const Connection = runInContext(`(${sdk.slice(connectionStart + 3, connectionEnd)})`, context);
  const connection = new Connection(), replies = [];
  connection.setOnMessage(message => replies.push(JSON.parse(message)));
  nativeHost.loadCompleted();
  connection.sendRawMessage('{"id":7,"method":"DOM.getDocument"}');
  receive('protocol', '{"id":7,"result":{"root":{"nodeId":1}}}');
  nativeHost.connectionReady(); nativeHost.readyForTest();
  receive('panel', 'elements');
  expect(replies).toEqual([{id: 7, result: {root: {nodeId: 1}}}]);
  expect(events).toContainEqual(['setUseSoftMenu', true]);
  expect(events).toContainEqual(['showPanel', 'elements']);
  expect(calls).toContainEqual(['loaded', '']);
  expect(calls).toContainEqual(['protocol', '{"id":7,"method":"DOM.getDocument"}']);
  expect(calls).toContainEqual(['roundtrip', '[7,null]']);
  expect(calls.slice(-2)).toEqual([['connected', ''], ['ready', '']]);
  expect(embedderCalls).toEqual([]);
  receive('revoke', '');
  const count = calls.length;
  nativeHost.sendMessageToBackend('{"id":8,"method":"DOM.getDocument"}');
  expect(calls).toHaveLength(count);
});

test.skipIf(!cef)('desktop entrypoint selects original SimpleApp without remote screencast layout', async () => {
  const html = resource(51409), desktop = resource(51412), remote = resource(51418);
  const engine = readFileSync(new URL('../src/devtools.rs', import.meta.url), 'utf8');
  expect(engine).toContain('pub const INSPECTOR_FRONTEND_URL: &str = "devtools://devtools/bundled/devtools_app.html?can_dock=true";');
  expect(html).toContain('src="./entrypoints/devtools_app/devtools_app.js"');
  expect(desktop).toContain('import"../shell/shell.js"');
  expect(desktop).not.toContain('screencast');
  expect(remote).toContain('import"../../panels/screencast/screencast-meta.js"');
  const main = resource(51424), meta = resource(51423), providers = [], rendered = [];
  const start = main.indexOf('var K=class'), end = main.indexOf(';export{', start);
  expect(start).toBeGreaterThan(0); expect(end).toBeGreaterThan(start);
  // Original desktop app only shows InspectorView in a RootView. There is no
  // injected replacement UI, hidden screencast, or synthetic docking request.
  const SimpleAppProvider = runInNewContext(`${main.slice(start, end)};te`, {_: {
    RootView: {RootView: class {
      element = {};
      attachToDocument(document) { rendered.push(['attach', document]); }
      focus() { rendered.push(['focus']); }
    }},
    InspectorView: {InspectorView: {instance() { return {show(element) { rendered.push(['inspector', element]); }}; }}},
  }});
  const registerStart = meta.indexOf('o.AppProvider.registerAppProvider(');
  const registerSuffix = '},order:10})';
  const registerEnd = meta.indexOf(registerSuffix, registerStart) + registerSuffix.length;
  expect(registerStart).toBeGreaterThan(0); expect(registerEnd).toBeGreaterThan(registerStart);
  runInNewContext(meta.slice(registerStart, registerEnd), {
    o: {AppProvider: {registerAppProvider: provider => providers.push(provider)}},
    n: async () => ({SimpleApp: {SimpleAppProvider}}),
  });
  expect(providers).toHaveLength(1);
  const provider = await providers[0].loadAppProvider();
  provider.createApp({}).presentUI('document');
  expect(rendered.map(([kind]) => kind)).toEqual(['inspector', 'attach', 'focus']);
});

test.skipIf(!cef)('installed Host prototype ABI has no missing methods or private-field stub fallbacks', () => {
  const text = resource(51395);
  // The exact installed minified stub is the class preceding the API class.
  const start = text.indexOf('k=class{');
  const end = text.indexOf(';var c,ze=class', start);
  expect(start).toBeGreaterThan(0); expect(end).toBeGreaterThan(start);
  const Stub = runInNewContext(`(${text.slice(start + 2, end)})`);
  const methods = Object.getOwnPropertyNames(Stub.prototype).filter(name => name !== 'constructor');
  expect(methods.length).toBeGreaterThan(70);
  const {host} = setup();
  expect(methods.filter(name => typeof host[name] !== 'function')).toEqual([]);
});
test.skipIf(!cef)('exact installed MainConnection accepts a reply before connectionReady', () => {
  const sdk = resource(51405);
  const start = sdk.indexOf('As=class{');
  const end = sdk.indexOf(',Ls=class{', start);
  expect(start).toBeGreaterThan(0); expect(end).toBeGreaterThan(start);
  const s = setup();
  const listeners = new Map();
  s.host.events = {addEventListener(name, callback, receiver) { listeners.set(name, event => callback.call(receiver, event)); return {}; }};
  const events = {DispatchMessage: 'dispatchMessage', DispatchMessageChunk: 'dispatchMessageChunk'};
  const Connection = runInNewContext(`(${sdk.slice(start + 3, end)})`, {
    Ft: {InspectorFrontendHost: {InspectorFrontendHostInstance: s.host}, InspectorFrontendHostAPI: {Events: events}},
    Oh: {EventTarget: {removeEventListeners() {}}},
  });
  s.global.InspectorFrontendAPI.dispatchMessage = message => listeners.get('dispatchMessage')({data: message});
  const connection = new Connection(), replies = [];
  // Listener registration alone must not release early notifications.
  s.receive('protocol', '{"method":"Runtime.executionContextCreated","params":{}}');
  connection.setOnMessage(message => replies.push(JSON.parse(message)));
  connection.sendRawMessage('{"id":1,"method":"Runtime.enable"}');
  s.receive('protocol', '{"id":1,"result":{}}');
  expect(replies).toHaveLength(2);
  expect(replies[1].id).toBe(1);
  expect(s.calls.some(([kind]) => kind === 'connected')).toBe(false);
  s.host.connectionReady(); s.host.readyForTest();
  expect(s.calls.slice(-2)).toEqual([['connected', ''], ['ready', '']]);
});

test('observed panel notifications and frontend close use original Host ABI', () => {
  const s = setup();
  s.host.recordUserMetricsAction('DevTools_PanelShown_network');
  s.host.setPreference('panel-selected-tab', JSON.stringify('console'));
  s.host.setPreference('drawer-view-selected-tab', JSON.stringify('console'));
  s.host.setPreference('panel-selected-tab', 'not-json');
  s.host.closeWindow();
  expect(s.calls).toEqual([['selected', 'network'], ['selected', 'console'], ['close', '']]);
});
test.skipIf(!cef)('exact bundled main-tab selection and manual preference signals exist', () => {
  const ui = resource(51929);
  expect(ui).toContain('tabSelected(e){xt.instance().panelShown(e,!1)}');
  expect(ui).toContain('this.lastSelectedTabSetting&&n&&this.lastSelectedTabSetting.set(t)');
  expect(ui).toContain('createSetting(i+"-selected-tab","")');
  const start = ui.indexOf('var xt=class '), end = ui.indexOf(';var R=', start);
  expect(start).toBeGreaterThan(0); expect(end).toBeGreaterThan(start);
  const s = setup();
  const Metrics = runInNewContext(`(${ui.slice(start + 7, end)})`, {
    Ze: {UserMetrics: {PanelCodes: {network: 5, MAX_VALUE: 100}}, InspectorFrontendHost: {InspectorFrontendHostInstance: s.host}},
  });
  new Metrics().panelShown('network', false);
  expect(s.calls).toContainEqual(['selected', 'network']);
  // Drawer/subpanel reports cannot be mistaken for one of the three known
  // top-level selections; the native state treats other names as unknown.
  new Metrics().panelShown('settings-preferences', false);
  expect(s.calls).toContainEqual(['selected', 'settings-preferences']);
});
