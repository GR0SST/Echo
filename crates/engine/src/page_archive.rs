//! Retained-document MHTML capture over CEF's local DevTools transport.
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
const LIMIT: usize = 128 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageArchiveError {
    Unavailable,
    Cancelled,
    TimedOut,
    InvalidReply,
    TooLarge,
}
impl std::fmt::Display for PageArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "The page could not be saved.",
            Self::Cancelled => "The page is no longer available.",
            Self::TimedOut => "The page export timed out.",
            Self::InvalidReply => "The page archive could not be read.",
            Self::TooLarge => "The page archive is too large.",
        })
    }
}
struct Reply {
    finished: bool,
    result: Option<Result<Vec<u8>, PageArchiveError>>,
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
    fn cancel(&mut self) {
        self.registration.take();
        let mut state = self.reply.lock().unwrap();
        state.finished = true;
        state.result = Some(Err(PageArchiveError::Cancelled));
    }
}

/// Main-thread, single-consumer export. Dropping it unregisters the native
/// observer. Logical page closure also invalidates retained outstanding leases.
pub struct PageArchiveRequest {
    lease: Rc<RefCell<Lease>>,
    decoded: bool,
    consumed: bool,
}
impl PageArchiveRequest {
    /// Engine adapters may supply an already captured archive without CEF.
    pub fn completed(result: Result<Vec<u8>, PageArchiveError>) -> Self {
        Self {
            lease: Rc::new(RefCell::new(Lease {
                registration: None,
                reply: Arc::new(Mutex::new(Reply {
                    finished: true,
                    result: Some(result),
                })),
                deadline: Instant::now() + TIMEOUT,
                document: None,
                _context: None,
                _runtime: None,
            })),
            decoded: true,
            consumed: false,
        }
    }
    /// `None` is pending. A result is delivered once, without pumping CEF.
    pub fn poll(&mut self) -> Option<Result<Vec<u8>, PageArchiveError>> {
        if self.consumed {
            return None;
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
                reply.result = Some(Err(PageArchiveError::TimedOut));
            }
            reply.result.take()
        }?;
        lease.registration.take();
        self.consumed = true;
        Some(if self.decoded {
            result
        } else {
            result.and_then(decode)
        })
    }
}
fn decode(bytes: Vec<u8>) -> Result<Vec<u8>, PageArchiveError> {
    if bytes.len() > LIMIT {
        return Err(PageArchiveError::TooLarge);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| PageArchiveError::InvalidReply)?;
    let data = value
        .get("data")
        .and_then(serde_json::Value::as_str)
        .ok_or(PageArchiveError::InvalidReply)?;
    if data.len() > LIMIT {
        return Err(PageArchiveError::TooLarge);
    }
    if !data.contains("MIME-Version:") || !data.contains("multipart/related") {
        return Err(PageArchiveError::InvalidReply);
    }
    Ok(data.as_bytes().to_vec())
}

