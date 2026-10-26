//! Arc's opt-in palette contract uses the colors already resolved for this window.
use crate::window::BrowserWindow;
use browser_core::settings::Toggle;
use gpui::{App, Hsla, Rgba};

fn rgba(color: Hsla) -> u32 {
    let color: Rgba = color.into();
    let bytes = [color.r, color.g, color.b, color.a]
        .map(|value| (value.clamp(0., 1.) * 255.).round() as u8);
    u32::from_be_bytes(bytes)
}
impl BrowserWindow {
    pub(crate) fn web_theme_palette(&self, cx: &App) -> Option<engine::PagePalette> {
        let enabled = if let Some(transient) = &self.transient {
            transient
                .source
                .profile
                .read_settings(|browser| browser.settings.enabled(Toggle::ThemeData))
        } else if self.browser.is_private() || self.browser.is_blank() {
            // Isolated lookup/private models must not reset an install-level opt-out.
            cx.try_global::<crate::new_window::NormalProfile>()
                .is_some_and(|normal| {
                    normal
                        .profile
                        .read_settings(|browser| browser.settings.enabled(Toggle::ThemeData))
                })
        } else {
            self.browser.settings.enabled(Toggle::ThemeData)
        };
        if !enabled {
            return None;
        }
        let surface = self.theme_surface_for(self.browser.space());
        let color = surface.sample(0.5, 0.5);
        let background: Hsla = Rgba {
            r: color.red,
            g: color.green,
            b: color.blue,
            a: 1.,
        }
        .into();
        let colors = self.theme_colors();
        let ink = colors.foreground;
        let accent = gpui::hsla(self.browser.space().accent_hue() / 360., 0.7, 0.5, 1.);
        Some(engine::PagePalette([
            rgba(ink),
            rgba(colors.secondary),
            rgba(colors.muted),
            rgba(ink),
            rgba(accent),
            rgba(background),
            rgba(background),
            rgba(colors.selected),
            rgba(colors.hover),
            rgba(accent),
            rgba(ink),
            rgba(colors.secondary),
        ]))
    }
    pub(crate) fn sync_web_theme_palette(&self, cx: &App) {
        let Some(pages) = &self.pages else {
            return;
        };
        let Ok(mut pages) = pages.try_borrow_mut() else {
            return;
        };
        pages.set_theme_palette(self.web_theme_palette(cx));
    }
}
