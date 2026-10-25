use super::*;
use engine::{ExtensionEvent, ExtensionService};
use serde_json::json;
fn selector(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}
const ONE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TWO: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
#[derive(Default)]
struct ServiceState {
    events: Vec<ExtensionEvent>,
    calls: Vec<String>,
}
#[derive(Clone, Default)]
struct Service(Rc<RefCell<ServiceState>>);
impl ExtensionService for Service {
    fn take_events(&self) -> Vec<ExtensionEvent> {
        std::mem::take(&mut self.0.borrow_mut().events)
    }
    fn set_enabled(&self, id: &str, enabled: bool, _: u64) -> Result<(), String> {
        self.0
            .borrow_mut()
            .calls
            .push(format!("enable {id} {enabled}"));
        Ok(())
    }
    fn remove(&self, id: &str, _: u64) -> Result<(), String> {
        self.0.borrow_mut().calls.push(format!("remove {id}"));
        Ok(())
    }
    fn set_shortcut(&self, id: &str, command: &str, key: &str, _: u64) -> Result<(), String> {
        self.0
            .borrow_mut()
            .calls
            .push(format!("shortcut {id} {command} {key}"));
        Ok(())
    }
    fn approve_install(&self, request: u64, accept: bool) -> Result<(), String> {
        self.0
            .borrow_mut()
            .calls
            .push(format!("approve {request} {accept}"));
        Ok(())
    }
    unsafe fn invoke_action(
        &self,
        id: &str,
        _: Box<dyn FnMut(engine::ExtensionPopupEvent)>,
    ) -> Result<engine::ExtensionPopup, String> {
        self.0.borrow_mut().calls.push(format!("invoke {id}"));
        Err("Native host is absent on the test platform".into())
    }
}
fn catalog() -> ExtensionEvent {
    ExtensionEvent::Catalog(json!({"extensions":[
        {"id":ONE,"name":"uBlock Origin Lite","version":"1","enabled":true,"commands":[{"name":"_execute_action","description":"Activate uBlock Origin Lite","shortcut":"Command+Shift+1"}]},
        {"id":TWO,"name":"Dark Reader","version":"2","enabled":true}
    ]}))
}
fn model_request(
    view: &Entity<BrowserWindow>,
    cx: &mut VisualTestContext,
    fake: &FakeEngine,
    token: u64,
    method: &str,
    arguments: serde_json::Value,
    tabs: serde_json::Value,
) {
    let (window, profile) = view.read_with(cx, |view, _| {
        (
            format!("window:{}", view.main_window.window_id().as_u64()),
            view.browser
                .active_user_profile_id()
                .storage_key()
                .unwrap_or_else(|| "default".into()),
        )
    });
    let mut state = fake.0.borrow_mut();
    state.extension_model_enabled = true;
    let owner = state.focused_page.unwrap();
    state
        .extension_model_requests
        .push(engine::ExtensionModelRequest {
            owner,
            token,
            data: json!({
                "method":method,"arguments":arguments,"extensionId":ONE,"profileKey":profile,
                "tabs":tabs,"windows":{"31":window},"currentWindow":window,"activeTabKey":null
            }),
        });
}
fn pump_model(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.run_until_parked();
}
#[gpui::test]
fn extension_background_tab_mutations_load_without_selecting_and_complete_after_the_model_changes(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    let selected = view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id);
    fake.take_calls();
    model_request(
        &view,
        cx,
        &fake,
        41,
        "tabs.create",
        json!([{"url":"https://extension-background.example/","active":false}]),
        json!({}),
    );
    pump_model(cx);
    let created = view.read_with(cx, |view, _| {
        assert_eq!(view.browser.active_tab().unwrap().id, selected);
        view.browser
            .all_tabs()
            .find(|tab| tab.url == "https://extension-background.example/")
            .unwrap()
            .id
    });
    let key = view.read_with(cx, |view, _| {
        format!(
            "{}:{}",
            view.profile.handle.model_namespace(),
            created.get()
        )
    });
    let result = fake
        .0
        .borrow()
        .extension_model_results
        .last()
        .cloned()
        .unwrap();
    assert_eq!(result, (41, json!({"tabKey":key})));
    let created_view = fake
        .take_calls()
        .into_iter()
        .find_map(|call| match call {
            Call::Create(id, url) if url == "https://extension-background.example/" => Some(id),
            _ => None,
        })
        .unwrap();
    model_request(
        &view,
        cx,
        &fake,
        42,
        "tabs.update",
        json!([23,{"url":"https://updated-extension.example/","muted":true}]),
        json!({"23":key}),
    );
    pump_model(cx);
    assert!(fake.take_calls().contains(&Call::Navigate(
        created_view,
        "https://updated-extension.example/".into()
    )));
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id),
        selected
    );
    model_request(
        &view,
        cx,
        &fake,
        43,
        "tabs.reload",
        json!([23,{"bypassCache":true}]),
        json!({"23":key}),
    );
    pump_model(cx);
    assert!(fake.take_calls().contains(&Call::ForceReload(created_view)));
    model_request(
        &view,
        cx,
        &fake,
        44,
        "tabs.remove",
        json!([23]),
        json!({"23":key}),
    );
    pump_model(cx);
    assert!(!view.read_with(cx, |view, _| view
        .browser
        .all_tabs()
        .any(|tab| tab.id == created)));
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.active_tab().unwrap().id),
        selected
    );
    assert!(fake.take_calls().contains(&Call::Close(created_view)));
    assert!(fake
        .0
        .borrow()
        .extension_model_results
        .iter()
        .all(|(_, value)| value.get("error").is_none()));
}
#[gpui::test]
fn an_expired_extension_request_never_creates_a_tab(cx: &mut TestAppContext) {
    let (view, cx, fake) = open_pages(cx);
    let before = view.read_with(cx, |view, _| view.browser.all_tabs().count());
    model_request(
        &view,
        cx,
        &fake,
        71,
        "tabs.create",
        json!([{"url":"https://must-not-open.example/"}]),
        json!({}),
    );
    fake.0.borrow_mut().extension_model_expired.insert(71);
    pump_model(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.all_tabs().count()),
        before
    );
    assert!(
        fake.0.borrow().extension_model_results.last().unwrap().1["error"]
            .as_str()
            .unwrap()
            .contains("expired")
    );
}
fn install(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> Service {
    let service = Service::default();
    service.0.borrow_mut().events.push(catalog());
    view.update(cx, |view, cx| {
        view.extensions.profile = Some(view.browser.active_user_profile_id());
        view.extensions.service = Some(Rc::new(service.clone()));
        view.poll_extensions(cx);
    });
    service
}
#[gpui::test]
fn extension_grid_metrics_pin_and_store_route_use_real_controls(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let _service = install(&view, cx);
    click(cx, "address-site-controls");
    let overlay = view.read_with(cx, |view, _| {
        view.site_controls.as_ref().unwrap().overlay.unwrap()
    });
    let mut panel = VisualTestContext::from_window(overlay.into(), &cx.cx);
    panel.run_until_parked();
    let first = panel
        .debug_bounds(selector(format!("site-extension-{TWO}")))
        .unwrap();
    let second = panel
        .debug_bounds(selector(format!("site-extension-{ONE}")))
        .unwrap();
    assert_eq!(first.size, gpui::size(gpui::px(48.), gpui::px(32.)));
    assert_eq!(second.left() - first.right(), gpui::px(8.));
    panel.simulate_mouse_move(second.center(), None, Modifiers::none());
    panel.simulate_mouse_down(second.center(), gpui::MouseButton::Right, Modifiers::none());
    panel.simulate_mouse_up(second.center(), gpui::MouseButton::Right, Modifiers::none());
    panel.run_until_parked();
    let menu = view.read_with(&panel, |view, _| {
        view.tab_menu.as_ref().unwrap().overlay.unwrap()
    });
    let mut menu = VisualTestContext::from_window(menu.into(), &cx.cx);
    menu.run_until_parked();
    menu.simulate_keystrokes("down down enter");
    menu.run_until_parked();
    assert!(view.read_with(&panel, |view, _| view
        .browser
        .active_user_profile()
        .pinned_extensions
        .contains(ONE)));
    panel.refresh().unwrap();
    panel.run_until_parked();
    // Pinning changes toolbar placement; every enabled action stays in SCC.
    assert_eq!(
        panel.debug_bounds("site-add-extension").unwrap().left(),
        second.right() + gpui::px(8.)
    );
    click(&mut panel, "site-add-extension");
    cx.run_until_parked();
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some(crate::extensions::STORE)
    );
    view.read_with(cx, |view, _| {
        assert!(view
            .browser
            .active_user_profile()
            .pinned_extensions
            .contains(ONE))
    });
}
#[gpui::test]
fn extensions_management_waits_for_native_catalog_and_confirms_removal(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let service = install(&view, cx);
    view.update(cx, |view, cx| view.open_extensions(cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("extensions-page").is_some());
    click(cx, selector(format!("extension-toggle-{ONE}")));
    assert_eq!(
        service.0.borrow().calls,
        vec![format!("enable {ONE} false")]
    );
    // The native catalog, rather than an optimistic UI flag, owns enabled state.
    assert!(view.read_with(cx, |view, _| view
        .extensions
        .entries
        .iter()
        .find(|e| e.id == ONE)
        .unwrap()
        .enabled));
    click(cx, selector(format!("extension-remove-{TWO}")));
    let overlay = view.read_with(cx, |view, _| view.extensions.overlay.unwrap());
    let mut sheet = VisualTestContext::from_window(overlay.into(), &cx.cx);
    sheet.run_until_parked();
    assert_eq!(
        sheet
            .debug_bounds("extension-remove-sheet")
            .unwrap()
            .size
            .width,
        gpui::px(450.)
    );
    sheet.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!service
        .0
        .borrow()
        .calls
        .iter()
        .any(|call| call.starts_with("remove")));
    click(cx, selector(format!("extension-remove-{TWO}")));
    let overlay = view.read_with(cx, |view, _| view.extensions.overlay.unwrap());
    let mut sheet = VisualTestContext::from_window(overlay.into(), &cx.cx);
    sheet.run_until_parked();
    sheet.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(service.0.borrow().calls.contains(&format!("remove {TWO}")));
    assert!(
        !view.read_with(cx, |view, _| view.pages.as_ref().is_some_and(|pages| pages
            .borrow()
            .has_page(view.browser.active_tab().unwrap().id)))
    );
}
#[gpui::test]
fn extension_permission_prompt_uses_native_continuation_and_escape_cancels(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let service = install(&view, cx);
    service.0.borrow_mut().events.push(ExtensionEvent::InstallPrompt{request:42,prompt:json!({"title":"Add Dark Reader?","accept":"Add extension","cancel":"Cancel","heading":"It can:","permissions":[{"message":"Read and change site data","details":"All websites"}]})});
    view.update(cx, |view, cx| view.poll_extensions(cx));
    cx.run_until_parked();
    let overlay = view.read_with(cx, |view, _| view.extensions.overlay.unwrap());
    let mut prompt = VisualTestContext::from_window(overlay.into(), &cx.cx);
    prompt.run_until_parked();
    assert!(prompt.debug_bounds("extension-install-confirm").is_some());
    assert!(service.0.borrow().calls.is_empty());
    prompt.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(service.0.borrow().calls, vec!["approve 42 false"]);
    assert!(view.read_with(cx, |view, _| view.extensions.overlay.is_none()));
}
#[gpui::test]
fn extension_command_bar_captures_profile_and_stale_profile_actions_are_rejected(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let service = install(&view, cx);
    cx.simulate_keystrokes("cmd-t");
    bar(&view, cx).simulate_input("extension uBlock");
    let action = view
        .read_with(cx, |view, cx| {
            view.suggestions(cx).into_iter().find_map(|row| {
                if let Suggestion::Action {
                    command: command @ crate::window::Command::ExtensionAction(..),
                    ..
                } = row
                {
                    Some(command)
                } else {
                    None
                }
            })
        })
        .unwrap();
    let old = view.read_with(cx, |view, _| view.browser.active_user_profile_id());
    view.update(cx, |view, cx| {
        let other = view.edit_profile(|browser| {
            browser
                .create_user_profile("Extension test second profile")
                .unwrap()
        });
        view.extensions.profile = Some(other);
        view.run_command(action, cx);
    });
    assert!(service.0.borrow().calls.is_empty());
    view.update(cx, |view, cx| {
        view.extensions.profile = Some(old);
        view.run_command(action, cx);
    });
    assert!(service.0.borrow().calls.contains(&format!("invoke {ONE}")));
}

