//! Local preferences. Controls for features not yet implemented retain intent;
//! storing a preference does not imply that its engine integration exists.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Toggle {
    AutomaticUpdates,
    WarnBeforeQuitting,
    LittleWindowShortcut,
    LittleWindowLinks,
    LittleWindowCommandClick,
    PeekPinned,
    PeekCommandShiftClick,
    SoundEffects,
    HapticFeedback,
    RestoreWindows,
    DragWebpages,
    FullToolbarUrl,
    ShowToolbar,
    AllowJavaScriptFromAppleEvents,
    SharedQuotes,
    AutomaticPictureInPicture,
    ThemeData,
    Boosts,
    PreviewFolders,
    PreviewGoogleCalendar,
    PreviewGoogleMail,
    PreviewOutlookCalendar,
    PreviewOutlookMail,
    PreviewRecentPages,
    PreviewNotionCalendar,
}
impl Toggle {
    pub fn default_value(self) -> bool {
        !matches!(
            self,
            Self::AllowJavaScriptFromAppleEvents | Self::ShowToolbar | Self::FullToolbarUrl | Self::DragWebpages | Self::LittleWindowLinks
        )
    }
}

/// Local application artwork; never tied to referrals or browser accounts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppIcon {
    #[default]
    Original,
    Candy,
    Neon,
    Glass,
    Schoolbook,
    Colorful,
}
impl AppIcon {
    pub const ALL: [Self; 6] = [Self::Original, Self::Candy, Self::Neon, Self::Glass, Self::Schoolbook, Self::Colorful];
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "The Original Echo",
            Self::Candy => "Candy Echo",
            Self::Neon => "Neon",
            Self::Glass => "Fluted Glass",
            Self::Schoolbook => "Schoolbook",
            Self::Colorful => "Colorful",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub shortcuts: crate::shortcuts::Shortcuts,
    pub app_icon: AppIcon,
    pub max: echo_ai::MaxSettings,
    /// Shared across local website profiles; absent in older sessions.
    pub air_traffic_control: Option<crate::routing::AirTrafficControl>,
    toggles: BTreeMap<Toggle, bool>,
    pub little_window_archive: LittleWindowArchive,
    /// Browser appearance is shared across Spaces and normal windows.
    pub theme_appearance: crate::themes::ThemeAppearance,
}
impl Settings {
    pub fn routing(&self) -> crate::routing::AirTrafficControl {
        self.air_traffic_control.clone().unwrap_or_else(|| {
            let mut routes = crate::routing::AirTrafficControl::default();
            if self.enabled(Toggle::LittleWindowLinks) { routes.default_destination = crate::routing::LinkDestination::LittleWindow; }
            routes
        })
    }
    pub fn edit_routing<R>(&mut self, edit: impl FnOnce(&mut crate::routing::AirTrafficControl) -> R) -> R {
        let mut routes = self.routing();
        let result = edit(&mut routes);
        self.toggles.insert(Toggle::LittleWindowLinks, routes.default_destination == crate::routing::LinkDestination::LittleWindow);
        self.air_traffic_control = Some(routes);
        result
    }

