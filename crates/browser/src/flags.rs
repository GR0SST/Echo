//! Chromium experiments (`echo://flags`), as in Arc: each one Default,
//! Enabled or Disabled. Choices belong to this installation, not a profile,
//! since one engine serves every profile; they apply when the engine starts,
//! so a change asks for a relaunch.
use crate::{icons, text_input::TextInput, window::BrowserWindow};
use gpui::{div, prelude::*, px, rgb, AnyElement, App, Context, Entity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) const URL: &str = "echo://flags";
const FILE: &str = "Flags.json";

/// One experiment from the fork's flag list (`scripts/gen_flags.py`).
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Flag {
    pub id: String,
    pub name: String,
    pub description: String,
    pub platforms: Vec<String>,
    pub feature: Option<String>,
    pub switch: Option<String>,
    pub value: Option<String>,
    /// The switch turns something off, so it applies when Disabled.
    #[serde(default)]
    pub on_disable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Choice {
    Enabled,
    Disabled,
}

static CATALOG: std::sync::LazyLock<Vec<Flag>> = std::sync::LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/flags.json")).expect("generated flag list is valid")
});

const PLATFORM: &str = if cfg!(target_os = "macos") { "mac" } else if cfg!(target_os = "windows") { "windows" } else { "linux" };

/// The experiments this platform offers.
pub(crate) fn catalog() -> impl Iterator<Item = &'static Flag> {
    CATALOG.iter().filter(|flag| flag.platforms.iter().any(|platform| platform == PLATFORM))
}

fn path() -> std::path::PathBuf {
    crate::profile_dir().join(FILE)
}

/// The saved choices; anything unknown or unreadable counts as Default.
pub(crate) fn load() -> BTreeMap<String, Choice> {
    // Tests never touch the real profile.
    if cfg!(test) { return BTreeMap::new(); }
    std::fs::read(path()).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn save(choices: &BTreeMap<String, Choice>) {
    if cfg!(test) { return; }
    let _ = std::fs::create_dir_all(crate::profile_dir());
    if let Ok(json) = serde_json::to_vec_pretty(choices) {
        let _ = std::fs::write(path(), json);
    }
}

/// The engine switches the saved choices ask for.
pub(crate) fn launch_switches(choices: &BTreeMap<String, Choice>) -> engine::LaunchSwitches {
    let mut launch = engine::LaunchSwitches::default();
    for flag in catalog() {
        let Some(&choice) = choices.get(&flag.id) else { continue; };
        if let Some(feature) = &flag.feature {
            match choice {
                Choice::Enabled => launch.enable_features.push(feature.clone()),
                Choice::Disabled => launch.disable_features.push(feature.clone()),
            }
        } else if let Some(switch) = &flag.switch {
            if (choice == Choice::Disabled) == flag.on_disable {
                launch.switches.push((switch.clone(), flag.value.clone()));
            }
        }
    }
    launch
}

/// Hands the saved choices to the engine before it first starts.
pub(crate) fn apply_at_launch() {
    engine::set_launch_switches(launch_switches(&load()));
}

pub(crate) struct State {
    pub(crate) search: Entity<TextInput>,
    /// The choices on screen, saved as they change.
    choices: BTreeMap<String, Choice>,
    /// The choices the running engine started with.
    launched: BTreeMap<String, Choice>,
    /// The flag whose choice menu is open.
    menu: Option<String>,
    scroll: gpui::ScrollHandle,
}

impl State {
    /// A saved choice differs from what the running engine started with.
    pub(crate) fn relaunch_pending(&self) -> bool { self.choices != self.launched }
    pub(crate) fn menu_open(&self) -> bool { self.menu.is_some() }
    pub(crate) fn new(cx: &mut Context<BrowserWindow>) -> Self {
        let choices = load();
        Self {
            search: cx.new(|cx| TextInput::new("Search flags", cx)),
            launched: choices.clone(),
            choices,
            menu: None,
            scroll: gpui::ScrollHandle::new(),
        }
    }
}

pub(crate) fn is_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| url.scheme() == "echo" && url.host_str() == Some("flags"))
}

