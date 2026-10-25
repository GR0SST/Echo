//! Arc's inspected page/link/image menu, presented by CEF's native menu host.
//! No browser model or application borrow escapes into the native tracking loop.
use super::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    BackgroundTab,
    Window,
    Split,
    Little,
    Incognito,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Open { url: String, target: OpenTarget },
    Copy(String),
    CopyImage(String),
    Search(String),
    Inspect { x: i32, y: i32 },
    Print,
    ViewSource,
    Speak(String),
    StopSpeaking,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub page_url: String,
    pub document_epoch: u64,
    pub action: Action,
}

#[derive(Clone, Debug)]
pub struct Policy {
    pub search_name: String,
    pub can_split: bool,
    pub allow_little: bool,
    pub can_print: bool,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            search_name: "Google".into(),
            can_split: true,
            allow_little: true,
            can_print: false,
        }
    }
}
thread_local! { static MENUS: std::cell::RefCell<HashMap<i32,u64>> = Default::default();
static POLICIES: std::cell::RefCell<HashMap<i32, Policy>> = Default::default(); }
pub(super) fn set_policy(browser: i32, policy: Policy) {
    POLICIES.with(|entries| {
        entries.borrow_mut().insert(browser, policy);
    });
}
pub(super) fn cancel(browser: i32) {
    MENUS.with_borrow_mut(|menus| {
        menus.remove(&browser);
    });
}
fn begin(browser: i32) {
    cancel(browser);
    if let Some(epoch) = crate::site_data::document_epoch(browser) {
        MENUS.with_borrow_mut(|menus| {
            menus.insert(browser, epoch);
        });
    }
}
fn accept(browser: i32) -> Option<u64> {
    MENUS
        .with_borrow_mut(|menus| menus.remove(&browser))
        .filter(|epoch| crate::site_data::document_epoch(browser) == Some(*epoch))
}
pub(super) fn forget(browser: i32) {
    cancel(browser);
    POLICIES.with(|entries| {
        entries.borrow_mut().remove(&browser);
    });
}
fn policy(browser: i32) -> Policy {
    POLICIES.with(|entries| entries.borrow().get(&browser).cloned().unwrap_or_default())
}
const LINK_TAB: i32 = 26500;
const LINK_WINDOW: i32 = 26501;
const LINK_SPLIT: i32 = 26502;
const LINK_LITTLE: i32 = 26503;
const LINK_PRIVATE: i32 = 26504;
const COPY_LINK: i32 = 26505;
const IMAGE_TAB: i32 = 26506;
const COPY_IMAGE_URL: i32 = 26507;
const INSPECT: i32 = 26508;
const SEARCH: i32 = 26509;
const PRINT: i32 = 26510;
const SAVE_LINK: i32 = 26511;
const SAVE_IMAGE: i32 = 26512;
const COPY_IMAGE: i32 = 26513;
const CAPTURE_IMAGE: i32 = 26514;
const SAVE_PAGE: i32 = 26515;
const CAST: i32 = 26516;
const TRANSLATE: i32 = 26517;
const HIGHLIGHT: i32 = 26518;
const SPEECH: i32 = 26519;
const START_SPEAKING: i32 = 26520;
const STOP_SPEAKING: i32 = 26521;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    id: i32,
    label: String,
    enabled: bool,
}
fn row(id: i32, label: impl Into<String>, enabled: bool) -> Row {
    Row {
        id,
        label: label.into(),
        enabled,
    }
}
fn separator() -> Row {
    row(-1, "", false)
}
fn speech_rows(speaking: bool) -> [Row; 2] {
    [row(START_SPEAKING, "Start Speaking", true), row(STOP_SPEAKING, "Stop Speaking", speaking)]
}
#[derive(Default)]
struct Context {
    link: String,
    image: String,
    selection: String,
    editable: bool,
    back: bool,
    forward: bool,
    policy: Policy,
}
fn rows(context: &Context) -> Vec<Row> {
    let mut out = Vec::new();
    if !context.link.is_empty() {
        out.extend([
            row(LINK_TAB, "Open Link in New Tab", true),
            row(LINK_WINDOW, "Open Link in New Window", true),
            row(
                LINK_SPLIT,
                "Open Link in Split View",
                context.policy.can_split,
            ),
            row(
                LINK_LITTLE,
                "Open Link in Little Echo",
                context.policy.allow_little,
            ),
            row(LINK_PRIVATE, "Open Link in Incognito Window", true),
            separator(),
            row(SAVE_LINK, "Save Link As…", false),
            row(COPY_LINK, "Copy Link Address", true),
        ]);
    }
    if !context.image.is_empty() {
        if !out.is_empty() {
            out.push(separator());
        }
        out.extend([
            row(IMAGE_TAB, "Open Image in New Tab", true),
            row(SAVE_IMAGE, "Save Image As…", false),
            row(CAPTURE_IMAGE, "Capture Image…", false),
            row(COPY_IMAGE, "Copy Image", true),
            row(COPY_IMAGE_URL, "Copy Image Address", true),
        ]);
    }
    if !context.selection.is_empty() && !context.editable {
        if !out.is_empty() {
            out.push(separator());
        }
        let excerpt = &context.selection;
        out.extend([
            row(*MenuId::COPY.as_ref() as i32, "Copy", true),
            row(HIGHLIGHT, "Copy Link to Highlight", false),
            row(
                SEARCH,
                format!("Search {} for “{excerpt}”", context.policy.search_name),
                true,
            ),
            row(PRINT, "Print…", context.policy.can_print),
        ]);
    }
    if out.is_empty() && !context.editable {
        out.extend([
            row(*MenuId::BACK.as_ref() as i32, "Back", context.back),
            row(*MenuId::FORWARD.as_ref() as i32, "Forward", context.forward),
            row(*MenuId::RELOAD.as_ref() as i32, "Reload", true),
            separator(),
            row(SAVE_PAGE, "Save As…", false),
            row(PRINT, "Print…", context.policy.can_print),
            row(CAST, "Cast…", false),
            row(TRANSLATE, "Translate to English", false),
            separator(),
            row(
                *MenuId::VIEW_SOURCE.as_ref() as i32,
                "View Page Source",
                true,
            ),
        ]);
    }
    let blank = context.link.is_empty()
        && context.image.is_empty()
        && context.selection.is_empty()
        && !context.editable;
    if !out.is_empty() && !blank {
        out.push(separator());
    }
    out.push(row(INSPECT, "Inspect", true));
    out
}
fn request(command: i32, params: &ContextMenuParams, document_epoch: u64) -> Option<Request> {
    let link = CefString::from(&params.link_url()).to_string();
    let image = CefString::from(&params.source_url()).to_string();
    let action = match command {
        LINK_TAB | LINK_WINDOW | LINK_SPLIT | LINK_LITTLE | LINK_PRIVATE if !link.is_empty() => {
            Action::Open {
                url: link,
                target: match command {
                    LINK_TAB => OpenTarget::BackgroundTab,
                    LINK_WINDOW => OpenTarget::Window,
                    LINK_SPLIT => OpenTarget::Split,
                    LINK_LITTLE => OpenTarget::Little,
                    _ => OpenTarget::Incognito,
                },
            }
        }
        IMAGE_TAB if !image.is_empty() => Action::Open {
            url: image,
            target: OpenTarget::BackgroundTab,
        },
        COPY_IMAGE if !image.is_empty() => Action::CopyImage(image),
        COPY_LINK if !link.is_empty() => Action::Copy(link),
        COPY_IMAGE_URL if !image.is_empty() => Action::Copy(image),
        SEARCH => Action::Search(CefString::from(&params.selection_text()).to_string()),
        command if command == *MenuId::VIEW_SOURCE.as_ref() as i32 => Action::ViewSource,
        INSPECT => Action::Inspect {
            x: params.xcoord(),
            y: params.ycoord(),
        },
        PRINT => Action::Print,
        command if command == *MenuId::PRINT.as_ref() as i32 => Action::Print,
        START_SPEAKING => {
            let text = CefString::from(&params.selection_text()).to_string();
            if text.is_empty() || !crate::speech::available() { return None; }
            Action::Speak(text)
        }
        STOP_SPEAKING if crate::speech::available() => Action::StopSpeaking,
        _ => return None,
    };
    Some(Request {
        page_url: CefString::from(&params.page_url()).to_string(),
        document_epoch,
        action,
    })
}
wrap_context_menu_handler! {
    pub(super) struct Handler { events: EventQueue }
    impl ContextMenuHandler {
        fn on_before_context_menu(&self,browser:Option<&mut Browser>,_frame:Option<&mut Frame>,params:Option<&mut ContextMenuParams>,model:Option<&mut MenuModel>) {
            let (Some(browser),Some(params),Some(model))=(browser,params,model) else{return;};
            begin(browser.identifier());
            let editable=params.is_editable()!=0;
            if !editable {model.clear();} // Preserve editing/spelling while Arc's extended native editing menu remains incomplete.
            let context=Context{link:CefString::from(&params.link_url()).to_string(),image:if params.media_type()==ContextMenuMediaType::IMAGE {CefString::from(&params.source_url()).to_string()}else{String::new()},
                selection:CefString::from(&params.selection_text()).to_string(),editable,back:browser.can_go_back()!=0,forward:browser.can_go_forward()!=0,policy:policy(browser.identifier())};
            if editable && model.count()>0 {model.add_separator();}
            for row in rows(&context) {if row.id<0 {model.add_separator();}else{model.add_item(row.id,Some(&CefString::from(row.label.as_str())));model.set_enabled(row.id,i32::from(row.enabled));}}
            if !context.editable && !context.selection.is_empty() && crate::speech::available() {
                model.add_separator();
                if let Some(mut speech) = model.add_sub_menu(SPEECH, Some(&CefString::from("Speech"))) {
                    for row in speech_rows(crate::speech::is_speaking()) {
                        speech.add_item(row.id, Some(&CefString::from(row.label.as_str())));
                        speech.set_enabled(row.id, i32::from(row.enabled));
                    }
                }
            }
        }
        fn on_context_menu_command(&self,browser:Option<&mut Browser>,_frame:Option<&mut Frame>,params:Option<&mut ContextMenuParams>,command_id:i32,_event_flags:EventFlags)->i32 {
            let Some(browser)=browser else{return 1;};
            let Some(epoch) = accept(browser.identifier()) else {return 1;};
            let Some(params)=params else{return 1;};
            if let Some(request)=request(command_id,params,epoch) { push(&self.events,Some(browser),RawEvent::Context(request));return 1; }
            i32::from((26500..=26521).contains(&command_id))
        }
        fn on_context_menu_dismissed(&self, browser:Option<&mut Browser>, _frame:Option<&mut Frame>) {
            if let Some(browser)=browser {cancel(browser.identifier());}
        }

    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reloading_the_same_url_and_retiring_an_attachment_invalidate_native_menu_input() {
        let browser = 8721;
        begin(browser);
        crate::site_data::navigated(browser);
        assert!(accept(browser).is_none());
        begin(browser);
        cancel(browser);
        assert!(accept(browser).is_none());
        begin(browser);
        assert!(accept(browser).is_some());
        assert!(accept(browser).is_none());
        forget(browser);
        crate::site_data::destroyed(browser);
    }
}

pub(super) fn copy_image(
    events: EventQueue,
    view: &super::View,
    request: u64,
    page_url: &str,
    url: &str,
) -> bool {
    if view.role != ViewRole::Page
        || view
            .browser
            .main_frame()
            .is_none_or(|frame| CefString::from(&frame.url()).to_string() != page_url)
    {
        return false;
    }
    let Some(epoch) = crate::site_data::document_epoch(view.browser.identifier()) else {
        return false;
    };
    let Some(host) = view.browser.host() else {
        return false;
    };
    let mut callback = ImageCopy::new(
        events,
        view.browser.identifier(),
        std::rc::Rc::downgrade(&view.action_lifetime),
        epoch,
        request,
        page_url.to_owned(),
    );
    host.download_image(Some(&CefString::from(url)), 0, 8192, 0, Some(&mut callback));
    true
}
wrap_download_image_callback! {
    struct ImageCopy { events:EventQueue, browser:i32, lifetime:std::rc::Weak<()>, epoch:u64, request:u64, page_url:String }
    impl DownloadImageCallback {
        fn on_download_image_finished(&self,_image_url:Option<&CefString>,_http_status_code:i32,image:Option<&mut Image>) {
            if self.lifetime.upgrade().is_none() || crate::site_data::document_epoch(self.browser)!=Some(self.epoch) {return;}
            let Some(image)=image.filter(|image| image.is_empty()==0) else{return;};
            let(mut width,mut height)=(0,0);
            let Some(data)=image.as_png(1.,1,Some(&mut width),Some(&mut height)) else{return;};
            if width<=0 || height<=0 || width>8192 || height>8192 || data.size()>32*1024*1024 {return;}
            let mut png=vec![0;data.size()];if data.data(Some(&mut png),0)!=png.len() {return;}
            self.events.push((self.browser,RawEvent::ContextImage(self.request,self.page_url.clone(),png)));
        }
    }
}
