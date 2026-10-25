use super::*;
use echo_ai::credentials::{CredentialError, CredentialStore, Secret};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
#[derive(Default)]
struct Store(Mutex<HashMap<String, String>>);
impl CredentialStore for Store {
    fn get(&self, id: &str) -> Result<Option<Secret>, CredentialError> {
        Ok(self.0.lock().unwrap().get(id).cloned().map(Secret::new))
    }
    fn set(&self, id: &str, value: &Secret) -> Result<(), CredentialError> {
        self.0
            .lock()
            .unwrap()
            .insert(id.into(), value.expose().into());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), CredentialError> {
        self.0.lock().unwrap().remove(id);
        Ok(())
    }
}
#[derive(Default)]
struct Api(Mutex<Vec<String>>);
impl echo_github::Transport for Api {
    fn get(
        &self,
        path: &str,
        token: &Secret,
        _: Option<&str>,
    ) -> Result<echo_github::Response, echo_github::Error> {
        assert_eq!(token.expose(), "test-only-github-token");
        self.0.lock().unwrap().push(path.into());
        let body = if path == "/user" {
            serde_json::json!({"login":"tester"})
        } else if path.starts_with("/user/teams") {
            serde_json::json!([])
        } else {
            serde_json::json!({"incomplete_results":false,"total_count":1,"items":[{"id":42,"title":"Parser review","html_url":"https://github.com/echo/test/pull/7","repository_url":"https://api.github.com/repos/echo/test","updated_at":"2026-10-25T00:00:00Z","pull_request":{}}]})
        };
        Ok(echo_github::Response {
            status: 200,
            headers: HashMap::new(),
            body: body.to_string(),
        })
    }
}
fn services(cx: &mut VisualTestContext) -> (Arc<Store>, Arc<Api>) {
    let store = Arc::new(Store::default());
    let api = Arc::new(Api::default());
    cx.cx.update(|cx| {
        cx.set_global(crate::live_folders::Services {
            store: store.clone(),
            transport: api.clone(),
        })
    });
    (store, api)
}
fn connection(view: &Entity<BrowserWindow>, cx: &mut VisualTestContext) -> VisualTestContext {
    cx.run_until_parked();
    let overlay = view.read_with(cx, |b, _| {
        b.live_folders.sheet.as_ref().unwrap().overlay.unwrap()
    });
    VisualTestContext::from_window(overlay.into(), &cx.cx)
}
#[gpui::test]
fn live_folder_creation_masks_token_and_cancel_never_connects(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (store, api) = services(cx);
    right_click(cx, "space-header-0");
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-live-folders");
    click(&mut menu, "tab-menu-live-create");
    let mut sheet = connection(&view, cx);
    assert_eq!(
        sheet.debug_bounds("github-connection").unwrap().size.width,
        gpui::px(450.)
    );
    sheet.simulate_input("test-only-github-token");
    sheet.simulate_keystrokes("cmd-a cmd-c");
    sheet.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |b, _| b.live_folders.sheet.is_none()));
    assert!(store.0.lock().unwrap().is_empty());
    assert!(api.0.lock().unwrap().is_empty());
    assert!(!view
        .read_with(cx, |b, _| b.session_json())
        .contains("test-only-github-token"));
}
#[gpui::test]
fn live_folder_connect_refresh_filters_and_repository_visibility_preserve_read_state(
    cx: &mut TestAppContext,
) {
    let (view, cx, _) = open_pages(cx);
    let (store, api) = services(cx);
    view.update(cx, |b, cx| b.create_live_folder(b.browser.space().id, cx));
    let mut sheet = connection(&view, cx);
    sheet.simulate_input("test-only-github-token");
    click(&mut sheet, "github-connect");
    cx.run_until_parked();
    let id = view.read_with(cx, |b, _| {
        assert!(b.live_folders.sheet.is_none());
        let f = b
            .browser
            .space()
            .folders
            .iter()
            .find(|f| f.live.is_some())
            .unwrap();
        assert_eq!(f.live.as_ref().unwrap().items.len(), 1);
        assert_eq!(f.live.as_ref().unwrap().unread_count(), 1);
        assert!(!b.session_json().contains("test-only-github-token"));
        f.id
    });
    assert_eq!(store.0.lock().unwrap().len(), 1);
    assert!(api
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.contains("review-requested%3Atester")));
    click(cx, "live-item-42");
    view.read_with(cx, |b, _| {
        assert_eq!(
            b.browser.active_tab().unwrap().url,
            "https://github.com/echo/test/pull/7"
        );
        assert_eq!(
            b.browser
                .folder(id)
                .unwrap()
                .live
                .as_ref()
                .unwrap()
                .unread_count(),
            0
        );
    });
    let selector = test_selector(format!("folder-{}", id.get()));
    let before = api.0.lock().unwrap().len();
    right_click(cx, selector);
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-pull-requests");
    click(&mut menu, "tab-menu-live-refresh");
    cx.run_until_parked();
    assert_eq!(api.0.lock().unwrap().len(), before + 3);
    right_click(cx, selector);
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-pull-requests");
    click(&mut menu, "tab-menu-live-review");
    cx.run_until_parked();
    view.read_with(cx, |b, _| {
        let live = b.browser.folder(id).unwrap().live.as_ref().unwrap();
        assert!(!live
            .filters
            .contains(&browser_core::live_folders::PullRequestFilter::ReviewRequests));
        assert!(live
            .filters
            .contains(&browser_core::live_folders::PullRequestFilter::CreatedByMe));
        assert_eq!(live.unread_count(), 0);
    });
    right_click(cx, selector);
    let mut menu = tab_menu(&view, cx);
    click(&mut menu, "tab-menu-repositories");
    click(&mut menu, "tab-menu-live-repository");
    view.read_with(cx, |b, _| {
        let live = b.browser.folder(id).unwrap().live.as_ref().unwrap();
        assert_eq!(live.visible_items().count(), 0);
        assert_eq!(live.items.len(), 1);
    });
}
#[gpui::test]
fn live_folder_delete_requires_confirmation_and_preserves_space(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    services(cx);
    let (space, id) = view.update(cx, |b, cx| {
        let space = b.browser.space().id;
        let id = b
            .edit_browser(|model| model.create_github_live_folder(space))
            .unwrap();
        cx.notify();
        (space, id)
    });
    cx.run_until_parked();
    right_click(cx, test_selector(format!("folder-{}", id.get())));
    click(&mut tab_menu(&view, cx), "tab-menu-folder-delete");
    let mut sheet = connection(&view, cx);
    assert!(sheet.debug_bounds("live-folder-delete").is_some());
    click(&mut sheet, "live-delete-cancel");
    assert!(view.read_with(cx, |b, _| b.browser.folder(id).is_some()));
    right_click(cx, test_selector(format!("folder-{}", id.get())));
    click(&mut tab_menu(&view, cx), "tab-menu-folder-delete");
    let mut sheet = connection(&view, cx);
    sheet.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |b, _| {
        assert!(b.browser.folder(id).is_none());
        assert_eq!(b.browser.space().id, space);
    });
}
#[gpui::test]
fn live_folder_sheet_rejects_profile_reassignment_without_connecting(cx: &mut TestAppContext) {
    let (view, cx, _) = open_pages(cx);
    let (store, api) = services(cx);
    view.update(cx, |b, cx| b.create_live_folder(b.browser.space().id, cx));
    let mut sheet = connection(&view, cx);
    sheet.simulate_input("test-only-github-token");
    view.update(cx, |b, cx| {
        b.edit_browser(|model| {
            let profile = model.create_user_profile("Other GitHub identity").unwrap();
            model
                .assign_space_user_profile(model.space().id, profile)
                .unwrap();
        });
        cx.notify();
    });
    cx.run_until_parked();
    assert!(view.read_with(cx, |b, _| b.live_folders.sheet.is_none()));
    assert!(store.0.lock().unwrap().is_empty());
    assert!(api.0.lock().unwrap().is_empty());
}
