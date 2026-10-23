//! Library Downloads presentation only. The host owns inventory and all effects.
use crate::{
    sidebar_title::SidebarTitle,
    text_input::{Changed, TextInput},
};
use browser_core::UserProfileId;
use gpui::{
    actions, div, prelude::*, px, AnyElement, App, Context, Entity, EventEmitter, FocusHandle,
    Focusable, Hsla, KeyBinding, Render, ScrollHandle, Subscription, Window,
};
use std::rc::Rc;

actions!(downloads_view, [NextFocus, PreviousFocus]);

/// Register alongside the host's TextInput bindings. These are Echo access
/// paths, not a claim about Arc's native keyboard behavior. Enter/Space use
/// GPUI's focused on_click keyup path; adding keydown activation doubles it.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", NextFocus, Some("DownloadsView")),
        KeyBinding::new("shift-tab", PreviousFocus, Some("DownloadsView")),
    ]);
}

pub(crate) const DOWNLOADS_COLUMN_WIDTH: f32 = 304.;
pub(crate) const FILTERED_EMPTY_TITLE: &str = "Nothing here yet!";
pub(crate) const FILTERED_EMPTY_BODY: &str =
    "Come back after you’ve found some\ncool stuff to save.";

/// A host-owned stable token, not a row index, filename or reconstructed CEF ID.
/// `Token` can be the engine's opaque DownloadToken without coupling this view
/// to CEF, its callbacks, disk paths, network requests or download controls.
#[derive(Clone, Debug)]
pub(crate) struct CompletedDownload<Token> {
    pub token: Token,
    pub profile: UserProfileId,
    pub title: String,
    /// Host-formatted file type plus optional source domain; never a disk path.
    pub subtitle: String,
}

#[derive(Clone, Debug)]
pub(crate) struct DownloadsGroup<Token> {
    /// Host-projected native date heading, e.g. Today or Yesterday.
    pub heading: String,
    /// Completed downloads only, in the host's newest-first order.
    pub rows: Vec<CompletedDownload<Token>>,
}

#[derive(Clone, Debug)]
pub(crate) struct DownloadsProfile {
    pub id: UserProfileId,
    pub name: String,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum DownloadsList<Token> {
    Completed(Vec<DownloadsGroup<Token>>),
    /// Verified only for an empty Profile-filter result. Do not infer this
    /// presentation for search misses, an empty database or failed downloads.
    FilteredEmpty,
}

#[derive(Clone, Debug)]
pub(crate) struct DownloadsProjection<Token> {
    pub profiles: Vec<DownloadsProfile>,
    pub list: DownloadsList<Token>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DownloadsIntent<Token> {
    SearchChanged(String),
    FilterVisibilityChanged(bool),
    /// Host applies its policy, then replaces the projection. Native
    /// multi-selection/persistence semantics remain unverified.
    ToggleProfile(UserProfileId),
    /// Disabled by default: row selection/preview behavior is unverified.
    RowPressed {
        token: Token,
        profile: UserProfileId,
    },
}

#[derive(Clone, Copy)]
pub(crate) struct DownloadsColors {
    pub column: Hsla,
    pub preview: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub control: Hsla,
    pub selected: Hsla,
    pub hover: Hsla,
}

pub(crate) type DownloadsPreview = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
pub(crate) type DownloadsArtwork<Token> =
    Rc<dyn Fn(&CompletedDownload<Token>, &mut Window, &mut App) -> AnyElement>;

/// Lives in the existing Library route, immediately after the host's 112 px nav.
/// The root's local x=0 is the list; the parent preview begins at local x=304.
pub(crate) struct DownloadsView<Token: Clone + Eq + 'static> {
    projection: DownloadsProjection<Token>,
    colors: DownloadsColors,
    preview: DownloadsPreview,
    artwork: Option<DownloadsArtwork<Token>>,
    input: Entity<TextInput>,
    filter_visible: bool,
    focus: FocusHandle,
    filter_focus: FocusHandle,
    done_focus: FocusHandle,
    profile_focus: Vec<(UserProfileId, FocusHandle)>,
    chips_scroll: ScrollHandle,
    // Keys are monotonic and never reused for a different opaque identity.
    // Keep positional debug selectors separate from GPUI interaction state.
    profile_keys: Vec<(UserProfileId, usize)>,
    row_keys: Vec<(Token, UserProfileId, usize)>,
    next_item_key: usize,
    row_selection_enabled: bool,
    _input_changes: Subscription,
}

impl<Token: Clone + Eq + 'static> EventEmitter<DownloadsIntent<Token>> for DownloadsView<Token> {}

impl<Token: Clone + Eq + 'static> Focusable for DownloadsView<Token> {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.filter_visible {
            self.done_focus.clone()
        } else {
            self.input.read(cx).focus_handle(cx)
        }
    }
}

