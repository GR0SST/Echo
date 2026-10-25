//! Shared scripting command dispatcher. The macOS transport owns only descriptors;
//! canonical profile edits and live-document operations stay on GPUI's app turn.
use crate::window::BrowserWindow;
use browser_core::{
    scripting::{
        self as model, code, Command, Error, Object, Query, Record, Result, Snapshot, Value,
    },
    TabId,
};
use gpui::{App, Context, WindowHandle};
use std::time::SystemTime;
fn handles(cx: &App) -> Vec<WindowHandle<BrowserWindow>> {
    cx.windows()
        .into_iter()
        .filter_map(|h| h.downcast::<BrowserWindow>())
        .collect()
}
fn frontmost(cx: &App) -> bool {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        let _ = cx;
        crate::native::scripting::frontmost()
    }
    #[cfg(any(not(target_os = "macos"), test))]
    {
        cx.active_window().is_some()
    }
}
fn window_id(h: WindowHandle<BrowserWindow>) -> String {
    format!("{:?}", h.window_id())
}
fn space_id(id: browser_core::SpaceId) -> String {
    serde_json::to_string(&id).unwrap()
}
fn handle(id: &str, cx: &App) -> Result<WindowHandle<BrowserWindow>> {
    handles(cx)
        .into_iter()
        .find(|h| window_id(*h) == id)
        .ok_or_else(Error::missing)
}
pub(crate) fn snapshot(cx: &mut App) -> Snapshot {
    let mut windows = Vec::new();
    for h in handles(cx) {
        let id = window_id(h);
        if let Ok(pair) = h.update(cx, |view, window, _| {
            view.refresh_profile_projection();
            let b = &view.browser;
            let tab = |t: &browser_core::Tab| model::Tab {
                id: t.id.get().to_string(),
                title: t.title.clone(),
                url: t.url.clone(),
                loading: view
                    .pages
                    .as_ref()
                    .is_some_and(|p| p.borrow().state(t.id).loading),
                location: if b.is_favorite(t.id) {
                    "topApp"
                } else if b
                    .spaces
                    .iter()
                    .any(|s| s.pinned.iter().any(|p| p.id == t.id))
                {
                    "pinned"
                } else {
                    "unpinned"
                }
                .into(),
            };
            let favorites = |profile| {
                b.user_profile(profile)
                    .map(|p| p.favorites.iter().map(&tab).collect::<Vec<_>>())
                    .unwrap_or_default()
            };
            let spaces = b
                .spaces
                .iter()
                .map(|s| {
                    let mut tabs = favorites(s.user_profile_id);
                    tabs.extend(s.pinned.iter().chain(&s.today).map(&tab));
                    model::Space {
                        id: space_id(s.id),
                        title: s.name.clone(),
                        tabs,
                    }
                })
                .collect();
            let mut tabs = favorites(b.active_user_profile_id());
            tabs.extend(b.space().pinned.iter().chain(&b.space().today).map(&tab));
            #[cfg(target_os = "macos")]
            let native = crate::native::scripting::state(crate::native::view_of(window));
            #[cfg(not(target_os = "macos"))]
            let native = (usize::MAX, false, true, false);
            (
                native.0,
                model::Window {
                    id: id.clone(),
                    title: b
                        .active_tab()
                        .map(|t| t.title.clone())
                        .unwrap_or_else(|| crate::APP_NAME.into()),
                    mode: if b.is_private() {
                        "incognito"
                    } else {
                        "normal"
                    }
                    .into(),
                    active_tab: b.active_tab().map(|t| t.id.get().to_string()),
                    active_space: Some(space_id(b.space().id)),
                    tabs,
                    spaces,
                    minimized: native.1,
                    visible: native.2,
                    zoomed: native.3,
                },
            )
        }) {
            windows.push(pair)
        }
    }
    windows.sort_by_key(|w| w.0);
    Snapshot {
        name: crate::APP_NAME.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        frontmost: frontmost(cx),
        windows: windows.into_iter().map(|(_, w)| w).collect(),
    }
}
fn tab_id(view: &BrowserWindow, id: &str) -> Result<TabId> {
    view.browser
        .all_tabs()
        .find(|t| t.id.get().to_string() == id)
        .map(|t| t.id)
        .ok_or_else(Error::missing)
}
fn select_tab(view: &mut BrowserWindow, id: TabId, cx: &mut Context<BrowserWindow>) -> Result<()> {
    if !view.browser.is_favorite(id) {
        let index = view
            .browser
            .spaces
            .iter()
            .position(|s| s.pinned.iter().chain(&s.today).any(|t| t.id == id))
            .ok_or_else(Error::missing)?;
        if index != view.browser.active_space {
            view.switch_space(index, cx)
        }
    }
    view.edit_browser(|b| b.activate_tab(id, SystemTime::now()));
    cx.notify();
    Ok(())
}
fn made_window(before: Vec<String>, cx: &App) -> Result<WindowHandle<BrowserWindow>> {
    handles(cx)
        .into_iter()
        .find(|h| !before.contains(&window_id(*h)))
        .ok_or_else(Error::missing)
}
fn new_window(private: bool, cx: &mut App) -> Result<WindowHandle<BrowserWindow>> {
    let before = handles(cx).into_iter().map(window_id).collect();
    if private {
        crate::open_incognito_window(cx)
    } else {
        crate::new_window::open(true, cx)
    }
    made_window(before, cx)
}
fn make(class: u32, container: Query, properties: Record, cx: &mut App) -> Result<Value> {
    if class == code(b"WiND") {
        if properties
            .keys()
            .any(|k| ![code(b"mode"), code(b"inco")].contains(k))
        {
            return Err(Error::invalid(
                "A new window accepts only mode or incognito.",
            ));
        }
        let mode = properties
            .get(&code(b"mode"))
            .map(Value::text)
            .transpose()?
            .unwrap_or("normal");
        if mode != "normal" && mode != "incognito" {
            return Err(Error::invalid("Window mode must be normal or incognito."));
        }
        let private = match properties.get(&code(b"inco")) {
            Some(Value::Bool(b)) => *b,
            None => mode == "incognito",
            _ => return Err(Error::invalid("Incognito must be boolean.")),
        };
        return Ok(Value::Object(Object::Window(window_id(new_window(
            private, cx,
        )?))));
    }
    if class != code(b"tAbB") {
        return Err(Error::unsupported());
    }
    if properties.keys().any(|k| *k != code(b"URL ")) {
        return Err(Error::invalid("A new tab accepts only URL."));
    }
    let url = properties
        .get(&code(b"URL "))
        .map(Value::text)
        .transpose()?
        .unwrap_or("about:blank")
        .to_owned();
    let target = snapshot(cx).one(&container)?;
    if target == Object::Application {
        let before = handles(cx).into_iter().map(window_id).collect();
        crate::transient::open(Some(url), cx);
        let h = made_window(before, cx)?;
        let tab = h
            .read(cx)
            .map_err(|_| Error::missing())?
            .browser
            .active_tab()
            .map(|t| t.id.get().to_string())
            .ok_or_else(Error::missing)?;
        return Ok(Value::Object(Object::Tab {
            window: window_id(h),
            id: tab,
        }));
    }
    let (window, space) = match target {
        Object::Window(w) => (w, None),
        Object::Space { window, id } => (window, Some(id)),
        _ => return Err(Error::invalid("Tabs must be created in a window or Space.")),
    };
    let h = handle(&window, cx)?;
    let id = h
        .update(cx, |view, _, cx| {
            if let Some(space) = space {
                let index = view
                    .browser
                    .spaces
                    .iter()
                    .position(|s| space_id(s.id) == space)
                    .ok_or_else(Error::missing)?;
                view.switch_space(index, cx);
            }
            let id = view.edit_browser(|b| b.new_tab(&url, SystemTime::now()));
            view.close_command_bar(cx);
            cx.notify();
            Ok(id.get().to_string())
        })
        .map_err(|_| Error::missing())??;
    Ok(Value::Object(Object::Tab { window, id }))
}
pub(crate) fn dispatch(command: Command, cx: &mut App) -> Result<Value> {
    if crate::quit::is_quitting() {
        return Err(Error {
            number: -600,
            message: "Echo is quitting.".into(),
        });
    }
    let state = snapshot(cx);
    match command {
        Command::Get(q) => return state.get(&q),
        Command::Count(q, c) => return state.count(&q, c),
        Command::Make {
            class,
            container,
            properties,
        } => return make(class, container, properties, cx),
        _ => {}
    }
    let query = match &command {
        Command::Set(q, _)
        | Command::Close(q)
        | Command::Select(q)
        | Command::Back(q)
        | Command::Forward(q)
        | Command::Reload(q)
        | Command::Stop(q)
        | Command::Execute(q, _)
        | Command::Focus(q) => q,
        _ => unreachable!(),
    };
    let (target, property) = if let Command::Set(
        Query::Property {
            property,
            container,
        },
        _,
    ) = &command
    {
        (state.one(container)?, Some(*property))
    } else {
        (state.one(query)?, None)
    };
    let window = match &target {
        Object::Window(w) => w,
        Object::Tab { window, .. } | Object::Space { window, .. } => window,
        _ => return Err(Error::unsupported()),
    };
    let h = handle(window, cx)?;
    h.update(cx, |view, window, cx| {
        view.refresh_profile_projection();
        match &target {
            Object::Window(_) => match &command {
                Command::Close(_) => view.close_window(cx),
                Command::Set(_, value) => {
                    let p = property.ok_or_else(Error::unsupported)?;
                    if p == code(b"pidx") && *value == Value::Int(1) {
                        window.activate_window()
                    } else {
                        #[cfg(target_os = "macos")]
                        {
                            crate::native::scripting::set_window(
                                crate::native::view_of(window),
                                p,
                                value,
                            )?;
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            return Err(Error::unsupported());
                        }
                    }
                }
                _ => return Err(Error::unsupported()),
            },
            Object::Space { id, .. } => {
                if !matches!(command, Command::Focus(_)) {
                    return Err(Error::unsupported());
                }
                let i = view
                    .browser
                    .spaces
                    .iter()
                    .position(|s| space_id(s.id) == *id)
                    .ok_or_else(Error::missing)?;
                view.switch_space(i, cx);
            }
            Object::Tab { id, .. } => {
                let id = tab_id(view, id)?;
                match &command {
                    Command::Select(_) => {
                        select_tab(view, id, cx)?;
                    }
                    Command::Close(_) => view.close_tab(id, cx),
                    Command::Set(_, value) => {
                        let p = property.ok_or_else(Error::unsupported)?;
                        if p == code(b"URL ") {
                            let url = value.text()?.to_owned();
                            view.edit_browser(|b| b.navigate(id, url));
                        } else if p == code(b"loca") {
                            let location = value.text()?;
                            if !["topApp", "pinned", "unpinned"].contains(&location) {
                                return Err(Error::invalid(
                                    "Location must be topApp, pinned or unpinned.",
                                ));
                            }
                            select_tab(view, id, cx)?;
                            view.edit_browser(|b| {
                                if location == "topApp" {
                                    b.favorite_tab(id);
                                } else {
                                    if b.is_favorite(id) {
                                        b.unfavorite_tab(id);
                                    }
                                    let pinned = b.space().pinned.iter().any(|t| t.id == id);
                                    if pinned != (location == "pinned") {
                                        b.toggle_pin(id);
                                    }
                                }
                            });
                        } else {
                            return Err(Error {
                                number: -10006,
                                message: "This property cannot be changed.".into(),
                            });
                        }
                    }
                    Command::Back(_)
                    | Command::Forward(_)
                    | Command::Reload(_)
                    | Command::Stop(_) => {
                        let Some(pages) = &view.pages else {
                            return Err(Error::invalid("This tab has no live page."));
                        };
                        let mut pages = pages.borrow_mut();
                        match command {
                            Command::Back(_) => pages.go_back(id),
                            Command::Forward(_) => pages.go_forward(id),
                            Command::Reload(_) => pages.reload(id),
                            Command::Stop(_) => pages.stop_loading(id),
                            _ => {}
                        }
                    }
                    Command::Execute(_, _) => {
                        return Err(Error::invalid(
                            "JavaScript evaluation is not available yet.",
                        ))
                    }
                    _ => return Err(Error::unsupported()),
                }
            }
            _ => return Err(Error::unsupported()),
        }
        cx.notify();
        Ok(Value::Null)
    })
    .map_err(|_| Error::missing())?
}
pub(crate) fn javascript_allowed(cx: &App) -> bool {
    cx.try_global::<crate::new_window::NormalProfile>()
        .is_some_and(|p| {
            p.profile.read_settings(|b| {
                b.settings
                    .enabled(browser_core::settings::Toggle::AllowJavaScriptFromAppleEvents)
            })
        })
}
pub(crate) fn toggle_javascript(cx: &mut App) {
    if crate::quit::is_quitting() {
        return;
    }
    if let Some(profile) = cx.try_global::<crate::new_window::NormalProfile>().cloned() {
        profile.profile.edit_settings(|b| {
            let t = browser_core::settings::Toggle::AllowJavaScriptFromAppleEvents;
            b.settings.set_enabled(t, !b.settings.enabled(t));
        });
    }
}

