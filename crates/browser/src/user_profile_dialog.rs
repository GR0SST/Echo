//! Native-reference Profile sheets, rendered inside the existing Overlay host.
//! This entity owns form state only; the BrowserWindow owns native lifecycle.
use crate::{
    icons::{self, Icon},
    text_input::{self, TextInput},
    window::BrowserWindow,
};
use browser_core::{
    Browser, SpaceId, UserProfileId,
    profiles::{UserProfileError, UserProfileTransition},
};
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
    Subscription, Window, actions, div, hsla, prelude::*, px, rgb,
};
use std::{cell::Cell, collections::HashSet, rc::Rc};

actions!(
    user_profile_dialog,
    [
        Accept,
        Cancel,
        NextFocus,
        PreviousFocus,
        ToggleSpace,
        NextSpace,
        PreviousSpace,
        OpenDropdown,
        NextChoice,
        PreviousChoice,
        FirstChoice,
        LastChoice,
        ActivateButton
    ]
);
const CONTEXT: &str = "UserProfileDialog";
const SWITCH_WARNING: &str = "Each Profile has different browser data, so you might get logged out of some websites, see different Favorites up top, or need to re-install extensions.";

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", Accept, Some(CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("tab", NextFocus, Some(CONTEXT)),
        KeyBinding::new("shift-tab", PreviousFocus, Some(CONTEXT)),
        KeyBinding::new("space", ToggleSpace, Some("UserProfileSpace")),
        KeyBinding::new("right", NextSpace, Some("UserProfileSpace")),
        KeyBinding::new("left", PreviousSpace, Some("UserProfileSpace")),
        KeyBinding::new("space", OpenDropdown, Some("UserProfileDropdown")),
        KeyBinding::new("down", NextChoice, Some("UserProfileDropdown")),
        KeyBinding::new("up", PreviousChoice, Some("UserProfileDropdown")),
        KeyBinding::new("home", FirstChoice, Some("UserProfileDropdown")),
        KeyBinding::new("end", LastChoice, Some("UserProfileDropdown")),
        KeyBinding::new("space", ActivateButton, Some("UserProfileButton")),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CreationSource {
    File(SpaceId),
    Settings,
}

#[derive(Clone, Debug)]
pub(crate) enum ProfileRequest {
    Create {
        source: CreationSource,
        name: String,
        spaces: Vec<SpaceId>,
    },
    Switch {
        space: SpaceId,
        from: UserProfileId,
        to: UserProfileId,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum DialogEvent {
    Submit(ProfileRequest),
    /// Replace this sheet with creation, preserving the originating Space.
    NewProfile(CreationSource),
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DialogError {
    Model(UserProfileError),
    NoSpaces,
    StaleSource,
}
impl From<UserProfileError> for DialogError {
    fn from(error: UserProfileError) -> Self {
        Self::Model(error)
    }
}

pub(crate) struct ProfileApplied {
    pub profile: UserProfileId,
    pub transitions: Vec<UserProfileTransition>,
}

impl BrowserWindow {
    /// All-or-nothing model edit. The caller must retire/recreate affected CEF
    /// pages before rendering the changed ownership; no engine pumping here.
    pub(crate) fn apply_user_profile_dialog(
        &mut self,
        request: &ProfileRequest,
    ) -> Result<ProfileApplied, DialogError> {
        self.edit_browser(|browser| {
            if browser.is_private() || browser.is_blank() {
                return Err(UserProfileError::WindowLocalModel.into());
            }
            let mut next = browser.clone();
            let mut transitions = Vec::new();
            let profile = match request {
                ProfileRequest::Create {
                    source,
                    name,
                    spaces,
                } => {
                    if spaces.is_empty() {
                        return Err(DialogError::NoSpaces);
                    }
                    if let CreationSource::File(source) = source {
                        if !next.spaces.iter().any(|space| space.id == *source) {
                            return Err(UserProfileError::UnknownSpace.into());
                        }
                    }
                    let id = next.create_user_profile(name)?;
                    let mut seen = HashSet::new();
                    for space in spaces.iter().filter(|space| seen.insert(**space)) {
                        if let Some(transition) = next.assign_space_user_profile(*space, id)? {
                            transitions.push(transition);
                        }
                    }
                    id
                }
                ProfileRequest::Switch { space, from, to } => {
                    let current = next
                        .spaces
                        .iter()
                        .find(|candidate| candidate.id == *space)
                        .ok_or(UserProfileError::UnknownSpace)?;
                    if current.user_profile_id != *from {
                        return Err(DialogError::StaleSource);
                    }
                    if let Some(transition) = next.assign_space_user_profile(*space, *to)? {
                        transitions.push(transition);
                    }
                    *to
                }
            };
            *browser = next;
            Ok(ProfileApplied {
                profile,
                transitions,
            })
        })
    }
}

#[derive(Clone)]
struct SpaceCard {
    id: SpaceId,
    name: String,
    hue: f32,
    focus: FocusHandle,
}
#[derive(Clone)]
struct ProfileChoice {
    id: UserProfileId,
    name: String,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Name,
    Spaces,
    Switch,
    ConfirmSwitch,
}

pub(crate) struct UserProfileDialog {
    pub(crate) input: Entity<TextInput>,
    focus: FocusHandle,
    source: CreationSource,
    step: Step,
    spaces: Vec<SpaceCard>,
    selected_spaces: HashSet<SpaceId>,
    profiles: Vec<ProfileChoice>,
    switch_space: Option<(SpaceId, String, UserProfileId)>,
    selected_profile: Option<UserProfileId>,
    dropdown_open: bool,
    dropdown_focus: FocusHandle,
    dropdown_index: usize,
    primary_focus: FocusHandle,
    secondary_focus: FocusHandle,
    cards_scroll: gpui::ScrollHandle,
    dropdown_scroll: gpui::ScrollHandle,
    dropdown_reveal_on_layout: Rc<Cell<bool>>,
    pending: bool,
    closed: bool,
    error: Option<String>,
    _changed: Subscription,
}
impl EventEmitter<DialogEvent> for UserProfileDialog {}
impl Focusable for UserProfileDialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.step == Step::Name {
            self.input.read(cx).focus_handle(cx)
        } else {
            self.focus.clone()
        }
    }
}

impl UserProfileDialog {
    pub(crate) fn creation(
        browser: &Browser,
        source: CreationSource,
        cx: &mut Context<Self>,
    ) -> Result<Self, DialogError> {
        Self::new(browser, source, None, cx)
    }

