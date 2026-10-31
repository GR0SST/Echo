//! Hovering a folder row for half a second previews its pages beside the
//! sidebar: a search field over every page in the folder and its subfolders,
//! flattened and ordered by when each was last opened.
use crate::text_input::{Changed, TextInput};
use crate::window::{BrowserWindow, Overlay, OverlayKind};
use browser_core::{folders::FolderId, TabId};
use gpui::{
    actions, div, hsla, point, prelude::*, px, size, AnyElement, App, Bounds, Context, Entity,
    Focusable, KeyBinding, Pixels, Subscription, Task, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions,
};
use std::time::{Duration, SystemTime};

/// Hover dwell before the preview appears.
pub(crate) const DELAY: Duration = Duration::from_millis(500);
/// Leaving the row and card for this long closes the preview.
const GRACE: Duration = Duration::from_millis(200);
const POLL: Duration = Duration::from_millis(40);
const WIDTH: f32 = 250.;
const SEARCH_HEIGHT: f32 = 44.;
const ROW_HEIGHT: f32 = 44.;
const PADDING: f32 = 6.;
const MAX_ROWS: usize = 8;
/// Gap between the sidebar edge and the card.
const GAP: f32 = 8.;
/// Transparent margin around the card for its shadow.
const MARGIN: f32 = 12.;
const CONTEXT: &str = "FolderPreview";
actions!(folder_preview, [Dismiss, Next, Previous, Open]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
        KeyBinding::new("down", Next, Some(CONTEXT)),
        KeyBinding::new("up", Previous, Some(CONTEXT)),
        KeyBinding::new("enter", Open, Some(CONTEXT)),
    ]);
}

pub(crate) struct FolderPreview {
    pub(crate) folder: FolderId,
    /// The hovered row, in window coordinates.
    row: Bounds<Pixels>,
    /// The card, in window coordinates, once shown.
    card: Option<Bounds<Pixels>>,
    pub(crate) overlay: Option<WindowHandle<Overlay>>,
    input: Entity<TextInput>,
    selected: usize,
    _poll: Task<()>,
    _changes: Subscription,
}

impl FolderPreview {
    pub(crate) fn query(&self, cx: &App) -> String { self.input.read(cx).text().to_owned() }
}

/// "Just now", "31 minutes ago", "2 days ago".
pub(crate) fn ago(when: SystemTime, now: SystemTime) -> String {
    let seconds = now.duration_since(when).unwrap_or_default().as_secs();
    let unit = |count: u64, name: &str| if count == 1 { format!("1 {name} ago") } else { format!("{count} {name}s ago") };
    match seconds {
        0..60 => "Just now".into(),
        60..3600 => unit(seconds / 60, "minute"),
        3600..86_400 => unit(seconds / 3600, "hour"),
        86_400..604_800 => unit(seconds / 86_400, "day"),
        604_800..2_592_000 => unit(seconds / 604_800, "week"),
        2_592_000..31_536_000 => unit(seconds / 2_592_000, "month"),
        _ => unit(seconds / 31_536_000, "year"),
    }
}

impl BrowserWindow {
    /// Every page in the folder and its subfolders, newest first, filtered by
    /// the query against title and address.
    pub(crate) fn folder_preview_items(&self, folder: FolderId, query: &str) -> Vec<TabId> {
        let query = query.trim().to_lowercase();
        let mut tabs: Vec<_> = self.browser.folder_tabs(folder).into_iter()
            .filter(|id| !self.browser.is_split_secondary(*id))
            .filter_map(|id| self.browser.all_tabs().find(|tab| tab.id == id))
            .filter(|tab| query.is_empty()
                || self.browser.item_title(tab.id).to_lowercase().contains(&query)
                || tab.url.to_lowercase().contains(&query))
            .map(|tab| (tab.last_active, tab.id))
            .collect();
        tabs.sort_by(|a, b| b.0.cmp(&a.0));
        tabs.into_iter().map(|(_, id)| id).collect()
    }