    pub fn enabled(&self, toggle: Toggle) -> bool {
        self.toggles
            .get(&toggle)
            .copied()
            .unwrap_or_else(|| toggle.default_value())
    }
    pub fn set_enabled(&mut self, toggle: Toggle, enabled: bool) {
        self.toggles.insert(toggle, enabled);
        if toggle == Toggle::LittleWindowLinks {
            if let Some(routes) = &mut self.air_traffic_control {
                routes.default_destination = if enabled { crate::routing::LinkDestination::LittleWindow } else { crate::routing::LinkDestination::MostRecentSpace };
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LittleWindowArchive {
    OneHour,
    #[default]
    SixHours,
    TwelveHours,
}

impl LittleWindowArchive {
    pub fn duration(self) -> std::time::Duration {
        std::time::Duration::from_secs(match self {Self::OneHour=>3600,Self::SixHours=>21600,Self::TwelveHours=>43200})
    }
}

pub use crate::omnibox::SearchEngine;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileSettings {
    pub search: crate::omnibox::CommandBarSearchPreferences,
    pub document_app: String,
    pub download_location: String,
    pub ask_for_download_location: bool,
    pub save_page_format: crate::save_page::Format,
    pub media: crate::media::MediaAccess,
}
impl Default for ProfileSettings {
    fn default() -> Self {
        Self {
            search: crate::omnibox::CommandBarSearchPreferences::default(),
            document_app: "—".into(),
            download_location: "Downloads".into(),
            ask_for_download_location: false,
            save_page_format: crate::save_page::Format::default(),
            media: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session, Browser};
    #[test]
    fn saved_page_format_is_profile_local_persistent_and_defaults_for_older_sessions() {
        let mut browser = Browser::new();
        let first = browser.active_user_profile_id();
        browser.active_user_profile_mut().settings.save_page_format = crate::save_page::Format::SingleFile;
        let second = browser.create_user_profile("Other").unwrap();
        let restored = session::from_json(&session::to_json(&browser)).unwrap();
        assert_eq!(restored.user_profile(first).unwrap().settings.save_page_format, crate::save_page::Format::SingleFile);
        assert_eq!(restored.user_profile(second).unwrap().settings.save_page_format, crate::save_page::Format::Complete);
        let mut old: serde_json::Value = serde_json::from_str(&session::to_json(&browser)).unwrap();
        old["profiles"][first.0.to_string()]["settings"].as_object_mut().unwrap().remove("save_page_format");
        let restored = session::from_json(&old.to_string()).unwrap();
        assert_eq!(restored.user_profile(first).unwrap().settings.save_page_format, crate::save_page::Format::Complete);
    }

    #[test]
    fn legacy_command_bar_preferences_migrate_to_canonical_profile_settings() {
        let mut value = serde_json::to_value(Browser::new()).unwrap();
        value["format_version"] = 2.into();
        value["profiles"]["1"]
            .as_object_mut()
            .unwrap()
            .remove("settings");
        value["profiles"]["1"]["command_bar_search"] = serde_json::json!({"engine":"DuckDuckGo", "suggestions":false, "notice_dismissed":true});
        let loaded = session::from_json(&value.to_string()).unwrap();
        let search = &loaded.active_user_profile().settings.search;
        assert_eq!(search.engine, SearchEngine::DuckDuckGo);
        assert!(!search.suggestions);
        assert!(search.notice_dismissed);
        let saved = session::to_json(&loaded);
        assert!(!saved.contains("command_bar_search"));
        value["profiles"]["1"]["settings"] = serde_json::json!({"search":{"engine":"Bing"}});
        assert_eq!(
            session::from_json(&value.to_string())
                .unwrap()
                .active_user_profile()
                .settings
                .search
                .engine,
            SearchEngine::Bing
        );
    }
    #[test]
    fn settings_round_trip_and_old_sessions_keep_defaults() {
        let mut browser = Browser::new();
        let profile = browser.create_user_profile("Work").unwrap();
        browser
            .settings
            .set_enabled(Toggle::WarnBeforeQuitting, false);
        browser
            .user_profile_mut(profile)
            .unwrap()
            .settings
            .search
            .engine = SearchEngine::DuckDuckGo;
        browser
            .user_profile_mut(profile)
            .unwrap()
            .settings
            .search
            .suggestions = false;
        let loaded = session::from_json(&session::to_json(&browser)).unwrap();
        assert!(!loaded.settings.enabled(Toggle::WarnBeforeQuitting));
        assert_eq!(
            loaded.user_profile(profile).unwrap().settings.search.engine,
            SearchEngine::DuckDuckGo
        );
        assert_eq!(
            loaded.active_user_profile().settings.search.engine,
            SearchEngine::Google
        );
        let blank = loaded.new_blank_for_user_profile(profile).unwrap();
        assert_eq!(
            blank.active_user_profile().settings.search.engine,
            SearchEngine::DuckDuckGo
        );
        assert!(!blank.active_user_profile().settings.search.suggestions);
        let mut value = serde_json::to_value(&browser).unwrap();
        value["format_version"] = 2.into();
        value.as_object_mut().unwrap().remove("settings");
        for profile in value["profiles"].as_object_mut().unwrap().values_mut() {
            profile.as_object_mut().unwrap().remove("settings");
        }
        let old = session::from_json(&value.to_string()).unwrap();
        assert!(old.settings.enabled(Toggle::WarnBeforeQuitting));
        assert!(old.active_user_profile().settings.search.suggestions);
    }
    #[test]
    fn browser_appearance_round_trips_independently_of_space_themes() {
        use crate::themes::ThemeAppearance;
        let mut browser = Browser::new();
        browser.settings.theme_appearance = ThemeAppearance::Light;
        let loaded = session::from_json(&session::to_json(&browser)).unwrap();
        assert_eq!(loaded.settings.theme_appearance, ThemeAppearance::Light);
        assert!(loaded.spaces.iter().all(|space| space.theme.is_none()));
        let blank = loaded.new_blank_for_user_profile(loaded.active_user_profile_id()).unwrap();
        assert_eq!(blank.settings.theme_appearance, ThemeAppearance::Light);
        assert_eq!(serde_json::from_str::<super::Settings>("{}").unwrap().theme_appearance, ThemeAppearance::Automatic);
    }

}

#[cfg(test)]
mod max_tests {
    #[test]
    fn max_settings_migrate_disabled_and_round_trip_without_credentials() {
        use crate::{Browser,session};
        use echo_ai::MaxFeature;
        let mut legacy:serde_json::Value=serde_json::from_str(&session::to_json(&Browser::new())).unwrap();legacy["settings"].as_object_mut().unwrap().remove("max");
        let mut browser=session::from_json(&legacy.to_string()).unwrap();
        assert!(browser.settings.max.enabled.is_empty());
        browser.settings.max.set_enabled(MaxFeature::TidyTabs,true);
        browser.settings.max.provider.api_key_id="opaque-os-credential-reference".into();
        let saved=session::to_json(&browser);let loaded=session::from_json(&saved).unwrap();
        assert_eq!(loaded.settings.max,browser.settings.max);
        assert!(!saved.contains("access_token")&&!saved.contains("refresh_token"));
    }
}
#[cfg(test)]
mod scripting_permission_tests {
    use super::*;
    #[test]
    fn legacy_sessions_deny_javascript_until_the_explicit_persisted_opt_in() {
        let mut browser=crate::Browser::new();let toggle=Toggle::AllowJavaScriptFromAppleEvents;
        assert!(!browser.settings.enabled(toggle));
        let restored=crate::session::from_json(&crate::session::to_json(&browser)).unwrap();assert!(!restored.settings.enabled(toggle));
        browser.settings.set_enabled(toggle,true);
        let restored=crate::session::from_json(&crate::session::to_json(&browser)).unwrap();assert!(restored.settings.enabled(toggle));
    }
}