pub(crate) struct ScriptRequest {
    ticket: engine::ScriptTicket,
    window: String,
    identity: crate::pages::PageIdentity,
    url: String,
}
impl ScriptRequest {
    pub(crate) fn poll(&self, cx: &mut App) -> Option<Result<Value>> {
        if !javascript_allowed(cx) {
            return Some(Err(Error::invalid("JavaScript permission was revoked.")));
        }
        let valid = handle(&self.window, cx)
            .ok()
            .and_then(|h| {
                h.update(cx, |view, _, _| {
                    view.refresh_profile_projection();
                    view.browser.user_profile_id_for_tab(self.identity.tab)
                        == Some(self.identity.profile)
                        && view
                            .browser
                            .all_tabs()
                            .any(|t| t.id == self.identity.tab && t.url == self.url)
                        && view.pages.as_ref().is_some_and(|p| {
                            p.borrow().identity_for_tab(self.identity.tab) == Some(self.identity)
                        })
                })
                .ok()
            })
            .unwrap_or(false);
        if !valid {
            return Some(Err(Error::missing()));
        }
        self.ticket
            .take()
            .map(|result| result.map(Value::Text).map_err(Error::invalid))
    }
}
pub(crate) enum Dispatch {
    Done(Result<Value>),
    Script(ScriptRequest),
}
pub(crate) fn start(command: Command, cx: &mut App) -> Dispatch {
    if let Command::Execute(query, source) = &command {
        if !javascript_allowed(cx) {
            return Dispatch::Done(Err(Error{number:-10000,message:"JavaScript from Apple Events is disabled. To enable it, choose View > Developer > Allow JavaScript from Apple Events.".into()}));
        }

        let result = (|| {
            if crate::quit::is_quitting() {
                return Err(Error {
                    number: -600,
                    message: "Echo is quitting.".into(),
                });
            }
            let state = snapshot(cx);
            let Object::Tab { window, id } = state.one(query)? else {
                return Err(Error::invalid("JavaScript requires a tab."));
            };
            handle(&window, cx)?
                .update(cx, |view, _, _| {
                    view.refresh_profile_projection();
                    let id = tab_id(view, &id)?;
                    let pages = view
                        .pages
                        .as_ref()
                        .ok_or_else(|| Error::invalid("This tab has no live page."))?;
                    let profile = view
                        .browser
                        .user_profile_id_for_tab(id)
                        .ok_or_else(Error::missing)?;
                    let url = view
                        .browser
                        .all_tabs()
                        .find(|t| t.id == id)
                        .ok_or_else(Error::missing)?
                        .url
                        .clone();
                    let mut pages = pages.borrow_mut();
                    let identity = pages.identity_for_tab(id).ok_or_else(Error::missing)?;
                    let ticket = pages
                        .evaluate_script(id, profile, &url, source)
                        .map_err(Error::invalid)?;
                    Ok(ScriptRequest {
                        ticket,
                        window: window.clone(),
                        identity,
                        url,
                    })
                })
                .map_err(|_| Error::missing())?
        })();
        return match result {
            Ok(ticket) => Dispatch::Script(ticket),
            Err(e) => Dispatch::Done(Err(e)),
        };
    }
    Dispatch::Done(dispatch(command, cx))
}
pub(crate) fn init(cx: &mut App) {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        use std::time::Duration;
        if let Err(e) = crate::native::scripting::install() {
            eprintln!("Cannot install scripting transport: {e}");
            return;
        }
        cx.spawn(async move |cx| {
            let mut pending: Vec<(usize, ScriptRequest, std::time::Instant)> = Vec::new();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(10))
                    .await;
                let quitting = crate::quit::is_quitting();
                let allowed = cx.update(|cx| javascript_allowed(cx)).unwrap_or(false);
                crate::native::scripting::sync_javascript_checkmark(allowed);
                pending.retain(|(id, request, deadline)| {
                    let result = if quitting {
                        Some(Err(Error {
                            number: -600,
                            message: "Echo is quitting.".into(),
                        }))
                    } else if std::time::Instant::now() >= *deadline {
                        Some(Err(Error::invalid("JavaScript evaluation timed out.")))
                    } else {
                        cx.update(|cx| request.poll(cx))
                            .unwrap_or_else(|_| Some(Err(Error::missing())))
                    };
                    if let Some(result) = result {
                        crate::native::scripting::finish(*id, result);
                        false
                    } else {
                        true
                    }
                });
                for (id, command) in crate::native::scripting::take() {
                    if pending.len() >= 64 && matches!(command, Command::Execute(..)) {
                        crate::native::scripting::finish(
                            id,
                            Err(Error::invalid("Too many pending script requests.")),
                        );
                        continue;
                    }
                    match cx.update(|cx| start(command, cx)) {
                        Ok(Dispatch::Done(result)) => crate::native::scripting::finish(id, result),
                        Ok(Dispatch::Script(ticket)) => pending.push((
                            id,
                            ticket,
                            std::time::Instant::now() + Duration::from_secs(30),
                        )),
                        Err(_) => crate::native::scripting::finish(
                            id,
                            Err(Error {
                                number: -600,
                                message: "Echo is unavailable.".into(),
                            }),
                        ),
                    }
                }
            }
        })
        .detach();
    }
    #[cfg(any(not(target_os = "macos"), test))]
    let _ = cx;
}