#[gpui::test]
fn extension_shortcuts_settings_records_through_the_profile_service_and_rejects_reserved_keys(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let service = install(&view, cx);
    cx.simulate_keystrokes("cmd-,");
    let (_, mut settings) = settings_window(cx);
    click(&mut settings, "settings-tab-Shortcuts");
    click(&mut settings, "shortcut-search");
    settings.simulate_input("ublock");
    settings.run_until_parked();
    click(
        &mut settings,
        selector(format!("extension-shortcut-{ONE}-_execute_action")),
    );
    settings.simulate_keystrokes("alt-9");
    settings.run_until_parked();
    assert!(service
        .0
        .borrow()
        .calls
        .contains(&format!("shortcut {ONE} _execute_action Alt+9")));
    click(
        &mut settings,
        selector(format!("extension-shortcut-{ONE}-_execute_action")),
    );
    settings.simulate_keystrokes("alt-left");
    settings.run_until_parked();
    assert!(service
        .0
        .borrow()
        .calls
        .contains(&format!("shortcut {ONE} _execute_action Alt+Left")));
    click(
        &mut settings,
        selector(format!("extension-shortcut-{ONE}-_execute_action")),
    );
    settings.simulate_keystrokes("ctrl-shift-y");
    settings.run_until_parked();
    assert!(service
        .0
        .borrow()
        .calls
        .contains(&format!("shortcut {ONE} _execute_action Ctrl+Shift+Y")));
    let count = service.0.borrow().calls.len();
    click(
        &mut settings,
        selector(format!("extension-shortcut-{ONE}-_execute_action")),
    );
    settings.simulate_keystrokes("cmd-alt-y");
    settings.run_until_parked();
    assert_eq!(service.0.borrow().calls.len(), count);
    settings.simulate_keystrokes("cmd-q");
    settings.run_until_parked();
    assert_eq!(service.0.borrow().calls.len(), count);
    assert!(!crate::quit::is_quitting());
}