    pub(crate) fn switching(
        browser: &Browser,
        space: SpaceId,
        cx: &mut Context<Self>,
    ) -> Result<Self, DialogError> {
        let item = browser
            .spaces
            .iter()
            .find(|item| item.id == space)
            .ok_or(UserProfileError::UnknownSpace)?;
        Self::new(
            browser,
            CreationSource::File(space),
            Some((space, item.name.clone(), item.user_profile_id)),
            cx,
        )
    }

    pub(crate) fn switching_to(browser: &Browser, space: SpaceId, to: UserProfileId, cx: &mut Context<Self>) -> Result<Self, DialogError> {
        browser.user_profile(to).ok_or(UserProfileError::UnknownProfile)?;
        let mut dialog = Self::switching(browser, space, cx)?;
        dialog.selected_profile = Some(to);
        dialog.step = Step::ConfirmSwitch;
        Ok(dialog)
    }

    fn new(
        browser: &Browser,
        source: CreationSource,
        switch_space: Option<(SpaceId, String, UserProfileId)>,
        cx: &mut Context<Self>,
    ) -> Result<Self, DialogError> {
        if browser.is_private() || browser.is_blank() {
            return Err(UserProfileError::WindowLocalModel.into());
        }
        if let CreationSource::File(id) = source {
            if !browser.spaces.iter().any(|space| space.id == id) {
                return Err(UserProfileError::UnknownSpace.into());
            }
        }
        let input = cx.new(|cx| TextInput::new("Personal", cx));
        let changed = cx.subscribe(&input, |_, _, _: &text_input::Changed, cx| cx.notify());
        Ok(Self {
            input,
            focus: cx.focus_handle(),
            source,
            step: if switch_space.is_some() {
                Step::Switch
            } else {
                Step::Name
            },
            spaces: browser
                .spaces
                .iter()
                .map(|space| SpaceCard {
                    id: space.id,
                    name: space.name.clone(),
                    hue: space.hue,
                    focus: cx.focus_handle(),
                })
                .collect(),
            selected_spaces: match source {
                CreationSource::File(id) => [id].into(),
                CreationSource::Settings => HashSet::new(),
            },
            profiles: browser
                .user_profiles()
                .map(|profile| ProfileChoice {
                    id: profile.id(),
                    name: profile.name().into(),
                })
                .collect(),
            selected_profile: switch_space.as_ref().map(|(_, _, profile)| *profile),
            switch_space,
            dropdown_open: false,
            dropdown_focus: cx.focus_handle(),
            dropdown_index: 0,
            primary_focus: cx.focus_handle(),
            secondary_focus: cx.focus_handle(),
            cards_scroll: gpui::ScrollHandle::new(),
            dropdown_scroll: gpui::ScrollHandle::new(),
            dropdown_reveal_on_layout: Rc::new(Cell::new(false)),
            pending: false,
            closed: false,
            error: None,
            _changed: changed,
        })
    }

