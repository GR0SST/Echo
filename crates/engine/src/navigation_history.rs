//! Retained back/forward entries over CEF's local protocol transport.
//! A request owns its observer lease; different windows cannot drain each
//! other's replies. Callbacks enqueue bytes and never enter application state.
use crate::{Engine, ViewId, ViewRole};
use cef::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const LIMIT: usize = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationHistoryError {
    Unavailable,
    Cancelled,
    TimedOut,
    InvalidReply,
    TooLarge,
}
impl std::fmt::Display for NavigationHistoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "Navigation history is unavailable.",
            Self::Cancelled => "The page is no longer available.",
            Self::TimedOut => "Navigation history timed out.",
            Self::InvalidReply => "Navigation history could not be read.",
            Self::TooLarge => "Navigation history is too large.",
        })
    }
}
struct Reply {
    finished: bool,
    result: Option<Result<Vec<u8>, NavigationHistoryError>>,
}
struct Lease {
    // Native handles must go away before their context/runtime owners.
    registration: Option<Registration>,
    reply: Arc<Mutex<Reply>>,
    deadline: Instant,
    document: Option<(i32, u64)>,
    _context: Option<Rc<RequestContext>>,
    _runtime: Option<Rc<crate::Runtime>>,
}
impl Lease {
    fn release(&mut self) {
        self.registration.take();
        self._context.take();
        self._runtime.take();
    }
    fn cancel(&mut self) {
        self.release();
        let mut state = self.reply.lock().unwrap();
        state.finished = true;
        state.result = Some(Err(NavigationHistoryError::Cancelled));
    }
}

