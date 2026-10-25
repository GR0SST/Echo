//! Settings colors sampled in owned Arc windows under macOS Light and Dark.
use gpui::{prelude::*, rgb, Rgba, WindowAppearance};
#[derive(Clone, Copy)]
pub(super) struct Colors {
    pub light: bool,
}
impl Colors {
    pub fn from_appearance(appearance: WindowAppearance) -> Self {
        Self {
            light: matches!(
                appearance,
                WindowAppearance::Light | WindowAppearance::VibrantLight
            ),
        }
    }
    pub fn rgb(self, dark: u32) -> Rgba {
        let value = if self.light {
            match dark {
                0x292b2d | 0x303334 | 0x343636 | 0x3b3e3e | 0x5b575f => 0xffffff,
                0x2c2e30 => 0xfcfcfc,
                0x3e3f41 => 0xe7e7e7,
                0x3c3e3e => 0xe6e6e6,
                0x34383c => 0xeef4fe,
                0x393b3b => 0xe4e4e4,
                0x393b3d => 0xececec,
                0x484a4a => 0xe0e0e0,
                0x263342 => 0xeef4fe,
                0x363838 => 0xfafafa,
                0x181919 | 0x636565 => 0xd8d8d8,
                0x161c2f | 0x050914 => 0xf9faff,
                0xe8e8ef => 0x222225,
                0xc4c5cd => 0x3c4163,
                0x8d9091 | 0xaaaeb0 | 0x96999a => 0x808080,
                0x242628 => 0xffffff,
                0x555759 => 0xd8d8d8,
                0x45525e => 0xeef4fe,
                0xc4c5c5 => 0x464646,
                0xacafb0 => 0x4c4c4c,
                0x929494 => 0x999999,
                0x858787 | 0x989a9a | 0x999b9b | 0xa6a8a8 => 0x808080,
                0xc9dafa => 0xe1eaf9,
                0x494b4b => 0xe5e5e5,
                0xdfdfdf => 0xffffff,
                _ => dark,
            }
        } else {
            dark
        };
        rgb(value)
    }
}

impl super::SettingsWindow {
    #[cfg(test)]
    pub(crate) fn uses_light_appearance(&self) -> bool {
        self.colors.light
    }
    pub(super) fn settings_group(&self) -> gpui::Div {
        super::group()
            .bg(self.colors.rgb(0x2c2e30))
            .border_color(self.colors.rgb(0x3e3f41))
    }
    pub(super) fn settings_rule(&self) -> gpui::Div {
        super::rule().bg(self.colors.rgb(0x3e3f41))
    }
    pub(super) fn settings_button(
        &self,
        label: impl Into<gpui::SharedString>,
    ) -> gpui::Stateful<gpui::Div> {
        super::button_palette(label, self.colors)
    }
    pub(super) fn settings_switch(&self, enabled: bool) -> gpui::AnyElement {
        use gpui::{div, prelude::*, px};
        let colors = self.colors;
        div()
            .w(px(36.))
            .h(px(16.))
            .rounded_full()
            .bg(colors.rgb(if enabled { 0x107cfc } else { 0x494b4b }))
            .child(
                div()
                    .relative()
                    .left(px(if enabled { 14. } else { 1. }))
                    .top(px(1.))
                    .w(px(21.))
                    .h(px(14.))
                    .rounded_full()
                    .bg(colors.rgb(if enabled { 0xdfebff } else { 0xdfdfdf })),
            )
            .into_any_element()
    }
}
