//! GPUI counterpart of Arc's Chromium Payments settings page
//! (`echo://settings/payments`): the profile's payment preferences and its
//! locally saved cards, sealed by `payment_store`. Google Wallet, Google Pay
//! and card benefits depend on a Google account and are not part of it.
use crate::{
    icons,
    payment_store::{CardSummary, Storage},
    text_input::{Changed, TextInput},
    window::BrowserWindow,
};
use browser_core::{
    payments::{expiration_label, CardError, CardInput, PaymentSettings},
    UserProfileId,
};
use chrono::Datelike;
use gpui::{div, prelude::*, px, rgb, AnyElement, Context, Entity, FocusHandle, Focusable, Subscription, Window};

pub(crate) const URL: &str = "echo://settings/payments";
pub(crate) const PAGE_TITLE: &str = "Settings - Payment methods";
const HELP: &str = "https://support.google.com/chrome/answer/142893";
const CARD: u32 = 0x292a2d;
const DIVIDER: u32 = 0x3c4043;
const SECONDARY: u32 = 0xbdc1c6;

gpui::actions!(payments_page, [Accept, Cancel, Next, Previous]);
pub(crate) fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("enter", Accept, Some("PaymentsEditor")),
        gpui::KeyBinding::new("escape", Cancel, Some("PaymentsPage")),
        gpui::KeyBinding::new("escape", Cancel, Some("PaymentsEditor")),
        gpui::KeyBinding::new("tab", Next, Some("PaymentsEditor")),
        gpui::KeyBinding::new("shift-tab", Previous, Some("PaymentsEditor")),
    ]);
}

pub(crate) fn is_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| url.scheme() == "echo" && url.host_str() == Some("settings") && url.path() == "/payments")
}

fn key(profile: UserProfileId) -> String {
    profile.storage_key().unwrap_or_else(|| "default".into())
}

