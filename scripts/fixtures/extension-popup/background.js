chrome.runtime.onMessage.addListener((message, _sender, reply) => {
    if (message !== "probe-model") return;
    (async () => {
        const tabs = await chrome.tabs.query({active: true, lastFocusedWindow: true, windowType: "normal"});
        const tab = tabs[0];
        if (tabs.length !== 1 || !tab) throw Error("Background worker did not resolve its profile's selected tab");
        const readable = chrome.runtime.getManifest().permissions.includes("activeTab");
        if (!readable) {
            if ("url" in tab || "title" in tab || "favIconUrl" in tab) throw Error("Unprivileged query exposed sensitive metadata");
            const filtered = await chrome.tabs.query({url: "http://127.0.0.1/*"});
            if (filtered.length) throw Error("Privileged URL filter leaked a tab through its result count");
            return {id: tab.id, redacted: true};
        }
        const filters = {
            active: true, currentWindow: true, lastFocusedWindow: true, windowType: "normal",
            url: "http://127.0.0.1/*", title: "Alloy profile A", pinned: false,
            index: 0, highlighted: true, audible: false, muted: false, discarded: false,
            frozen: false, autoDiscardable: true, groupId: -1, splitViewId: -1,
            status: "complete", windowId: tab.windowId,
        };
        const positive = await chrome.tabs.query(filters);
        if (positive.length !== 1 || positive[0].id !== tab.id) throw Error("Combined tab query filters did not match the owner");
        const mismatches = {
            active: false, currentWindow: false, lastFocusedWindow: false, windowType: "popup",
            url: "https://does-not-match.invalid/*", title: "Other profile*", pinned: true,
            index: 1, highlighted: false, audible: true, muted: true, discarded: true,
            frozen: true, autoDiscardable: false, groupId: 99999, splitViewId: 99999,
            status: "loading", windowId: tab.windowId + 99999,
        };
        for (const [key,value] of Object.entries(mismatches)) {
            const result = await chrome.tabs.query({...filters,[key]:value});
            if (result.length) throw Error(`Tab query ignored ${key}`);
        }
        const [current,last,all,byId] = await Promise.all([
            chrome.windows.getCurrent({populate:true}), chrome.windows.getLastFocused({populate:true}),
            chrome.windows.getAll({populate:true}), chrome.windows.get(tab.windowId,{populate:true}),
        ]);
        if (all.length !== 1 || [current,last,all[0],byId].some(window =>
            window.id !== tab.windowId || window.tabs.length !== 1 || window.tabs[0].id !== tab.id)) {
            throw Error("Window metadata escaped its profile or lost its tab identity");
        }
        if (await chrome.tabs.getCurrent() !== undefined) throw Error("A background worker was exposed as a model tab");
        const retrieved = await chrome.tabs.get(tab.id);
        if (retrieved.url !== tab.url) throw Error("Tab lookup and query disagree");
        return {id:tab.id,url:tab.url,filters:Object.keys(filters).length,redacted:false};
    })().then(value => reply({value}),error => reply({error:String(error)}));
    return true;
});