/// Restarts Echo after it has fully quit: the profile stays locked until
/// this process exits, so a helper waits for it and then opens Echo again.
fn relaunch(cx: &mut App) {
    let pid = std::process::id();
    let Ok(exe) = std::env::current_exe() else { return; };
    #[cfg(target_os = "macos")]
    let target = exe.ancestors().find(|path| path.extension().is_some_and(|ext| ext == "app")).map(|bundle| bundle.to_path_buf());
    #[cfg(unix)]
    {
        #[cfg(target_os = "macos")]
        let open = match &target {
            Some(bundle) => format!("open -n {}", shell_quote(&bundle.to_string_lossy())),
            None => shell_quote(&exe.to_string_lossy()),
        };
        #[cfg(not(target_os = "macos"))]
        let open = format!("{} >/dev/null 2>&1 &", shell_quote(&exe.to_string_lossy()));
        let script = format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; {open}");
        let _ = std::process::Command::new("/bin/sh").arg("-c").arg(script)
            .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    }
    #[cfg(windows)]
    {
        let script = format!("Wait-Process -Id {pid} -ErrorAction SilentlyContinue; Start-Process -FilePath '{}'", exe.to_string_lossy().replace('\'', "''"));
        let _ = std::process::Command::new("powershell").args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script]).spawn();
    }
    crate::quit::request(cx);
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

impl BrowserWindow {
    pub(crate) fn flags_is_open(&self) -> bool {
        self.browser.active_tab().is_some_and(|tab| is_url(&tab.url)) && self.browser.active_split().is_none()
    }

    fn set_flag(&mut self, id: &str, choice: Option<Choice>, cx: &mut Context<Self>) {
        match choice {
            Some(choice) => { self.flags.choices.insert(id.to_owned(), choice); }
            None => { self.flags.choices.remove(id); }
        }
        self.flags.menu = None;
        save(&self.flags.choices);
        cx.notify();
    }