    /// Call in the child window after attachment and activation.
    pub(crate) fn focus(&self, window: &mut Window, cx: &App) {
        self.focus_handle(cx).focus(window);
    }

    /// Invalidate before removing the child, including owner-window closure.
    /// Repeated/late callbacks cannot submit or close a replacement dialog.
    pub(crate) fn close(&mut self, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        cx.emit(DialogEvent::Closed);
        cx.notify();
    }

    /// Keep the form open on an integration/model failure; do not invent a
    /// loaded-tab warning. The adapter supplies its actionable error message.
    pub(crate) fn submission_failed(&mut self, message: String, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.pending = false;
        self.error = Some(message);
        cx.notify();
    }

    fn enabled(&self, cx: &App) -> bool {
        !self.closed
            && !self.pending
            && match self.step {
                Step::Name => !self.input.read(cx).text().trim().is_empty(),
                Step::Spaces => {
                    !self.selected_spaces.is_empty()
                        && !self.input.read(cx).text().trim().is_empty()
                }
                Step::Switch | Step::ConfirmSwitch => self.selected_profile.is_some(),
            }
    }

    fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closed || self.pending {
            return;
        }
        if self.secondary_focus.is_focused(window) {
            if self.step == Step::Spaces {
                self.back(window, cx);
            } else {
                self.close(cx);
            }
            return;
        }
        if self.step == Step::Switch && self.dropdown_focus.is_focused(window) {
            if self.dropdown_open {
                self.choose_dropdown(cx);
            } else {
                self.open_dropdown(cx);
            }
            return;
        }
        self.accept_primary(window, cx);
    }

