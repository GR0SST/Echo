//! Store presentation compatibility, granted only by the source bridge's native
//! browser metadata. Installation itself stays in Chromium's Web Store APIs.
use cef::*;
use std::{cell::RefCell, collections::HashSet};
pub(crate) const ROLE_KEY: &str = "Echo.ExtensionStoreCompatible";
thread_local! { static OWNERS: RefCell<HashSet<i32>> = RefCell::new(HashSet::new()); }
pub(crate) fn created(browser: Option<&Browser>, extra: Option<&DictionaryValue>) {
    if let (Some(browser), Some(extra)) = (browser, extra) {
        if extra.bool(Some(&CefString::from(ROLE_KEY))) != 0 {
            OWNERS.with_borrow_mut(|owners| {
                owners.insert(browser.identifier());
            });
        }
    }
}
pub(crate) fn destroyed(browser: Option<&Browser>) {
    if let Some(browser) = browser {
        OWNERS.with_borrow_mut(|owners| {
            owners.remove(&browser.identifier());
        });
    }
}
pub(crate) fn context_created(browser: &Browser, frame: &Frame, context: &V8Context) {
    if frame.is_main() == 0 || !OWNERS.with_borrow(|owners| owners.contains(&browser.identifier()))
    {
        return;
    }
    let url = CefString::from(&frame.url()).to_string();
    if !url::Url::parse(&url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("chromewebstore.google.com")
            && url.port_or_known_default() == Some(443)
    }) {
        return;
    }
    let (mut result, mut exception) = (None, None);
    context.eval(
        Some(&CefString::from(include_str!("extension_store.js"))),
        Some(&CefString::from("echo-extension-store")),
        1,
        Some(&mut result),
        Some(&mut exception),
    );
}
