//! Local DevTools evaluation for explicit scripting commands. No remote port,
//! frontend, global context, or renderer IPC authority is exposed to clients.
use crate::{devtools::MessageIds, site_data::document_epoch, ViewId};
use cef::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
type Outcome = Result<String, String>;
#[derive(Clone)]
pub struct ScriptTicket(Arc<Mutex<Option<Outcome>>>);
impl ScriptTicket {
    pub fn completed(result: Outcome) -> Self {
        Self(Arc::new(Mutex::new(Some(result))))
    }
    pub fn take(&self) -> Option<Outcome> {
        self.0.lock().unwrap().take()
    }
}
struct Pending {
    view: ViewId,
    browser: i32,
    epoch: u64,
    url: String,
    deadline: Instant,
    ticket: ScriptTicket,
    _registration: Option<Registration>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        let mut out = self.ticket.0.lock().unwrap();
        if out.is_none() {
            *out = Some(Err("The scripted document was closed or replaced.".into()));
        }
    }
}
type Replies = Arc<Mutex<Vec<(i32, Outcome)>>>;
#[derive(Default)]
pub(crate) struct Evaluations {
    pending: HashMap<i32, Pending>,
    replies: Replies,
}
impl Evaluations {
    pub fn request(
        &mut self,
        view: ViewId,
        browser: &Browser,
        ids: &MessageIds,
        expected_url: &str,
        source: &str,
    ) -> Result<ScriptTicket, String> {
        if source.len() > 1024 * 1024 {
            return Err("JavaScript source exceeds 1 MiB.".into());
        }
        if self.pending.len() >= 64 {
            return Err("Too many pending script evaluations.".into());
        }
        if browser
            .main_frame()
            .map(|f| CefString::from(&f.url()).to_string())
            .as_deref()
            != Some(expected_url)
        {
            return Err("The scripted URL is not the current document.".into());
        }
        let host = browser.host().ok_or("The tab has no live browser host.")?;
        let message = ids.next().ok_or("Script request IDs exhausted.")?;
        let epoch = document_epoch(browser.identifier()).ok_or("Document generation exhausted.")?;
        let ticket = ScriptTicket(Arc::new(Mutex::new(None)));
        let mut observer = EvaluationObserver::new(self.replies.clone(), message);
        let registration = host
            .add_dev_tools_message_observer(Some(&mut observer))
            .ok_or("Cannot observe script result.")?;
        self.pending.insert(
            message,
            Pending {
                view,
                browser: browser.identifier(),
                epoch,
                url: expected_url.into(),
                deadline: Instant::now() + Duration::from_secs(25),
                ticket: ticket.clone(),
                _registration: Some(registration),
            },
        );
        let expression = format!(
            "String((0,eval)({}))",
            serde_json::to_string(source).map_err(|e| e.to_string())?
        );
        let mut params = dictionary_value_create().ok_or("Cannot allocate script arguments.")?;
        params.set_string(
            Some(&"expression".into()),
            Some(&expression.as_str().into()),
        );
        params.set_bool(Some(&"returnByValue".into()), 1);
        if host.execute_dev_tools_method(
            message,
            Some(&"Runtime.evaluate".into()),
            Some(&mut params),
        ) != message
        {
            self.pending.remove(&message);
            return Err("Chromium rejected script evaluation.".into());
        }
        Ok(ticket)
    }
    pub fn close(&mut self, view: ViewId) {
        self.pending.retain(|_, p| p.view != view);
    }
    pub fn poll(&mut self, mut valid: impl FnMut(ViewId, i32, &str) -> bool) {
        let replies = std::mem::take(&mut *self.replies.lock().unwrap());
        for (id, result) in replies {
            if let Some(p) = self.pending.remove(&id) {
                let result = if document_epoch(p.browser) != Some(p.epoch)
                    || !valid(p.view, p.browser, &p.url)
                {
                    Err("The scripted document navigated before evaluation completed.".into())
                } else {
                    result
                };
                *p.ticket.0.lock().unwrap() = Some(result);
            }
        }
        self.pending.retain(|_, p| {
            if Instant::now() >= p.deadline {
                *p.ticket.0.lock().unwrap() = Some(Err("JavaScript evaluation timed out.".into()));
                false
            } else {
                document_epoch(p.browser) == Some(p.epoch) && valid(p.view, p.browser, &p.url)
            }
        });
    }
}
fn result(success: bool, bytes: &[u8]) -> Outcome {
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("JavaScript result exceeds 2 MiB.".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid JavaScript reply: {e}"))?;
    if !success {
        return Err(value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Script evaluation failed.")
            .into());
    }
    if let Some(exception) = value.get("exceptionDetails") {
        return Err(exception
            .pointer("/exception/description")
            .or_else(|| exception.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("JavaScript threw an exception.")
            .into());
    }
    value
        .pointer("/result/value")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "JavaScript did not return text.".into())
}
wrap_dev_tools_message_observer! {
 struct EvaluationObserver{replies:Replies,message:i32}
 impl DevToolsMessageObserver{
  fn on_dev_tools_method_result(&self,_browser:Option<&mut Browser>,message:i32,success:i32,data:Option<&[u8]>){if message==self.message{self.replies.lock().unwrap().push((message,result(success!=0,data.unwrap_or(b"{}"))));}}
  fn on_dev_tools_agent_detached(&self,_browser:Option<&mut Browser>){self.replies.lock().unwrap().push((self.message,Err("The scripting debugger detached.".into())));}
 }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn values_and_javascript_exceptions_are_distinguished() {
        assert_eq!(
            result(true, br#"{"result":{"type":"string","value":"2"}}"#),
            Ok("2".into())
        );
        assert_eq!(result(true,br#"{"exceptionDetails":{"text":"Uncaught","exception":{"description":"Error: probe"}}}"#),Err("Error: probe".into()));
        assert!(result(false, br#"{"message":"denied"}"#).is_err());
    }
    #[test]
    fn dictionary_text_results_preserve_javascript_string_coercion_output() {
        // Runtime evaluates String(eval(source)); Arc.sdef declares text, not
        // an AppleEvent number/list/record. Do not reinterpret numeric-looking text.
        for text in [
            "undefined",
            "null",
            "true",
            "false",
            "42",
            "NaN",
            "Infinity",
            "1,2",
            "[object Object]",
            "Symbol(x)",
            "",
            "💜",
        ] {
            let reply = serde_json::json!({"result":{"type":"string", "value":text}});
            assert_eq!(
                result(true, &serde_json::to_vec(&reply).unwrap()),
                Ok(text.into())
            );
        }
        assert!(result(true, br#"{"result":{"type":"number","value":42}}"#).is_err());
    }
    #[test]
    fn dropping_an_owner_completes_its_ticket_and_never_another() {
        let first = ScriptTicket(Arc::new(Mutex::new(None)));
        let second = ScriptTicket(Arc::new(Mutex::new(None)));
        let p = Pending {
            view: ViewId::from_raw(1),
            browser: 1,
            epoch: 0,
            url: "https://first.example/".into(),
            deadline: Instant::now(),
            ticket: first.clone(),
            _registration: None,
        };
        drop(p);
        assert!(first.take().unwrap().is_err());
        assert!(second.take().is_none());
    }
}
#[cfg(test)]
mod lifetime_tests {
    use super::*;
    #[test]
    fn navigation_and_late_replies_cannot_settle_a_replacement_request() {
        let browser = 9_731_621;
        crate::site_data::destroyed(browser);
        let old = ScriptTicket(Arc::new(Mutex::new(None)));
        let mut evaluations = Evaluations::default();
        evaluations.pending.insert(
            1,
            Pending {
                view: ViewId::from_raw(1),
                browser,
                epoch: 0,
                url: "https://first.example/".into(),
                deadline: Instant::now() + Duration::from_secs(5),
                ticket: old.clone(),
                _registration: None,
            },
        );
        crate::site_data::navigated(browser);
        evaluations.poll(|_, _, _| true);
        assert!(old.take().unwrap().is_err());
        let new = ScriptTicket(Arc::new(Mutex::new(None)));
        evaluations.pending.insert(
            2,
            Pending {
                view: ViewId::from_raw(1),
                browser,
                epoch: 1,
                url: "https://first.example/".into(),
                deadline: Instant::now() + Duration::from_secs(5),
                ticket: new.clone(),
                _registration: None,
            },
        );
        evaluations
            .replies
            .lock()
            .unwrap()
            .push((1, Ok("stale".into())));
        evaluations.poll(|_, _, _| true);
        assert!(new.take().is_none());
        evaluations
            .replies
            .lock()
            .unwrap()
            .push((2, Ok("current".into())));
        evaluations.poll(|_, _, _| true);
        assert_eq!(new.take(), Some(Ok("current".into())));
        crate::site_data::destroyed(browser);
    }
}
