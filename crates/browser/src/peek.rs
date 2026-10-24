//! Page-local Peek. A transparent child hosts native content inside the card.
use std::time::{Instant, SystemTime};
use browser_core::splits::Placement;
use gpui::{actions, div, point, prelude::*, px, AnyElement, App, Context, KeyBinding,
    WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions};
use crate::{icons::{self, Icon}, native, new_window::NormalProfile, pages::StorageSelection,
    profile::ProfileHandle, transient::Transient, window::BrowserWindow};

actions!(peek, [ExitPeek]);
pub(crate) struct PeekOwner {
    owner: WeakEntity<BrowserWindow>,
    sidebar: f32,
    opened: Instant,
    origin: gpui::Point<gpui::Pixels>,
    closing: Option<Instant>,
}

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", ExitPeek, Some("Peek"))]);
}

impl BrowserWindow {
    pub(crate) fn open_peek(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.peek_owner.is_some() || self.transient.is_some() || self.peek_child.is_some() || crate::quit::is_quitting() { return; }
        let Ok(url) = url::Url::parse(&url) else { return; };
        if !matches!(url.scheme(), "http" | "https") { return; }
        self.close_command_bar(cx);
        let source = NormalProfile { profile: self.profile.handle.clone(), pages: self.pages.clone() };
        let space = self.browser.space().id;
        let theme = self.browser.space().theme.clone();
        let hue = self.browser.space().hue;
        let website_profile = self.browser.active_user_profile_id();
        let private = self.browser.is_private();
        let share_history = !private && !self.browser.is_blank();
        let storage = if private { StorageSelection::Current } else { StorageSelection::for_profile(website_profile) };
        let bounds = window.bounds();
        let origin = window.mouse_position();
        let sidebar = self.sidebar_width() * self.sidebar_amount();
        let parent = self.native_view;
        let owner = cx.weak_entity();
        if let Some(pages) = &self.pages { pages.borrow_mut().request_ui_focus(); }
        cx.defer(move |cx| {
            if owner.upgrade().is_none() || crate::quit::is_quitting() { return; }
            let handle = cx.open_window(WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: None,
                kind: WindowKind::Normal, is_movable: false, is_resizable: false, is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
            }, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, native::view_of(window)) { native::attach_overlay(parent, child, false); }
                let pages = source.pages.as_ref().and_then(|pages| pages.borrow().independent_in_storage(
                    native::view_of(window).unwrap_or(std::ptr::null_mut()), &storage));
                let mut browser = if private { browser_core::Browser::new_private() } else { source.profile.blank_browser(website_profile) };
                browser.spaces[0].hue = hue;
                browser.spaces[0].theme = theme;
                browser.new_tab(url.as_str(), SystemTime::now());
                let profile = if share_history { ProfileHandle::new_with_shared_history(browser, &source.profile, cx) } else { ProfileHandle::new(browser, cx) };
                let selection = profile.selection();
                cx.new(|cx| {
                    let mut view = BrowserWindow::with_profile_pages(profile, Some(selection), pages, window, cx);
                    if let Some(pages) = &view.pages { pages.borrow_mut().peek_mode = true; }
                    view.sidebar_visible = false;
                    view.transient = Some(Transient { source, space, picker: None });
                    view.peek_owner = Some(PeekOwner { owner: owner.clone(), sidebar, origin, opened: cx.background_executor().now(), closing: None });
                    view.focus(window);
                    view
                })
            });
            if let Ok(handle) = handle {
                let _ = owner.update(cx, |view, cx| { view.peek_child = Some(handle); cx.notify(); });
            }
        });
    }

    pub(crate) fn exit_peek(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = &mut self.peek_owner {
            if state.closing.is_none() {
                state.closing = Some(cx.background_executor().now());
                let child = cx.weak_entity();
                cx.spawn(async move |_, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(180)).await;
                    let _ = child.update(cx, |view, cx| view.close_window(cx));
                }).detach();
                cx.notify();
            }
        } else if let Some(child) = self.peek_child {
            cx.defer(move |cx| { let _ = child.update(cx, |view, _, cx| view.exit_peek(cx)); });
        }
    }

    pub(crate) fn detach_peek(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.peek_owner.take() else { return; };
        let handle = self.main_window;
        cx.defer(move |cx| {
            let _ = state.owner.update(cx, |view, cx| {
                if view.peek_child.is_some_and(|child| child.window_id() == handle.window_id()) {
                    view.peek_child = None;
                    if let Some(pages) = &view.pages { pages.borrow_mut().request_page_focus(); }
                    let root = view.main_window;
                    let focus = view.focus_handle.clone();
                    cx.defer(move |cx| { let _ = root.update(cx, |_, window, _| { window.activate_window(); focus.focus(window); }); });
                    cx.notify();
                }
            });
        });
    }

    fn split_peek(&mut self, cx: &mut Context<Self>) {
        let Some(state) = &self.peek_owner else { return; };
        let Some(tab) = self.browser.active_tab() else { return; };
        let url = tab.url.clone();
        let owner = state.owner.clone();
        let child = cx.weak_entity();
        cx.defer(move |cx| {
            let opened = owner.update(cx, |view, cx| {
                let Some(source) = view.browser.space().active_tab else { return false; };
                if !view.can_add_split(source) { return false; }
                view.begin_split(source, Placement::Right, cx);
                let Some(draft) = view.split_draft.take() else { return false; };
                view.edit_browser(|browser| browser.set_url(draft.pane, url));
                cx.notify();
                true
            }).unwrap_or(false);
            if opened { let _ = child.update(cx, |view, cx| view.close_window(cx)); }
        });
    }

    pub(crate) fn render_peek(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let state = self.peek_owner.as_ref().unwrap();
        let now = cx.background_executor().now();
        let elapsed = now.saturating_duration_since(state.opened).as_secs_f32();
        let closing = state.closing.map(|start| now.saturating_duration_since(start).as_secs_f32() / 0.18);
        let progress = if native::prefers_reduced_motion() { 1. } else {
            // A damped spring follows Arc's measured opening overshoot.
            let t = (elapsed / 0.34).min(1.);
            if t >= 1. { 1. } else { 1. - (-8. * t).exp() * ((12. * t).cos() + (8. / 12.) * (12. * t).sin()) }
        };
        let amount = closing.map_or(progress, |t| 1. - t.clamp(0., 1.).powi(2));
        if elapsed < 0.34 || closing.is_some() { cx.on_next_frame(window, |_, _, cx| cx.notify()); }
        let sidebar = state.sidebar;
        let extent = window.viewport_size();
        let available = (f32::from(extent.width) - sidebar).max(0.);
        let target_width = available * 0.8;
        let (left, top, width, height) = if closing.is_some() {
            let x = f32::from(state.origin.x).clamp(sidebar, f32::from(extent.width));
            let y = f32::from(state.origin.y).clamp(12., f32::from(extent.height) - 12.);
            (x + (sidebar + available * 0.1 - x) * amount, y + (12. - y) * amount,
                target_width * amount, (f32::from(extent.height) - 24.) * amount)
        } else {
            let top = 12. + 40. * (1. - amount);
            (sidebar + available * 0.1 - 8. * (1. - amount), top,
                target_width * (0.82 + 0.18 * amount), (f32::from(extent.height) - top * 2.).max(0.))
        };
        let control = self.command_bar_panel_fill(false);
        let button = |id: &'static str, icon: AnyElement| div().id(id).debug_selector(move || id.into())
            .size(px(34.)).flex_none().rounded_full().bg(control).text_color(gpui::white())
            .flex().items_center().justify_center().shadow_md()
            .hover(|style| style.bg(gpui::hsla(control.h, control.s, (control.l + 0.08).min(1.), 1.)))
            .child(icon);
        let content = div().key_context("Peek").track_focus(&self.focus_handle).size_full().relative()
            .child(div().absolute().size_0().track_focus(&self.native_action_focus))
            .child(div().id("peek-backdrop").absolute().top(px(10.)).bottom(px(10.)).left(px(sidebar)).right(px(10.))
                .rounded(px(8.)).opacity(0.6 * amount.clamp(0., 1.)).child(self.theme_surface(false)).on_click(cx.listener(|view, _, _, cx| view.exit_peek(cx))))
            .child(div().debug_selector(|| "peek-card".into()).absolute().left(px(left)).top(px(top))
                .w(px(width)).h(px(height)).rounded(px(8.)).bg(gpui::white()).shadow_xl()
                .when(self.pages.is_none(), |view| view.child(self.browser.active_tab().map(|tab| tab.url.clone()).unwrap_or_default()))
                .children(self.render_page_viewport_canvas()))
            .child(div().absolute().left(px(left + width + 13.)).top(px(20.)).opacity(amount.clamp(0., 1.)).flex().flex_col().gap(px(9.5))
                .child(button("peek-close", icons::control_sized(Icon::Close, 24.)).on_click(cx.listener(|view, _, _, cx| view.exit_peek(cx))))
                .child(button("peek-expand", expand_icon()).on_click(cx.listener(|view, _, _, cx| view.promote_transient(None, cx))))
                .child(button("peek-split", icons::control(Icon::Split)).on_click(cx.listener(|view, _, _, cx| view.split_peek(cx)))))
            .into_any_element();
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, true); }
        self.render_overlay_shell(content, cx)
    }
}

fn expand_icon() -> AnyElement {
    gpui::canvas(|_, _, _| (), |bounds, _, window, _| {
        for points in [vec![(7.,3.),(3.,3.),(3.,7.)],vec![(3.,3.),(7.,7.)],vec![(11.,15.),(15.,15.),(15.,11.)],vec![(11.,11.),(15.,15.)]] {
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            for (i,(x,y)) in points.into_iter().enumerate() {
                let p = bounds.origin + point(px(x),px(y));
                if i == 0 { path.move_to(p); } else { path.line_to(p); }
            }
            if let Ok(path) = path.build() { window.paint_path(path, window.text_style().color); }
        }
    }).size(px(18.)).into_any_element()
}
