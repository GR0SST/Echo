//! A renderer-only geometry bridge. Policy, document reads, inference and UI
//! stay in Rust outside the engine. Native key events separately prove Shift.
use cef::*;
use serde::Deserialize;
use std::{cell::RefCell, collections::HashMap};
pub(crate) const CONTROL: &str = "Echo.LinkPreview.Control.v1";
pub(crate) const REPORT: &str = "Echo.LinkPreview.Hover.v1";
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkHover {
    pub active: bool,
    #[serde(default)]
    pub dismiss: bool,
    pub shift: bool,
    pub url: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
}
impl LinkHover {
    pub fn valid(&self) -> bool {
        let nums = [
            self.x,
            self.y,
            self.width,
            self.height,
            self.viewport_width,
            self.viewport_height,
        ];
        if nums.iter().any(|v| !v.is_finite() || v.abs() > 100_000.) {
            return false;
        }
        if !self.active {
            return self.url.is_empty();
        }
        if self.width <= 0.
            || self.height <= 0.
            || self.viewport_width <= 0.
            || self.viewport_height <= 0.
            || self.x + self.width < 0.
            || self.y + self.height < 0.
            || self.x > self.viewport_width
            || self.y > self.viewport_height
            || self.url.len() > 8192
        {
            return false;
        }
        url::Url::parse(&self.url).is_ok_and(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
    }
}
pub(crate) fn decode(json: &str) -> Option<LinkHover> {
    if json.len() > 16 * 1024 {
        return None;
    }
    let value: LinkHover = serde_json::from_str(json).ok()?;
    value.valid().then_some(value)
}
#[derive(Clone)]
struct Observer {
    context: V8Context,
    command: Option<V8Value>,
}
thread_local! {static OBSERVERS:RefCell<HashMap<(i32,String),Observer>>=RefCell::default();}
wrap_v8_handler! {
    struct Report {browser:i32,frame:String}
    impl V8Handler {
        fn execute(&self,_name:Option<&CefString>,_object:Option<&mut V8Value>,args:Option<&[Option<V8Value>]>,_retval:Option<&mut Option<V8Value>>,_exception:Option<&mut CefString>)->i32 {
            let Some(args)=args.filter(|a|a.len()==2&&a.iter().all(|v|v.as_ref().is_some_and(|v|v.is_string()!=0))) else{return 0;};
            let token=CefString::from(&args[0].as_ref().unwrap().string_value()).to_string();
            let json=CefString::from(&args[1].as_ref().unwrap().string_value()).to_string();
            if token.parse::<u64>().ok().is_none_or(|t|t==0)||decode(&json).is_none(){return 0;}
            let Some(context)=v8_context_get_current_context() else{return 0;};
            let (Some(browser),Some(frame))=(context.browser(),context.frame()) else{return 0;};
            if browser.identifier()!=self.browser||frame.is_main()==0||CefString::from(&frame.identifier()).to_string()!=self.frame{return 0;}
            if !OBSERVERS.with_borrow(|s|s.get(&(self.browser,self.frame.clone())).is_some_and(|o|o.context.is_same(Some(&mut context.clone()))!=0)){return 0;}
            if let Some(mut message)=process_message_create(Some(&CefString::from(REPORT))) {if let Some(args)=message.argument_list(){args.set_string(0,Some(&CefString::from(token.as_str())));args.set_string(1,Some(&CefString::from(json.as_str())));frame.send_process_message(ProcessId::BROWSER,Some(&mut message));}}
            1
        }
    }
}
pub(crate) fn created(browser: &Browser, frame: &Frame, context: &V8Context) {
    if frame.is_main() == 0 {
        return;
    }
    let key = (
        browser.identifier(),
        CefString::from(&frame.identifier()).to_string(),
    );
    let mut handler = Report::new(key.0, key.1.clone());
    let Some(callback) =
        v8_value_create_function(Some(&CefString::from("linkHover")), Some(&mut handler))
    else {
        return;
    };
    OBSERVERS.with_borrow_mut(|s| {
        s.insert(
            key.clone(),
            Observer {
                context: context.clone(),
                command: None,
            },
        )
    });
    let (mut factory, mut exception) = (None, None);
    context.eval(
        Some(&CefString::from(SOURCE)),
        Some(&CefString::from("echo-link-geometry")),
        1,
        Some(&mut factory),
        Some(&mut exception),
    );
    if let Some(command) = factory.and_then(|f| f.execute_function(None, Some(&[Some(callback)]))) {
        OBSERVERS.with_borrow_mut(|s| {
            s.insert(
                key,
                Observer {
                    context: context.clone(),
                    command: Some(command),
                },
            )
        });
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
    if let Some(b) = browser {
        OBSERVERS.with_borrow_mut(|s| s.retain(|(id, _), _| *id != b.identifier()));
    }
}
pub(crate) fn request(browser: &Browser, token: u64) {
    let Some(frame) = browser.main_frame() else {
        return;
    };
    if let Some(mut message) = process_message_create(Some(&CefString::from(CONTROL))) {
        if let Some(args) = message.argument_list() {
            args.set_string(0, Some(&CefString::from(token.to_string().as_str())));
            frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
        }
    }
}
pub(crate) fn receive(
    browser: Option<&Browser>,
    frame: Option<&Frame>,
    source: ProcessId,
    message: Option<&ProcessMessage>,
) -> bool {
    if source != ProcessId::BROWSER {
        return false;
    }
    let (Some(browser), Some(frame), Some(message)) = (browser, frame, message) else {
        return false;
    };
    if CefString::from(&message.name()).to_string() != CONTROL {
        return false;
    }
    if frame.is_main() == 0 {
        return true;
    }
    let Some(args) = message
        .argument_list()
        .filter(|a| a.size() == 1 && a.get_type(0) == ValueType::STRING)
    else {
        return true;
    };
    let token = CefString::from(&args.string(0)).to_string();
    if token.parse::<u64>().is_err() {
        return true;
    }
    let observer = OBSERVERS.with_borrow(|s| {
        s.get(&(
            browser.identifier(),
            CefString::from(&frame.identifier()).to_string(),
        ))
        .cloned()
    });
    if let Some(Observer {
        context,
        command: Some(command),
    }) = observer
    {
        if context.enter() != 0 {
            command.execute_function(
                None,
                Some(&[v8_value_create_string(Some(&CefString::from(
                    token.as_str(),
                )))]),
            );
            context.exit();
        }
    }
    true
}
// Only DOM observation lives here. No requests, model prompts or browser UI.
const SOURCE: &str = r#"(report => {
 let token='0', anchor=null, shift=false;
 const empty=(dismiss=false)=>({active:false,dismiss,shift:false,url:'',x:0,y:0,width:0,height:0,viewport_width:innerWidth,viewport_height:innerHeight});
 const send=()=>{
  if(token==='0')return;
  if(!anchor||!anchor.isConnected){report(token,JSON.stringify(empty()));return;}
  const r=anchor.getBoundingClientRect();
  report(token,JSON.stringify({active:true,shift,url:anchor.href,x:r.x,y:r.y,width:r.width,height:r.height,viewport_width:innerWidth,viewport_height:innerHeight}));
 };
 const over=e=>{if(!e.isTrusted)return;const a=e.target.closest?.('a[href]');const changed=a!==anchor||shift!==e.shiftKey;shift=e.shiftKey;if(changed){anchor=a;send();}};
 const out=e=>{if(!e.isTrusted)return;if(anchor&&!anchor.contains(e.relatedTarget)){anchor=null;send();}};
 const key=e=>{if(!e.isTrusted||e.key!=='Shift')return;shift=e.shiftKey;send();};
 const clear=e=>{anchor=null;if(token!=='0')report(token,JSON.stringify(empty(e.type==='pointerdown'||e.type==='scroll')));};
 const events=[['pointerover',over],['pointermove',over],['pointerout',out],['keydown',key],['keyup',key],['scroll',clear],['pointerdown',clear],['blur',clear]];
 return next=>{if(next===token)return;for(const [name,fn] of events)window.removeEventListener(name,fn,true);token=next;anchor=null;shift=false;if(token!=='0')for(const [name,fn] of events)window.addEventListener(name,fn,true);};
})"#;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn link_geometry_rejects_nonweb_credentials_and_invalid_rectangles() {
        let mut h = LinkHover {
            active: true,
            dismiss: false,
            shift: true,
            url: "https://example.test/a".into(),
            x: 10.,
            y: 20.,
            width: 100.,
            height: 20.,
            viewport_width: 800.,
            viewport_height: 600.,
        };
        assert!(h.valid());
        for url in [
            "file:///private",
            "javascript:alert(1)",
            "https://user:password@example.test",
        ] {
            h.url = url.into();
            assert!(!h.valid());
        }
        h.url = "https://example.test".into();
        h.width = f64::NAN;
        assert!(!h.valid());
    }
}