    fn folder_preview_blocked(&self, cx: &App) -> bool {
        cx.has_active_drag() || self.tab_menu.is_some() || self.command_bar.is_some()
            || self.folder_rename.is_some() || self.sidebar_resizing() || self.new_menu.is_some()
    }

    /// Starts the hover dwell for a folder row; the preview opens only if the
    /// pointer is still over the row when it ends.
    pub(crate) fn hover_folder(&mut self, folder: FolderId, cx: &mut Context<Self>) {
        if self.folder_preview.as_ref().is_some_and(|preview| preview.folder == folder) { return; }
        if self.folder_preview_blocked(cx) { return; }
        // An open folder already shows its pages.
        if !self.browser.folder(folder).is_some_and(|folder| folder.collapsed) { return; }
        let Some(icon) = self.icon_anchors.borrow().get(&crate::icons::IconTarget::Folder(folder)).copied() else { return; };
        self.close_folder_preview(cx);
        let edge = self.sidebar_edge();
        let row = Bounds::new(point(px(0.), icon.center().y - px(crate::window::SIDEBAR_ROW_HEIGHT / 2.)),
            size(px(edge), px(crate::window::SIDEBAR_ROW_HEIGHT)));
        let name = self.browser.folder(folder).map(|folder| folder.name.clone()).unwrap_or_default();
        let input = cx.new(|cx| TextInput::new(format!("Search {name}…"), cx));
        let changes = cx.subscribe(&input, |this, _, _: &Changed, cx| {
            if let Some(preview) = &mut this.folder_preview { preview.selected = 0; }
            this.resize_folder_preview(cx);
            cx.notify();
        });
        let started = cx.background_executor().now();
        let poll = cx.spawn(async move |this, cx| {
            let mut outside_since: Option<std::time::Instant> = None;
            loop {
                cx.background_executor().timer(POLL).await;
                let keep = this.update(cx, |this, cx| {
                    let Some(preview) = &this.folder_preview else { return false; };
                    if preview.folder != folder { return false; }
                    // Pages are native views that GPUI never sees the pointer over.
                    let pointer = this.native_view.and_then(crate::native::pointer_position)
                        .map(|[x, y]| point(px(x), px(y))).or(this.last_pointer);
                    let now = cx.background_executor().now();
                    let on_row = pointer.is_some_and(|p| preview.row.contains(&p));
                    let Some(card) = preview.card else {
                        if !on_row || this.folder_preview_blocked(cx) { this.close_folder_preview(cx); return false; }
                        if now.duration_since(started) >= DELAY { this.show_folder_preview(cx); }
                        return true;
                    };
                    // The row, the card and the gap between them keep it open.
                    let bridge = Bounds::new(point(preview.row.right(), preview.row.top()),
                        size(card.left() - preview.row.right(), preview.row.size.height));
                    let inside = pointer.is_some_and(|p| on_row || card.dilate(px(4.)).contains(&p) || bridge.contains(&p));
                    let typing = !preview.input.read(cx).text().is_empty();
                    if inside || typing { outside_since = None; return true; }
                    let since = *outside_since.get_or_insert(now);
                    if now.duration_since(since) >= GRACE { this.close_folder_preview(cx); return false; }
                    true
                }).unwrap_or(false);
                if !keep { break; }
            }
        });
        self.folder_preview = Some(FolderPreview { folder, row, card: None, overlay: None, input, selected: 0, _poll: poll, _changes: changes });
    }

    /// Right edge of the visible sidebar, pinned or floating.
    fn sidebar_edge(&self) -> f32 {
        if self.sidebar_visible { self.sidebar_width() * self.sidebar_amount() }
        else { crate::window::FLOATING_SIDEBAR_INSET + self.sidebar_width() }
    }

    fn folder_preview_card_height(&self) -> f32 {
        let Some(preview) = &self.folder_preview else { return 0.; };
        let rows = self.folder_preview_items(preview.folder, "").len().clamp(1, MAX_ROWS);
        SEARCH_HEIGHT + 1. + PADDING * 2. + rows as f32 * ROW_HEIGHT
    }