fn today() -> (u16, u8) {
    let now = chrono::Local::now();
    (now.year() as u16, now.month() as u8)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Picker {
    Month,
    Year,
}

struct Editor {
    owner: UserProfileId,
    id: Option<u64>,
    number: Entity<TextInput>,
    name: Entity<TextInput>,
    nickname: Entity<TextInput>,
    cvc: Entity<TextInput>,
    month: u8,
    year: u16,
    picker: Option<Picker>,
    error: Option<CardError>,
    saving: bool,
    cancel: FocusHandle,
    save: FocusHandle,
    _changed: Vec<Subscription>,
}

pub(crate) struct State {
    focus: FocusHandle,
    owner: Option<UserProfileId>,
    cards: Vec<CardSummary>,
    loaded: bool,
    failed: bool,
    generation: u64,
    menu: Option<u64>,
    editor: Option<Editor>,
}

impl State {
    pub(crate) fn new(cx: &mut Context<BrowserWindow>) -> Self {
        Self { focus: cx.focus_handle(), owner: None, cards: Vec::new(), loaded: false, failed: false, generation: 0, menu: None, editor: None }
    }
}

pub(crate) fn open_from_settings(
    profile: crate::profile::ProfileHandle,
    owner: Option<gpui::WeakEntity<BrowserWindow>>,
    selected: UserProfileId,
    cx: &mut gpui::App,
) {
    cx.defer(move |cx| open_from_settings_now(profile, owner, selected, true, cx));
}

fn open_from_settings_now(
    profile: crate::profile::ProfileHandle,
    owner: Option<gpui::WeakEntity<BrowserWindow>>,
    selected: UserProfileId,
    create: bool,
    cx: &mut gpui::App,
) {
    if crate::quit::is_quitting() {
        return;
    }
    let preferred = owner.and_then(|owner| owner.upgrade()).map(|owner| owner.read(cx).main_window);
    let target = cx
        .windows()
        .into_iter()
        .filter_map(|window| window.downcast::<BrowserWindow>())
        .filter(|window| {
            window.read(cx).is_ok_and(|w| {
                !w.browser.is_private() && !w.browser.is_blank() && w.transient.is_none() && w.peek_owner.is_none() && w.profile.handle.same_workspace(&profile)
            })
        })
        .min_by_key(|window| if preferred.is_some_and(|p| p.window_id() == window.window_id()) { 0 } else { 1 });
    if let Some(target) = target {
        let _ = target.update(cx, |this, window, cx| this.open_payments(selected, window, cx));
    } else if create {
        crate::new_window::open(false, cx);
        cx.defer(move |cx| open_from_settings_now(profile, None, selected, false, cx));
    }
}

impl BrowserWindow {
    /// The profile whose payment methods the active tab shows.
    pub(crate) fn payments_owner(&self) -> Option<UserProfileId> {
        if self.browser.is_private() || self.browser.active_split().is_some() {
            return None;
        }
        let tab = self.browser.active_tab()?;
        if !is_url(&tab.url) {
            return None;
        }
        let url = url::Url::parse(&tab.url).ok()?;
        let owner = match url.query_pairs().find(|(key, _)| key == "profile") {
            Some((_, id)) => serde_json::from_str(&id).ok()?,
            None => self.browser.active_user_profile_id(),
        };
        self.profile.handle.read_settings(|b| b.user_profile(owner).map(|_| owner))
    }

    pub(crate) fn open_payments(&mut self, owner: UserProfileId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.profile.handle.read_settings(|b| b.user_profile(owner).is_some()) {
            return;
        }
        self.close_command_bar(cx);
        self.close_tab_menu(cx);
        let url = format!("{URL}?profile={}", serde_json::to_string(&owner).expect("Profile ID serializes"));
        let existing = self.browser.space().today.iter().chain(&self.browser.space().pinned).find(|t| t.url == url).map(|t| t.id);
        let id = self.edit_browser(|b| match existing {
            Some(id) => {
                b.activate_tab(id, std::time::SystemTime::now());
                id
            }
            None => b.new_tab(url, std::time::SystemTime::now()),
        });
        self.edit_browser(|b| b.set_title(id, PAGE_TITLE));
        self.sidebar_navigation.controller.select(&self.browser, crate::sidebar_pages::SidebarPage::Space(self.browser.space().id));
        self.sync_payments(cx);
        self.payments.focus.focus(window);
        window.activate_window();
        cx.notify();
    }

    pub(crate) fn sync_payments(&mut self, cx: &mut Context<Self>) {
        let owner = self.payments_owner();
        if let Some(id) = self.browser.active_tab().filter(|tab| owner.is_some() && tab.page_title.as_deref() != Some(PAGE_TITLE)).map(|tab| tab.id) {
            self.edit_browser(|browser| browser.set_title(id, PAGE_TITLE));
        }
        if self.payments.owner != owner {
            let generation = self.payments.generation + 1;
            self.payments = State::new(cx);
            self.payments.generation = generation;
            self.payments.owner = owner;
            self.load_payments(cx);
        }
    }

    /// Another window wrote this profile's cards.
    fn payments_changed(&mut self, profile: UserProfileId, cx: &mut Context<Self>) {
        if self.payments.owner == Some(profile) {
            self.load_payments(cx);
        }
    }

    fn load_payments(&mut self, cx: &mut Context<Self>) {
        let Some(owner) = self.payments.owner else { return };
        self.payments.generation += 1;
        let generation = self.payments.generation;
        let store = Storage::get(cx);
        let profile = key(owner);
        let task = cx.background_executor().spawn(async move { store.list(&profile) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.payments.generation != generation || this.payments.owner != Some(owner) {
                    return;
                }
                match result {
                    Ok(mut cards) => {
                        cards.sort_by_key(|c| c.id);
                        this.payments.cards = cards;
                        this.payments.failed = false;
                    }
                    Err(error) => {
                        eprintln!("{error}");
                        this.payments.failed = true;
                    }
                }
                this.payments.loaded = true;
                cx.notify();
            });
        })
        .detach();
    }

    fn payment_settings(&self) -> Option<PaymentSettings> {
        let owner = self.payments.owner?;
        self.profile.handle.read_settings(|b| b.user_profile(owner).map(|p| p.settings.payments.clone()))
    }

    fn edit_payment_settings(&mut self, edit: impl FnOnce(&mut PaymentSettings), cx: &mut Context<Self>) {
        let Some(owner) = self.payments.owner else { return };
        self.profile.handle.edit_settings(|b| {
            if let Some(profile) = b.user_profile_mut(owner) {
                edit(&mut profile.settings.payments);
            }
        });
        self.refresh_profile_projection();
        cx.notify();
    }

    fn present_payment_editor(&mut self, id: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.payments.owner else { return };
        self.payments.menu = None;
        let Some(id) = id else {
            let (year, month) = today();
            self.show_payment_editor(owner, None, None, (month, year), window, cx);
            return;
        };
        let Some(summary) = self.payments.cards.iter().find(|c| c.id == id).cloned() else { return };
        let store = Storage::get(cx);
        let profile = key(owner);
        let task = cx.background_executor().spawn(async move { store.secrets(&profile, id) });
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let Ok(secrets) = task.await else { return };
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    if this.payments.owner != Some(owner) {
                        return;
                    }
                    this.show_payment_editor(owner, Some(&summary), Some(secrets), (summary.month, summary.year), window, cx);
                });
            });
        })
        .detach();
    }

    fn show_payment_editor(
        &mut self,
        owner: UserProfileId,
        card: Option<&CardSummary>,
        secrets: Option<(zeroize::Zeroizing<String>, zeroize::Zeroizing<String>)>,
        (month, year): (u8, u16),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = |text: Option<&str>, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut input = TextInput::new("", cx);
                if let Some(text) = text {
                    input.set_text(text.to_owned(), cx);
                }
                input
            })
        };
        let (number, cvc) = match &secrets {
            Some((number, cvc)) => (Some(number.as_str()), Some(cvc.as_str())),
            None => (None, None),
        };
        let number = field(number, cx);
        let name = field(card.map(|c| c.name.as_str()), cx);
        let nickname = field(card.map(|c| c.nickname.as_str()), cx);
        let cvc = field(cvc, cx);
        let changed = [&number, &name, &nickname, &cvc].into_iter().map(|input| cx.subscribe(input, |_, _, _: &Changed, cx| cx.notify())).collect();
        number.read(cx).focus_handle(cx).focus(window);
        self.payments.editor = Some(Editor {
            owner,
            id: card.map(|c| c.id),
            number,
            name,
            nickname,
            cvc,
            month,
            year,
            picker: None,
            error: None,
            saving: false,
            cancel: cx.focus_handle(),
            save: cx.focus_handle(),
            _changed: changed,
        });
        cx.notify();
    }

    fn cancel_payment_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = &mut self.payments.editor {
            if editor.picker.take().is_none() {
                self.payments.editor = None;
                self.payments.focus.focus(window);
            }
        }
        self.payments.menu = None;
        cx.notify();
    }

    fn payment_editor_focus(&self, backwards: bool, window: &mut Window, cx: &Context<Self>) {
        let Some(e) = &self.payments.editor else { return };
        let save_codes = self.payment_settings().is_some_and(|s| s.save_security_codes);
        let mut handles = vec![e.number.read(cx).focus_handle(cx), e.name.read(cx).focus_handle(cx), e.nickname.read(cx).focus_handle(cx)];
        if save_codes {
            handles.push(e.cvc.read(cx).focus_handle(cx));
        }
        handles.extend([e.cancel.clone(), e.save.clone()]);
        let index = handles.iter().position(|f| f.is_focused(window)).unwrap_or(0);
        handles[if backwards { (index + handles.len() - 1) % handles.len() } else { (index + 1) % handles.len() }].focus(window);
    }

    fn accept_payment_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.payments.editor.as_ref().is_some_and(|e| e.cancel.is_focused(window)) {
            self.cancel_payment_editor(window, cx);
        } else {
            self.save_payment_editor(window, cx);
        }
    }

    fn save_payment_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let save_codes = self.payment_settings().is_some_and(|s| s.save_security_codes);
        let Some(e) = &mut self.payments.editor else { return };
        if e.saving || self.payments.owner != Some(e.owner) {
            return;
        }
        let input = CardInput {
            number: e.number.read(cx).text().trim().to_owned(),
            month: e.month,
            year: e.year,
            name: e.name.read(cx).text().trim().to_owned(),
            nickname: e.nickname.read(cx).text().trim().to_owned(),
            cvc: if save_codes { e.cvc.read(cx).text().trim().to_owned() } else { String::new() },
        };
        if let Err(error) = input.validate(today()) {
            e.error = Some(error);
            cx.notify();
            return;
        }
        e.error = None;
        e.saving = true;
        let (owner, id) = (e.owner, e.id);
        let store = Storage::get(cx);
        let profile = key(owner);
        // With "Save security codes" off the form has no CVC field and the
        // stored code is left as it is.
        let task = cx.background_executor().spawn(async move {
            let cvc = save_codes.then_some(input.cvc.as_str());
            store.upsert(&profile, id, &input, cvc)
        });
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    let Some(e) = &mut this.payments.editor else { return };
                    if e.owner != owner || e.id != id {
                        return;
                    }
                    e.saving = false;
                    match result {
                        Ok(_) => {
                            this.payments.editor = None;
                            this.payments.focus.focus(window);
                        }
                        Err(_) => e.error = Some(CardError::Number),
                    }
                    cx.notify();
                });
            });
            let _ = cx.update(|cx| broadcast_change(owner, cx));
        })
        .detach();
    }

    fn remove_payment_card(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(owner) = self.payments.owner else { return };
        self.payments.menu = None;
        self.payments.cards.retain(|c| c.id != id);
        let store = Storage::get(cx);
        let profile = key(owner);
        let task = cx.background_executor().spawn(async move { store.remove(&profile, id) });
        cx.spawn(async move |_, cx| {
            if let Err(error) = task.await {
                eprintln!("{error}");
            }
            let _ = cx.update(|cx| broadcast_change(owner, cx));
        })
        .detach();
        cx.notify();
    }

    fn open_payments_help(&mut self, cx: &mut Context<Self>) {
        self.edit_browser(|b| b.new_tab(HELP.to_owned(), std::time::SystemTime::now()));
        cx.notify();
    }

    fn payments_back(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.browser.active_tab().map(|t| t.id) else { return };
        self.edit_browser(|b| b.set_url(tab, crate::settings_shell::Section::Autofill.url()));
        cx.notify();
    }
}