    pub(crate) fn render_flags(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.flags;
        let query = state.search.read(cx).text().trim().to_lowercase();
        let pending = state.relaunch_pending();
        let shown: Vec<&Flag> = catalog()
            .filter(|flag| query.is_empty() || [&flag.name, &flag.description, &flag.id].iter().any(|text| text.to_lowercase().contains(&query)))
            .collect();
        let rows = shown.into_iter().enumerate().map(|(index, flag)| {
            let choice = state.choices.get(&flag.id).copied();
            let id = flag.id.clone();
            let menu_open = state.menu.as_deref() == Some(flag.id.as_str());
            div()
                .id(("flag", index))
                .debug_selector(move || format!("flag-{index}"))
                .py(px(14.))
                .flex()
                .gap(px(24.))
                .when(index > 0, |row| row.border_t_1().border_color(rgb(0x3c4043)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_size(px(14.)).font_weight(gpui::FontWeight::MEDIUM)
                            .when(choice.is_some(), |name| name.text_color(rgb(0x8ab4f8)))
                            .child(flag.name.clone()))
                        .child(div().mt(px(4.)).text_color(rgb(0xbdc1c6)).child(flag.description.clone()))
                        .child(div().mt(px(4.)).text_color(rgb(0x9aa0a6)).child(format!("#{}", flag.id))),
                )
                .child(
                    div()
                        .relative()
                        .flex_none()
                        .child(
                            div()
                                .id(("flag-choice", index))
                                .debug_selector(move || format!("flag-choice-{index}"))
                                .w(px(140.))
                                .h(px(32.))
                                .px(px(10.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(rgb(0x5f6368))
                                .flex()
                                .items_center()
                                .justify_between()
                                .cursor_pointer()
                                .child(match choice { Some(Choice::Enabled) => "Enabled", Some(Choice::Disabled) => "Disabled", None => "Default" })
                                .child(icons::ionicon("chevron-down-outline", 12., 1.))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.flags.menu = if this.flags.menu.as_deref() == Some(id.as_str()) { None } else { Some(id.clone()) };
                                    cx.notify();
                                })),
                        )
                        .when(menu_open, |anchor| {
                            let id = flag.id.clone();
                            let owner = cx.entity().downgrade();
                            // Later rows paint after this one; keep the menu above them.
                            anchor.child(gpui::deferred(
                                div()
                                    .absolute()
                                    .top(px(34.))
                                    .right_0()
                                    .w(px(140.))
                                    .py(px(4.))
                                    .rounded(px(6.))
                                    .bg(rgb(0x3c4043))
                                    .shadow_lg()
                                    .occlude()
                                    .children([(None, "Default"), (Some(Choice::Enabled), "Enabled"), (Some(Choice::Disabled), "Disabled")].into_iter().map(move |(value, label)| {
                                        let id = id.clone();
                                        let owner = owner.clone();
                                        div()
                                            .id(gpui::SharedString::from(format!("flag-option-{label}")))
                                            .debug_selector(move || format!("flag-option-{label}"))
                                            .h(px(28.))
                                            .px(px(12.))
                                            .flex()
                                            .items_center()
                                            .cursor_pointer()
                                            .hover(|option| option.bg(gpui::hsla(0., 0., 1., 0.08)))
                                            .child(label)
                                            .on_click(move |_, _, cx| { let _ = owner.update(cx, |this, cx| this.set_flag(&id, value, cx)); })
                                    })),
                            ).with_priority(1))
                        }),
                )
        }).collect::<Vec<_>>();
        self.echo_page_corners(div())
            .id("flags-page")
            .debug_selector(|| "flags-page".into())
            .absolute()
            .inset_0()
            .overflow_hidden()
            .whitespace_normal()
            .bg(rgb(0x202124))
            .text_color(rgb(0xe3e3e3))
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .line_height(px(20.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .px(px(24.))
                    .pt(px(18.))
                    .pb(px(14.))
                    .border_b_1()
                    .border_color(rgb(0x3c4043))
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(
                        div()
                            .debug_selector(|| "flags-search".into())
                            .flex_1()
                            .max_w(px(690.))
                            .h(px(36.))
                            .px(px(14.))
                            .rounded(px(18.))
                            .bg(rgb(0x282828))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(icons::ionicon("search-outline", 16., 1.))
                            .child(div().flex_1().min_w_0().child(state.search.clone())),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("flags-reset-all")
                            .debug_selector(|| "flags-reset-all".into())
                            .px(px(16.))
                            .h(px(32.))
                            .rounded(px(16.))
                            .bg(rgb(0x8ab4f8))
                            .text_color(rgb(0x202124))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .child("Reset all")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.flags.choices.clear();
                                this.flags.menu = None;
                                save(&this.flags.choices);
                                cx.notify();
                            })),
                    ),
            )
            .child({
                let mut list = div()
                    .id("flags-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&state.scroll);
                // A horizontal swipe belongs to the sidebar and Library.
                list.style().restrict_scroll_to_axis = Some(true);
                list.child(
                        div()
                            .w_full()
                            .max_w(px(820.))
                            .mx_auto()
                            .px(px(24.))
                            .pb(px(40.))
                            .child(div().pt(px(24.)).text_size(px(22.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Experiments"))
                            .child(div().mt(px(12.)).p(px(12.)).rounded(px(8.)).bg(rgb(0x3b2a12)).text_color(rgb(0xfdd663)).child(
                                "Warning: Experimental features ahead! By enabling these features, you could lose browser data or compromise your security or privacy. Enabled features apply to all users of this browser.",
                            ))
                            .child(div().mt(px(16.)).children(rows)),
                    )
            })
            .when(pending, |page| {
                page.child(
                    div()
                        .debug_selector(|| "flags-relaunch-banner".into())
                        .rounded_b(px(crate::pages::CONTENT_CORNER_RADIUS))
                        .flex_none()
                        .h(px(64.))
                        .px(px(24.))
                        .bg(rgb(0x292a2d))
                        .border_t_1()
                        .border_color(rgb(0x3c4043))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child("Your changes will take effect the next time you relaunch Echo.")
                        .child(
                            div()
                                .id("flags-relaunch")
                                .debug_selector(|| "flags-relaunch".into())
                                .px(px(16.))
                                .h(px(32.))
                                .rounded(px(16.))
                                .bg(rgb(0x8ab4f8))
                                .text_color(rgb(0x202124))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .child("Relaunch")
                                .on_click(|_, _, cx| relaunch(cx)),
                        ),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn choices_become_feature_and_switch_launch_arguments() {
        let feature = catalog().find(|flag| flag.feature.is_some()).unwrap();
        let switch = catalog().find(|flag| flag.switch.is_some() && !flag.on_disable).unwrap();
        let disabling = catalog().find(|flag| flag.switch.is_some() && flag.on_disable);
        let mut choices = BTreeMap::new();
        choices.insert(feature.id.clone(), Choice::Disabled);
        choices.insert(switch.id.clone(), Choice::Enabled);
        let launch = launch_switches(&choices);
        assert_eq!(launch.disable_features, vec![feature.feature.clone().unwrap()]);
        assert!(launch.switches.iter().any(|(name, _)| Some(name) == switch.switch.as_ref()));
        // Disabled leaves an enabling switch out; a disabling switch needs Disabled.
        choices.insert(switch.id.clone(), Choice::Disabled);
        assert!(!launch_switches(&choices).switches.iter().any(|(name, _)| Some(name) == switch.switch.as_ref()));
        if let Some(disabling) = disabling {
            let mut choices = BTreeMap::new();
            choices.insert(disabling.id.clone(), Choice::Disabled);
            assert!(launch_switches(&choices).switches.iter().any(|(name, _)| Some(name) == disabling.switch.as_ref()));
        }
    }
    #[test]
    fn the_catalog_offers_this_platforms_experiments() {
        assert!(catalog().count() > 100);
        assert!(catalog().all(|flag| flag.feature.is_some() || flag.switch.is_some()));
    }
}