impl<Token: Clone + Eq + 'static> DownloadsView<Token> {
    pub(crate) fn new(
        projection: DownloadsProjection<Token>,
        colors: DownloadsColors,
        preview: DownloadsPreview,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new("Search Downloads…", cx));
        let input_changes = cx.subscribe(&input, |_, input, _: &Changed, cx| {
            let query = input.read(cx).text().to_owned();
            cx.emit(DownloadsIntent::<Token>::SearchChanged(query));
        });
        let profile_focus = projection
            .profiles
            .iter()
            .map(|profile| (profile.id, cx.focus_handle()))
            .collect();
        let mut view = Self {
            focus: cx.focus_handle(),
            filter_focus: cx.focus_handle(),
            done_focus: cx.focus_handle(),
            profile_focus,
            chips_scroll: ScrollHandle::new(),
            profile_keys: Vec::new(),
            row_keys: Vec::new(),
            next_item_key: 0,
            projection,
            colors,
            preview,
            artwork: None,
            input,
            filter_visible: false,
            row_selection_enabled: false,
            _input_changes: input_changes,
        };
        view.reconcile_item_keys();
        view
    }

    fn reconcile_item_keys(&mut self) {
        let mut next = self.next_item_key;
        let mut allocate = || {
            let key = next;
            next = next.checked_add(1).expect("Downloads item key exhausted");
            key
        };
        self.profile_keys = self
            .projection
            .profiles
            .iter()
            .map(|profile| {
                let key = self
                    .profile_keys
                    .iter()
                    .find(|(id, _)| *id == profile.id)
                    .map(|(_, key)| *key)
                    .unwrap_or_else(&mut allocate);
                (profile.id, key)
            })
            .collect();
        self.row_keys = match &self.projection.list {
            DownloadsList::FilteredEmpty => Vec::new(),
            DownloadsList::Completed(groups) => groups
                .iter()
                .flat_map(|group| &group.rows)
                .map(|row| {
                    let key = self
                        .row_keys
                        .iter()
                        .find(|(token, profile, _)| token == &row.token && *profile == row.profile)
                        .map(|(_, _, key)| *key)
                        .unwrap_or_else(&mut allocate);
                    (row.token.clone(), row.profile, key)
                })
                .collect(),
        };
        self.next_item_key = next;
    }

    /// This does not decide search or Profile-filter semantics, reorder rows,
    /// mutate profiles, or clear the retained query when Done is pressed.
    /// The host Window preserves scoped focus and reveals a retained focused
    /// Profile at its new position. Removed focused chips transfer to Done.
    pub(crate) fn set_projection(
        &mut self,
        projection: DownloadsProjection<Token>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focused_profile = self
            .profile_focus
            .iter()
            .find(|(_, focus)| focus.is_focused(window))
            .map(|(id, _)| *id);
        // Transfer before dropping the last handle of a removed focused chip.
        if self.filter_visible
            && focused_profile
                .is_some_and(|id| !projection.profiles.iter().any(|profile| profile.id == id))
        {
            self.done_focus.focus(window);
        }
        self.profile_focus = projection
            .profiles
            .iter()
            .map(|profile| {
                let focus = self
                    .profile_focus
                    .iter()
                    .find(|(id, _)| *id == profile.id)
                    .map(|(_, focus)| focus.clone())
                    .unwrap_or_else(|| cx.focus_handle());
                (profile.id, focus)
            })
            .collect();
        self.projection = projection;
        self.reconcile_item_keys();
        if self.filter_visible {
            if let Some(index) = self
                .profile_focus
                .iter()
                .position(|(id, _)| Some(*id) == focused_profile)
            {
                self.chips_scroll.scroll_to_item(index);
            }
        }
        cx.notify();
    }

    pub(crate) fn set_row_artwork(
        &mut self,
        artwork: DownloadsArtwork<Token>,
        cx: &mut Context<Self>,
    ) {
        self.artwork = Some(artwork);
        cx.notify();
    }

    /// Opt in only when the host intentionally supplies row/preview behavior.
    /// No open/reveal/delete/pause/context-menu affordances are added here.
    pub(crate) fn set_row_selection_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.row_selection_enabled = enabled;
        cx.notify();
    }

    fn set_filter_visible(&mut self, visible: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.filter_visible = visible;
        if visible {
            self.done_focus.focus(window);
        } else {
            self.filter_focus.focus(window);
        }
        cx.emit(DownloadsIntent::<Token>::FilterVisibilityChanged(visible));
        cx.notify();
    }

    fn move_focus(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        let targets = if self.filter_visible {
            std::iter::once(self.done_focus.clone())
                .chain(self.profile_focus.iter().map(|(_, focus)| focus.clone()))
                .collect::<Vec<_>>()
        } else {
            vec![
                self.input.read(cx).focus_handle(cx),
                self.filter_focus.clone(),
            ]
        };
        let current = targets.iter().position(|focus| focus.is_focused(window));
        let next = match current {
            Some(index) if backwards => (index + targets.len() - 1) % targets.len(),
            Some(index) => (index + 1) % targets.len(),
            None if backwards => targets.len() - 1,
            None => 0,
        };
        targets[next].focus(window);
        if self.filter_visible && next > 0 {
            self.chips_scroll.scroll_to_item(next - 1);
        }
        cx.notify();
    }

    fn toolbar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        if self.filter_visible {
            return div()
                .debug_selector(|| "downloads-filter-inline".into())
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(12.))
                .p(px(12.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .h(px(32.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_size(px(13.))
                                .line_height(px(18.))
                                .child(SidebarTitle::new("Filter Downloads…")),
                        )
                        .child(
                            div()
                                .id("downloads-filter-done")
                                .debug_selector(|| "downloads-filter-done".into())
                                .track_focus(&self.done_focus)
                                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                                    window.prevent_default()
                                })
                                .flex_none()
                                .h(px(32.))
                                .px(px(10.))
                                .rounded(px(8.))
                                .bg(colors.control)
                                .flex()
                                .items_center()
                                .text_size(px(12.))
                                .cursor_pointer()
                                .hover(move |s| s.bg(colors.hover))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_filter_visible(false, window, cx)
                                }))
                                .child("Done"),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(colors.muted)
                        .child("Profiles"),
                )
                .child({
                    let mut chips = div()
                        .id("downloads-profile-chips")
                        .debug_selector(|| "downloads-profile-chips".into())
                        // Keep the heading/Done fixed and leave results usable
                        // at 640 × 480 even with many full-width Profile names.
                        .max_h(px(130.))
                        .min_h_0()
                        .overflow_y_scroll()
                        .overflow_x_hidden()
                        .track_scroll(&self.chips_scroll)
                        .flex()
                        .flex_wrap()
                        .content_start()
                        .gap(px(6.))
                        .children(self.projection.profiles.iter().enumerate().map(
                            |(index, profile)| {
                                let id = profile.id;
                                let focus = self.profile_focus[index].1.clone();
                                div()
                                    .id(("downloads-profile", self.profile_keys[index].1))
                                    .track_focus(&focus)
                                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                                        window.prevent_default()
                                    })
                                    .debug_selector(move || format!("downloads-profile-{index}"))
                                    .flex_none()
                                    .w(profile_chip_width(&profile.name, window))
                                    .h(px(28.))
                                    .px(px(10.))
                                    .rounded(px(8.))
                                    .bg(if profile.selected {
                                        colors.selected
                                    } else {
                                        colors.control
                                    })
                                    .flex()
                                    .items_center()
                                    .text_size(px(12.))
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(colors.hover))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if let Some((_, focus)) = this
                                            .profile_focus
                                            .iter()
                                            .find(|(profile, _)| *profile == id)
                                        {
                                            focus.focus(window);
                                        }
                                        cx.emit(DownloadsIntent::<Token>::ToggleProfile(id))
                                    }))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .line_height(px(16.))
                                            .debug_selector(move || {
                                                format!("downloads-profile-label-{index}")
                                            })
                                            .child(SidebarTitle::new(profile.name.clone())),
                                    )
                            },
                        ));
                    chips.style().restrict_scroll_to_axis = Some(true);
                    chips
                })
                .into_any_element();
        }
        div()
            .debug_selector(|| "downloads-search-toolbar".into())
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .p(px(12.))
            .child(
                div()
                    .id("downloads-search")
                    .debug_selector(|| "downloads-search".into())
                    .flex_1()
                    .min_w_0()
                    .h(px(36.))
                    .px(px(8.))
                    .rounded(px(10.))
                    .bg(colors.control)
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            // Bubble after the editor's own caret/selection handling.
                            let focus = this.input.read(cx).focus_handle(cx);
                            if !focus.is_focused(window) {
                                focus.focus(window);
                            }
                            // A focusable enclosing browser must not take focus back.
                            window.prevent_default();
                        }),
                    )
                    .child(crate::icons::control(crate::icons::Icon::Search))
                    .child(
                        div()
                            .debug_selector(|| "downloads-search-input".into())
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .child(self.input.clone()),
                    ),
            )
            .child(
                div()
                    .id("downloads-filter")
                    .debug_selector(|| "downloads-filter".into())
                    .track_focus(&self.filter_focus)
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.prevent_default()
                    })
                    .flex_none()
                    .h(px(36.))
                    .px(px(8.))
                    .rounded(px(10.))
                    .bg(colors.control)
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(colors.hover))
                    .on_click(
                        cx.listener(|this, _, window, cx| {
                            this.set_filter_visible(true, window, cx)
                        }),
                    )
                    .child(filter_icon())
                    .child("Filter"),
            )
            .into_any_element()
    }

    fn completed_row(
        &self,
        group: usize,
        index: usize,
        row: &CompletedDownload<Token>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let token = row.token.clone();
        let profile = row.profile;
        let artwork = self
            .artwork
            .as_ref()
            .map_or_else(file_icon, |render| render(row, window, cx));
        div()
            .id((
                "downloads-row",
                self.row_keys
                    .iter()
                    .find(|(token, profile, _)| token == &row.token && *profile == row.profile)
                    .expect("completed row must have an identity key")
                    .2,
            ))
            .debug_selector(move || format!("downloads-row-{group}-{index}"))
            .flex_none()
            .h(px(56.))
            .w_full()
            .min_w_0()
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(10.))
            .when(self.row_selection_enabled, |el| {
                el.cursor_pointer()
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.stop_propagation();
                        cx.emit(DownloadsIntent::RowPressed {
                            token: token.clone(),
                            profile,
                        });
                    }))
            })
            .child(
                div()
                    .debug_selector(move || format!("downloads-artwork-{group}-{index}"))
                    .flex_none()
                    .size(px(36.))
                    .overflow_hidden()
                    .child(artwork),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .debug_selector(move || format!("downloads-title-{group}-{index}"))
                            .min_w_0()
                            .overflow_hidden()
                            .text_size(px(14.))
                            .line_height(px(18.))
                            .child(SidebarTitle::new(row.title.clone())),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("downloads-subtitle-{group}-{index}"))
                            .min_w_0()
                            .overflow_hidden()
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .text_color(self.colors.muted)
                            .child(SidebarTitle::new(row.subtitle.clone())),
                    ),
            )
            .into_any_element()
    }
}