/// Every window showing `profile`'s cards reloads them after a write.
fn broadcast_change(profile: UserProfileId, cx: &mut gpui::App) {
    for window in cx.windows().into_iter().filter_map(|w| w.downcast::<BrowserWindow>()) {
        let _ = window.update(cx, |this, _, cx| this.payments_changed(profile, cx));
    }
}

impl BrowserWindow {
    pub(crate) fn render_payments(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let page = self.render_payments_page(window, cx);
        self.settings_shell(crate::settings_shell::Section::Autofill, page, cx)
    }

    fn render_payments_page(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(settings) = self.payment_settings() else { return div().into_any_element() };
        let state = &self.payments;
        type Toggle = (&'static str, &'static str, Option<&'static str>, fn(&PaymentSettings) -> bool, fn(&mut PaymentSettings));
        let toggles: [Toggle; 4] = [
            ("save-and-fill", "Save and fill payment methods", Some("Fills in payment forms with your saved payment methods"), |s| s.save_and_fill, |s| s.save_and_fill = !s.save_and_fill),
            (
                "verify",
                "Verify it's you to autofill payment methods",
                Some("For added protection, always use your fingerprint, face, or other screen lock when you pay using autofill"),
                |s| s.verify_before_fill,
                |s| s.verify_before_fill = !s.verify_before_fill,
            ),
            ("save-cvc", "Save security codes", Some("Check out faster when your CVCs are saved"), |s| s.save_security_codes, |s| s.save_security_codes = !s.save_security_codes),
            ("sites-check", "Allow sites to check if you have payment methods saved", None, |s| s.sites_can_check, |s| s.sites_can_check = !s.sites_can_check),
        ];
        let header = div()
            .h(px(56.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .id("payments-back")
                    .debug_selector(|| "payments-back".into())
                    .size(px(40.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|b| b.bg(gpui::hsla(0., 0., 1., 0.08)))
                    .text_color(rgb(SECONDARY))
                    .child(icons::ionicon("arrow-back", 20., 1.))
                    .on_click(cx.listener(|this, _, _, cx| this.payments_back(cx))),
            )
            .child(div().flex_1().text_size(px(16.)).child("Payments"))
            .child(
                div()
                    .id("payments-help")
                    .debug_selector(|| "payments-help".into())
                    .size(px(40.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|b| b.bg(gpui::hsla(0., 0., 1., 0.08)))
                    .text_color(rgb(SECONDARY))
                    .child(icons::ionicon("help-circle-outline", 20., 1.))
                    .on_click(cx.listener(|this, _, _, cx| this.open_payments_help(cx))),
            );
        let preferences = card()
            .debug_selector(|| "payments-preferences".into())
            .child(header)
            .children(toggles.into_iter().map(|(name, title, description, get, flip)| {
                let on = get(&settings);
                row(true)
                    .id(gpui::SharedString::from(format!("payments-toggle-{name}")))
                    .debug_selector(move || format!("payments-toggle-{name}"))
                    .cursor_pointer()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_size(px(14.)).child(title))
                            .when_some(description, |text, description| text.child(div().text_color(rgb(SECONDARY)).child(description))),
                    )
                    .child(crate::settings_shell::switch(on))
                    .on_click(cx.listener(move |this, _, _, cx| this.edit_payment_settings(flip, cx)))
            }));
        let methods = card()
            .mt(px(16.))
            .debug_selector(|| "payments-methods".into())
            .child(
                div()
                    .h(px(56.))
                    .px(px(20.))
                    .flex()
                    .items_center()
                    .child(div().flex_1().text_size(px(14.)).font_weight(gpui::FontWeight::MEDIUM).child("Payment methods"))
                    .child(
                        outline_button("Add", 66.)
                            .id("payments-add")
                            .debug_selector(|| "payments-add".into())
                            .on_click(cx.listener(|this, _, window, cx| this.present_payment_editor(None, window, cx))),
                    ),
            )
            .when(state.loaded && state.cards.is_empty(), |list| {
                list.child(
                    row(true)
                        .debug_selector(|| "payments-empty".into())
                        .text_color(rgb(SECONDARY))
                        .child(if state.failed { "Saved payment methods could not be opened" } else { "Saved payment methods will appear here" }),
                )
            })
            .children(state.cards.iter().enumerate().map(|(index, card)| self.render_payment_card(index, card, cx)));
        self.echo_page_corners(div())
            .id("payments-page")
            .debug_selector(|| "payments-page".into())
            .on_click(cx.listener(|this, _, _, cx| {
                if this.payments.menu.take().is_some() {
                    cx.notify();
                }
            }))
            .absolute()
            .inset_0()
            .overflow_hidden()
            .whitespace_normal()
            .flex()
            .flex_col()
            .track_focus(&state.focus)
            .key_context("PaymentsPage")
            .when(state.editor.is_some(), |page| page.key_context("PaymentsEditor"))
            .on_action(cx.listener(|this, _: &Accept, window, cx| this.accept_payment_editor(window, cx)))
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.cancel_payment_editor(window, cx)))
            .on_action(cx.listener(|this, _: &Next, window, cx| this.payment_editor_focus(false, window, cx)))
            .on_action(cx.listener(|this, _: &Previous, window, cx| this.payment_editor_focus(true, window, cx)))
            .child(
                div()
                    .id("payments-scroll")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(div().w_full().max_w(px(680.)).mx_auto().pt(px(18.)).pb(px(40.)).child(preferences).child(methods)),
            )
            .when_some(state.editor.as_ref(), |page, editor| page.child(self.render_payment_editor(editor, settings.save_security_codes, window, cx)))
            .into_any_element()
    }

