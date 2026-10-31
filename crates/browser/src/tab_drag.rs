//! Sidebar drags carry a logical item, including complete Split Views.
use crate::window::{BrowserWindow, Target};
use browser_core::{folders::FolderId, Section, SpaceId, Tab, TabId};
use gpui::{div, hsla, prelude::*, px, Context, Window};
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Clone)]
pub(crate) struct TabDrag {
    pub id: TabId,
    pub(crate) card_hotspot: std::rc::Rc<std::cell::Cell<gpui::Point<gpui::Pixels>>>,
    source: gpui::AnyWindowHandle,
    workspace: crate::profile::ProfileHandle,
    profile: browser_core::UserProfileId,
    blank: bool,
    focus: TabId,
    title: String,
    icon: Option<String>,
    favicon: Option<Vec<u8>>,
    split: bool,
    ink: gpui::Hsla,
    dark: bool,
    fill: gpui::Hsla,
    row: std::rc::Rc<std::cell::Cell<gpui::Size<gpui::Pixels>>>,
    pub(crate) reorder_target: std::rc::Rc<std::cell::Cell<Option<(TabId,bool)>>>,
}

impl TabDrag {
    pub(crate) fn is_split(&self) -> bool { self.split }
    /// The page shown for this item: the focused pane of a Split View.
    pub(crate) fn focus_page(&self) -> TabId { self.focus }

    pub(crate) fn start_at(&self, owner: gpui::WeakEntity<BrowserWindow>, offset: gpui::Point<gpui::Pixels>, width: f32, height: f32, cx: &mut gpui::App) -> gpui::Entity<Self> {
        self.row.set(gpui::size(px(width), px(height)));
        self.card_hotspot.set(gpui::point(px((f32::from(offset.x) / width).clamp(0.,1.) * 128.), px((f32::from(offset.y) / height).clamp(0.,1.) * 250. - 5.)));
        self.start(owner,cx)
    }
    pub(crate) fn start(
        &self,
        owner: gpui::WeakEntity<BrowserWindow>,
        cx: &mut gpui::App,
    ) -> gpui::Entity<Self> {
        let _ = owner.update(cx, |this, cx| {
            this.tab_drag_active = true;
            cx.notify();
        });
        cx.new(|cx| {
            // Tie restoration of a dismissed card to this typed drag's lifetime, including
            // cancellations and drops outside the sidebar.
            cx.on_release(move |drag: &mut TabDrag, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.tab_drag_active = false;
                    this.finish_tear_off(drag, cx);
                    this.close_split_drop(cx);
                    cx.notify();
                });
            })
            .detach();
            let mut started=self.clone();
            started.reorder_target=std::rc::Rc::new(std::cell::Cell::new(None));
            started
        })
    }
}

impl BrowserWindow {
    /// Both halves beside a gap identify the same insertion boundary.
    fn reorder_boundary(&self, target: (TabId, bool)) -> (TabId, bool) {
        let (tab, after) = target;
        let tab = self.browser.item_anchor(tab);
        if !after { return (tab, false); }
        for space in &self.browser.spaces {
            if space.today.iter().any(|entry| entry.id == tab) {
                let rows = self.browser.today_rows(space.id);
                return rows.iter().position(|row| matches!(row, browser_core::max::TodayRow::Tab(id) if *id == tab))
                    .and_then(|index| rows.get(index + 1))
                    .and_then(|row| match row { browser_core::max::TodayRow::Tab(id) => Some(*id), _ => None })
                    .map_or((tab, true), |next| (next, false));
            }
            if space.pinned.iter().any(|entry| entry.id == tab) {
                let parent = self.browser.folder_ancestors(tab).last().copied();
                let items = parent.and_then(|id| self.browser.folder(id)).map_or(&space.pinned_items, |folder| &folder.items);
                // A folder header is its own drop target; never merge across it.
                let item = self.browser.pinned_item(tab);
                return items.iter().position(|entry| *entry == item)
                    .and_then(|index| items.get(index + 1))
                    .and_then(|item| self.browser.pinned_item_tab(space.id, *item))
                    .map_or((tab, true), |next| (next, false));
            }
        }
        (tab, after)
    }

    pub(crate) fn reorder_feedback(&self, drag: &TabDrag, target: Option<(TabId, bool)>, cx: &gpui::App) {
        if !crate::feedback::enabled(self, browser_core::settings::Toggle::HapticFeedback, cx) {
            drag.reorder_target.set(None);
            return;
        }
        let target = target.filter(|(tab, _)| self.accepts_tab_drag(drag)
            && self.browser.user_profile_id_for_tab(*tab) == Some(drag.profile)
            && self.browser.item_anchor(*tab) != drag.id)
            .map(|target| self.reorder_boundary(target));
        if drag.reorder_target.get() == target { return; }
        drag.reorder_target.set(target);
        if target.is_some() { crate::native::reorder_haptic(); }
    }