    fn accept_primary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled(cx) {
            return;
        }
        if self.step == Step::Name {
            self.step = Step::Spaces;
            self.focus.focus(window);
            cx.notify();
            return;
        }
        if self.step == Step::Switch {
            self.step = Step::ConfirmSwitch;
            self.dropdown_open = false;
            self.focus.focus(window);
            cx.notify();
            return;
        }
        let request = if let Some((space, _, from)) = self.switch_space.as_ref() {
            ProfileRequest::Switch {
                space: *space,
                from: *from,
                to: self.selected_profile.unwrap(),
            }
        } else {
            ProfileRequest::Create {
                source: self.source,
                name: self.input.read(cx).text().trim().into(),
                // Preserve workspace ordering rather than HashSet iteration.
                spaces: self
                    .spaces
                    .iter()
                    .filter(|space| self.selected_spaces.contains(&space.id))
                    .map(|space| space.id)
                    .collect(),
            }
        };
        self.pending = true;
        self.error = None;
        cx.emit(DialogEvent::Submit(request));
        cx.notify();
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closed || self.pending || self.step != Step::Spaces {
            return;
        }
        self.step = Step::Name;
        self.focus(window, cx);
        cx.notify();
    }

    fn move_focus(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.closed || self.pending {
            return;
        }
        self.dropdown_open = false;
        let mut targets = match self.step {
            Step::Name => vec![self.input.read(cx).focus_handle(cx)],
            Step::Spaces => self
                .spaces
                .iter()
                .map(|space| space.focus.clone())
                .collect(),
            Step::Switch => vec![self.dropdown_focus.clone()],
            Step::ConfirmSwitch => Vec::new(),
        };
        targets.push(self.secondary_focus.clone());
        if self.enabled(cx) {
            targets.push(self.primary_focus.clone());
        }
        let current = targets.iter().position(|target| target.is_focused(window));
        let next = match current {
            Some(index) if backwards => (index + targets.len() - 1) % targets.len(),
            Some(index) => (index + 1) % targets.len(),
            None if backwards => targets.len() - 1,
            None => 0,
        };
        targets[next].focus(window);
        if self.step == Step::Spaces && next < self.spaces.len() {
            self.cards_scroll.scroll_to_item(next);
        }
        cx.notify();
    }

    fn toggle_focused_space(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.closed || self.pending || self.step != Step::Spaces {
            return;
        }
        if let Some(space) = self
            .spaces
            .iter()
            .find(|space| space.focus.is_focused(window))
        {
            if !self.selected_spaces.remove(&space.id) {
                self.selected_spaces.insert(space.id);
            }
            cx.notify();
        }
    }

    fn move_space_focus(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.closed || self.pending || self.step != Step::Spaces || self.spaces.is_empty() {
            return;
        }
        let current = self
            .spaces
            .iter()
            .position(|space| space.focus.is_focused(window))
            .unwrap_or(0);
        let next = if backwards {
            current.saturating_sub(1)
        } else {
            (current + 1).min(self.spaces.len() - 1)
        };
        self.spaces[next].focus.focus(window);
        self.cards_scroll.scroll_to_item(next);
        cx.notify();
    }

    fn open_dropdown(&mut self, cx: &mut Context<Self>) {
        if self.closed || self.pending || self.step != Step::Switch {
            return;
        }
        self.dropdown_open = true;
        self.dropdown_reveal_on_layout.set(true);
        self.dropdown_index = self
            .profiles
            .iter()
            .position(|profile| Some(profile.id) == self.selected_profile)
            .unwrap_or(0);
        self.dropdown_scroll.scroll_to_item(self.dropdown_index);
        cx.notify();
    }

    fn move_choice(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.closed || self.pending || self.step != Step::Switch {
            return;
        }
        if !self.dropdown_open {
            self.open_dropdown(cx);
            return;
        }
        self.dropdown_index =
            (self.dropdown_index as isize + delta).clamp(0, self.profiles.len() as isize) as usize;
        self.dropdown_scroll.scroll_to_item(self.dropdown_index);
        cx.notify();
    }

    fn choose_dropdown(&mut self, cx: &mut Context<Self>) {
        if self.closed || self.pending || self.step != Step::Switch {
            return;
        }
        self.dropdown_open = false;
        if let Some(profile) = self.profiles.get(self.dropdown_index) {
            self.selected_profile = Some(profile.id);
        } else {
            self.closed = true;
            cx.emit(DialogEvent::NewProfile(self.source));
        }
        cx.notify();
    }

    fn confirmation_heading(&self) -> String {
        let destination = self
            .profiles
            .iter()
            .find(|profile| Some(profile.id) == self.selected_profile)
            .map(|profile| profile.name.as_str())
            .unwrap_or_default();
        format!("Switch to your {destination} Profile?")
    }

    fn switch_heading_extra_height(heading: &str, window: &Window) -> f32 {
        let mut style = window.text_style().clone();
        style.font_weight = gpui::FontWeight::BOLD;
        let lines = window
            .text_system()
            .shape_text(
                heading.to_owned().into(),
                px(18.),
                &[style.to_run(heading.len())],
                Some(px(374.)),
                None,
            )
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| line.wrap_boundaries.len() + 1)
                    .sum::<usize>()
            })
            .unwrap_or(1)
            .max(1);
        (lines - 1) as f32 * 19.
    }

    fn confirmation_body(&self) -> String {
        let space = &self.switch_space.as_ref().expect("switch source").1;
        format!(
            "Each Profile has different browser data, so your {space} Space might look a little different! For example, you might get logged out of some websites, see different Favorites up top, or need to re-install an extension."
        )
    }

    fn button(
        &self,
        label: &'static str,
        selector: &'static str,
        primary: bool,
        enabled: bool,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(selector)
            .debug_selector(move || selector.into())
            .key_context("UserProfileButton")
            .track_focus(if primary {
                &self.primary_focus
            } else {
                &self.secondary_focus
            })
            .focus(|style| style.bg(rgb(0x716be7)))
            // The sheet tracks its own focus handle. Suppress its default
            // mouse-down focus transfer for buttons, particularly disabled
            // Next, so the name editor keeps receiving subsequent typing.
            // Step-changing click handlers explicitly focus their new target.
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.prevent_default()
            })
            .px(px(15.))
            .py(px(7.))
            .when(matches!(self.step, Step::Name | Step::Spaces), |button| {
                button.h(px(36.)).flex_none().justify_center()
            })
            .when(self.step == Step::Switch, |button| {
                button.h(px(36.)).px(px(12.)).flex_none().justify_center()
            })
            .rounded(px(7.))
            .text_size(px(13.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(label)
            .child({
                let keycap_selector = if primary {
                    "profile-enter-keycap"
                } else if selector == "profile-back" {
                    "profile-back-keycap"
                } else {
                    "profile-cancel-keycap"
                };
                div()
                    .debug_selector(move || keycap_selector.into())
                    .h(px(18.))
                    .min_w(px(23.))
                    .px(px(3.))
                    .rounded(px(3.))
                    .border_1()
                    .border_color(rgb(0x8d86bf))
                    .text_size(px(9.))
                    .line_height(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(primary, |keycap| {
                        keycap.child(icons::control_sized(Icon::Enter, 14.))
                    })
                    .when(!primary, |keycap| keycap.child("ESC"))
            })
            .bg(if primary {
                rgb(0x5851eb)
            } else {
                rgb(0x383456)
            })
            .text_color(rgb(0xffffff))
            .when(!enabled, |element| element.opacity(0.35))
            .when(enabled, |element| element.cursor_pointer())
    }

    fn profile_illustration(&self) -> AnyElement {
        // Echo's own browser-window geometry, with no reference-brand assets.
        div()
            .debug_selector(|| "profile-illustration".into())
            .size(px(64.))
            .flex_none()
            .rounded(px(16.))
            .bg(rgb(0x383456))
            .text_color(rgb(0xc7c4ff))
            .flex()
            .items_center()
            .justify_center()
            .child(icons::control_sized(Icon::Sidebar, 40.))
            .into_any_element()
    }

    fn space_cards(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("profile-space-cards")
            .debug_selector(|| "profile-space-cards".into())
            .h(px(154.))
            .flex_none()
            .flex()
            .gap(px(12.))
            .track_scroll(&self.cards_scroll)
            .overflow_x_scroll()
            .children(self.spaces.iter().enumerate().map(|(index, space)| {
                let id = space.id;
                let selected = self.selected_spaces.contains(&id);
                let selector = format!("profile-space-{index}");
                div()
                    .id(("profile-space", index))
                    .debug_selector(move || selector.clone())
                    .key_context("UserProfileSpace")
                    .track_focus(&space.focus)
                    .rounded(px(10.))
                    .focus(|style| style.bg(rgb(0x51496f)))
                    .w(px(64.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.closed || this.pending {
                            return;
                        }
                        if !this.selected_spaces.remove(&id) {
                            this.selected_spaces.insert(id);
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .debug_selector(move || format!("profile-space-tile-{index}"))
                            .h(px(95.))
                            .w_full()
                            .flex_none()
                            .rounded(px(10.))
                            .bg(hsla(space.hue / 360., 0.35, 0.42, 1.)),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("profile-space-check-{index}"))
                            .mt(px(4.))
                            .size(px(21.))
                            .flex_none()
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(0xf5f5f5))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(selected, |circle| {
                                circle
                                    .bg(rgb(0xf5f5f5))
                                    .text_color(rgb(0x35353b))
                                    .child(icons::control_sized(Icon::Check, 15.))
                            }),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("profile-space-label-{index}"))
                            .text_size(px(12.))
                            .line_height(px(18.))
                            .child(space.name.clone()),
                    )
            }))
            .into_any_element()
    }

    fn profile_dropdown(
        &self,
        max_height: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self
            .profiles
            .iter()
            .find(|profile| Some(profile.id) == self.selected_profile)
            .map(|profile| profile.name.clone())
            .unwrap_or_default();
        let mut style = window.text_style().clone();
        style.font_weight = gpui::FontWeight::NORMAL;
        let text_width = window
            .text_system()
            .shape_line(
                selected.clone().into(),
                px(13.),
                &[style.to_run(selected.len())],
                None,
            )
            .width;
        // A max-width alone lets the flex column expand to the cap. Size the
        // pill from its actual label plus 20 px padding, 6 px gap and 18 px
        // vector chevron, leaving the Space label its remaining row width.
        let pill_width = (f32::from(text_width).ceil() + 44.).min(160.);
        div()
            .relative()
            .w(px(pill_width))
            .max_w(px(160.))
            .min_w_0()
            .flex_none()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("profile-dropdown")
                    .debug_selector(|| "profile-dropdown".into())
                    .key_context("UserProfileDropdown")
                    .track_focus(&self.dropdown_focus)
                    .focus(|style| style.bg(rgb(0x51496f)))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(24.))
                    .px(px(10.))
                    .min_w_0()
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::NORMAL)
                    .rounded(px(7.))
                    .bg(rgb(0x383456))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.closed && !this.pending {
                            if this.dropdown_open {
                                this.dropdown_open = false;
                                cx.notify();
                            } else {
                                this.open_dropdown(cx);
                            }
                        }
                    }))
                    .child(div().min_w_0().truncate().child(selected))
                    .child(icons::control(Icon::Down)),
            )
            .when(self.dropdown_open, |list| {
                list.child(
                    gpui::deferred(
                        gpui::anchored()
                            .position_mode(gpui::AnchoredPositionMode::Local)
                            .position(gpui::point(px(0.), px(28.)))
                            .snap_to_window_with_margin(px(16.))
                            .child(
                                div()
                                    .on_children_prepainted({
                                        let pending = self.dropdown_reveal_on_layout.clone();
                                        let scroll = self.dropdown_scroll.clone();
                                        let index = self.dropdown_index;
                                        move |_, window, _| {
                                            // The initial prepaint establishes ScrollHandle's
                                            // viewport/overflow. Reveal again with those valid
                                            // bounds; subsequent user scrolling stays free.
                                            if pending.replace(false) {
                                                scroll.scroll_to_item(index);
                                                window.refresh();
                                            }
                                        }
                                    })
                                    .id("profile-dropdown-options")
                                    .debug_selector(|| "profile-dropdown-options".into())
                                    // Deferred menus paint above the footer. Their
                                    // hitbox must also block controls underneath,
                                    // before those controls can arm a click.
                                    .block_mouse_except_scroll()
                                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .max_h(px(max_height))
                                    .min_w(px(160.))
                                    .max_w(px(374.))
                                    .overflow_y_scroll()
                                    .track_scroll(&self.dropdown_scroll)
                                    .rounded(px(7.))
                                    .bg(rgb(0x383456))
                                    .py(px(4.))
                                    .children(self.profiles.iter().enumerate().map(
                                        |(index, profile)| {
                                            let id = profile.id;
                                            let selector = format!("profile-choice-{index}");
                                            div()
                                                .id(("profile-choice", index))
                                                .debug_selector(move || selector.clone())
                                                .px(px(10.))
                                                .h(px(32.))
                                                .flex_none()
                                                .min_w_0()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.))
                                                .justify_between()
                                                .cursor_pointer()
                                                .when(index == self.dropdown_index, |row| {
                                                    row.bg(rgb(0x51496f))
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    if this.closed || this.pending {
                                                        return;
                                                    }
                                                    this.selected_profile = Some(id);
                                                    this.dropdown_open = false;
                                                    cx.notify();
                                                }))
                                                // Keep the complete name as text; truncate
                                                // only its visual layout, never the model.
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            format!("profile-choice-label-{index}")
                                                        })
                                                        .flex_1()
                                                        .min_w_0()
                                                        .truncate()
                                                        .child(profile.name.clone()),
                                                )
                                                .when(Some(id) == self.selected_profile, |row| {
                                                    row.child(icons::control_sized(
                                                        Icon::Check,
                                                        15.,
                                                    ))
                                                })
                                        },
                                    ))
                                    .child(
                                        div()
                                            .id("profile-new")
                                            .debug_selector(|| "profile-new".into())
                                            .px(px(10.))
                                            .h(px(32.))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .border_t_1()
                                            .border_color(rgb(0x606066))
                                            .cursor_pointer()
                                            .child(div().min_w_0().truncate().child("New Profile"))
                                            .when(
                                                self.dropdown_index == self.profiles.len(),
                                                |row| row.bg(rgb(0x51496f)),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                cx.stop_propagation();
                                                if this.closed || this.pending {
                                                    return;
                                                }
                                                this.closed = true;
                                                cx.emit(DialogEvent::NewProfile(this.source));
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    )
                    .with_priority(1),
                )
            })
            .into_any_element()
    }
}