    fn render_payment_card(&self, index: usize, card: &CardSummary, cx: &mut Context<Self>) -> AnyElement {
        let id = card.id;
        let open = self.payments.menu == Some(id);
        row(true)
            .id(("payments-card", index))
            .debug_selector(move || format!("payments-card-{index}"))
            .gap(px(16.))
            .child(
                div()
                    .w(px(40.))
                    .h(px(26.))
                    .flex_none()
                    .rounded(px(4.))
                    .bg(rgb(0xe8eaed))
                    .text_color(rgb(0x202124))
                    .text_size(px(9.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(card.brand),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .gap(px(6.))
                    .child(div().text_size(px(14.)).truncate().child(card.label.clone()))
                    .child(div().text_color(rgb(SECONDARY)).child("·"))
                    .child(div().text_color(rgb(SECONDARY)).child(expiration_label(card.month, card.year))),
            )
            .child(
                div()
                    .id(("payments-more", index))
                    .debug_selector(move || format!("payments-more-{index}"))
                    .size(px(40.))
                    .flex_none()
                    .relative()
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|b| b.bg(gpui::hsla(0., 0., 1., 0.08)))
                    .text_color(rgb(SECONDARY))
                    .child(icons::ionicon("ellipsis-vertical-sharp", 18., 1.))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.payments.menu = if this.payments.menu == Some(id) { None } else { Some(id) };
                        cx.notify();
                    }))
                    .when(open, |button| {
                        button.child(
                            gpui::deferred(
                                div()
                                    .absolute()
                                    .right(px(8.))
                                    .top(px(34.))
                                    .w(px(128.))
                                    .py(px(8.))
                                    .rounded(px(4.))
                                    .bg(rgb(0x202124))
                                    .text_color(rgb(0xe3e3e3))
                                    .shadow_lg()
                                    .child(menu_item("Edit").on_click(cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.present_payment_editor(Some(id), window, cx);
                                    })))
                                    .child(menu_item("Remove").on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.remove_payment_card(id, cx);
                                    }))),
                            )
                            .with_priority(1),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_payment_editor(&self, e: &Editor, save_codes: bool, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let field = |label: &'static str, selector: &'static str, input: &Entity<TextInput>, error: bool| {
            let focused = input.read(cx).focus_handle(cx).is_focused(window);
            div()
                .mt(px(12.))
                .child(div().h(px(20.)).text_size(px(11.)).font_weight(gpui::FontWeight::MEDIUM).text_color(rgb(0xc4c7c5)).child(label))
                .child(
                    div()
                        .debug_selector(move || selector.into())
                        .h(px(36.))
                        .rounded_t(px(8.))
                        .bg(rgb(0x444746))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .border_b_1()
                        .when(focused, |f| f.border_b_2())
                        .border_color(rgb(if error { 0xf28b82 } else if focused { 0xa8c7fa } else { 0x9aa0a6 }))
                        .child(input.clone()),
                )
        };
        let picker = |kind: Picker, text: String, width: f32| {
            div()
                .id(if kind == Picker::Month { "payments-month" } else { "payments-year" })
                .debug_selector(move || if kind == Picker::Month { "payments-month".into() } else { "payments-year".into() })
                .w(px(width))
                .h(px(36.))
                .px(px(10.))
                .rounded(px(8.))
                .bg(rgb(0x444746))
                .flex()
                .items_center()
                .justify_between()
                .cursor_pointer()
                .child(text)
                .child(icons::ionicon("chevron-down-outline", 14., 1.))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(e) = &mut this.payments.editor {
                        e.picker = if e.picker == Some(kind) { None } else { Some(kind) };
                    }
                    cx.notify();
                }))
        };
        let (this_year, _) = today();
        let options: Vec<(u16, String)> = match e.picker {
            Some(Picker::Month) => (1..=12).map(|m| (m, format!("{m:02}"))).collect(),
            Some(Picker::Year) => (this_year..this_year + 20).map(|y| (y, y.to_string())).collect(),
            None => Vec::new(),
        };
        let error = e.error.map(|error| match error {
            CardError::Number => "Invalid card number",
            CardError::Expiration => "Invalid expiration date",
            CardError::Cvc => "Invalid security code",
            CardError::Nickname => "Nickname can't include numbers and must be 25 characters or fewer",
        });
        let dialog = div()
            .w(px(512.))
            .relative()
            .rounded(px(8.))
            .bg(rgb(CARD))
            .shadow_lg()
            .p(px(20.))
            .debug_selector(|| "payments-editor".into())
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .child(div().text_size(px(15.)).font_weight(gpui::FontWeight::MEDIUM).child(if e.id.is_some() { "Edit card" } else { "Add card" }))
            .child(field("Card number", "payments-field-number", &e.number, e.error == Some(CardError::Number)))
            .child(
                div()
                    .mt(px(12.))
                    .child(div().h(px(20.)).text_size(px(11.)).font_weight(gpui::FontWeight::MEDIUM).text_color(rgb(0xc4c7c5)).child("Expiration date"))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .gap(px(8.))
                            .child(picker(Picker::Month, format!("{:02}", e.month), 90.))
                            .child(picker(Picker::Year, e.year.to_string(), 110.))
                            .when(!options.is_empty(), |row| {
                                let kind = e.picker;
                                row.child(
                                    gpui::deferred(
                                        div()
                                            .id("payments-picker")
                                            .absolute()
                                            .top(px(40.))
                                            .left(px(if kind == Some(Picker::Year) { 98. } else { 0. }))
                                            .w(px(110.))
                                            .max_h(px(220.))
                                            .overflow_y_scroll()
                                            .py(px(4.))
                                            .rounded(px(4.))
                                            .bg(rgb(0x202124))
                                            .shadow_lg()
                                            .children(options.into_iter().map(|(value, text)| {
                                                let selected = if kind == Some(Picker::Month) { u16::from(e.month) == value } else { e.year == value };
                                                div()
                                                    .id(gpui::SharedString::from(format!("payments-option-{value}")))
                                                    .debug_selector(move || if kind == Some(Picker::Month) { format!("payments-month-{value}") } else { format!("payments-year-{value}") })
                                                    .h(px(30.))
                                                    .px(px(12.))
                                                    .flex()
                                                    .items_center()
                                                    .cursor_pointer()
                                                    .when(selected, |o| o.bg(gpui::hsla(0., 0., 1., 0.08)))
                                                    .hover(|o| o.bg(gpui::hsla(0., 0., 1., 0.12)))
                                                    .child(text)
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        if let Some(e) = &mut this.payments.editor {
                                                            match e.picker {
                                                                Some(Picker::Month) => e.month = value as u8,
                                                                Some(Picker::Year) => e.year = value,
                                                                None => (),
                                                            }
                                                            e.picker = None;
                                                        }
                                                        cx.notify();
                                                    }))
                                            })),
                                    )
                                    .with_priority(2),
                                )
                            }),
                    ),
            )
            .child(field("Name on card", "payments-field-name", &e.name, false))
            .child(field("Card nickname (optional)", "payments-field-nickname", &e.nickname, e.error == Some(CardError::Nickname)))
            .when(save_codes, |dialog| dialog.child(field("CVC", "payments-field-cvc", &e.cvc, e.error == Some(CardError::Cvc))))
            .when_some(error, |dialog, error| dialog.child(div().debug_selector(|| "payments-editor-error".into()).mt(px(8.)).text_color(rgb(0xf28b82)).child(error)))
            .child(
                div()
                    .mt(px(20.))
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        outline_button("Cancel", 76.)
                            .id("payments-editor-cancel")
                            .track_focus(&e.cancel)
                            .debug_selector(|| "payments-editor-cancel".into())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.payments.editor = None;
                                this.payments.focus.focus(window);
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("payments-editor-save")
                            .track_focus(&e.save)
                            .debug_selector(|| "payments-editor-save".into())
                            .w(px(67.))
                            .h(px(36.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(0xa8c7fa))
                            .text_color(rgb(0x062e6f))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .cursor_pointer()
                            .child("Save")
                            .on_click(cx.listener(|this, _, window, cx| this.save_payment_editor(window, cx))),
                    ),
            );
        div()
            .absolute()
            .inset_0()
            .bg(gpui::black().opacity(0.6))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|_, _, _, cx| cx.stop_propagation()))
            .child(dialog)
            .into_any_element()
    }
}

