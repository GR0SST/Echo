// The original worker inspector should display this message without an evaluator.
console.log('Echo Developer Fixture worker ready', chrome.runtime.getManifest().version);
chrome.runtime.onInstalled.addListener(() => {
  chrome.storage.local.set({ installedVersion: chrome.runtime.getManifest().version });
});
