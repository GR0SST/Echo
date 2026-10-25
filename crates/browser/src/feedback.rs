//! UI feedback follows live local preferences independently of page audio.
use crate::{new_window::NormalProfile, window::BrowserWindow};
use browser_core::settings::Toggle;
use gpui::App;
pub(crate) fn enabled(view: &BrowserWindow, toggle: Toggle, cx: &App) -> bool {
    if let Some(transient) = &view.transient {
        return transient
            .source
            .profile
            .read_settings(|browser| browser.settings.enabled(toggle));
    }
    if view.browser.is_blank() {
        return view
            .profile
            .handle
            .download_normal_owner(view.browser.active_user_profile_id())
            .is_some_and(|(normal, _, _)| {
                normal.read_settings(|browser| browser.settings.enabled(toggle))
            });
    }
    if view.browser.is_private() {
        if let Some(normal) = cx.try_global::<NormalProfile>() {
            return normal
                .profile
                .read_settings(|browser| browser.settings.enabled(toggle));
        }
    }
    view.profile
        .handle
        .read_settings(|browser| browser.settings.enabled(toggle))
}
pub(crate) fn capture(view: &BrowserWindow, cx: &App) -> bool {
    if !enabled(view, Toggle::SoundEffects, cx) {
        return false;
    }
    #[cfg(test)]
    {
        CAPTURE_CUES.with(|count| count.set(count.get() + 1));
        true
    }
    #[cfg(not(test))]
    {
        engine::play_ui_sound(engine::UiSound::Capture)
    }
}
#[cfg(test)]
thread_local! {pub(crate) static CAPTURE_CUES:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}