/// Opaque CEF history-entry ID. It is not a URL navigation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationEntry {
    pub id: i32,
    pub title: String,
    pub url: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationHistory {
    entries: Vec<NavigationEntry>,
    current: usize,
    source: Option<(i32, u64)>,
}
impl NavigationHistory {
    /// Alternative engines may create snapshots; native navigation accepts
    /// only snapshots issued for its exact retained document.
    pub fn new(entries: Vec<NavigationEntry>, current: usize) -> Option<Self> {
        let mut ids = std::collections::HashSet::new();
        (entries.len() <= 512
            && current < entries.len()
            && entries.iter().all(|entry| ids.insert(entry.id)))
        .then_some(Self {
            entries,
            current,
            source: None,
        })
    }
    pub fn entries(&self) -> &[NavigationEntry] {
        &self.entries
    }
    pub fn current_index(&self) -> usize {
        self.current
    }
    pub fn directional_entries(&self, forward: bool) -> Vec<NavigationEntry> {
        if forward {
            self.entries[self.current + 1..].to_vec()
        } else {
            self.entries[..self.current].iter().rev().cloned().collect()
        }
    }
}
/// Single-consumer, main-thread ticket. Polling never pumps the engine.
pub struct NavigationHistoryRequest {
    lease: Rc<RefCell<Lease>>,
    completed: Option<Result<NavigationHistory, NavigationHistoryError>>,
    consumed: bool,
}
impl NavigationHistoryRequest {
    pub fn completed(result: Result<NavigationHistory, NavigationHistoryError>) -> Self {
        Self {
            lease: Rc::new(RefCell::new(Lease {
                registration: None,
                reply: Arc::new(Mutex::new(Reply {
                    finished: true,
                    result: None,
                })),
                deadline: Instant::now() + TIMEOUT,
                document: None,
                _context: None,
                _runtime: None,
            })),
            completed: Some(result),
            consumed: false,
        }
    }
    pub fn poll(&mut self) -> Option<Result<NavigationHistory, NavigationHistoryError>> {
        if self.consumed {
            return None;
        }
        if let Some(result) = self.completed.take() {
            self.consumed = true;
            return Some(result);
        }
        let mut lease = self.lease.borrow_mut();
        if lease.document.is_some_and(|(browser, epoch)| {
            crate::site_data::document_epoch(browser) != Some(epoch)
        }) {
            lease.cancel();
        }
        let result = {
            let mut reply = lease.reply.lock().unwrap();
            if !reply.finished && Instant::now() >= lease.deadline {
                reply.finished = true;
                reply.result = Some(Err(NavigationHistoryError::TimedOut));
            }
            reply.result.take()
        }?;
        let source = lease.document;
        lease.release();
        self.consumed = true;
        Some(result.and_then(decode).map(|mut history| {
            history.source = source;
            history
        }))
    }
}
fn decode(bytes: Vec<u8>) -> Result<NavigationHistory, NavigationHistoryError> {
    if bytes.len() > LIMIT {
        return Err(NavigationHistoryError::TooLarge);
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        id: i32,
        title: String,
        url: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Response {
        current_index: usize,
        entries: Vec<Entry>,
    }
    let response: Response =
        serde_json::from_slice(&bytes).map_err(|_| NavigationHistoryError::InvalidReply)?;
    NavigationHistory::new(
        response
            .entries
            .into_iter()
            .map(|entry| NavigationEntry {
                id: entry.id,
                title: entry.title,
                url: entry.url,
            })
            .collect(),
        response.current_index,
    )
    .ok_or(NavigationHistoryError::InvalidReply)
}

#[derive(Default)]
pub(crate) struct Histories {
    requests: HashMap<ViewId, Vec<Weak<RefCell<Lease>>>>,
}
impl Histories {
    fn busy(&mut self, view: ViewId) -> bool {
        let requests = self.requests.entry(view).or_default();
        requests.retain(|lease| lease.strong_count() > 0);
        requests
            .iter()
            .filter_map(Weak::upgrade)
            .any(|lease| !lease.borrow().reply.lock().unwrap().finished)
    }
    pub(crate) fn close(&mut self, view: ViewId) {
        for request in self
            .requests
            .remove(&view)
            .into_iter()
            .flatten()
            .filter_map(|request| request.upgrade())
        {
            request.borrow_mut().cancel();
        }
    }
}
impl Engine {
    pub fn request_navigation_history(&mut self, id: ViewId) -> Option<NavigationHistoryRequest> {
        self._runtime.lifecycle.require_running().ok()?;
        if self.histories.busy(id) {
            return None;
        }
        let page = self
            .views
            .get(&id)
            .filter(|page| page.role == ViewRole::Page)?;
        let host = page.browser.host()?;
        let epoch = crate::site_data::document_epoch(page.browser.identifier())?;
        let message_id = self.message_ids.next()?;
        let reply = Arc::new(Mutex::new(Reply {
            finished: false,
            result: None,
        }));
        let mut observer = HistoryObserver::new(reply.clone(), message_id);
        let registration = host.add_dev_tools_message_observer(Some(&mut observer))?;
        let lease = Rc::new(RefCell::new(Lease {
            registration: Some(registration),
            reply,
            deadline: Instant::now() + TIMEOUT,
            document: Some((page.browser.identifier(), epoch)),
            _context: Some(page.context.clone()),
            _runtime: Some(self._runtime.clone()),
        }));
        self.histories
            .requests
            .entry(id)
            .or_default()
            .push(Rc::downgrade(&lease));
        if host.execute_dev_tools_method(
            message_id,
            Some(&"Page.getNavigationHistory".into()),
            None,
        ) != message_id
        {
            return None;
        }
        Some(NavigationHistoryRequest {
            lease,
            completed: None,
            consumed: false,
        })
    }
    /// Refuse stale, foreign and adapter-created snapshots rather than
    /// reloading their URLs or navigating another window's page.
    pub fn navigate_history_entry(
        &mut self,
        id: ViewId,
        history: &NavigationHistory,
        entry: i32,
    ) -> bool {
        if self._runtime.lifecycle.require_running().is_err() {
            return false;
        }
        let Some(page) = self
            .views
            .get(&id)
            .filter(|page| page.role == ViewRole::Page)
        else {
            return false;
        };
        let browser = page.browser.identifier();
        if !history.entries.iter().any(|item| item.id == entry)
            || history.source
                != crate::site_data::document_epoch(browser).map(|epoch| (browser, epoch))
            || history.source.is_none()
        {
            return false;
        }
        let Some(host) = page.browser.host() else {
            return false;
        };
        let Some(message) = self.message_ids.next() else {
            return false;
        };
        let Some(mut params) = dictionary_value_create() else {
            return false;
        };
        if params.set_int(Some(&"entryId".into()), entry) == 0 {
            return false;
        }
        self.site_data.close(id);
        self.script_evaluations.close(id);
        crate::permissions::cancel_browser(browser);
        host.execute_dev_tools_method(
            message,
            Some(&"Page.navigateToHistoryEntry".into()),
            Some(&mut params),
        ) == message
    }
}
wrap_dev_tools_message_observer! {
    struct HistoryObserver {reply:Arc<Mutex<Reply>>,message_id:i32}
    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(&self,_browser:Option<&mut Browser>,message_id:i32,success:i32,result:Option<&[u8]>) {
            if message_id!=self.message_id {return;}
            let mut reply=self.reply.lock().unwrap();if reply.finished {return;}
            reply.finished=true;
            reply.result=Some(if success==0 {Err(NavigationHistoryError::Unavailable)}else if let Some(bytes)=result {
                if bytes.len()>LIMIT {Err(NavigationHistoryError::TooLarge)}else{Ok(bytes.to_vec())}
            }else{Err(NavigationHistoryError::InvalidReply)});
        }
        fn on_dev_tools_agent_detached(&self,_browser:Option<&mut Browser>) {
            let mut reply=self.reply.lock().unwrap();
            if !reply.finished {reply.finished=true;reply.result=Some(Err(NavigationHistoryError::Cancelled));}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_order_and_ids_survive_directional_selection() {
        let history = decode(br#"{"currentIndex":2,"entries":[{"id":11,"title":"One","url":"https://example.com/1"},{"id":22,"title":"Two","url":"https://example.com/2"},{"id":33,"title":"Three","url":"https://example.com/3"},{"id":44,"title":"Four","url":"https://example.com/4"}]}"#.to_vec()).unwrap();
        assert_eq!(
            history
                .directional_entries(false)
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![22, 11]
        );
        assert_eq!(
            history
                .directional_entries(true)
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![44]
        );
        let mut request = NavigationHistoryRequest::completed(Ok(history.clone()));
        assert_eq!(request.poll(), Some(Ok(history)));
        assert_eq!(request.poll(), None);
    }
    #[test]
    fn invalid_or_unbounded_histories_are_rejected() {
        for bytes in [br#"{"currentIndex":0,"entries":[]}"#.as_slice(), br#"{"currentIndex":1,"entries":[{"id":1,"title":"a","url":"a"}]}"#, br#"{"currentIndex":0,"entries":[{"id":1,"title":"a","url":"a"},{"id":1,"title":"b","url":"b"}]}"#] {
            assert_eq!(decode(bytes.to_vec()), Err(NavigationHistoryError::InvalidReply));
        }
        assert_eq!(
            decode(vec![b' '; LIMIT + 1]),
            Err(NavigationHistoryError::TooLarge)
        );
    }
    #[test]
    fn logical_closure_cancels_only_its_source_and_consumes_once() {
        fn pending() -> NavigationHistoryRequest {
            let mut request =
                NavigationHistoryRequest::completed(Err(NavigationHistoryError::Unavailable));
            request.completed = None;
            request.lease.borrow_mut().reply.lock().unwrap().finished = false;
            request
        }
        let mut a = pending();
        let mut b = pending();
        let mut histories = Histories::default();
        histories
            .requests
            .insert(ViewId(1), vec![Rc::downgrade(&a.lease)]);
        histories
            .requests
            .insert(ViewId(2), vec![Rc::downgrade(&b.lease)]);
        histories.close(ViewId(1));
        assert_eq!(a.poll(), Some(Err(NavigationHistoryError::Cancelled)));
        assert_eq!(a.poll(), None);
        assert!(b.poll().is_none());
        assert!(histories.busy(ViewId(2)));
        b.lease.borrow_mut().deadline = Instant::now();
        assert_eq!(b.poll(), Some(Err(NavigationHistoryError::TimedOut)));
        assert!(!histories.busy(ViewId(2)));
    }
}
