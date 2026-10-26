//! Short-lived notifications above native page views.
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use gpui::{
    div, hsla, point, prelude::*, px, size, AnyElement, Bounds, ClipboardItem, Context,
    FontWeight, TextRun, linear_gradient, linear_color_stop, rgb,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::time::{Duration, Instant};

pub(crate) struct Toast {
    pub message: String,
    pub overlay: Option<WindowHandle<Overlay>>,
    pub(crate) copied_link: bool,
    undo_hint: bool,
    download_undo: bool,
    compact: bool,
    screenshot: bool,
    generation: u64,
    started: Instant,
    dismissed: Option<(Instant, f32)>,
    motion: Option<gpui::Task<()>>,
    action: Option<ToastAction>,
}

#[derive(Clone,Copy)]
enum Style { Regular, Compact, Screenshot }

#[derive(Clone)]
enum ToastAction {
    ShareLinks(Vec<String>),
    Screenshot(std::sync::Arc<Vec<u8>>, browser_core::UserProfileId),
    UndoDownload(crate::max_downloads::Receipt),
    UndoMedia(gpui::WeakEntity<crate::library::media::MediaView>,browser_core::UserProfileId),
    RevealTab(browser_core::TabId, browser_core::UserProfileId),
    RevealFolder(browser_core::folders::FolderId, browser_core::UserProfileId),
    UndoArchive(Vec<browser_core::TabId>, browser_core::UserProfileId),
}

// Arc 1.170.0, measured from arc-copy.mov (variable-frame-rate PTS):
// spring ζ=0.694, ω=15.42; dismissal starts 4.986 s after presentation
// and shrinks/fades about the top center over 200 ms.
pub(crate) const TOAST_LIFETIME: Duration = Duration::from_secs(5);
const ENTER_SETTLED: Duration = Duration::from_millis(650);
const EXIT: Duration = Duration::from_millis(200);
const FRAME: Duration = Duration::from_millis(16);
const PAD_X: f32 = 12.;
const PAD_Y: f32 = 8.;

impl Toast {
    fn progress(&self, now: Instant, reduced: bool) -> (f32, f32, bool) {
        let elapsed = now.saturating_duration_since(self.started);
        let exit = self.dismissed.map(|(at, from)| (now.saturating_duration_since(at), from))
            .or_else(|| (elapsed >= TOAST_LIFETIME).then(|| (elapsed - TOAST_LIFETIME, 1.)));
        if let Some((elapsed, from)) = exit {
            if reduced || elapsed >= EXIT { return (0., 0., true); }
            let t = (elapsed.as_secs_f32() / EXIT.as_secs_f32()).clamp(0., 1.);
            let remaining = 1. - t * t * (3. - 2. * t);
            return (from * remaining, remaining, false);
        }
        if reduced || elapsed >= ENTER_SETTLED { return (1., 1., false); }
        let t = elapsed.as_secs_f32();
        let damping: f32 = 0.694;
        let frequency: f32 = 15.42;
        let damped = (1. - damping * damping).sqrt();
        let scale = 1. - (-damping * frequency * t).exp()
            * ((frequency * damped * t).cos() + damping / damped * (frequency * damped * t).sin());
        (scale.max(0.), scale.clamp(0., 1.), false)
    }
}

impl BrowserWindow {
    pub(crate) fn copy_url_as_quote(&mut self, cx: &mut Context<Self>) {
        self.pending_context_image=None;
        let Some(tab) = self.browser.space().active_tab else { return; };
        let Some(pages) = &self.pages else { return; };
        let identity = pages.borrow().identity_for_tab(tab);
        let Some(identity) = identity else { return; };
        let Some(url) = self.browser.all_tabs().find(|entry| entry.id == tab).map(|entry| entry.url.clone()) else { return; };
        self.quote_request = self.quote_request.wrapping_add(1);
        let request = self.quote_request;
        self.pending_quote = Some((request, identity, url));
        if !pages.borrow_mut().request_quote(tab, request) { self.pending_quote = None; }
    }

    pub(crate) fn complete_quote(&mut self, request: u64, identity: Option<crate::pages::PageIdentity>, text: &str, source_url: &str, cx: &mut Context<Self>) {
        let Some((pending, captured, url)) = self.pending_quote.as_ref() else { return; };
        if *pending != request || identity != Some(*captured) { return; }
        let captured = *captured;
        let same_document = source_url == url;
        let valid = same_document && self.browser.space().active_tab == Some(captured.tab) && self.browser.all_tabs().any(|tab| tab.id == captured.tab && tab.url == *url);
        self.pending_quote = None;
        if !valid { return; }
        if let Some(link) = browser_core::links::quote_link(source_url, text) {
            cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
            self.show_link_toast("Copied Current URL".into(), vec![link], cx);
        }
    }

    pub(crate) fn copy_url(&mut self, markdown: bool, cx: &mut Context<Self>) {
        self.pending_context_image=None;
        if self.copy_sidebar_selection(markdown, cx) { return; }
        let Some(id) = self.browser.space().active_tab else { return; };
        self.copy_tab_url(id, markdown, cx);
    }

    pub(crate) fn copy_tab_url(&mut self, id: browser_core::TabId, markdown: bool, cx: &mut Context<Self>) {
        self.pending_context_image=None;
        let Some(tab) = self.browser.all_tabs().find(|tab| tab.id == id) else { return; };
        let clean = browser_core::links::clean_url(&tab.url);
        let message = if markdown {
            "Copied URL as Markdown"
        } else if clean != tab.url {
            "Copied a clean link without trackers"
        } else {
            "Copied Current URL"
        };
        let share_url = clean.clone();
        let text = if markdown {
            browser_core::links::markdown_link(&tab.title, &tab.url)
        } else {
            clean
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.show_link_toast(message.into(), vec![share_url], cx);
    }

    pub(crate) fn show_link_toast(&mut self, message: String, urls: Vec<String>, cx: &mut Context<Self>) {
        self.show_toast(message, cx);
        if let Some(toast) = &mut self.toast { toast.action = Some(ToastAction::ShareLinks(urls)); }
    }

    pub(crate) fn show_media_trash_undo(&mut self,media:gpui::WeakEntity<crate::library::media::MediaView>,profile:browser_core::UserProfileId,cx:&mut Context<Self>) {
        if self.browser.active_user_profile_id()!=profile {return;}
        self.show_toast("Moved to Trash. ⌘Z to Undo.".into(),cx);
        if let Some(toast)=&mut self.toast {toast.action=Some(ToastAction::UndoMedia(media,profile));}
    }

    pub(crate) fn show_archive_undo(&mut self, title: &str, cx: &mut Context<Self>) {
        if self.browser.is_blank() && self.browser.all_tabs().next().is_none() { return; }
        // Arc ToastController's generic Undo template, ARC_PARITY section 18.
        self.show_toast(format!("“{title}”. ⌘Z to Undo."), cx);
        self.attach_archive_undo();
    }

    pub(crate) fn dismiss_archive_feedback(&mut self, cx: &mut Context<Self>) {
        if self.toast.as_ref().is_some_and(|toast| matches!(toast.action, Some(ToastAction::UndoArchive(..)))) {
            self.retire_toast(cx);
        }
    }

    fn attach_archive_undo(&mut self) {
        if let Some(ids) = self.browser.archive_undo_items() {
            if let Some(toast) = &mut self.toast {
                toast.action = Some(ToastAction::UndoArchive(ids, self.browser.active_user_profile_id()));
            }
        }
    }

    pub(crate) fn show_reveal_toast(&mut self, message: String, tab: browser_core::TabId, cx: &mut Context<Self>) {
        let Some(profile) = self.browser.user_profile_id_for_tab(tab) else { return; };
        self.show_toast(message, cx);
        if let Some(toast) = &mut self.toast { toast.action = Some(ToastAction::RevealTab(tab, profile)); }
    }

    pub(crate) fn show_folder_restore(&mut self, folder: browser_core::folders::FolderId, cx: &mut Context<Self>) {
        let Some(space)=self.browser.folder_space(folder).and_then(|id|self.browser.spaces.iter().find(|s|s.id==id)) else{return;};
        let (profile,name)=(space.user_profile_id,space.name.clone());
        self.show_toast(format!("Folder restored to {name}. Click to view."),cx);
        if let Some(toast)=&mut self.toast {toast.action=Some(ToastAction::RevealFolder(folder,profile));}
    }

    pub(crate) fn show_background_tab(&mut self, tab: browser_core::TabId, cx: &mut Context<Self>) {
        if self.sidebar_visible { return; }
        if let Some(name) = self.browser.spaces.iter().find(|space| space.pinned.iter().chain(&space.today).any(|entry| entry.id == tab)).map(|space| space.name.clone()) {
            self.show_reveal_toast(format!("New Tab Created in {name}"), tab, cx);
        }
    }

    pub(crate) fn show_tab_move(&mut self, tab: browser_core::TabId, cx: &mut Context<Self>) {
        self.show_reveal_toast("Tab moved! Click to go there.".into(), tab, cx);
    }

    pub(crate) fn show_folder_pin(&mut self, folder: browser_core::folders::FolderId, cx: &mut Context<Self>) {
        if let Some(name) = self.browser.folder(folder).map(|folder| folder.name.clone()) {
            self.show_toast(format!("Pinned to {name}"), cx);
        }
    }

    fn activate_toast_action(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some(toast) = self.toast.as_ref().filter(|toast| toast.generation == generation && toast.dismissed.is_none()) else { return; };
        let Some(action) = toast.action.clone() else { return; };
        let urls = match action {
            ToastAction::Screenshot(png,profile) => {
                if profile!=self.browser.active_user_profile_id() {return;}
                let main=self.main_window;
                cx.spawn(async move |owner,cx| {
                    let Ok((view,width))=main.update(cx,|_,window,_|(crate::native::view_of(window),f32::from(window.viewport_size().width))) else{return;};
                    let mut picker=crate::native::sharing::show_image(view,[width-48.,14.,32.,28.],&png);
                    if std::env::var_os("BROWSER_SCRIPT").is_some(){eprintln!("[screenshot-share] presented={} png_bytes={}",picker.is_some(),png.len());}
                    let previous=owner.update(cx,|this,_|std::mem::replace(&mut this.share_picker,picker.take())).ok().flatten();
                    drop(previous);drop(picker);
                }).detach();
                return;
            }
            ToastAction::ShareLinks(urls) => urls,
            ToastAction::UndoMedia(media,profile)=>{if self.browser.active_user_profile_id()==profile {let _=media.update(cx,|media,cx|media.undo_trash(profile,cx));}return;},
            ToastAction::UndoDownload(receipt) => {
                if receipt.belongs_to(&self.profile.handle, self.browser.active_user_profile_id()) {
                    crate::max_downloads::undo(receipt, cx);
                }
                return;
            }
            ToastAction::RevealFolder(folder, profile) => {
                if profile!=self.browser.active_user_profile_id() || self.browser.folder_space(folder).and_then(|id|self.browser.spaces.iter().find(|s|s.id==id)).is_none_or(|s|s.user_profile_id!=profile) {return;}
                self.sidebar_visible=true;
                self.expand_folder(folder,cx);
                self.settle_pager(cx);
                self.retire_toast(cx);
                cx.notify();
                return;
            }
            ToastAction::RevealTab(tab, profile) => {
                if profile != self.browser.active_user_profile_id() || self.browser.user_profile_id_for_tab(tab) != Some(profile) { return; }
                if let Some(index) = self.browser.spaces.iter().position(|space| space.pinned.iter().chain(&space.today).any(|entry| entry.id == tab)) {
                    self.switch_space(index, cx);
                    self.edit_browser(|browser| browser.activate_sidebar_item(tab, std::time::SystemTime::now()));
                    self.settle_pager(cx);
                    self.retire_toast(cx);
                    cx.notify();
                }
                return;
            }
            ToastAction::UndoArchive(items, profile) => {
                if profile == self.browser.active_user_profile_id() && self.edit_browser(|browser| browser.undo_archive_items(&items, std::time::SystemTime::now())) {
                    self.settle_pager(cx);
                    self.retire_toast(cx);
                    cx.notify();
                }
                return;
            }
        };
        let main = self.main_window;
        // Native menu presentation enters AppKit's event loop. End GPUI
        // application, entity and window borrows before native tracking.
        cx.spawn(async move |owner, cx| {
            let Ok((view, width)) = main.update(cx, |_, window, _| (crate::native::view_of(window), f32::from(window.viewport_size().width))) else { return; };
            let mut picker = crate::native::sharing::show(view, [width - 60., 29., 32., 28.], &urls);
            let previous = owner.update(cx, |this, _| std::mem::replace(&mut this.share_picker, picker.take())).ok().flatten();
            drop(previous);
            drop(picker);
        }).detach();
    }

    pub(crate) fn show_toast(&mut self, message: String, cx: &mut Context<Self>) {
        self.present_toast(message, None, cx);
    }

    pub(crate) fn show_download_renamed(&mut self, receipt: crate::max_downloads::Receipt, cx: &mut Context<Self>) {
        self.present_toast("Download renamed!".into(), Some(receipt), cx);
    }

    fn present_toast(&mut self, message: String, receipt: Option<crate::max_downloads::Receipt>, cx: &mut Context<Self>) {
        self.present_toast_style(message, receipt, Style::Regular, cx);
    }

    pub(crate) fn show_screenshot(&mut self, png: std::sync::Arc<Vec<u8>>, profile: browser_core::UserProfileId, cx: &mut Context<Self>) {
        self.present_toast_style("Screenshot Copied to Clipboard".into(),None,Style::Screenshot,cx);
        if let Some(toast)=&mut self.toast {toast.action=Some(ToastAction::Screenshot(png,profile));}
    }
    fn save_screenshot_toast(&mut self, generation: u64, cx: &mut Context<Self>) {
        if self.save_prompt_active || crate::quit::is_quitting() {return;}
        let Some(toast)=self.toast.as_ref().filter(|t|t.generation==generation && t.dismissed.is_none()) else{return;};
        let Some(ToastAction::Screenshot(png,profile))=toast.action.clone() else{return;};
        if profile!=self.browser.active_user_profile_id() {return;}
        self.close_popups_for_window_operation(false,cx);
        self.save_prompt_active=true;
        self.retire_toast(cx);
        let owner=cx.weak_entity();let main=self.main_window;
        cx.defer(move |cx| {
            let _=main.update(cx,|_,window,_|window.activate_window());
            let directory=dirs::download_dir().unwrap_or_else(crate::profile_dir);
            let name=format!("{}.png",chrono::Local::now().format("%Y-%m-%d-%H%M%S"));
            let choice=cx.prompt_for_new_path(&directory,Some(&name));
            cx.spawn(async move |cx| {
                let chosen=choice.await;
                let _=owner.update(cx,|view,cx| {view.save_prompt_active=false;cx.notify();});
                if let Ok(Ok(Some(path)))=chosen {let _=std::fs::write(path,png.as_ref());}
            }).detach();
        });
    }

    pub(crate) fn show_compact_toast(&mut self, message: String, cx: &mut Context<Self>) {
        self.present_toast_style(message, None, Style::Compact, cx);
    }

    fn present_toast_style(&mut self, message: String, receipt: Option<crate::max_downloads::Receipt>, style: Style, cx: &mut Context<Self>) {
        let compact=matches!(style,Style::Compact);
        let screenshot=matches!(style,Style::Screenshot);
        let download_undo = receipt.is_some();
        self.retire_toast(cx);
        self.toast_generation += 1;
        let generation = self.toast_generation;
        self.toast = Some(Toast {
            copied_link: !compact && message.starts_with("Copied"),
            compact,
            screenshot,
            undo_hint: message.contains("⌘Z"),
            download_undo,
            message,
            overlay: None,
            generation,
            started: cx.background_executor().now(),
            dismissed: None,
            motion: None,
            action: receipt.map(ToastAction::UndoDownload),
        });
        if self.toast.as_ref().is_some_and(|toast| toast.undo_hint) { self.attach_archive_undo(); }
        let main = self.main_window;
        let browser = cx.entity().downgrade();
        let parent = self.native_view;
        let message = self.toast.as_ref().unwrap().message.clone();
        let copied_link = self.toast.as_ref().unwrap().copied_link || download_undo;
        let undo_hint = self.toast.as_ref().unwrap().undo_hint;
        cx.defer(move |cx| {
            if !browser
                .read_with(cx, |this, _| {
                    this.toast.is_some() && this.toast_generation == generation
                })
                .unwrap_or(false)
            {
                return;
            }
            let Ok((main, text_width)) = main.update(cx, |_, window, _| {
                let mut font = gpui::font(".SystemUIFont");
                font.weight = FontWeight::SEMIBOLD;
                let width = window.text_system().shape_line(message.clone().into(), px(if compact { 13. } else { 14. }), &[TextRun {
                    len: message.len(), font, color: gpui::white(),
                    background_color: None, underline: None, strikethrough: None,
                }], None).width;
                (window.bounds(), width)
            }) else {
                return;
            };
            // Arc's copy toast is content-sized (203 × 48 for the English
            // current-URL message), 16 px from the right and 19 px from the top.
            let width = (text_width.ceil() + px(if screenshot {120.} else if copied_link { 68. } else if undo_hint { 41. } else { 24. })).min(main.size.width - px(32.));
            let bounds = Bounds::new(
                point(
                    main.origin.x + main.size.width - width - px(if compact { 19. } else if copied_link { 16. } else { 4. }) - px(PAD_X),
                    main.origin.y + px(if compact || copied_link { 19. } else { 4. }) - px(PAD_Y),
                ),
                size(width + px(PAD_X * 2.), px(if compact { 40. } else if copied_link || undo_hint || screenshot { 48. } else { 42. }) + px(PAD_Y * 2.)),
            );
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                focus: false,
                kind: WindowKind::Normal,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                    crate::native::attach_overlay(parent, child, false);
                    if !(screenshot || copied_link || undo_hint || message.contains("Click to") || message.starts_with("New Tab Created")) { crate::native::set_overlay_passthrough(child); }
                    crate::native::prepare_toast_window(child);
                }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::Toast(generation), window, cx))
            }) {
                let retained = browser.update(cx, |this, _| {
                    if this.toast_generation != generation { return false; }
                    if let Some(toast) = &mut this.toast {
                        toast.overlay = Some(overlay);
                        return true;
                    }
                    false
                }).unwrap_or(false);
                if !retained {
                    let _ = overlay.update(cx, |_, window, _| window.remove_window());
                }
            }
        });
        self.drive_toast(generation, cx);
        cx.notify();
    }

    fn toast_by_generation(&self, generation: u64) -> Option<&Toast> {
        self.toast.as_ref().filter(|toast| toast.generation == generation)
            .or_else(|| self.retiring_toasts.iter().find(|toast| toast.generation == generation))
    }

    fn drive_toast(&mut self, generation: u64, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| loop {
            let now = cx.background_executor().now();
            let next = this.update(cx, |this, cx| {
                let toast = this.toast_by_generation(generation)?;
                let reduced = crate::native::prefers_reduced_motion();
                let elapsed = now.saturating_duration_since(toast.started);
                if toast.progress(now, reduced).2 {
                    this.remove_toast(generation, cx);
                    return None;
                }
                let next = if toast.dismissed.is_some() || elapsed < ENTER_SETTLED || elapsed >= TOAST_LIFETIME {
                    FRAME
                } else { TOAST_LIFETIME - elapsed };
                Some((next, toast.overlay))
            }).ok().flatten();
            let Some((next, overlay)) = next else { break; };
            if let Some(overlay) = overlay {
                let _ = overlay.update(cx, |_, _, cx| cx.notify());
            }
            cx.background_executor().timer(next).await;
        });
        if let Some(toast) = self.toast.as_mut().filter(|toast| toast.generation == generation) {
            toast.motion = Some(task);
        } else if let Some(toast) = self.retiring_toasts.iter_mut().find(|toast| toast.generation == generation) {
            toast.motion = Some(task);
        }
    }

    pub(crate) fn retire_toast(&mut self, cx: &mut Context<Self>) {
        let Some(mut toast) = self.toast.take() else { return; };
        let now = cx.background_executor().now();
        let from = toast.progress(now, crate::native::prefers_reduced_motion()).0;
        toast.motion.take();
        toast.dismissed = Some((now, from));
        let generation = toast.generation;
        self.retiring_toasts.push(toast);
        self.drive_toast(generation, cx);
    }

    fn remove_toast(&mut self, generation: u64, cx: &mut Context<Self>) {
        let toast = if self.toast.as_ref().is_some_and(|toast| toast.generation == generation) {
            self.toast.take()
        } else {
            self.retiring_toasts.iter().position(|toast| toast.generation == generation)
                .map(|index| self.retiring_toasts.remove(index))
        };
        if let Some(overlay) = toast.and_then(|toast| toast.overlay) {
            cx.defer(move |cx| { let _ = overlay.update(cx, |_, window, _| window.remove_window()); });
        }
        cx.notify();
    }

    pub(crate) fn render_toast(&self, generation: u64, width: f32, now: Instant, owner: gpui::WeakEntity<Self>) -> AnyElement {
        let Some(toast) = self.toast_by_generation(generation) else {
            return div().into_any_element();
        };
        let message = toast.message.clone();
        let screenshot=toast.screenshot;
        let copied_link = toast.copied_link || toast.download_undo;
        let download_undo = toast.download_undo;
        let compact = toast.compact;
        let undo_hint = toast.undo_hint;
        let (scale, opacity, _) = toast.progress(now, crate::native::prefers_reduced_motion());
        if scale < 0.001 { return div().size_full().into_any_element(); }
        let actionable = toast.action.is_some();
        let body_owner = owner.clone();
        let save_owner=owner.clone();
        let (undo_before, undo_after) = message.split_once("⌘Z").unwrap_or(("", ""));
        let surface = div()
            .id("toast")
            .when(actionable && !copied_link && !screenshot, |this| this.cursor_pointer().on_click(move |_, _, cx| {
                let _ = body_owner.update(cx, |this, cx| this.activate_toast_action(generation, cx));
            }))
            .debug_selector(|| "toast".into())
            .w(px((width - PAD_X * 2.) * scale))
            .h(px(if compact { 40. } else if copied_link || undo_hint || screenshot { 48. } else { 42. } * scale))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8. * scale))
            .px(px(12. * scale))
            .rounded(px(12. * scale))
            .border_1()
            .border_color(hsla(0.76, 0.5, 0.4, 0.25))
            .bg(linear_gradient(180.,
                linear_color_stop(rgb(if compact { 0x402a69 } else { 0x28154e }), 0.),
                linear_color_stop(rgb(if compact { 0x301e4e } else { 0x201140 }), 1.)))
            .text_color(gpui::white())
            .font_family(".SystemUIFont")
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(if compact { 13. } else { 14. } * scale))
            .line_height(px(18. * scale))
            .when(!undo_hint && message!=crate::page_fullscreen::INSTRUCTION, |this| this.child(div().flex_1().min_w_0().truncate().child(message.clone())))
            .when(message==crate::page_fullscreen::INSTRUCTION, |this| this.child(div().flex().items_center()
                .child("To exit full screen, press ").child(crate::icons::escape(14.*scale)).child(".")))
            .when(undo_hint, |this| this.child(div().flex_1().min_w_0().flex().items_center().gap(px(4. * scale))
                .child(div().flex_1().min_w_0().truncate().child(undo_before.trim_end().to_string()))
                .child(div().debug_selector(|| "toast-undo-shortcut".into()).px(px(6. * scale)).h(px(28. * scale))
                    .flex().items_center().rounded(px(4. * scale)).bg(hsla(0., 0., 1., 0.1)).child("⌘Z"))
                .child(undo_after.trim_start().to_string())))
            .when(screenshot,|this|this.child(div().id("toast-screenshot-save").debug_selector(||"toast-screenshot-save".into())
                .cursor_pointer().w(px(48.*scale)).h(px(28.*scale)).flex_none().rounded(px(6.*scale))
                .bg(hsla(0.,0.,1.,0.2)).flex().items_center().justify_center().child("Save")
                .on_click(move |_,_,cx| {let _=save_owner.update(cx,|this,cx|this.save_screenshot_toast(generation,cx));})))
            .when(copied_link || screenshot, |this| this.child(
                div().id("toast-share").debug_selector(move || if download_undo { "toast-download-undo".into() } else if screenshot {"toast-screenshot-share".into()} else { "toast-share".into() }).cursor_pointer()
                    .on_click(move |_, _, cx| { let _ = owner.update(cx, |this, cx| this.activate_toast_action(generation, cx)); })
                    .w(px(32. * scale)).h(px(28. * scale)).flex_none().rounded(px(6. * scale))
                    .bg(hsla(0., 0., 1., 0.2)).flex().items_center().justify_center()
                    .child(crate::icons::control_sized(if download_undo { crate::icons::Icon::Restore } else { crate::icons::Icon::Share }, 18. * scale))));
        div().size_full().flex().items_start().justify_center().pt(px(PAD_Y))
            .opacity(opacity).child(surface).into_any_element()
    }

    /// Arc confirms pinning only while its destination is hidden.
    pub(crate) fn toggle_tab_pin(&mut self, id: browser_core::TabId, cx: &mut Context<Self>) {
        if self.browser.is_private() || self.browser.is_blank() { return; }
        let before = self.browser.all_tabs().find(|tab| tab.id == id).map(|tab| tab.pinned_url.is_some());
        let Some(before) = before else { return; };
        self.edit_browser(|browser| browser.toggle_pin(id));
        let changed = self.browser.all_tabs().any(|tab| tab.id == id && tab.pinned_url.is_some() != before);
        if changed && !self.sidebar_visible {
            let space = &self.browser.space().name;
            self.show_toast(format!("{} Tab {} “{space}”", if before { "Un-Pinned" } else { "Pinned" }, if before { "from" } else { "to" }), cx);
        }
        cx.notify();
    }
}
