//! The frame around every `echo://settings` page, as in Arc: a header with
//! the title and a settings search, the section list on the left and the page
//! in a column centred on the window. Narrow windows fold the list behind a
//! menu button. Pages without a richer view of their own render here too.
use crate::{icons, text_input::TextInput, window::BrowserWindow};
use gpui::{div, prelude::*, px, rgb, AnyElement, Context, Entity};

const NAV_WIDTH: f32 = 275.;
/// Below this width the section list folds behind the menu button.
const NARROW: f32 = 980.;
const BACKGROUND: u32 = 0x202124;
const CARD: u32 = 0x292a2d;
const DIVIDER: u32 = 0x3c4043;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    You,
    Autofill,
    Privacy,
    SearchEngine,
    DefaultBrowser,
    Downloads,
    Extensions,
    About,
}

impl Section {
    /// The list in Arc's order; `true` closes a group with a divider.
    const LIST: [(Section, bool); 8] = [
        (Section::You, false),
        (Section::Autofill, false),
        (Section::Privacy, false),
        (Section::SearchEngine, false),
        (Section::DefaultBrowser, true),
        (Section::Downloads, true),
        (Section::Extensions, false),
        (Section::About, false),
    ];
    fn label(self) -> &'static str {
        match self {
            Section::You => "You and Google",
            Section::Autofill => "Autofill and passwords",
            Section::Privacy => "Privacy and security",
            Section::SearchEngine => "Search engine",
            Section::DefaultBrowser => "Default browser",
            Section::Downloads => "Downloads",
            Section::Extensions => "Extensions",
            Section::About => "About Echo",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Section::You => "person-outline",
            Section::Autofill => "key-outline",
            Section::Privacy => "shield-checkmark-outline",
            Section::SearchEngine => "search-outline",
            Section::DefaultBrowser => "browsers-outline",
            Section::Downloads => "download-outline",
            Section::Extensions => "cube-outline",
            Section::About => "information-circle-outline",
        }
    }
    pub(crate) fn url(self) -> &'static str {
        match self {
            Section::You => "echo://settings/people",
            Section::Autofill => "echo://settings/autofill",
            Section::Privacy => crate::browsing_data::PRIVACY,
            Section::SearchEngine => "echo://settings/search",
            Section::DefaultBrowser => "echo://settings/defaultBrowser",
            Section::Downloads => "echo://settings/downloads",
            Section::Extensions => crate::extensions::URL,
            Section::About => "echo://settings/help",
        }
    }
    fn selector(self) -> String {
        format!("settings-nav-{self:?}")
    }
    /// The section a settings address belongs to.
    pub(crate) fn of(url: &str) -> Option<Section> {
        let url = url::Url::parse(url).ok()?;
        if url.scheme() != "echo" || url.host_str() != Some("settings") {
            return None;
        }
        Some(match url.path() {
            "" | "/" | "/people" => Section::You,
            "/autofill" => Section::Autofill,
            "/privacy" | "/clearBrowserData" | "/content" => Section::Privacy,
            path if path.starts_with("/content/") => Section::Privacy,
            "/search" | "/searchEngines" => Section::SearchEngine,
            "/defaultBrowser" => Section::DefaultBrowser,
            "/downloads" => Section::Downloads,
            "/help" => Section::About,
            _ => return None,
        })
    }
    /// Sections whose page is drawn by this frame alone.
    fn simple(url: &str) -> Option<Section> {
        let section = Section::of(url)?;
        let path = url::Url::parse(url).ok()?.path().to_owned();
        let own_page = matches!(path.as_str(), "/privacy" | "/clearBrowserData" | "/searchEngines") || path.starts_with("/content");
        (!own_page).then_some(section)
    }
}

/// What the settings search finds: a section and the rows on its page.
const INDEX: &[(Section, &str, &str)] = &[
    (Section::You, "Profile", "Your local profile name and picture"),
    (Section::Autofill, "Password Manager", "Saved passwords and passkeys"),
    (Section::Privacy, "Delete browsing data", "Delete history, cookies, cache, and more"),
    (Section::Privacy, "Site settings", "Controls what information sites can use and show"),
    (Section::Privacy, "Cookies", "Clear cookies and site data"),
    (Section::SearchEngine, "Search engine used in the address bar", "The default search engine"),
    (Section::SearchEngine, "Manage search engines and site search", "Shortcuts for searching sites"),
    (Section::DefaultBrowser, "Default browser", "Make Echo the default browser"),
    (Section::Downloads, "Location", "Where downloaded files are saved"),
    (Section::Downloads, "Ask where to save each file before downloading", "Choose a folder for each download"),
    (Section::Extensions, "Extensions", "Add and manage extensions"),
    (Section::About, "About Echo", "Version and updates"),
];

