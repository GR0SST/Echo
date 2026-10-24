//! Renderer media state and transport commands. All CEF/V8 types stay here.
use cef::*;
use std::{cell::RefCell, collections::HashMap};
use serde::{Deserialize, Serialize};
pub(crate) const STATE: &str = "Echo.MediaState.v1";
const COMMAND: &str = "Echo.MediaCommand.v1";
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaState {
    pub active: bool,
    pub playing: bool,
    pub video: bool,
    /// Chromium's standard video PiP session for this document.
    pub picture_in_picture: bool,
    pub title: String,
    pub artist: String,
    pub artwork: String,
    pub previous: bool,
    pub next: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MediaAction { Toggle, Previous, Next, SeekBackward, SeekForward, SetVolume(f64) }
impl MediaAction {
    fn name(self) -> &'static str { match self { Self::Toggle => "toggle", Self::Previous => "previoustrack", Self::Next => "nexttrack", Self::SeekBackward => "seekbackward", Self::SeekForward => "seekforward", Self::SetVolume(_) => "volume" } }
}
#[derive(Clone)]
struct RendererMedia { context: V8Context, command: Option<V8Value> }
thread_local! { static CONTEXTS: RefCell<HashMap<(i32,String),RendererMedia>> = RefCell::new(HashMap::new()); }
wrap_v8_handler! {
    struct Report { browser: i32, frame: String }
    impl V8Handler {
        fn execute(&self, _name: Option<&CefString>, _object: Option<&mut V8Value>, arguments: Option<&[Option<V8Value>]>, _retval: Option<&mut Option<V8Value>>, _exception: Option<&mut CefString>) -> i32 {
            let Some(value) = arguments.filter(|args| args.len()==1).and_then(|args| args[0].as_ref()).filter(|v| v.is_string()!=0) else { return 0; };
            let json = CefString::from(&value.string_value()).to_string();
            if json.len()>16384 { return 0; }
            let Some(context)=v8_context_get_current_context() else { return 0; };
            let (Some(browser),Some(frame))=(context.browser(),context.frame()) else { return 0; };
            if browser.identifier()!=self.browser || CefString::from(&frame.identifier()).to_string()!=self.frame { return 0; }
            if !CONTEXTS.with_borrow(|contexts|contexts.get(&(self.browser,self.frame.clone())).is_some_and(|entry|entry.context.is_same(Some(&mut context.clone()))!=0)) { return 0; }
            if let Some(mut message)=process_message_create(Some(&CefString::from(STATE))) {
                if let Some(args)=message.argument_list() { args.set_string(0,Some(&CefString::from(json.as_str()))); frame.send_process_message(ProcessId::BROWSER,Some(&mut message)); }
            }
            1
        }
    }
}
pub(crate) fn created(browser: &Browser, frame: &Frame, context: &V8Context) {
    if frame.is_main()==0 { return; }
    let key=(browser.identifier(),CefString::from(&frame.identifier()).to_string());
    let mut handler=Report::new(key.0,key.1.clone());
    let Some(callback)=v8_value_create_function(Some(&CefString::from("reportMedia")),Some(&mut handler)) else { return; };
    CONTEXTS.with_borrow_mut(|contexts|contexts.insert(key.clone(),RendererMedia { context: context.clone(), command: None }));
    let (mut factory,mut exception)=(None,None);
    context.eval(Some(&CefString::from(include_str!("media.js"))),Some(&CefString::from("echo-media")),1,Some(&mut factory),Some(&mut exception));
    if let Some(command)=factory.and_then(|factory|factory.execute_function(None,Some(&[Some(callback)]))) {
        CONTEXTS.with_borrow_mut(|contexts|contexts.insert(key,RendererMedia { context: context.clone(), command: Some(command) }));
    }
}
pub(crate) fn release(browser: &Browser, frame: &Frame, context: &V8Context) {
    let key=(browser.identifier(),CefString::from(&frame.identifier()).to_string());
    CONTEXTS.with_borrow_mut(|contexts| {
        if contexts.get(&key).is_some_and(|entry|entry.context.is_same(Some(&mut context.clone()))!=0) { contexts.remove(&key); }
    });
}
pub(crate) fn destroyed(browser: Option<&Browser>) {
    if let Some(browser)=browser { CONTEXTS.with_borrow_mut(|contexts|contexts.retain(|(id,_),_|*id!=browser.identifier())); }
}
pub(crate) fn receive(browser: Option<&Browser>, frame: Option<&Frame>, source: ProcessId, message: Option<&ProcessMessage>) -> bool {
    let (Some(browser),Some(frame),Some(message))=(browser,frame,message) else { return false; };
    if source!=ProcessId::BROWSER || CefString::from(&message.name()).to_string()!=COMMAND { return false; }
    let Some(args)=message.argument_list().filter(|args|(args.size()==1||args.size()==2)&&args.get_type(0)==ValueType::STRING) else { return true; };
    let name=CefString::from(&args.string(0)).to_string();
    if !["toggle","previoustrack","nexttrack","seekbackward","seekforward","volume"].contains(&name.as_str()) { return true; }
    let retained=CONTEXTS.with_borrow(|contexts| contexts.get(&(browser.identifier(),CefString::from(&frame.identifier()).to_string())).cloned());
    let volume=if name=="volume" {
        if args.size()!=2||args.get_type(1)!=ValueType::DOUBLE { return true; }
        let value=args.double(1);if !value.is_finite()||!(0. ..=1.).contains(&value) { return true; }Some(value)
    }else{if args.size()!=1{return true;}None};
    if let Some(entry)=retained {
        if let Some(command)=entry.command.filter(|_|frame.v8_context().is_some_and(|mut actual| entry.context.is_same(Some(&mut actual))!=0)) {
            if entry.context.enter()!=0 {
                let mut values=vec![v8_value_create_string(Some(&CefString::from(name.as_str())))];
                if let Some(value)=volume { values.push(v8_value_create_double(value)); }
                command.execute_function(None,Some(&values));
                entry.context.exit();
            }
        }
    }
    true
}
pub(crate) fn command(browser: &Browser, action: MediaAction) -> bool {
    let Some(frame)=browser.main_frame() else { return false; };
    let Some(mut message)=process_message_create(Some(&CefString::from(COMMAND))) else { return false; };
    let Some(args)=message.argument_list() else { return false; };
    args.set_string(0,Some(&CefString::from(action.name())));
    if let MediaAction::SetVolume(value)=action { if !value.is_finite()||!(0. ..=1.).contains(&value){return false;} args.set_double(1,value); }
    frame.send_process_message(ProcessId::RENDERER,Some(&mut message)); true
}
pub(crate) fn decode(json: &str) -> Option<MediaState> {
    if json.len()>16384 { return None; }
    let mut state:MediaState=serde_json::from_str(json).ok()?;
    state.title=state.title.chars().take(512).collect(); state.artist=state.artist.chars().take(512).collect();
    if !url::Url::parse(&state.artwork).is_ok_and(|url|matches!(url.scheme(),"https"|"http")) || state.artwork.len()>2048 { state.artwork.clear(); }
    if !state.position.is_finite() || !state.duration.is_finite() || state.position<0. || state.duration<0. || !(0. ..=1.).contains(&state.volume) { return None; }
    state.playing &= state.active;
    Some(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_reports_reject_nonfinite_ranges_and_local_artwork() {
        assert!(decode(r#"{"position":-1}"#).is_none());
        assert!(decode(r#"{"volume":2}"#).is_none());
        assert!(decode(&" ".repeat(16385)).is_none());
        let state=decode(r#"{"active":false,"playing":true,"artwork":"file:///private/image.png"}"#).unwrap();
        assert!(!state.playing);assert!(state.artwork.is_empty());
        let title="界".repeat(600);
        let state=decode(&serde_json::json!({"title":title,"active":true,"playing":true,"volume":0.5,"next":true}).to_string()).unwrap();
        assert_eq!(state.title.chars().count(),512);assert!(state.next);assert!(state.playing);
    }
}

/// Fixed native command; no page-supplied expression is evaluated. The protocol
/// grants activation only for this user/browser-requested media operation.
pub(crate) fn picture_in_picture(browser: &Browser, enabled: bool, ids: &crate::devtools::MessageIds) -> bool {
    let Some(host)=browser.host() else { return false; };
    let Some(id)=ids.next() else { return false; };
    let message=serde_json::json!({"id":id,"method":"Runtime.evaluate","params":{
        "expression": if enabled { ENTER_PIP } else { EXIT_PIP },
        "userGesture":true,"awaitPromise":true,"returnByValue":true
    }});
    host.send_dev_tools_message(Some(message.to_string().as_bytes()))!=0
}
const ENTER_PIP: &str = r#"(async()=>{
 if(document.pictureInPictureElement)return true;
 const video=[...document.querySelectorAll('video')].find(v=>!v.paused&&!v.ended&&v.readyState>=1&&v.videoWidth>0&&!v.disablePictureInPicture);
 if(!video||!document.pictureInPictureEnabled)return false;
 try{await video.requestPictureInPicture();return true;}catch{return false;}
})()"#;
const EXIT_PIP: &str = "document.pictureInPictureElement ? document.exitPictureInPicture().then(()=>true,()=>false) : false";
