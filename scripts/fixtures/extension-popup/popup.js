const views = chrome.extension.getViews({ type: "popup" });
await chrome.storage.local.set({ profileMarker: "fixture-profile-A" });
const { profileMarker } = await chrome.storage.local.get("profileMarker");
const background = await chrome.runtime.sendMessage("probe-model");
if (background.error) {
    document.title = "POPUP_GATE:background-model-failed";
    document.querySelector("#status").textContent = background.error;
    throw Error(background.error);
}
const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
if (background.value.redacted) {
    if (!tab || "url" in tab || "title" in tab || background.value.id !== tab.id) {
        document.title = "POPUP_GATE:unprivileged-metadata-leaked";
        throw Error("Unprivileged popup must receive the same redacted model tab");
    }
    document.querySelector("#status").textContent = `Redacted owner ${tab.id}; ${profileMarker}`;
} else {
if (!tab || !Number.isInteger(tab.id) || !tab.active ||
    tab.title !== "Alloy profile A" ||
    !/^http:\/\/127\.0\.0\.1:\d+\/A$/.test(tab.url ?? "")) {
    document.title = "POPUP_GATE:wrong-action-owner";
    throw new Error("Action popup query must return its exact Alloy owner");
}
const retrieved = await chrome.tabs.get(tab.id);
if (background.value.id !== tab.id || background.value.url !== tab.url ||
    await chrome.tabs.getCurrent() !== undefined) {
    document.title = "POPUP_GATE:popup-worker-owner-mismatch";
    throw Error("Popup, worker and tab lookup must share the model identity");
}
const filtered = await chrome.tabs.query({
    active: true, currentWindow: true, url: "https://does-not-match.invalid/*",
});
if (retrieved.url !== tab.url || filtered.length !== 0) {
    document.title = "POPUP_GATE:tab-query-isolation-failed";
    throw new Error("Tab lookup or query filters escaped the owner boundary");
}
document.querySelector("#status").textContent =
    `Popup views: ${views.length}; ${profileMarker}; owner ${tab.id}: ${tab.url}`;
}
document.querySelector("#identity").textContent = chrome.runtime.id;
document.title = `POPUP_GATE:${views.length}:${profileMarker}`;