    pub(crate) fn render_tab_list_end(&self, space: SpaceId, interactive: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        div().id(gpui::SharedString::from(format!("tab-list-end-{space:?}")))
            .debug_selector(|| self.sidebar_debug_name("tab-list-end".into()))
            .flex_1().min_h_0().mt(px(-crate::window::SIDEBAR_ROW_GAP))
            .can_drop(move |value, _, cx| interactive && value.downcast_ref::<TabDrag>().is_some_and(|drag|
                owner.read_with(cx, |this, _| this.accepts_tab_drag(drag)
                    && this.browser.spaces.iter().any(|entry| entry.id == space && entry.user_profile_id == drag.profile)).unwrap_or(false)))
            .on_drop(cx.listener(move |this, drag: &TabDrag, _, cx| {
                if !interactive { return; }
                cx.stop_propagation();
                let last = this.browser.spaces.iter().find(|entry| entry.id == space)
                    .and_then(|space| space.today.last()).map(|tab| this.browser.item_anchor(tab.id));
                if let Some(last) = last {
                    this.drop_tab_relative(drag, last, true, cx);
                } else {
                    this.drop_tab_on_section(drag, space, Section::Today, cx);
                }
            }))
    }

    pub(crate) fn accepts_tab_drag(&self, drag: &TabDrag) -> bool {
        self.profile.handle.same_workspace(&drag.workspace)
            && self.browser.user_profile_id_for_tab(drag.id) == Some(drag.profile)
            && (!(self.browser.is_blank() || drag.blank) || self.main_window == drag.source)
    }
    pub(crate) fn drop_tab_on_space(
        &mut self,
        drag: &TabDrag,
        space: SpaceId,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_tab_drag(drag) { return; }
        let tab = drag.id;
        self.cancel_space_drag_hover();
        let section = if self.browser.is_favorite(tab)
            || self
                .browser
                .spaces
                .iter()
                .any(|space| space.pinned.iter().any(|entry| entry.id == tab))
        {
            Section::Pinned
        } else {
            Section::Today
        };
        self.drop_tab_on_section(drag, space, section, cx);
    }

    pub(crate) fn drop_tab_relative(
        &mut self,
        drag: &TabDrag,
        target: TabId,
        after: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_tab_drag(drag) { return; }
        if self.browser.user_profile_id_for_tab(target) != Some(drag.profile) { return; }
        let tab = drag.id;
        let was_pinned = self.browser.all_tabs().find(|entry| entry.id == tab).is_some_and(|entry| entry.pinned_url.is_some());
        let old_folder = self.browser.folder_ancestors(tab).last().copied();
        let old_space = self.browser.spaces.iter().find(|space| space.pinned.iter().chain(&space.today).any(|entry| entry.id == tab)).map(|space| space.id);
        if !self.edit_browser(|browser| browser.move_tab_relative(tab, target, after)) {
            return;
        }
        if let Some(index) = self.browser.spaces.iter().position(|space| {
            space
                .pinned
                .iter()
                .chain(&space.today)
                .any(|entry| entry.id == tab)
        }) {
            self.switch_space(index, cx);
        }
        self.activate_dragged_tab(drag);
        if self
            .browser
            .space()
            .pinned
            .iter()
            .any(|entry| entry.id == tab)
        {
            self.scroll_to_pinned_item(self.browser.pinned_item(tab), cx);
        } else {
            self.scroll_to_today_item(tab, cx);
        }
        if old_space != Some(self.browser.space().id) { self.show_tab_move(tab, cx); }
        else if let Some(folder) = self.browser.folder_ancestors(tab).last().copied().filter(|folder| Some(*folder) != old_folder) { self.show_folder_pin(folder, cx); }
        else if was_pinned != self.browser.all_tabs().find(|entry| entry.id == tab).is_some_and(|entry| entry.pinned_url.is_some()) { self.show_toast(format!("{} Tab {} “{}”", if was_pinned { "Un-Pinned" } else { "Pinned" }, if was_pinned { "from" } else { "to" }, self.browser.space().name), cx); }
        cx.notify();
    }