impl<Token: Clone + Eq + 'static> Render for DownloadsView<Token> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toolbar = self.toolbar(window, cx);
        let list = match &self.projection.list {
            DownloadsList::FilteredEmpty => div()
                .debug_selector(|| "downloads-filtered-empty".into())
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .child(
                    div()
                        .debug_selector(|| "downloads-empty-icon".into())
                        .flex_none()
                        .size(px(48.))
                        .text_color(self.colors.muted)
                        .child(download_circle()),
                )
                .child(
                    div()
                        .debug_selector(|| "downloads-empty-title".into())
                        .text_size(px(16.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(FILTERED_EMPTY_TITLE),
                )
                .child(
                    div()
                        .debug_selector(|| "downloads-empty-body".into())
                        .w_full()
                        .text_center()
                        .text_size(px(14.))
                        .line_height(px(18.))
                        .text_color(self.colors.muted)
                        .child(FILTERED_EMPTY_BODY),
                )
                .into_any_element(),
            DownloadsList::Completed(groups) => {
                let mut scroll = div()
                    .id("downloads-list-scroll")
                    .debug_selector(|| "downloads-list-scroll".into())
                    .size_full()
                    .overflow_y_scroll()
                    .overflow_x_hidden();
                scroll.style().restrict_scroll_to_axis = Some(true);
                let groups = groups
                    .iter()
                    .enumerate()
                    .map(|(group_index, group)| {
                        let rows = group
                            .rows
                            .iter()
                            .enumerate()
                            .map(|(index, row)| {
                                self.completed_row(group_index, index, row, window, cx)
                            })
                            .collect::<Vec<_>>();
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .debug_selector(move || {
                                        format!("downloads-group-{group_index}")
                                    })
                                    .h(px(28.))
                                    .px(px(12.))
                                    .flex()
                                    .items_center()
                                    .text_size(px(12.))
                                    .text_color(self.colors.muted)
                                    .line_height(px(16.))
                                    .child(SidebarTitle::new(group.heading.clone())),
                            )
                            .children(rows)
                    })
                    .collect::<Vec<_>>();
                scroll
                    .child(div().flex().flex_col().children(groups))
                    .into_any_element()
            }
        };
        let preview = (self.preview)(window, cx);
        div()
            .id("downloads-view")
            .debug_selector(|| "downloads-view".into())
            .key_context("DownloadsView")
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &NextFocus, window, cx| this.move_focus(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousFocus, window, cx| {
                    this.move_focus(true, window, cx)
                }),
            )
            .relative()
            .size_full()
            .text_color(self.colors.text)
            .child(
                div()
                    .debug_selector(|| "downloads-column".into())
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(DOWNLOADS_COLUMN_WIDTH))
                    .bg(self.colors.column)
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(toolbar)
                    .child(
                        div()
                            .debug_selector(|| "downloads-list-body".into())
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .overflow_hidden()
                            .child(list),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "downloads-preview".into())
                    .absolute()
                    .left(px(DOWNLOADS_COLUMN_WIDTH))
                    .right_0()
                    .top(px(10.))
                    .bottom(px(10.))
                    .bg(self.colors.preview)
                    .rounded_l(px(12.))
                    .overflow_hidden()
                    .child(preview),
            )
    }
}