#[gpui::test]
fn extension_confirmation_requires_click_and_quit_cancels_a_pending_install(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let service = install(&view, cx);
    let prompt = |request| ExtensionEvent::InstallPrompt {
        request,
        prompt: json!({"title":"Add Dark Reader?","accept":"Add extension","cancel":"Cancel","permissions":[]}),
    };
    service.0.borrow_mut().events.push(prompt(42));
    view.update(cx, |view, cx| view.poll_extensions(cx));
    cx.run_until_parked();
    assert!(service.0.borrow().calls.is_empty());
    let overlay = view.read_with(cx, |view, _| view.extensions.overlay.unwrap());
    let mut panel = VisualTestContext::from_window(overlay.into(), &cx.cx);
    panel.run_until_parked();
    click(&mut panel, "extension-install-confirm");
    cx.run_until_parked();
    assert_eq!(service.0.borrow().calls, ["approve 42 true"]);
    service.0.borrow_mut().events.push(prompt(43));
    view.update(cx, |view, cx| view.poll_extensions(cx));
    cx.run_until_parked();
    let pending = view.update(cx, |view, cx| view.retire_extensions_for_quit(cx));
    cx.run_until_parked();
    assert_eq!(pending.get(), 0);
    assert_eq!(
        service.0.borrow().calls,
        ["approve 42 true", "approve 43 false"]
    );
    assert!(
        view.read_with(cx, |view, _| view.extensions.service.is_none()
            && view.extensions.overlay.is_none())
    );
}

