//! Origin-bound form reporting/fill without page globals, remote debugging, or
//! storage authority in the renderer. Secrets never appear in Debug output.
use cef::*;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    sync::{LazyLock, Mutex},
};
use zeroize::Zeroizing;

pub(crate) const ENABLED: &str = "echo.passwords.enabled";
const REGISTER: &str = "Echo.PasswordDocument.v1";
const REPORT: &str = "Echo.PasswordForm.v1";
const FILL: &str = "Echo.PasswordFill.v1";
const MAX_FIELD: usize = 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PasswordToken(u128);
impl PasswordToken {
    pub fn from_raw(value: u128) -> Option<Self> {
        (value != 0).then_some(Self(value))
    }
    fn parse(value: &str) -> Option<Self> {
        if value.len() > 39 {
            return None;
        }
        Self::from_raw(value.parse().ok()?)
    }
    fn fresh() -> Option<Self> {
        let mut bytes = [0; 16];
        SystemRandom::new().fill(&mut bytes).ok()?;
        Self::from_raw(u128::from_le_bytes(bytes))
    }
}
impl std::fmt::Debug for PasswordToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PasswordToken")
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct LoginCredential {
    username: Zeroizing<String>,
    password: Zeroizing<String>,
}
impl LoginCredential {
    pub fn new(username: &str, password: &str) -> Option<Self> {
        if username.len() > MAX_FIELD || password.is_empty() || password.len() > MAX_FIELD {
            return None;
        }
        Some(Self {
            username: Zeroizing::new(username.into()),
            password: Zeroizing::new(password.into()),
        })
    }
    pub fn username(&self) -> &str {
        &self.username
    }
    pub fn password(&self) -> &str {
        &self.password
    }
}
impl std::fmt::Debug for LoginCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LoginCredential([redacted])")
    }
}
#[derive(Clone, PartialEq)]
pub enum PasswordEvent {
    Document {
        token: PasswordToken,
        origin: String,
        has_password_form: bool,
    },
    Dismissed {
        token: PasswordToken,
        origin: String,
    },
    FrameDocument {
        token: PasswordToken,
        origin: String,
        has_password_form: bool,
        frame: String,
    },
    Submitted {
        token: PasswordToken,
        origin: String,
        credential: LoginCredential,
    },
    FillRequested {
        token: PasswordToken,
        origin: String,
        username_hint: Zeroizing<String>,
        anchor: Option<[f64; 4]>,
        field: u32,
    },
}
impl std::fmt::Debug for PasswordEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Dismissed { .. } | Self::Document { .. } | Self::FrameDocument { .. } => {
                "PasswordDocument"
            }
            Self::Submitted { .. } => "PasswordSubmitted([redacted])",
            Self::FillRequested { .. } => "PasswordFillRequested([redacted])",
        })
    }
}
#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Wire {
    #[serde(rename = "document")]
    Document { has_password_form: bool },
    #[serde(rename = "submitted")]
    Submitted {
        username: Zeroizing<String>,
        password: Zeroizing<String>,
        action_origin: String,
    },
    #[serde(rename = "dismiss")]
    Dismiss,
    #[serde(rename = "fill")]
    Fill {
        username_hint: Zeroizing<String>,
        #[serde(default)]
        anchor: Option<[f64; 4]>,
        #[serde(default)]
        field: u32,
    },
}
pub(crate) fn web_origin(url: &str) -> Option<String> {
    let url = url::Url::parse(url).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .then(|| url.origin().ascii_serialization())
}
fn decode(token: PasswordToken, origin: &str, json: &str) -> Option<PasswordEvent> {
    if json.len() > MAX_FIELD * 2 + 8192 {
        return None;
    }
    match serde_json::from_str::<Wire>(json).ok()? {
        Wire::Dismiss => Some(PasswordEvent::Dismissed {
            token,
            origin: origin.into(),
        }),
        Wire::Document { has_password_form } => Some(PasswordEvent::Document {
            token,
            origin: origin.into(),
            has_password_form,
        }),
        Wire::Fill {
            username_hint,
            anchor,
            field,
        } if username_hint.len() <= MAX_FIELD
            && anchor.is_none_or(|r| {
                r.iter().all(|v| v.is_finite() && v.abs() < 1e7) && r[2] > 0. && r[3] > 0.
            }) =>
        {
            Some(PasswordEvent::FillRequested {
                token,
                origin: origin.into(),
                username_hint,
                anchor,
                field,
            })
        }
        Wire::Submitted {
            username,
            password,
            action_origin,
        } if action_origin == origin => Some(PasswordEvent::Submitted {
            token,
            origin: origin.into(),
            credential: LoginCredential::new(&username, &password)?,
        }),
        _ => None,
    }
}
#[derive(Default)]
struct Known {
    browsers: HashMap<(i32, String), VecDeque<(PasswordToken, String)>>,
    selections: HashSet<i32>,
}
impl Known {
    fn register(&mut self, browser: i32, frame: &str, token: PasswordToken, origin: String) {
        if !self.browsers.contains_key(&(browser, frame.into()))
            && self
                .browsers
                .keys()
                .filter(|(id, _)| *id == browser)
                .count()
                >= 256
        {
            return;
        }
        let entries = self.browsers.entry((browser, frame.into())).or_default();
        if entries.iter().any(|(old, _)| *old == token) {
            return;
        }
        entries.push_back((token, origin));
        while entries.len() > 2 {
            entries.pop_front();
        }
    }
    fn origin(&self, browser: i32, frame: &str, token: PasswordToken) -> Option<&str> {
        self.browsers
            .get(&(browser, frame.into()))?
            .iter()
            .find(|(old, _)| *old == token)
            .map(|(_, o)| o.as_str())
    }
    fn current(&self, browser: i32, frame: &str, token: PasswordToken) -> bool {
        self.browsers
            .get(&(browser, frame.into()))
            .and_then(|v| v.back())
            .is_some_and(|(t, _)| *t == token)
    }
}
static KNOWN: LazyLock<Mutex<Known>> = LazyLock::new(|| Mutex::new(Known::default()));
pub(crate) fn closed(browser: i32) {
    if let Ok(mut known) = KNOWN.lock() {
        known.browsers.retain(|(id, _), _| *id != browser);
        known.selections.remove(&browser);
    }
}