#[derive(Default)]
pub(crate) struct Archives {
    requests: HashMap<ViewId, Vec<Weak<RefCell<Lease>>>>,
}
impl Archives {
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
    pub fn page_document_epoch(&self, id: ViewId) -> Option<u64> {
        let page = self
            .views
            .get(&id)
            .filter(|page| page.role == ViewRole::Page)?;
        crate::site_data::document_epoch(page.browser.identifier())
    }
    pub fn request_page_archive(&mut self, id: ViewId) -> Option<PageArchiveRequest> {
        self._runtime.lifecycle.require_running().ok()?;
        if self.archives.busy(id) {
            return None;
        }
        let page = self
            .views
            .get(&id)
            .filter(|page| page.role == ViewRole::Page)?;
        let host = page.browser.host()?;
        let epoch = crate::site_data::document_epoch(page.browser.identifier())?;
        let message_id = self.message_ids.next()?;
        let mut params = dictionary_value_create()?;
        if params.set_string(Some(&"format".into()), Some(&"mhtml".into())) == 0 {
            return None;
        }
        let reply = Arc::new(Mutex::new(Reply {
            finished: false,
            result: None,
        }));
        let mut observer = ArchiveObserver::new(reply.clone(), message_id);
        let registration = host.add_dev_tools_message_observer(Some(&mut observer))?;
        let lease = Rc::new(RefCell::new(Lease {
            registration: Some(registration),
            reply,
            deadline: Instant::now() + TIMEOUT,
            document: Some((page.browser.identifier(), epoch)),
            _context: Some(page.context.clone()),
            _runtime: Some(self._runtime.clone()),
        }));
        self.archives
            .requests
            .entry(id)
            .or_default()
            .push(Rc::downgrade(&lease));
        if host.execute_dev_tools_method(
            message_id,
            Some(&"Page.captureSnapshot".into()),
            Some(&mut params),
        ) != message_id
        {
            return None;
        }
        Some(PageArchiveRequest {
            lease,
            decoded: false,
            consumed: false,
        })
    }
}
wrap_dev_tools_message_observer! {
    struct ArchiveObserver {reply:Arc<Mutex<Reply>>,message_id:i32}
    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(&self,_browser:Option<&mut Browser>,message_id:i32,success:i32,result:Option<&[u8]>) {
            if message_id!=self.message_id {return;}
            let mut reply=self.reply.lock().unwrap();if reply.finished {return;}
            reply.finished=true;
            reply.result=Some(if success==0 {Err(PageArchiveError::Unavailable)}else if let Some(bytes)=result {
                if bytes.len()>LIMIT {Err(PageArchiveError::TooLarge)}else{Ok(bytes.to_vec())}
            }else{Err(PageArchiveError::InvalidReply)});
        }
        fn on_dev_tools_agent_detached(&self,_browser:Option<&mut Browser>) {
            let mut reply=self.reply.lock().unwrap();
            if !reply.finished {reply.finished=true;reply.result=Some(Err(PageArchiveError::Cancelled));}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(value: u64) -> ViewId {
        ViewId(value)
    }
    fn pending() -> PageArchiveRequest {
        let request = PageArchiveRequest::completed(Ok(Vec::new()));
        *request.lease.borrow().reply.lock().unwrap() = Reply {
            finished: false,
            result: None,
        };
        request
    }
    #[test]
    fn outstanding_requests_are_scoped_and_closure_cancels_only_the_source() {
        let mut a = pending();
        let mut b = pending();
        let mut archives = Archives::default();
        archives
            .requests
            .insert(id(1), vec![Rc::downgrade(&a.lease)]);
        archives
            .requests
            .insert(id(2), vec![Rc::downgrade(&b.lease)]);
        assert!(archives.busy(id(1)));
        assert!(a.poll().is_none());
        archives.close(id(1));
        assert_eq!(a.poll(), Some(Err(PageArchiveError::Cancelled)));
        assert!(b.poll().is_none());
        assert!(!archives.busy(id(1)));
        assert!(archives.busy(id(2)));
        drop(b);
        assert!(!archives.busy(id(2)));
    }
    #[test]
    fn replies_are_single_consumer_and_timeout_cannot_become_a_later_success() {
        let mut request = pending();
        request.lease.borrow_mut().deadline = Instant::now() - Duration::from_secs(1);
        assert_eq!(request.poll(), Some(Err(PageArchiveError::TimedOut)));
        assert!(request.poll().is_none());
        let mut done = PageArchiveRequest::completed(Ok(b"owned archive".to_vec()));
        assert_eq!(done.poll(), Some(Ok(b"owned archive".to_vec())));
        assert!(done.poll().is_none());
    }
    #[test]
    fn a_same_address_reload_invalidates_the_captured_document_epoch() {
        let browser = -88431;
        crate::site_data::destroyed(browser);
        let mut request = PageArchiveRequest::completed(Ok(b"old document".to_vec()));
        request.lease.borrow_mut().document = Some((browser, 0));
        crate::site_data::navigated(browser);
        assert_eq!(request.poll(), Some(Err(PageArchiveError::Cancelled)));
        assert!(request.poll().is_none());
        crate::site_data::destroyed(browser);
    }

    #[test]
    fn protocol_reply_is_not_a_remote_envelope_or_arbitrary_page_text() {
        let mime = "MIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=x\r\n";
        assert_eq!(
            decode(serde_json::to_vec(&serde_json::json!({"data":mime})).unwrap()),
            Ok(mime.as_bytes().to_vec())
        );
        for value in [
            serde_json::json!({"result":{"data":mime}}),
            serde_json::json!({"data":"not an archive"}),
            serde_json::json!({"data":42}),
        ] {
            assert_eq!(
                decode(serde_json::to_vec(&value).unwrap()),
                Err(PageArchiveError::InvalidReply)
            );
        }
    }
}
