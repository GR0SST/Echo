const views = chrome.extension.getViews({ type: "popup" });
await chrome.storage.local.set({ profileMarker: "fixture-profile-A" });
const { profileMarker } = await chrome.storage.local.get("profileMarker");
document.querySelector("#status").textContent = `Popup views: ${views.length}; ${profileMarker}`;
document.querySelector("#identity").textContent = chrome.runtime.id;
document.title = `POPUP_GATE:${views.length}:${profileMarker}`;