#[cfg(test)]
impl BrowserWindow {
    /// The list as shown: "Mastercard •• 4248 · 02/28".
    pub(crate) fn payment_rows(&self) -> Vec<String> {
        self.payments.cards.iter().map(|c| format!("{} · {}", c.label, expiration_label(c.month, c.year))).collect()
    }
    pub(crate) fn payment_editor_open(&self) -> bool {
        self.payments.editor.is_some()
    }
    pub(crate) fn payment_editor_error(&self) -> bool {
        self.payments.editor.as_ref().is_some_and(|e| e.error.is_some())
    }
}

fn card() -> gpui::Div {
    div().rounded(px(8.)).overflow_hidden().bg(rgb(CARD))
}

fn row(divider: bool) -> gpui::Div {
    div()
        .min_h(px(64.))
        .px(px(20.))
        .py(px(8.))
        .flex()
        .items_center()
        .gap(px(16.))
        .when(divider, |row| row.border_t_1().border_color(rgb(DIVIDER)))
}

fn outline_button(label: &'static str, width: f32) -> gpui::Div {
    div()
        .w(px(width))
        .h(px(36.))
        .flex_none()
        .rounded_full()
        .border_1()
        .border_color(rgb(0x0984b5))
        .text_color(rgb(0xa8c7fa))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(label)
}

fn menu_item(label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(label)
        .debug_selector(move || format!("payments-menu-{label}"))
        .h(px(36.))
        .px(px(24.))
        .flex()
        .items_center()
        .cursor_pointer()
        .hover(|b| b.bg(gpui::hsla(0., 0., 1., 0.08)))
        .child(label)
}
