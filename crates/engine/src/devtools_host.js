// CEF 154 bundled Host ABI (resources.pak 51395). No hosted-stub inheritance:
// several stub methods require private fields that an embedder cannot possess.
(function(send) {
  'use strict';
  let live = true, bound = false, queued = [], queuedBytes = 0;
  const preferences = Object.create(null);
  // The frontend builds its dock controller from these; the parent releases
  // them with the remembered dock side.
  let preferencesReady = false, waitingPreferences = [];
  const docks = ['right', 'left', 'bottom', 'undocked'];
  const emit = (kind, payload = '') => live && send(kind, payload);
  const api = () => globalThis.InspectorFrontendAPI;
  const dispatch = message => {
    api().dispatchMessage(message);
    const reply = JSON.parse(message);
    if (typeof reply.id === 'number' && reply.result && typeof reply.result === 'object')
      emit('roundtrip', JSON.stringify([reply.id, reply.sessionId ?? null]));
  };
  const host = {
    platform() { return navigator.userAgent.includes('Windows NT') ? 'windows' : navigator.userAgent.includes('Mac OS X') ? 'mac' : 'linux'; },
    isHostedMode() { return false; },
    getPreferences(callback) { if (preferencesReady) callback({...preferences}); else waitingPreferences.push(() => callback({...preferences})); },
    getPreference(name, callback) { if (preferencesReady) callback(preferences[name]); else waitingPreferences.push(() => callback(preferences[name])); },
    registerPreference() {},
    setPreference(name, value) {
      if (live && name === 'currentDockState') {
        try { const dock = JSON.parse(value); if (docks.includes(dock) && preferences[name] !== value) emit('dock', dock); } catch {}
      }
      if (live && name === 'panel-selected-tab') {
        try { const panel = JSON.parse(value); if (typeof panel === 'string') emit('selected', panel); } catch {}
      }
      if (typeof name === 'string' && typeof value === 'string' && name.length < 4096 && value.length < 1048576 && (Object.keys(preferences).length < 1024 || name in preferences)) preferences[name] = value; },
    removePreference(name) { delete preferences[name]; },
    clearPreferences() { for (const name of Object.keys(preferences)) delete preferences[name]; },
    getHostConfig(callback) { callback({devToolsVeLogging: {enabled: false}}); },
    getSyncInformation(callback) { callback({isSyncActive: false, arePreferencesSynced: false}); },
    loadCompleted() { api().setUseSoftMenu(true); emit('loaded'); },
    sendMessageToBackend(message) {
      if (typeof message !== 'string') return;
      // First send occurs after MainConnection.setOnMessage, not merely after
      // the API object or its event listeners have been constructed.
      if (!bound) {
        bound = true;
        for (const message of queued) dispatch(message);
        queued = []; queuedBytes = 0;
      }
      emit('protocol', message);
    },
    closeWindow() { emit('close'); },
    bringToFront() { emit('focus'); },
    recordUserMetricsAction(action) {
      const prefix = 'DevTools_PanelShown_';
      if (typeof action === 'string' && action.startsWith(prefix)) emit('selected', action.slice(prefix.length));
    },
    connectionReady() { emit('connected'); },
    readyForTest() { emit('ready'); },
    zoomFactor() { return 1; },
    setIsDocked(_docked, callback) { callback?.(); },
    // Docked, the frontend fills its pane and reserves this rectangle for the
    // inspected page, which the parent places above it.
    setInspectedPageBounds(bounds) {
      const values = [bounds?.x, bounds?.y, bounds?.width, bounds?.height].map(Math.round);
      if (values.every(value => Number.isFinite(value) && value >= 0 && value <= 100000)) emit('bounds', values.join(','));
    },
    initialTargetId() { return Promise.resolve(null); },
    requestFileSystems() { api().fileSystemsLoaded([]); },
    isolatedFileSystem() { return null; },
    loadNetworkResource(_url, _headers, _stream, callback) { callback({statusCode: 404, netError: -2, netErrorName: 'ERR_FAILED'}); },
    connectAutomaticFileSystem(_path, _uuid, _add, callback) { callback({success: false}); },
    showSurvey(_trigger, callback) { callback({surveyShown: false}); },
    canShowSurvey(_trigger, callback) { callback({canShowSurvey: false}); },
    reattach(callback) { callback?.(); },
    showContextMenuAtPoint() { throw new Error('Embedded DevTools requires soft menus'); },
    doAidaConversation(_request, _stream, callback) { callback({error: 'Unsupported'}); },
    registerAidaClientEvent(_request, callback) { callback({error: 'Unsupported'}); },
    aidaCodeComplete(_request, callback) { callback({error: 'Unsupported'}); },
    dispatchHttpRequest(_request, callback) { callback({error: 'Unsupported'}); },
  };
  // Explicit, version-matched optional ABI. These features need parent-owned
  // integrations; none is silently borrowed from the hosted implementation.
  for (const name of [
    'inspectElementCompleted',
    'setInjectedScriptForOrigin','inspectedURLChanged','copyText','openInNewTab',
    'openSearchResultsInNewTab','showItemInFolder','save','append','close',
    'recordCountHistogram','recordEnumeratedHistogram','recordPerformanceHistogram',
    'recordPerformanceHistogramMedium','recordNewBadgeUsage',
    'disconnectAutomaticFileSystem','addFileSystem','removeFileSystem',
    'upgradeDraggedFileSystemPermissions','indexPath','stopIndexing','searchInPath',
    'zoomIn','zoomOut','resetZoom','setWhitelistedShortcuts','setEyeDropperActive',
    'showCertificateViewer','setOpenNewWindowForPopups','setDevicesDiscoveryConfig',
    'setDevicesUpdatesEnabled','openRemotePage','openNodeFrontend','setAddExtensionCallback',
    'recordImpression','recordResize','recordClick','recordHover','recordDrag','recordChange',
    'recordKeyDown','recordSettingAccess','recordFunctionCall','setChromeFlag','requestRestart'
  ]) host[name] = function() {};
  // Blink runs bundled devtools_compatibility.js after OnContextCreated. Its
  // unconditional assignment would replace this native transport with the
  // unconnected DevToolsHost embedder. Keep this document's authenticated host
  // stable, including when core/host installs it into its module-local binding.
  // A setter permits those strict-mode assignments without throwing. The host
  // object stays extensible: the original module owns host.events and creates
  // InspectorFrontendAPI itself. No dispatcher or frontend module is replaced.
  Object.defineProperty(globalThis, 'InspectorFrontendHost', {
    configurable: false, enumerable: true,
    get() { return host; },
    set(_value) {},
  });
  return function(kind, payload) {
    if (kind === 'revoke') { live = false; queued = []; queuedBytes = 0; return; }
    if (!live) return;
    if (kind === 'protocol') {
      if (bound) dispatch(payload);
      else if (queued.length < 8192 && queuedBytes + payload.length <= 16777216) {
        queued.push(payload); queuedBytes += payload.length;
      } else { emit('release'); live = false; }
    } else if (kind === 'panel' && ['elements','console','network'].includes(payload)) api().showPanel(payload);
    else if (kind === 'inspect') api().enterInspectElementMode();
    else if (kind === 'preferences' && docks.includes(payload) && !preferencesReady) {
      preferences.currentDockState = JSON.stringify(payload);
      preferencesReady = true;
      for (const resume of waitingPreferences.splice(0)) resume();
    }
  };
})