    pub(crate) fn drop_tab_at_favorite(
        &mut self,
        drag: &TabDrag,
        target: TabId,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_tab_drag(drag) || self.browser.active_user_profile_id() != drag.profile { return; }
        let tab = drag.id;
        let Some(destination) = self
            .browser.active_user_profile().favorites
            .iter()
            .filter(|tab| !self.browser.is_split_secondary(tab.id))
            .position(|tab| tab.id == target)
        else {
            return;
        };
        if !self.browser.is_favorite(tab) {
            self.drop_tab_in_favorites(drag, cx);
        }
        if self.browser.is_favorite(tab) && self.edit_browser(|browser| browser.move_favorite(tab, destination)) {
            cx.notify();
        }
    }

    pub(crate) fn drop_tab_on_section(
        &mut self,
        drag: &TabDrag,
        space: SpaceId,
        section: Section,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_tab_drag(drag) { return; }
        if !self.browser.spaces.iter().any(|entry| entry.id == space && entry.user_profile_id == drag.profile) { return; }
        let tab = drag.id;
        let source = self.browser.spaces.iter().find(|space| space.pinned.iter().chain(&space.today).any(|entry| entry.id == tab)).map(|space| space.id);
        let was_pinned = self.browser.all_tabs().find(|entry| entry.id == tab).is_some_and(|entry| entry.pinned_url.is_some());
        let changed = match section {
            Section::Pinned => self.edit_browser(|browser| browser.move_item_to_pinned_root(tab, space)),
            Section::Today => self.edit_browser(|browser| browser.move_tab(tab, space, section)),
        };
        if !changed {
            return;
        }
        if let Some(index) = self
            .browser
            .spaces
            .iter()
            .position(|entry| entry.id == space)
        {
            self.switch_space(index, cx);
        }
        self.activate_dragged_tab(drag);
        if source != Some(space) { self.show_tab_move(tab, cx); }
        else if was_pinned != (section == Section::Pinned) { self.show_toast(format!("{} Tab {} “{}”", if section == Section::Pinned { "Pinned" } else { "Un-Pinned" }, if section == Section::Pinned { "to" } else { "from" }, self.browser.space().name), cx); }
        match section {
            Section::Pinned => self.scroll_to_pinned_item(self.browser.pinned_item(tab), cx),
            Section::Today => self.scroll_to_today_start(cx),
        }
        cx.notify();
    }

    pub(crate) fn can_drop_in_favorites(&self, drag: &TabDrag) -> bool {
        self.accepts_tab_drag(drag) && self.browser.active_user_profile_id() == drag.profile && !self.browser.is_blank() && !self.browser.is_favorite(drag.id)
            && self.browser.favorite_count() < browser_core::favorites::MAX_FAVORITES
    }

    pub(crate) fn drop_tab_in_favorites(&mut self, drag: &TabDrag, cx: &mut Context<Self>) {
        if !self.accepts_tab_drag(drag) { return; }
        if self.can_drop_in_favorites(drag)
            && self.edit_browser(|browser| browser.favorite_tab(drag.id))
        {
            self.activate_dragged_tab(drag);
            cx.notify();
        }
    }

    fn activate_dragged_tab(&mut self, drag: &TabDrag) {
        if self.browser.related_tabs(drag.id).contains(&drag.focus) {
            self.edit_browser(|browser| browser.activate_tab(drag.focus, SystemTime::now()));
        } else {
            self.edit_browser(|browser| browser.activate_sidebar_item(drag.id, SystemTime::now()));
        }
    }

    pub(crate) fn tab_drag(&self, tab: &Tab) -> TabDrag {
        let pages = self.browser.related_tabs(tab.id);
        let focus = self
            .browser
            .space()
            .active_tab
            .filter(|id| pages.contains(id))
            .unwrap_or_else(|| {
                self.browser
                    .split_for(tab.id)
                    .map_or(tab.id, |split| split.active)
            });
        TabDrag {
            reorder_target: std::rc::Rc::new(std::cell::Cell::new(None)),
            card_hotspot: std::rc::Rc::new(std::cell::Cell::new(gpui::point(px(64.),px(120.)))),
            source: self.main_window,
            workspace: self.profile.handle.clone(),
            profile: self.browser.user_profile_id_for_tab(tab.id).expect("dragged tab owner"),
            blank: self.browser.is_blank(),
            id: self.browser.item_anchor(tab.id),
            focus,
            title: self.browser.item_title(tab.id),
            icon: self.browser.item_icon(tab.id).map(str::to_string),
            favicon: self.browser.favicon(self.browser.active_user_profile_id(), &tab.url).map(|bytes| bytes.to_vec()),
            split: self.browser.split_for(tab.id).is_some(),
            ink: self.theme_foreground_for(self.browser.space().id),
            dark: self.theme_uses_light_ink_for(self.browser.space().id),
            fill: self.theme_tab_selection_for(self.browser.space().id),
            row: std::rc::Rc::new(std::cell::Cell::new(gpui::size(px(212.), px(crate::window::SIDEBAR_ROW_HEIGHT)))),
        }
    }