    fn show_folder_preview(&mut self, cx: &mut Context<Self>) {
        let height = self.folder_preview_card_height();
        let edge = self.sidebar_edge();
        let Some(preview) = &mut self.folder_preview else { return; };
        if preview.card.is_some() { return; }
        // As in the reference, the first page lines up with the folder row and
        // the search field sits above it.
        let top = preview.row.center().y - px(SEARCH_HEIGHT + 1. + PADDING + ROW_HEIGHT / 2.);
        let card = Bounds::new(point(px(edge + GAP), top), size(px(WIDTH), px(height)));
        preview.card = Some(card);
        let main = self.main_window;
        let parent = self.native_view;
        let browser = cx.entity().downgrade();
        let folder = preview.folder;
        cx.notify();
        cx.defer(move |cx| {
            let wanted = browser.read_with(cx, |this, _| this.folder_preview.as_ref().is_some_and(|p| p.folder == folder && p.overlay.is_none())).unwrap_or(false);
            if !wanted { return; }
            let Ok(window) = main.update(cx, |_, window, _| window.bounds()) else { return; };
            // Keep the card inside the window near its top and bottom edges.
            let mut card = card;
            card.origin.y = card.origin.y.min(window.size.height - card.size.height - px(8.)).max(px(8.));
            let _ = browser.update(cx, |this, _| { if let Some(preview) = &mut this.folder_preview { preview.card = Some(card); } });
            let origin = window.origin + card.origin - point(px(MARGIN), px(MARGIN));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(origin, size(card.size.width + px(2. * MARGIN), card.size.height + px(2. * MARGIN))))),
                titlebar: None, focus: false, kind: WindowKind::Normal, is_movable: false, is_resizable: false,
                is_minimizable: false, window_background: WindowBackgroundAppearance::Transparent, ..Default::default()
            };
            if let Ok(overlay) = cx.open_window(options, |window, cx| {
                if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) { crate::native::attach_overlay(parent, child, false); }
                cx.new(|cx| Overlay::new(browser.clone(), OverlayKind::FolderPreview, window, cx))
            }) {
                let keep = browser.update(cx, |this, _| match &mut this.folder_preview {
                    Some(preview) if preview.folder == folder => { preview.overlay = Some(overlay); true }
                    _ => false,
                }).unwrap_or(false);
                if !keep { let _ = overlay.update(cx, |_, window, _| window.remove_window()); }
            }
        });
    }

    /// Search results change the card's height; its window follows.
    fn resize_folder_preview(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = &self.folder_preview else { return; };
        let count = self.folder_preview_items(preview.folder, preview.input.read(cx).text()).len().clamp(1, MAX_ROWS);
        let height = SEARCH_HEIGHT + 1. + PADDING * 2. + count as f32 * ROW_HEIGHT;
        let Some(preview) = &mut self.folder_preview else { return; };
        let Some(card) = &mut preview.card else { return; };
        card.size.height = px(height);
        if let Some(overlay) = preview.overlay {
            let full = size(card.size.width + px(2. * MARGIN), px(height + 2. * MARGIN));
            cx.defer(move |cx| { let _ = overlay.update(cx, |_, window, _| window.resize(full)); });
        }
    }

    pub(crate) fn close_folder_preview(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.folder_preview.take() else { return; };
        let focused = preview.overlay.is_some() && !preview.input.read(cx).text().is_empty();
        self.close_popup_overlay(preview.overlay, if focused { crate::window::PopupFocus::Restore } else { crate::window::PopupFocus::OnActivation }, false, cx);
        cx.notify();
    }

    fn open_folder_preview_item(&mut self, id: TabId, cx: &mut Context<Self>) {
        self.close_folder_preview(cx);
        if let Some(pages) = &self.pages { pages.borrow_mut().request_page_focus(); }
        self.edit_browser(|browser| browser.activate_sidebar_item(id, SystemTime::now()));
        cx.notify();
    }

    fn move_folder_preview_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(preview) = &self.folder_preview else { return; };
        let count = self.folder_preview_items(preview.folder, preview.input.read(cx).text()).len().min(MAX_ROWS) as isize;
        if count == 0 { return; }
        if let Some(preview) = &mut self.folder_preview {
            preview.selected = (preview.selected as isize + delta).rem_euclid(count) as usize;
        }
        cx.notify();
    }

    pub(crate) fn render_folder_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(preview) = &self.folder_preview else { return div().into_any_element(); };
        let dark = self.theme_is_dark();
        let ink = if dark { 1. } else { 0. };
        let fill = gpui::rgb(if dark { 0x1e1e21 } else { 0xfdfdff });
        let muted = hsla(0., 0., ink, 0.5);
        let query = preview.input.read(cx).text().to_owned();
        let items = self.folder_preview_items(preview.folder, &query);
        let now = SystemTime::now();
        let hue = self.accent();
        let rows: Vec<AnyElement> = items.iter().take(MAX_ROWS).enumerate().filter_map(|(index, id)| {
            let tab = self.browser.all_tabs().find(|tab| tab.id == *id)?;
            let id = *id;
            Some(div().id(("folder-preview-item", id.get() as usize)).debug_selector(move || format!("folder-preview-item-{}", id.get()))
                .h(px(ROW_HEIGHT)).px(px(8.)).flex().items_center().gap(px(10.)).rounded(px(8.)).flex_none()
                .when(preview.selected == index, |this| this.bg(hsla(0., 0., ink, 0.07)))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered { if let Some(preview) = &mut this.folder_preview { preview.selected = index; } cx.notify(); }
                }))
                .on_click(cx.listener(move |this, _, _, cx| this.open_folder_preview_item(id, cx)))
                .child(self.render_favicon_on(tab, hue, dark))
                .child(div().flex_1().min_w_0().flex().flex_col()
                    .child(div().text_size(px(13.)).font_weight(gpui::FontWeight::MEDIUM).truncate().child(self.browser.item_title(id)))
                    .child(div().text_size(px(11.)).text_color(muted).truncate().child(ago(tab.last_active, now))))
                .into_any_element())
        }).collect();
        let empty = rows.is_empty();
        div().id("folder-preview-root").size_full().p(px(MARGIN))
            .key_context(CONTEXT)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close_folder_preview(cx)))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.move_folder_preview_selection(1, cx)))
            .on_action(cx.listener(|this, _: &Previous, _, cx| this.move_folder_preview_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &Open, _, cx| {
                let Some(preview) = &this.folder_preview else { return; };
                let items = this.folder_preview_items(preview.folder, preview.input.read(cx).text());
                if let Some(id) = items.get(preview.selected).copied() { this.open_folder_preview_item(id, cx); }
            }))
            .child(div().id("folder-preview").debug_selector(|| "folder-preview".into())
                .size_full().flex().flex_col().rounded(px(12.)).bg(fill).border_1().border_color(hsla(0., 0., ink, 0.1)).shadow_lg()
                .text_color(hsla(0., 0., ink, 0.95)).font_family(".SystemUIFont").overflow_hidden()
                .child(div().id("folder-preview-search").debug_selector(|| "folder-preview-search".into())
                    .h(px(SEARCH_HEIGHT)).flex_none().px(px(14.)).flex().items_center().gap(px(10.)).text_size(px(13.))
                    .on_click({
                        let focus = preview.input.read(cx).focus_handle(cx);
                        move |_, window, _| { window.activate_window(); focus.focus(window); }
                    })
                    .child(div().text_color(muted).child(crate::icons::ionicon("search", 14., 1.)))
                    .child(div().flex_1().min_w_0().child(preview.input.clone())))
                .child(div().h(px(1.)).flex_none().bg(hsla(0., 0., ink, 0.1)))
                .child(div().flex().flex_col().p(px(PADDING)).children(rows)
                    .when(empty, |this| this.child(div().h(px(ROW_HEIGHT)).px(px(8.)).flex().items_center()
                        .text_size(px(13.)).text_color(muted).child("No matching pages")))))
            .into_any_element()
    }
}
