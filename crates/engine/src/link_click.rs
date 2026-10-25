//! Modifier metadata never opens a window itself: Chromium must subsequently
//! accept the site's click and request the corresponding user-gesture popup.
use cef::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    time::{Duration, Instant},
};
pub(crate) const MESSAGE: &str = "Echo.LinkClick.v1";
#[derive(Clone)]
struct Observer {
    context: V8Context,
}
thread_local! {
    static OBSERVERS: RefCell<HashMap<(i32, String), Observer>> = RefCell::default();
    static CLICKS: RefCell<HashMap<i32, Click>> = RefCell::default();
}
struct Click {
    frame: String,
    document: String,
    target: String,
    epoch: u64,
    at: Instant,
}
impl Click {
    fn matches(
        &self,
        frame: &str,
        document: &str,
        target: &str,
        epoch: u64,
        age: Duration,
    ) -> bool {
        self.frame == frame
            && self.document == document
            && self.target == target
            && self.epoch == epoch
            && age <= Duration::from_millis(1500)
    }
}
fn web_url(value: &str) -> bool {
    value.len() <= 8192
        && url::Url::parse(value).is_ok_and(|u| {
            matches!(u.scheme(), "http" | "https")
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
        })
}
wrap_v8_handler! {
    struct Report { browser: i32, frame: String }
    impl V8Handler {
        fn execute(&self, _name: Option<&CefString>, _object: Option<&mut V8Value>, args: Option<&[Option<V8Value>]>, _retval: Option<&mut Option<V8Value>>, _exception: Option<&mut CefString>) -> i32 {
            let Some(args) = args.filter(|a| a.len()==1 && a[0].as_ref().is_some_and(|v|v.is_string()!=0)) else {return 0;};
            let target = CefString::from(&args[0].as_ref().unwrap().string_value()).to_string();
            if !target.is_empty() && !web_url(&target) {return 0;}
            let Some(context) = v8_context_get_current_context() else {return 0;};
            let (Some(browser), Some(frame)) = (context.browser(), context.frame()) else {return 0;};
            if browser.identifier()!=self.browser || CefString::from(&frame.identifier()).to_string()!=self.frame {return 0;}
            if !OBSERVERS.with_borrow(|s|s.get(&(self.browser,self.frame.clone())).is_some_and(|o|o.context.is_same(Some(&mut context.clone()))!=0)) {return 0;}
            if let Some(mut message) = process_message_create(Some(&CefString::from(MESSAGE))) {
                if let Some(args)=message.argument_list() {
                    args.set_string(0,Some(&CefString::from(CefString::from(&frame.url()).to_string().as_str())));
                    args.set_string(1,Some(&CefString::from(target.as_str())));
                    frame.send_process_message(ProcessId::BROWSER,Some(&mut message));
                }
            }
            1
        }
    }
}
pub(crate) fn created(browser: &Browser, frame: &Frame, context: &V8Context) {
    let key = (
        browser.identifier(),
        CefString::from(&frame.identifier()).to_string(),
    );
    let mut handler = Report::new(key.0, key.1.clone());
    let Some(callback) =
        v8_value_create_function(Some(&CefString::from("linkClick")), Some(&mut handler))
    else {
        return;
    };
    OBSERVERS.with_borrow_mut(|s| {
        s.insert(
            key,
            Observer {
                context: context.clone(),
            },
        )
    });
    let (mut factory, mut exception) = (None, None);
    context.eval(
        Some(&CefString::from(SOURCE)),
        Some(&CefString::from("echo-link-click")),
        1,
        Some(&mut factory),
        Some(&mut exception),
    );
    if let Some(factory) = factory {
        factory.execute_function(None, Some(&[Some(callback)]));
    }
}
pub(crate) fn release(browser: &Browser, frame: &Frame, context: &V8Context) {
    let key = (
        browser.identifier(),
        CefString::from(&frame.identifier()).to_string(),
    );
    OBSERVERS.with_borrow_mut(|s| {
        if s.get(&key)
            .is_some_and(|o| o.context.is_same(Some(&mut context.clone())) != 0)
        {
            s.remove(&key);
        }
    });
}
pub(crate) fn destroyed(browser: Option<&Browser>) {
    if let Some(browser) = browser {
        OBSERVERS.with_borrow_mut(|s| s.retain(|(id, _), _| *id != browser.identifier()));
    }
}
pub(crate) fn forget(browser: i32) {
    CLICKS.with_borrow_mut(|s| s.remove(&browser));
}
pub(crate) fn receive(
    browser: Option<&Browser>,
    frame: Option<&Frame>,
    source: ProcessId,
    message: Option<&ProcessMessage>,
) -> bool {
    let Some(message) = message.filter(|m| {
        source == ProcessId::RENDERER && CefString::from(&m.name()).to_string() == MESSAGE
    }) else {
        return false;
    };
    let (Some(browser), Some(frame)) = (browser, frame) else {
        return true;
    };
    forget(browser.identifier());
    let Some(args) = message
        .argument_list()
        .filter(|a| a.size() == 2 && (0..2).all(|i| a.get_type(i) == ValueType::STRING))
    else {
        return true;
    };
    let document = CefString::from(&args.string(0)).to_string();
    let target = CefString::from(&args.string(1)).to_string();
    let Some(epoch) = crate::site_data::document_epoch(browser.identifier()) else {
        return true;
    };
    if !web_url(&document)
        || !web_url(&target)
        || document != CefString::from(&frame.url()).to_string()
    {
        return true;
    }
    CLICKS.with_borrow_mut(|s| {
        s.insert(
            browser.identifier(),
            Click {
                frame: CefString::from(&frame.identifier()).to_string(),
                document,
                target,
                epoch,
                at: Instant::now(),
            },
        )
    });
    true
}
/// Consume once, only at Chromium's real user-gesture navigation callback.
pub(crate) fn take(
    browser: &Browser,
    frame: &Frame,
    target: &str,
    gesture: bool,
    enabled: bool,
) -> Option<crate::RawEvent> {
    let click = CLICKS.with_borrow_mut(|s| s.remove(&browser.identifier()))?;
    let epoch = crate::site_data::document_epoch(browser.identifier())?;
    if !gesture
        || !click.matches(
            &CefString::from(&frame.identifier()).to_string(),
            &CefString::from(&frame.url()).to_string(),
            target,
            epoch,
            click.at.elapsed(),
        )
    {
        return None;
    }
    let page_url = CefString::from(&browser.main_frame()?.url()).to_string();
    Some(crate::RawEvent::Context(crate::page_context::Request {
        page_url,
        document_epoch: epoch,
        action: crate::page_context::Action::Open {
            url: target.into(),
            target: if enabled { crate::page_context::OpenTarget::Little } else { crate::page_context::OpenTarget::Split },
        },
    }))
}
// Private closure, trusted native input only. Never cancel DOM/default behavior.
// pointerdown gives IPC a head start; click refreshes a long held press. A later
// unrelated input clears metadata, so it cannot reroute a different link.
const SOURCE: &str = r#"(report => {
 const command = /Mac/.test(navigator.platform) ? 'metaKey' : 'ctrlKey';
 const click = e => {
  if(!e.isTrusted)return;
  const a=e.composedPath().find(n=>n?.localName==='a'&&n.href);
  report(e.button===0&&e[command]&&e.altKey&&!e.shiftKey&&a ? a.href : '');
 };
 for(const event of ['pointerdown','click'])window.addEventListener(event,click,true);
 window.addEventListener('keydown',e=>{if(e.isTrusted&&!['Alt','Meta','Control','Shift'].includes(e.key))report('');},true);
 for(const event of ['dragstart','blur'])window.addEventListener(event,e=>{if(e.isTrusted)report('');},true);
})"#;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn link_metadata_is_one_document_frame_target_and_short_lived() {
        let c = Click {
            frame: "frame1".into(),
            document: "https://source.test".into(),
            target: "https://target.test".into(),
            epoch: 4,
            at: Instant::now(),
        };
        assert!(c.matches(
            "frame1",
            "https://source.test",
            "https://target.test",
            4,
            Duration::from_millis(1500)
        ));
        for (f, d, t, e, age) in [
            ("frame2", "https://source.test", "https://target.test", 4, 0),
            ("frame1", "https://other.test", "https://target.test", 4, 0),
            ("frame1", "https://source.test", "https://other.test", 4, 0),
            ("frame1", "https://source.test", "https://target.test", 5, 0),
            (
                "frame1",
                "https://source.test",
                "https://target.test",
                4,
                1501,
            ),
        ] {
            assert!(!c.matches(f, d, t, e, Duration::from_millis(age)));
        }
        for url in [
            "javascript:alert(1)",
            "file:///tmp/a",
            "https://user:secret@test/",
        ] {
            assert!(!web_url(url));
        }
    }
}