impl Render for UserProfileDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.closed {
            return div().into_any_element();
        }
        let heading = match self.step {
            Step::Name => "Create a new Profile".to_string(),
            Step::Spaces => "Which Spaces should use this Profile?".to_string(),
            Step::Switch => format!("Change {}’s Profile", self.switch_space.as_ref().unwrap().1),
            Step::ConfirmSwitch => self.confirmation_heading(),
        };
        let creation = matches!(self.step, Step::Name | Step::Spaces);
        let switch_extra = if self.step == Step::Switch {
            Self::switch_heading_extra_height(&heading, window)
        } else {
            0.
        };
        let available_height = (f32::from(window.viewport_size().height) - 32.).max(0.);
        // Reserve the closed sheet's chrome and a gutter. Short lists retain
        // their natural height; large lists scroll rather than growing forever.
        let dropdown_height = (available_height - 338. - switch_extra).clamp(32., 240.);
        let mut sheet = div()
            .id("user-profile-sheet")
            .debug_selector(|| "user-profile-sheet".into())
            .w(px(450.))
            .max_w_full()
            .relative()
            .flex_none()
            .when(creation, |sheet| {
                // A definite height prevents flex shrink from collapsing the
                // reference's vertical rhythm into the top of a taller card.
                sheet.h(px(if self.step == Step::Spaces {
                    506.
                } else {
                    426.
                }))
            })
            .when(self.step == Step::ConfirmSwitch, |sheet| {
                sheet.min_h(px(425.))
            })
            .when(self.step == Step::Switch, |sheet| {
                sheet.h(px(338. + switch_extra)).max_h(px(available_height))
            })
            .p(px(38.))
            .rounded(px(16.))
            .bg(rgb(0x151c2e))
            .text_color(rgb(0xffffff))
            .text_size(px(14.))
            .line_height(px(18.))
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(if creation || self.step == Step::Switch {
                0.
            } else {
                16.
            }))
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Accept, window, cx| this.accept(window, cx)))
            .on_action(cx.listener(|this, _: &ActivateButton, window, cx| this.accept(window, cx)))
            .on_action(
                cx.listener(|this, _: &NextFocus, window, cx| this.move_focus(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousFocus, window, cx| {
                    this.move_focus(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &ToggleSpace, window, cx| {
                this.toggle_focused_space(window, cx)
            }))
            .on_action(cx.listener(|this, _: &NextSpace, window, cx| {
                this.move_space_focus(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PreviousSpace, window, cx| {
                this.move_space_focus(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &OpenDropdown, _, cx| {
                if this.dropdown_open {
                    this.choose_dropdown(cx);
                } else {
                    this.open_dropdown(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextChoice, _, cx| this.move_choice(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousChoice, _, cx| this.move_choice(-1, cx)))
            .on_action(cx.listener(|this, _: &FirstChoice, _, cx| {
                this.open_dropdown(cx);
                this.dropdown_index = 0;
                this.dropdown_scroll.scroll_to_item(0);
            }))
            .on_action(cx.listener(|this, _: &LastChoice, _, cx| {
                this.open_dropdown(cx);
                this.dropdown_index = this.profiles.len();
                this.dropdown_scroll.scroll_to_item(this.dropdown_index);
            }))
            .on_action(cx.listener(|this, _: &Cancel, window, cx| {
                if this.dropdown_open {
                    this.dropdown_open = false;
                    cx.notify();
                } else if this.step == Step::Spaces {
                    this.back(window, cx);
                } else {
                    this.close(cx);
                }
            }))
            .child(self.profile_illustration())
            .child(
                div()
                    .debug_selector(|| "profile-heading".into())
                    .flex_none()
                    .when(creation, |heading| heading.mt(px(18.)).h(px(22.)))
                    .when(self.step == Step::Switch, |heading| {
                        heading
                            .absolute()
                            .left(px(38.))
                            .right(px(38.))
                            .top(px(126.))
                            .h(px(22. + switch_extra))
                    })
                    .text_size(px(18.))
                    .line_height(px(22.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .when(self.step == Step::Switch, |heading| {
                        // 19 px wrapped leading, with 1.5 px above/below:
                        // the first baseline stays identical to the 22 px
                        // single-line box; each extra line adds exactly 19 px.
                        heading.line_height(px(19.)).py(px(1.5))
                    })
                    .child(heading),
            );
        sheet = match self.step {
            Step::Name => sheet
                .child(div().debug_selector(|| "profile-body".into())
                    .flex_none().mt(px(12.)).h(px(72.))
                    .text_color(rgb(0xb8b8c0))
                    .child("Profiles make your Spaces truly separate — with their own history, saved passwords, extensions, and other browsing data. No more flipping between work and personal accounts!"))
                .child(div().debug_selector(|| "profile-name-label".into())
                    .flex_none().mt(px(18.)).h(px(18.))
                    .child("What do you want to call this Profile?"))
                .child(div().id("profile-name").debug_selector(|| "profile-name".into())
                    .flex_none().mt(px(6.)).h(px(48.)).px(px(12.))
                    .flex().items_center().rounded(px(7.)).border_1()
                    .border_color(rgb(0x444650)).bg(rgb(0x101728))
                    .child(self.input.clone())),
            Step::Spaces => sheet
                .child(div().debug_selector(|| "profile-body".into())
                    .flex_none().mt(px(12.)).h(px(90.))
                    .text_color(rgb(0xb8b8c0))
                    .child("These Spaces will start fresh in your brand new Profile — so you’ll have to log back in to websites, re-save your passwords, add extensions, etc. You can always switch back to your previous Profile though if you forgot something!"))
                .child(div().flex_none().mt(px(10.)).child(self.space_cards(cx))),
            Step::Switch => sheet
                .child(div().debug_selector(|| "profile-body".into())
                    .absolute().left(px(38.)).right(px(38.)).top(px(160. + switch_extra)).h(px(54.))
                    .text_color(rgb(0xb8b8c0)).child(SWITCH_WARNING))
                .child(div().debug_selector(|| "profile-switch-row".into())
                    .absolute().left(px(38.)).right(px(38.)).top(px(228. + switch_extra)).h(px(24.))
                    .flex().items_center().gap(px(8.))
                    .child(div().debug_selector(|| "profile-switch-label".into())
                        .min_w_0().truncate().text_size(px(12.)).font_weight(gpui::FontWeight::BOLD)
                        .child(format!("{}’s Profile", self.switch_space.as_ref().unwrap().1)))
                    .child(self.profile_dropdown(dropdown_height, window, cx))),
            Step::ConfirmSwitch => sheet
                .child(div().debug_selector(|| "profile-switch-warning".into()).child(self.confirmation_body()))
                .child(div().debug_selector(|| "profile-switch-irreversible".into()).child("This action cannot be undone.")),
        };
        if let Some(error) = &self.error {
            sheet = sheet.child(
                div()
                    .text_color(rgb(0xffb9ae))
                    .when(creation, |error| {
                        error
                            .absolute()
                            .left(px(38.))
                            .right(px(38.))
                            .top(px(if self.step == Step::Name { 344. } else { 420. }))
                            .text_size(px(12.))
                    })
                    .child(error.clone()),
            );
        }
        let enabled = self.enabled(cx);
        let label = match self.step {
            Step::Name => "Next",
            Step::Spaces => "Create Profile",
            Step::Switch => "Change Profile",
            Step::ConfirmSwitch => "Switch it up",
        };
        sheet = sheet.child(
            div()
                .debug_selector(|| "profile-footer".into())
                .flex()
                .flex_none()
                .when(creation, |footer| {
                    footer.absolute().left(px(38.)).right(px(38.)).top(px(
                        if self.step == Step::Spaces {
                            442.
                        } else {
                            362.
                        },
                    ))
                })
                // Auto margins belong only to the flow-based confirmation
                // footer. On the absolute switching footer they add free
                // sheet height on top of its measured 274 px inset.
                .when(self.step == Step::ConfirmSwitch, |footer| footer.mt_auto())
                .when(self.step == Step::Switch, |footer| {
                    footer
                        .absolute()
                        .left(px(38.))
                        .right(px(38.))
                        .top(px(274. + switch_extra))
                        .h(px(36.))
                })
                .justify_end()
                .gap(px(8.))
                .items_center()
                .child(div().when(self.step == Step::Spaces, |row| {
                    row.child(
                        self.button("Back", "profile-back", false, !self.pending)
                            .w(px(97.))
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                    )
                }))
                .child(
                    div()
                        .flex()
                        .gap(px(if creation || self.step == Step::Switch {
                            8.
                        } else {
                            9.
                        }))
                        .when(self.step != Step::Spaces, |buttons| {
                            buttons.child(
                                self.button("Cancel", "profile-cancel", false, true)
                                    .when(creation, |button| button.w(px(110.)))
                                    .when(self.step == Step::Switch, |button| button.w(px(110.)))
                                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                            )
                        })
                        .child(
                            self.button(label, "profile-accept", true, enabled)
                                .when(creation, |button| {
                                    button.w(px(if self.step == Step::Spaces { 142. } else { 88. }))
                                })
                                .when(self.step == Step::Switch, |button| button.w(px(148.)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.accept_primary(window, cx)
                                })),
                        ),
                ),
        );
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(sheet)
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "user_profile_dialog_tests.rs"]
mod tests;
