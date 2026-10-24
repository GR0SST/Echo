//! Origin-scoped site-data operations on the existing page's CEF context.
//! Protocol callbacks only queue replies; reload happens after polling verifies
//! that the same view still displays the origin the user chose to clear.
use crate::{devtools::MessageIds, ViewId};
use cef::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

thread_local! { static DOCUMENTS: std::cell::RefCell<HashMap<i32, Option<u64>>> = std::cell::RefCell::new(HashMap::new()); }
pub(crate) fn document_epoch(browser: i32) -> Option<u64> {
    DOCUMENTS.with_borrow(|epochs| epochs.get(&browser).copied().unwrap_or(Some(0)))
}
pub(crate) fn navigated(browser: i32) {
    DOCUMENTS.with_borrow_mut(|epochs| {
        let previous = epochs.get(&browser).copied().unwrap_or(Some(0));
        epochs.insert(browser, previous.and_then(|value| value.checked_add(1)));
    });
}
pub(crate) fn destroyed(browser: i32) {
    DOCUMENTS.with_borrow_mut(|epochs| {
        epochs.remove(&browser);
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearSiteData {
    Cache,
    Cookies,
}
#[derive(Debug)]
pub(crate) struct Finished {
    pub view: ViewId,
    pub origin: String,
    pub success: bool,
    pub epoch: u64,
}
type Replies = Arc<Mutex<Vec<(ViewId, i32, bool)>>>;
struct Pending {
    message: i32,
    epoch: u64,
    origin: String,
    deadline: Instant,
    _registration: Option<Registration>,
}
#[derive(Default)]
pub(crate) struct SiteData {
    pending: HashMap<ViewId, Pending>,
    replies: Replies,
}
impl SiteData {
    pub(crate) fn request(
        &mut self,
        view: ViewId,
        origin: &str,
        kind: ClearSiteData,
        browser: &Browser,
        ids: &MessageIds,
    ) -> bool {
        let Some(host) = browser.host() else {
            return false;
        };
        let Some(message) = ids.next() else {
            return false;
        };
        let Some(epoch) = document_epoch(browser.identifier()) else {
            return false;
        };
        if !self.begin(view, origin, message, epoch, Instant::now()) {
            return false;
        }
        let mut observer = SiteDataObserver::new(self.replies.clone(), view, message);
        let Some(registration) = host.add_dev_tools_message_observer(Some(&mut observer)) else {
            self.close(view);
            return false;
        };
        self.pending.get_mut(&view).unwrap()._registration = Some(registration);
        let method = match kind {
            ClearSiteData::Cache => "Network.clearBrowserCache",
            ClearSiteData::Cookies => "Storage.clearDataForOrigin",
        };
        let mut params = dictionary_value_create();
        if kind == ClearSiteData::Cookies {
            let Some(params) = params.as_mut() else {
                self.close(view);
                return false;
            };
            if params.set_string(Some(&"origin".into()), Some(&origin.into())) == 0
                || params.set_string(Some(&"storageTypes".into()), Some(&"all".into())) == 0
            {
                self.close(view);
                return false;
            }
        }
        if host.execute_dev_tools_method(message, Some(&method.into()), params.as_mut()) != message
        {
            self.close(view);
            return false;
        }
        true
    }
    fn begin(
        &mut self,
        view: ViewId,
        origin: &str,
        message: i32,
        epoch: u64,
        now: Instant,
    ) -> bool {
        if self.pending.contains_key(&view)
            || crate::site_info::web_origin(origin).as_deref() != Some(origin)
        {
            return false;
        }
        self.pending.insert(
            view,
            Pending {
                message,
                epoch,
                origin: origin.into(),
                deadline: now + Duration::from_secs(5),
                _registration: None,
            },
        );
        true
    }
    pub(crate) fn close(&mut self, view: ViewId) {
        self.pending.remove(&view);
    }
    pub(crate) fn take(&mut self) -> Vec<Finished> {
        self.collect(Instant::now())
    }
    fn collect(&mut self, now: Instant) -> Vec<Finished> {
        let mut replies = std::mem::take(&mut *self.replies.lock().unwrap());
        replies.extend(
            self.pending
                .iter()
                .filter(|(_, p)| now >= p.deadline)
                .map(|(&view, p)| (view, p.message, false)),
        );
        let mut finished = Vec::new();
        for (view, message, success) in replies {
            if !self
                .pending
                .get(&view)
                .is_some_and(|p| p.message == message)
            {
                continue;
            }
            let pending = self.pending.remove(&view).unwrap();
            finished.push(Finished {
                view,
                origin: pending.origin,
                epoch: pending.epoch,
                success,
            });
        }
        finished
    }
}
wrap_dev_tools_message_observer! {
    struct SiteDataObserver { replies:Replies, view:ViewId, message:i32 }
    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(&self,_browser:Option<&mut Browser>,message_id:i32,success:i32,_result:Option<&[u8]>) {
            if message_id==self.message {self.replies.lock().unwrap().push((self.view,self.message,success!=0));}
        }
        fn on_dev_tools_agent_detached(&self,_browser:Option<&mut Browser>) {self.replies.lock().unwrap().push((self.view,self.message,false));}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_views_and_old_replies_cannot_complete_a_retry() {
        let mut data = SiteData::default();
        let now = Instant::now();
        let view = ViewId(1);
        assert!(data.begin(view, "https://site.example", 10, 0, now));
        assert!(!data.begin(view, "https://other.example", 11, 0, now));
        data.close(view);
        assert!(data.begin(view, "https://other.example", 12, 0, now));
        data.replies
            .lock()
            .unwrap()
            .extend([(view, 10, true), (view, 12, true), (view, 12, true)]);
        let results = data.collect(now);
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert_eq!(results[0].origin, "https://other.example");
    }
    #[test]
    fn timeout_is_terminal_and_scope_requires_an_exact_web_origin() {
        let mut data = SiteData::default();
        let now = Instant::now();
        let view = ViewId(1);
        for origin in [
            "file:///tmp/a",
            "https://site.example/path",
            "https://site.example:443",
        ] {
            assert!(!data.begin(view, origin, 1, 0, now));
        }
        assert!(data.begin(view, "https://site.example", 2, 0, now));
        assert!(!data.collect(now + Duration::from_secs(6))[0].success);
        data.replies.lock().unwrap().push((view, 2, true));
        assert!(data.collect(now).is_empty());
    }
    #[test]
    fn a_transferred_views_late_reply_cannot_complete_a_new_pools_request() {
        let runtime_ids = MessageIds::default();
        let source_ids = runtime_ids.clone();
        let target_ids = runtime_ids.clone();
        let old = source_ids.next().unwrap();
        let next = target_ids.next().unwrap();
        assert_ne!(old, next);
        let mut target = SiteData::default();
        let view = ViewId(1);
        let now = Instant::now();
        assert!(target.begin(view, "https://site.example", next, 0, now));
        target.replies.lock().unwrap().push((view, old, true));
        assert!(target.collect(now).is_empty());
        target.replies.lock().unwrap().push((view, next, true));
        assert!(target.collect(now)[0].success);
    }
    #[test]
    fn navigation_invalidates_completion_even_before_the_new_address_commits() {
        let browser = 937001;
        destroyed(browser);
        let epoch = document_epoch(browser).unwrap();
        let mut data = SiteData::default();
        let now = Instant::now();
        let view = ViewId(1);
        assert!(data.begin(view, "https://site.example", 1, epoch, now));
        navigated(browser);
        data.replies.lock().unwrap().push((view, 1, true));
        let done = data.collect(now);
        assert!(done[0].success);
        assert_ne!(Some(done[0].epoch), document_epoch(browser));
        destroyed(browser);
    }
}