#[gpui::test]
fn extension_manager_search_escape_and_details_back_use_live_controls(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let _service = install(&view, cx);
    view.update(cx, |view, cx| view.open_extensions(cx));
    cx.run_until_parked();
    let card = cx
        .debug_bounds(selector(format!("extension-card-{ONE}")))
        .unwrap();
    assert_eq!(card.size, gpui::size(gpui::px(400.), gpui::px(160.)));
    click(cx, "extension-search-open");
    cx.simulate_input("Dark Reader");
    cx.run_until_parked();
    let search = view.read_with(cx, |view, _| view.extensions.search.clone());
    assert_eq!(
        search.read_with(cx, |input, _| input.text().to_owned()),
        "Dark Reader"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(search.read_with(cx, |input, _| input.text().is_empty()));
    click(cx, selector(format!("extension-details-{ONE}")));
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some(format!("{}?id={ONE}", crate::extensions::URL).as_str())
    );
    assert!(cx.debug_bounds("extension-details").is_some());
    click(cx, selector(format!("extension-pin-{ONE}")));
    assert!(view.read_with(cx, |view, _| view
        .browser
        .active_user_profile()
        .pinned_extensions
        .contains(ONE)));
    click(cx, "extension-details-back");
    assert_eq!(
        active_url(&view, cx).as_deref(),
        Some(crate::extensions::URL)
    );
    assert!(cx.debug_bounds("extensions-page").is_some());
}