pub(crate) struct State {
    pub(crate) search: Entity<TextInput>,
    drawer: bool,
    width: std::rc::Rc<std::cell::Cell<f32>>,
}

impl State {
    pub(crate) fn new(cx: &mut Context<BrowserWindow>) -> Self {
        Self {
            search: cx.new(|cx| TextInput::new("Search settings", cx)),
            drawer: false,
            width: std::rc::Rc::new(std::cell::Cell::new(f32::MAX)),
        }
    }
}

impl BrowserWindow {
    /// A settings section drawn by the frame alone, if one is open.
    pub(crate) fn simple_settings_section(&self) -> Option<Section> {
        self.browser.active_tab().and_then(|tab| Section::simple(&tab.url))
    }

    fn open_settings_section(&mut self, section: Section, cx: &mut Context<Self>) {
        let Some(tab) = self.browser.active_tab().map(|tab| tab.id) else { return; };
        self.settings_shell.drawer = false;
        self.settings_shell.search.update(cx, |input, cx| input.set_text("", cx));
        self.edit_browser(|browser| browser.set_url(tab, section.url()));
        cx.notify();
    }

    /// `page` framed by the settings header and section list.
    pub(crate) fn settings_shell(&self, section: Section, page: impl IntoElement, cx: &mut Context<Self>) -> AnyElement {
        let wide = self.settings_shell.width.get() >= NARROW;
        let query = self.settings_shell.search.read(cx).text().trim().to_lowercase();
        let width = self.settings_shell.width.clone();
        let header = div()
            .h(px(72.))
            .flex_none()
            .flex()
            .items_center()
            .child(
                div()
                    .w(px(NAV_WIDTH))
                    .flex_none()
                    .pl(px(if wide { 30. } else { 20. }))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .when(!wide, |title| {
                        title.child(
                            div()
                                .id("settings-menu")
                                .debug_selector(|| "settings-menu".into())
                                .cursor_pointer()
                                .child(icons::ionicon("menu-outline", 20., 1.))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.settings_shell.drawer = !this.settings_shell.drawer;
                                    cx.notify();
                                })),
                        )
                    })
                    .child(icons::ionicon("settings-outline", 24., 1.))
                    .child(div().text_size(px(22.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Settings")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .justify_center()
                    .px(px(20.))
                    .when(wide, |row| row.mr(px(NAV_WIDTH - 20.)))
                    .child(
                        div()
                            .debug_selector(|| "settings-search".into())
                            .w_full()
                            .max_w(px(690.))
                            .h(px(36.))
                            .px(px(14.))
                            .rounded(px(18.))
                            .bg(rgb(0x282828))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .text_color(rgb(0xc4c7c5))
                            .child(icons::ionicon("search-outline", 16., 1.))
                            .child(div().flex_1().min_w_0().text_color(rgb(0xe3e3e3)).child(self.settings_shell.search.clone())),
                    ),
            );
        let nav = div()
            .debug_selector(|| "settings-nav".into())
            .w(px(NAV_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .children(Section::LIST.iter().map(|&(item, divider)| {
                let selected = item == section && query.is_empty();
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id(gpui::SharedString::from(item.selector()))
                            .debug_selector(move || item.selector())
                            .w(px(262.))
                            .h(px(40.))
                            .pl(px(33.))
                            .flex()
                            .items_center()
                            .gap(px(20.))
                            .rounded_r(px(20.))
                            .cursor_pointer()
                            .when(selected, |row| row.bg(rgb(0x8ab4f8)).text_color(rgb(BACKGROUND)))
                            .when(!selected, |row| row.hover(|row| row.bg(gpui::hsla(0., 0., 1., 0.06))))
                            .child(icons::ionicon(item.icon(), 18., 1.))
                            .child(div().text_size(px(14.)).font_weight(gpui::FontWeight::MEDIUM).child(item.label()))
                            .when(item == Section::Extensions, |row| row.child(icons::ionicon("open-outline", 16., 1.)))
                            .on_click(cx.listener(move |this, _, _, cx| this.open_settings_section(item, cx))),
                    )
                    .when(divider, |group| group.child(div().ml(px(10.)).my(px(8.)).w(px(265.)).h(px(1.)).bg(rgb(0x37383a))))
            }));
        // The list shows beside the page when there is room, else in a drawer.
        let (side, drawer) = if wide { (Some(nav), None) } else if self.settings_shell.drawer { (None, Some(nav)) } else { (None, None) };
        let body = if query.is_empty() {
            div().size_full().relative().child(page).into_any_element()
        } else {
            self.settings_search_results(&query, cx)
        };
        div()
            .id("settings-shell")
            .debug_selector(|| "settings-shell".into())
            .absolute()
            .inset_0()
            .overflow_hidden()
            .whitespace_normal()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(0xe3e3e3))
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .line_height(px(20.))
            .flex()
            .flex_col()
            .child(gpui::canvas(move |bounds, _, _| width.set(f32::from(bounds.size.width)), |_, _, _, _| {}).absolute().inset_0())
            .child(header)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .flex()
                    .children(side)
                    .child(div().flex_1().min_w_0().h_full().relative().when(wide, |page| page.mr(px(NAV_WIDTH))).child(body))
                    .children(drawer.map(|nav| div().absolute().top_0().left_0().bottom_0().bg(rgb(BACKGROUND)).shadow_lg().child(nav))),
            )
            .into_any_element()
    }

    fn settings_search_results(&self, query: &str, cx: &mut Context<Self>) -> AnyElement {
        let found: Vec<_> = INDEX
            .iter()
            .filter(|(section, title, description)| {
                [section.label(), title, description].iter().any(|text| text.to_lowercase().contains(query))
            })
            .copied()
            .collect();
        column(
            div()
                .child(heading("Search results"))
                .child(if found.is_empty() {
                    card().child(div().h(px(64.)).px(px(20.)).flex().items_center().text_color(rgb(0xbdc1c6)).child("No search results found"))
                } else {
                    card().children(found.into_iter().enumerate().map(|(index, (section, title, description))| {
                        row(index, title, Some(description.into()))
                            .id(("settings-result", index))
                            .debug_selector(move || format!("settings-result-{index}"))
                            .cursor_pointer()
                            .child(div().text_color(rgb(0xbdc1c6)).child(section.label()))
                            .on_click(cx.listener(move |this, _, _, cx| this.open_settings_section(section, cx)))
                    }))
                }),
        )
    }

    /// A section with no richer page of its own.
    pub(crate) fn render_simple_settings(&self, section: Section, cx: &mut Context<Self>) -> AnyElement {
        let content = match section {
            Section::You => div().child(heading("You and Google")).child(card().child(
                row_with(0, Some(icons::ionicon("person-circle", 40., 1.)), self.browser.active_user_profile().name().to_owned(), None)
                    .pl(px(16.))
                    .id("settings-profile")
                    .debug_selector(|| "settings-profile".into())
                    .cursor_pointer()
                    .child(icons::ionicon("chevron-forward-outline", 16., 1.))
                    .on_click(cx.listener(|_, _, _, cx| crate::settings::open_section(crate::settings::Tab::Profiles, cx))),
            )),
            Section::Autofill => div().child(heading("Autofill and passwords")).child(card().child(
                row(0, "Password Manager", None)
                    .id("settings-password-manager")
                    .debug_selector(|| "settings-password-manager".into())
                    .cursor_pointer()
                    .child(icons::ionicon("chevron-forward-outline", 16., 1.))
                    .on_click(cx.listener(|this, _, _, cx| this.open_settings_url(crate::password_manager::URL, cx))),
            )),
            Section::SearchEngine => {
                let engine = self.browser.active_user_profile().settings.search.default_name().to_owned();
                div().child(heading("Search engine")).child(
                    card()
                        .child(row(0, "Search engine used in the address bar", None).child(div().text_color(rgb(0xbdc1c6)).child(engine)))
                        .child(
                            row(1, "Manage search engines and site search", None)
                                .id("settings-manage-search")
                                .debug_selector(|| "settings-manage-search".into())
                                .cursor_pointer()
                                .child(icons::ionicon("chevron-forward-outline", 16., 1.))
                                .on_click(cx.listener(|this, _, _, cx| this.open_settings_url(crate::search_settings::URL, cx))),
                        ),
                )
            }
            Section::DefaultBrowser => {
                let state = crate::setup::default_browser_state(cx);
                let line = match state {
                    crate::browser_registration::DefaultState::Ours => "Echo is your default browser",
                    _ => "Make Echo the default browser",
                };
                div().child(heading("Default browser")).child(card().child(
                    row(0, line, None).when(state == crate::browser_registration::DefaultState::Other, |row| {
                        row.child(
                            div()
                                .id("settings-make-default")
                                .debug_selector(|| "settings-make-default".into())
                                .px(px(16.))
                                .h(px(32.))
                                .rounded(px(16.))
                                .border_1()
                                .border_color(rgb(0x8ab4f8))
                                .text_color(rgb(0x8ab4f8))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .child("Make default")
                                .on_click(cx.listener(|_, _, _, cx| {
                                    if let Some(task) = crate::setup::request_default_browser_task(cx) {
                                        cx.spawn(async move |this, cx| {
                                            let _ = task.await;
                                            let _ = this.update(cx, |_, cx| cx.notify());
                                        })
                                        .detach();
                                    }
                                })),
                        )
                    }),
                ))
            }
            Section::Downloads => {
                let settings = self.browser.active_user_profile().settings.clone();
                let ask = settings.ask_for_download_location;
                div().child(heading("Downloads")).child(
                    card()
                        .child(row(0, "Location", Some(settings.download_location.into())))
                        .child(
                            row(1, "Ask where to save each file before downloading", None)
                                .id("settings-ask-download")
                                .debug_selector(|| "settings-ask-download".into())
                                .cursor_pointer()
                                .child(switch(ask))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let profile = this.browser.active_user_profile_id();
                                    this.edit_browser(|browser| {
                                        if let Some(profile) = browser.user_profile_mut(profile) {
                                            profile.settings.ask_for_download_location = !profile.settings.ask_for_download_location;
                                        }
                                    });
                                    cx.notify();
                                })),
                        ),
                )
            }
            Section::About => div().child(heading("About Echo")).child(card().child(row(
                0,
                "Echo",
                Some(format!("Version {}", option_env!("ECHO_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))).into()),
            ))),
            Section::Privacy | Section::Extensions => div(),
        };
        self.settings_shell(section, column(content), cx)
    }

    fn open_settings_url(&mut self, url: &str, cx: &mut Context<Self>) {
        let Some(tab) = self.browser.active_tab().map(|tab| tab.id) else { return; };
        self.edit_browser(|browser| browser.set_url(tab, url));
        cx.notify();
    }
}