    /// A standalone page held over another standalone page of the same profile
    /// can join it as a two-page Split View in place.
    pub(crate) fn can_split_in_sidebar(&self, drag: &TabDrag, target: TabId) -> bool {
        self.accepts_tab_drag(drag) && !drag.split && target != drag.id
            && self.browser.split_for(target).is_none() && self.browser.split_for(drag.id).is_none()
            && !self.browser.is_favorite(target)
            && self.browser.user_profile_id_for_tab(target) == Some(drag.profile)
            && self.can_add_split(target) && self.can_add_split(drag.id)
    }

    /// Unlike a drop on the page, joining rows in the sidebar keeps the group
    /// where the target row is, including inside Pinned and its folders.
    pub(crate) fn drop_tab_split_in_sidebar(&mut self, drag: &TabDrag, target: TabId, right: bool, cx: &mut Context<Self>) {
        if !self.can_split_in_sidebar(drag, target) { return; }
        let placement = if right { browser_core::splits::Placement::Right } else { browser_core::splits::Placement::Left };
        let now = SystemTime::now();
        // A Favorite stays in the dock; the split takes an independent copy.
        let page = if self.browser.is_favorite(drag.id) {
            let Some(copy) = self.edit_browser(|browser| browser.copy_item_to_today(drag.id, now)) else { return; };
            copy
        } else { drag.id };
        if self.edit_browser(|browser| browser.add_split(target, page, placement, now)).is_err() { return; }
        if let Some(pages) = &self.pages { pages.borrow_mut().sync(&self.browser, self.page_content_visible()); }
        cx.notify();
    }

    pub(crate) fn drop_tab_in_folder(
        &mut self,
        drag: &TabDrag,
        folder: FolderId,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_tab_drag(drag) { return; }
        if let Some(space) = self.browser.folder_space(folder) {
            if self.confirm_folder_destination(
                Target::MoveToFolder(drag.id),
                space,
                Some(folder),
                cx,
            ) {
                self.activate_dragged_tab(drag);
                cx.notify();
            }
        }
    }
}

impl Render for TabDrag {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // GPUI paints the active drag in every window. The passive page overlay
        // already owns the tall card, so it must not paint a second row ghost.
        let page_card = window.root::<BrowserWindow>().flatten().is_some_and(|view| view.read(cx).split_drop.is_some());
        if page_card || window.root::<crate::window::Overlay>().flatten().is_some_and(|overlay| overlay.read(cx).draws_split_drag_card()) {
            return div().debug_selector(|| "tab-drag-hidden".into()).into_any_element();
        }
        let icon = if let Some(icon) = &self.icon {
            crate::icons::glyph(icon)
        } else if self.split {
            crate::icons::control(crate::icons::Icon::Split)
        } else if let Some(bytes) = &self.favicon {
            gpui::img(Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                bytes.clone(),
            )))
            .size(px(16.))
            .flex_none()
            .into_any_element()
        } else {
            crate::window::favicon_placeholder(self.dark).into_any_element()
        };
        let icon = div().size(px(18.)).flex_none().flex().items_center().justify_center().child(icon);
        // Arc lifts a copy of the sidebar row: a translucent fill under crisp
        // icon and title, the same size as the row it left.
        let row = self.row.get();
        div()
            .debug_selector(|| "tab-drag-row".into())
            .w(row.width)
            .h(row.height)
            .flex()
            .items_center()
            .child(div()
                .relative()
                .flex()
                .items_center()
                .gap(px(9.))
                .w_full()
                .h(px(crate::window::SIDEBAR_TAB_HEIGHT))
                .pl(px(9.))
                .pr(px(6.))
                .rounded(px(crate::window::SIDEBAR_ROW_RADIUS))
                .bg(gpui::Hsla { a: (self.fill.a * 0.85).max(0.12), ..self.fill })
                .shadow(vec![gpui::BoxShadow { color: hsla(0., 0., 0., 0.12), offset: gpui::point(px(0.), px(2.)), blur_radius: px(8.), spread_radius: px(0.) }])
                .text_color(self.ink)
                .text_size(px(14.))
                .line_height(px(18.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(icon)
                .child(div().min_w_0().truncate().child(self.title.clone())))
            .into_any_element()
    }
}