/// Chip layout needs a finite intrinsic width, unlike a stretchable row title.
/// Measure with a fresh full-byte run, then shape its label again only after
/// layout through SidebarTitle. No GPUI truncated-run cache/probe is reused.
fn profile_chip_width(name: &str, window: &Window) -> gpui::Pixels {
    let text = name.replace(['\n', '\r'], " ");
    let run = window.text_style().to_run(text.len());
    let width = window
        .text_system()
        .shape_line(text.into(), px(12.), &[run], None)
        .width;
    (width + px(20.)).min(px(DOWNLOADS_COLUMN_WIDTH - 24.))
}

/// Echo-owned fixed vector geometry; no font glyphs or branded reference assets.
fn strokes(size: f32, contours: Vec<Vec<(f32, f32)>>) -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for points in &contours {
                let mut path = gpui::PathBuilder::stroke(px(1.6));
                for (index, (x, y)) in points.iter().enumerate() {
                    let point = bounds.origin + gpui::point(px(*x), px(*y));
                    if index == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, window.text_style().color);
                }
            }
        },
    )
    .size(px(size))
    .flex_none()
    .into_any_element()
}
fn circle(cx: f32, cy: f32, radius: f32) -> Vec<(f32, f32)> {
    (0..=48)
        .map(|index| {
            let angle = index as f32 * std::f32::consts::TAU / 48.;
            (cx + radius * angle.cos(), cy + radius * angle.sin())
        })
        .collect()
}
fn filter_icon() -> AnyElement {
    strokes(
        18.,
        vec![
            circle(9., 9., 7.),
            vec![(5., 6.), (13., 6.)],
            vec![(6.5, 9.), (11.5, 9.)],
            vec![(8., 12.), (10., 12.)],
        ],
    )
}
fn download_circle() -> AnyElement {
    strokes(
        48.,
        vec![
            circle(24., 24., 20.),
            vec![(24., 12.), (24., 33.)],
            vec![(15., 25.), (24., 34.), (33., 25.)],
        ],
    )
}
fn file_icon() -> AnyElement {
    strokes(
        36.,
        vec![
            vec![
                (9., 3.),
                (22., 3.),
                (28., 9.),
                (28., 33.),
                (9., 33.),
                (9., 3.),
            ],
            vec![(22., 3.), (22., 9.), (28., 9.)],
        ],
    )
}

#[cfg(test)]
#[path = "downloads_view_tests.rs"]
mod tests;