/// The page column, centred on the window and scrolling on its own.
fn column(content: impl IntoElement) -> AnyElement {
    div()
        .id("settings-column-scroll")
        .size_full()
        .overflow_y_scroll()
        .child(div().w_full().max_w(px(680.)).mx_auto().px(px(0.)).pt(px(18.)).pb(px(40.)).child(content))
        .into_any_element()
}

fn heading(text: &'static str) -> gpui::Div {
    div().h(px(28.)).mb(px(8.)).flex().items_center().text_size(px(14.)).font_weight(gpui::FontWeight::MEDIUM).child(text)
}

fn card() -> gpui::Div {
    div().rounded(px(8.)).overflow_hidden().bg(rgb(CARD))
}

fn row(index: usize, title: impl Into<gpui::SharedString>, description: Option<gpui::SharedString>) -> gpui::Div {
    row_with(index, None, title, description)
}

/// A card row, optionally led by an avatar or icon.
fn row_with(index: usize, leading: Option<AnyElement>, title: impl Into<gpui::SharedString>, description: Option<gpui::SharedString>) -> gpui::Div {
    div()
        .min_h(px(64.))
        .px(px(20.))
        .flex()
        .items_center()
        .gap(px(16.))
        .when(index > 0, |row| row.border_t_1().border_color(rgb(DIVIDER)))
        .children(leading)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(div().text_size(px(14.)).child(title.into()))
                .when_some(description, |text, description| text.child(div().text_color(rgb(0xbdc1c6)).child(description))),
        )
}

fn switch(on: bool) -> gpui::Div {
    div()
        .w(px(36.))
        .h(px(20.))
        .rounded_full()
        .bg(if on { rgb(0x8ab4f8) } else { rgb(0x5f6368) })
        .p(px(2.))
        .flex()
        .when(on, |track| track.justify_end())
        .child(div().size(px(16.)).rounded_full().bg(rgb(if on { 0x062e6f } else { 0xe3e3e3 })))
}