pub(crate) fn set_selection(browser: i32, active: bool) {
    if let Ok(mut known) = KNOWN.lock() {
        if active {
            known.selections.insert(browser);
        } else {
            known.selections.remove(&browser);
        }
    }
}
pub(crate) fn selection_active(browser: i32) -> bool {
    KNOWN
        .lock()
        .ok()
        .is_some_and(|k| k.selections.contains(&browser))
}

// Browser-process endpoint: CEF supplies the sender browser/frame; the document
// registration supplies its source origin even if a successful login navigates.
pub(crate) fn receive(
    browser: Option<&Browser>,
    frame: Option<&Frame>,
    source: ProcessId,
    message: Option<&ProcessMessage>,
    events: &crate::EventQueue,
) -> bool {
    let Some(message) = message else {
        return false;
    };
    let name = CefString::from(&message.name()).to_string();
    if name != REGISTER && name != REPORT {
        return false;
    }
    let (Some(browser), Some(frame)) = (browser, frame) else {
        return true;
    };
    if source != ProcessId::RENDERER {
        return true;
    }
    if !browser
        .host()
        .and_then(|h| h.request_context())
        .is_some_and(|c| !CefString::from(&c.cache_path()).to_string().is_empty())
    {
        return true;
    }
    let Some(args) = message.argument_list().filter(|a| {
        a.size() == 2 && a.get_type(0) == ValueType::STRING && a.get_type(1) == ValueType::STRING
    }) else {
        return true;
    };
    let Some(token) = PasswordToken::parse(&CefString::from(&args.string(0)).to_string()) else {
        return true;
    };
    let text = Zeroizing::new(CefString::from(&args.string(1)).to_string());
    let Ok(mut known) = KNOWN.lock() else {
        return true;
    };
    let frame_id = CefString::from(&frame.identifier()).to_string();
    if name == REGISTER {
        if text.len() <= 8192
            && web_origin(&CefString::from(&frame.url()).to_string()).as_deref()
                == Some(text.as_str())
        {
            if frame.is_main() != 0 {
                known
                    .browsers
                    .retain(|(id, f), _| *id != browser.identifier() || *f == frame_id);
            }
            known.register(browser.identifier(), &frame_id, token, text.to_string());
            if !known.current(browser.identifier(), &frame_id, token) {
                return true;
            }
            // Until DOM discovery finishes, conservatively suppress save prompts.
            let event = if frame.is_main() != 0 {
                PasswordEvent::Document {
                    token,
                    origin: text.to_string(),
                    has_password_form: true,
                }
            } else {
                PasswordEvent::FrameDocument {
                    token,
                    origin: text.to_string(),
                    has_password_form: true,
                    frame: frame_id.clone(),
                }
            };
            events.push((browser.identifier(), crate::RawEvent::Password(event)));
        }
    } else if let Some(origin) = known.origin(browser.identifier(), &frame_id, token) {
        if let Some(mut event) = decode(token, origin, &text) {
            if frame.is_main() == 0 {
                if let PasswordEvent::Document {
                    token,
                    origin,
                    has_password_form,
                } = event
                {
                    event = PasswordEvent::FrameDocument {
                        token,
                        origin,
                        has_password_form,
                        frame: frame_id.clone(),
                    };
                }
            }
            let submitted = matches!(event, PasswordEvent::Submitted { .. });
            if submitted
                || (known.current(browser.identifier(), &frame_id, token)
                    && web_origin(&CefString::from(&frame.url()).to_string()).as_deref()
                        == Some(origin))
            {
                events.push((browser.identifier(), crate::RawEvent::Password(event)));
            }
        }
    }
    true
}