#[gpui::test]
fn extension_developer_toggle_changes_only_real_metadata_and_persists(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let _service = install(&view, cx);
    view.update(cx, |view, cx| view.open_extensions(cx));
    cx.run_until_parked();
    click(cx, "extension-developer-mode");
    assert!(view.read_with(cx, |view, _| view
        .browser
        .active_user_profile()
        .extension_developer_mode));
    let card = cx
        .debug_bounds(selector(format!("extension-card-{ONE}")))
        .unwrap();
    assert_eq!(card.size.height, gpui::px(208.));
    let serialized = view.read_with(cx, |view, _| {
        serde_json::to_value(view.browser.active_user_profile()).unwrap()
    });
    assert_eq!(serialized["extension_developer_mode"], true);
    // Advanced tools are a separate bridge task; none are advertised here.
    assert!(cx.debug_bounds("extension-load-unpacked").is_none());
    assert!(cx.debug_bounds("extension-pack").is_none());
    assert!(cx.debug_bounds("extension-update").is_none());
    click(cx, "extension-developer-mode");
    assert!(!view.read_with(cx, |view, _| view
        .browser
        .active_user_profile()
        .extension_developer_mode));
    assert_eq!(
        cx.debug_bounds(selector(format!("extension-card-{ONE}")))
            .unwrap()
            .size
            .height,
        gpui::px(160.)
    );
}

#[gpui::test]
fn extension_removal_sheet_closes_if_the_native_catalog_removes_its_target(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let service = install(&view, cx);
    view.update(cx, |view, cx| view.open_extensions(cx));
    cx.run_until_parked();
    click(cx, selector(format!("extension-remove-{TWO}")));
    assert!(view.read_with(cx, |view, _| view.extensions.overlay.is_some()));
    service
        .0
        .borrow_mut()
        .events
        .push(ExtensionEvent::Catalog(json!({"extensions":[]})));
    view.update(cx, |view, cx| view.poll_extensions(cx));
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.extensions.overlay.is_none()));
    assert!(!service
        .0
        .borrow()
        .calls
        .iter()
        .any(|call| call.starts_with("remove")));
}

#[gpui::test]
fn extension_pin_places_a_live_action_in_the_toolbar_and_unpin_keeps_scc_access(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    let service = install(&view, cx);
    view.update(cx, |view, cx| {
        view.edit_browser(|browser| {
            browser
                .settings
                .set_enabled(browser_core::settings::Toggle::ShowToolbar, true)
        });
        view.open_extensions(cx);
    });
    cx.run_until_parked();
    click(cx, selector(format!("extension-details-{ONE}")));
    click(cx, selector(format!("extension-pin-{ONE}")));
    click(cx, selector(format!("toolbar-extension-{ONE}")));
    assert!(service.0.borrow().calls.contains(&format!("invoke {ONE}")));
    click(cx, selector(format!("extension-pin-{ONE}")));
    assert!(!view.read_with(cx, |view, _| view
        .browser
        .active_user_profile()
        .pinned_extensions
        .contains(ONE)));
    click(cx, "toolbar-site-controls");
    let overlay = view.read_with(cx, |view, _| {
        view.site_controls.as_ref().unwrap().overlay.unwrap()
    });
    let mut panel = VisualTestContext::from_window(overlay.into(), &cx.cx);
    panel.run_until_parked();
    click(&mut panel, selector(format!("site-extension-{ONE}")));
    assert_eq!(
        service
            .0
            .borrow()
            .calls
            .iter()
            .filter(|call| *call == &format!("invoke {ONE}"))
            .count(),
        2
    );
}

#[gpui::test]
fn extension_manager_never_paints_a_catalog_for_a_different_profile(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let _service = install(&view, cx);
    view.update(cx, |view, cx| view.open_extensions(cx));
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        let other = view.edit_profile(|browser| {
            browser
                .create_user_profile("Extension catalog isolation")
                .unwrap()
        });
        // Simulate the interval before the native service's profile change is polled.
        view.extensions.profile = Some(other);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("extensions-empty").is_some());
}