#[derive(Clone)]
struct ContextEntry {
    context: V8Context,
    token: PasswordToken,
    origin: String,
    fill: Option<V8Value>,
}
thread_local! {
    static ENABLE:RefCell<HashMap<i32,bool>>=RefCell::new(HashMap::new());
    static CONTEXTS:RefCell<HashMap<(i32,String),ContextEntry>>=RefCell::new(HashMap::new());
}
fn send(frame: &Frame, name: &str, token: PasswordToken, text: &str) {
    if let Some(mut message) = process_message_create(Some(&CefString::from(name))) {
        if let Some(args) = message.argument_list() {
            args.set_string(0, Some(&CefString::from(token.0.to_string().as_str())));
            args.set_string(1, Some(&CefString::from(text)));
            frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
        }
    }
}
wrap_v8_handler! {
    struct Report { browser:i32,frame:String,token:PasswordToken }
    impl V8Handler {
        fn execute(&self,_name:Option<&CefString>,_object:Option<&mut V8Value>,arguments:Option<&[Option<V8Value>]>,_retval:Option<&mut Option<V8Value>>,_exception:Option<&mut CefString>)->i32 {
            let Some(value)=arguments.filter(|a|a.len()==1).and_then(|a|a[0].as_ref()).filter(|v|v.is_string()!=0) else{return 0;};
            let Some(context)=v8_context_get_current_context() else{return 0;};
            let (Some(browser),Some(frame))=(context.browser(),context.frame()) else{return 0;};
            if browser.identifier()!=self.browser || CefString::from(&frame.identifier()).to_string()!=self.frame {return 0;}
            let valid=CONTEXTS.with_borrow(|c|c.get(&(self.browser,self.frame.clone())).is_some_and(|e|e.token==self.token&&e.context.is_same(Some(&mut context.clone()))!=0));
            if !valid{return 0;}
            let json=Zeroizing::new(CefString::from(&value.string_value()).to_string());
            if json.len()<=MAX_FIELD*2+8192 {send(&frame,REPORT,self.token,&json);}
            1
        }
    }
}
pub(crate) fn browser_created(browser: Option<&Browser>, extra: Option<&DictionaryValue>) {
    if let Some(browser) = browser {
        let enabled = extra.is_some_and(|e| {
            e.has_key(Some(&ENABLED.into())) != 0 && e.bool(Some(&ENABLED.into())) != 0
        });
        ENABLE.with_borrow_mut(|e| e.insert(browser.identifier(), enabled));
    }
}
pub(crate) fn destroyed(browser: Option<&Browser>) {
    if let Some(browser) = browser {
        ENABLE.with_borrow_mut(|e| e.remove(&browser.identifier()));
        CONTEXTS.with_borrow_mut(|e| e.retain(|(id, _), _| *id != browser.identifier()));
    }
}
pub(crate) fn created(browser: &Browser, frame: &Frame, context: &V8Context) {
    if !ENABLE.with_borrow(|e| e.get(&browser.identifier()) == Some(&true)) {
        return;
    }
    let Some(origin) = web_origin(&CefString::from(&frame.url()).to_string()) else {
        return;
    };
    let Some(token) = PasswordToken::fresh() else {
        return;
    };
    let key = (
        browser.identifier(),
        CefString::from(&frame.identifier()).to_string(),
    );
    CONTEXTS.with_borrow_mut(|e| {
        e.insert(
            key.clone(),
            ContextEntry {
                context: context.clone(),
                token,
                origin: origin.clone(),
                fill: None,
            },
        )
    });
    send(frame, REGISTER, token, &origin);
    let mut handler = Report::new(key.0, key.1.clone(), token);
    let Some(callback) =
        v8_value_create_function(Some(&"reportPasswordForm".into()), Some(&mut handler))
    else {
        return;
    };
    let (mut factory, mut exception) = (None, None);
    context.eval(
        Some(&CefString::from(include_str!("passwords.js"))),
        Some(&"echo-password-forms".into()),
        1,
        Some(&mut factory),
        Some(&mut exception),
    );
    if let Some(fill) = factory.and_then(|f| f.execute_function(None, Some(&[Some(callback)]))) {
        CONTEXTS.with_borrow_mut(|e| {
            if let Some(entry) = e.get_mut(&key).filter(|entry| {
                entry.token == token && entry.context.is_same(Some(&mut context.clone())) != 0
            }) {
                entry.fill = Some(fill);
            }
        });
    }
}
pub(crate) fn release(browser: &Browser, frame: &Frame, context: &V8Context) {
    let key = (
        browser.identifier(),
        CefString::from(&frame.identifier()).to_string(),
    );
    CONTEXTS.with_borrow_mut(|e| {
        if e.get(&key)
            .is_some_and(|e| e.context.is_same(Some(&mut context.clone())) != 0)
        {
            e.remove(&key);
        }
    });
}
pub(crate) fn renderer_receive(
    browser: Option<&Browser>,
    frame: Option<&Frame>,
    source: ProcessId,
    message: Option<&ProcessMessage>,
) -> bool {
    let (Some(browser), Some(frame), Some(message)) = (browser, frame, message) else {
        return false;
    };
    if source != ProcessId::BROWSER || CefString::from(&message.name()).to_string() != FILL {
        return false;
    }
    let Some(args) = message
        .argument_list()
        .filter(|a| a.size() == 6 && (0..6).all(|i| a.get_type(i) == ValueType::STRING))
    else {
        return true;
    };
    let Some(token) = PasswordToken::parse(&CefString::from(&args.string(0)).to_string()) else {
        return true;
    };
    let origin = CefString::from(&args.string(1)).to_string();
    let key = (
        browser.identifier(),
        CefString::from(&frame.identifier()).to_string(),
    );
    let entry = CONTEXTS.with_borrow(|entries| {
        entries
            .get(&key)
            .filter(|e| e.token == token && e.origin == origin)
            .cloned()
    });
    let Some(entry) = entry else {
        return true;
    };
    {
        if !frame
            .v8_context()
            .is_some_and(|mut c| entry.context.is_same(Some(&mut c)) != 0)
            || web_origin(&CefString::from(&frame.url()).to_string()).as_deref()
                != Some(origin.as_str())
        {
            return true;
        }
        let user = Zeroizing::new(CefString::from(&args.string(2)).to_string());
        let password = Zeroizing::new(CefString::from(&args.string(3)).to_string());
        if user.len() > MAX_FIELD || password.is_empty() || password.len() > MAX_FIELD {
            return true;
        }
        if let Some(fill) = &entry.fill {
            if entry.context.enter() != 0 {
                let result = fill.execute_function(
                    None,
                    Some(&[
                        v8_value_create_string(Some(&CefString::from(user.as_str()))),
                        v8_value_create_string(Some(&CefString::from(password.as_str()))),
                        v8_value_create_bool(i32::from(
                            CefString::from(&args.string(4)).to_string() == "1",
                        )),
                        v8_value_create_uint(
                            CefString::from(&args.string(5))
                                .to_string()
                                .parse::<u32>()
                                .unwrap_or(0),
                        ),
                    ]),
                );
                // Disposable native acceptance reports only success, never a
                // credential, origin, field value or document capability.
                #[cfg(debug_assertions)]
                if std::env::var("BROWSER_PASSWORD_STORE_FIXTURE").as_deref() == Ok("1")
                    && std::env::var_os("BROWSER_DATA_DIR").as_deref().map(std::path::Path::new)
                        .and_then(std::path::Path::file_name) == Some(std::ffi::OsStr::new("browser-drive-profile"))
                {
                    eprintln!("[password-fill-probe] applied={}", result.as_ref().is_some_and(|v| v.is_bool() != 0 && v.bool_value() != 0));
                }
                #[cfg(not(debug_assertions))]
                let _ = result;
                entry.context.exit();
            }
        }
    }
    true
}
pub(crate) fn fill(
    browser: &Browser,
    token: PasswordToken,
    origin: &str,
    credential: &LoginCredential,
    selected: bool,
    field: u32,
) -> bool {
    let frame_id = {
        let Ok(known) = KNOWN.lock() else {
            return false;
        };
        let Some(((_, frame), _)) = known.browsers.iter().find(|((id, frame), _)| {
            *id == browser.identifier()
                && known.current(*id, frame, token)
                && known.origin(*id, frame, token) == Some(origin)
        }) else {
            return false;
        };
        frame.clone()
    };
    let Some(frame) = browser.frame_by_identifier(Some(&CefString::from(frame_id.as_str()))) else {
        return false;
    };
    if web_origin(&CefString::from(&frame.url()).to_string()).as_deref() != Some(origin) {
        return false;
    }
    let Some(mut message) = process_message_create(Some(&FILL.into())) else {
        return false;
    };
    let Some(args) = message.argument_list() else {
        return false;
    };
    let wire_token = token.0.to_string();
    let wire_field = field.to_string();
    for (i, value) in [
        wire_token.as_str(),
        origin,
        credential.username(),
        credential.password(),
        if selected { "1" } else { "0" },
        wire_field.as_str(),
    ]
    .into_iter()
    .enumerate()
    {
        args.set_string(i, Some(&CefString::from(value)));
    }
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_origin_and_action_do_not_cross_authority_boundaries() {
        assert_eq!(
            web_origin("https://user:pass@EXAMPLE.test:443/login?q=private").as_deref(),
            Some("https://example.test")
        );
        assert_eq!(
            web_origin("http://example.test:8123/login").as_deref(),
            Some("http://example.test:8123")
        );
        for url in [
            "file:///test",
            "about:blank",
            "browser://password-manager",
            "data:text/html,test",
        ] {
            assert!(web_origin(url).is_none());
        }
        let token = PasswordToken::from_raw(1).unwrap();
        let json = r#"{"kind":"submitted","username":"test-user","password":"synthetic-test-secret","action_origin":"https://example.test"}"#;
        assert!(matches!(
            decode(token, "https://example.test", json),
            Some(PasswordEvent::Submitted { .. })
        ));
        assert!(decode(token, "https://other.test", json).is_none());
        assert!(decode(token,"https://example.test",r#"{"kind":"submitted","username":"u","password":"","action_origin":"https://example.test"}"#).is_none());
        assert!(
            decode(
                token,
                "https://example.test",
                r#"{"kind":"fill","username_hint":"u","unexpected":"field"}"#
            )
            .is_none()
        );
    }
    #[test]
    fn iframe_documents_cannot_fill_sibling_frames_or_reuse_navigated_tokens() {
        let mut known = Known::default();
        let a = PasswordToken::from_raw(1).unwrap();
        let b = PasswordToken::from_raw(2).unwrap();
        let c = PasswordToken::from_raw(3).unwrap();
        known.register(10, "main", a, "https://outer.test".into());
        known.register(10, "child", b, "https://login.test".into());
        assert!(known.current(10, "main", a));
        assert!(known.current(10, "child", b));
        assert!(known.origin(10, "main", b).is_none());
        assert!(known.origin(20, "child", b).is_none());
        known.register(10, "child", c, "https://other.test".into());
        assert!(!known.current(10, "child", b));
        assert!(known.current(10, "child", c));
        assert!(known.current(10, "main", a));
    }
    #[test]
    fn password_debug_and_event_debug_never_include_credentials() {
        let credential =
            LoginCredential::new("synthetic-private-user", "synthetic-private-password").unwrap();
        let event = crate::Event::Password {
            view: crate::ViewId::from_raw(1),
            event: PasswordEvent::Submitted {
                token: PasswordToken::from_raw(2).unwrap(),
                origin: "https://private-test.invalid".into(),
                credential: credential.clone(),
            },
        };
        for text in [format!("{credential:?}"), format!("{event:?}")] {
            for secret in [
                "synthetic-private-user",
                "synthetic-private-password",
                "private-test.invalid",
            ] {
                assert!(!text.contains(secret));
            }
        }
        assert!(LoginCredential::new("u", "").is_none());
    }
    #[test]
    fn password_documents_are_browser_bound_current_for_fill_and_bounded_for_submit() {
        let mut known = Known::default();
        let a = PasswordToken::from_raw(1).unwrap();
        let b = PasswordToken::from_raw(2).unwrap();
        let c = PasswordToken::from_raw(3).unwrap();
        known.register(10, "main", a, "https://a.test".into());
        known.register(20, "main", b, "https://b.test".into());
        assert_eq!(known.origin(10, "main", a), Some("https://a.test"));
        assert!(known.origin(20, "main", a).is_none());
        known.register(10, "main", b, "https://a.test".into());
        assert!(!known.current(10, "main", a));
        assert!(known.current(10, "main", b));
        assert_eq!(
            known.origin(10, "main", a),
            Some("https://a.test"),
            "A submitted old document may finish navigation"
        );
        known.register(10, "main", c, "https://other.test".into());
        assert!(known.origin(10, "main", a).is_none());
        assert!(known.current(10, "main", c));
        assert!(PasswordToken::from_raw(0).is_none());
        assert!(PasswordToken::parse("0").is_none());
        assert!(PasswordToken::parse(&"1".repeat(40)).is_none());
    }
}
