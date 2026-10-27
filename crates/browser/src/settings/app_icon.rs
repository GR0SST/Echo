//! Six-row native-reference selector, populated solely with Echo artwork.
use super::*;
use browser_core::settings::AppIcon;
impl SettingsWindow {
    /// Shows the choice at once; the Dock catches up after this frame, since
    /// handing the 1024 px artwork to AppKit is slower than a frame.
    fn choose_app_icon(&mut self, icon: AppIcon, cx: &mut Context<Self>) {
        self.profile.edit_settings(|browser| browser.settings.app_icon = icon);
        cx.notify();
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(16)).await;
            crate::app_icons::apply(icon);
        })
        .detach();
    }

    pub(super) fn icon(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors;
        let selected = self
            .profile
            .read_settings(|browser| browser.settings.app_icon);
        self.settings_group()
            .when(colors.light, |group| group.bg(gpui::rgb(0xffffff)))
            .px(px(16.))
            .pt(px(9.))
            .pb(px(20.))
            .children(AppIcon::ALL.into_iter().enumerate().map(|(index, icon)| {
                div()
                    .when(index > 0, |row| {
                        row.child(
                            div()
                                .ml(px(24.))
                                .mr(px(22.))
                                .h(px(1.))
                                .bg(colors.rgb(0x3c3e3e)),
                        )
                    })
                    .child(
                        div()
                            .id(("app-icon", index))
                            .debug_selector(move || format!("settings-app-icon-{index}"))
                            .h(px(80.))
                            .focusable()
                            .tab_stop(true)
                            .on_key_down(cx.listener(
                                move |this, event: &gpui::KeyDownEvent, _, cx| {
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                        && crate::app_icons::supported()
                                    {
                                        this.choose_app_icon(icon, cx);
                                        cx.stop_propagation();
                                    }
                                },
                            ))
                            .flex()
                            .items_center()
                            .gap(px(26.))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if crate::app_icons::supported() {
                                    this.choose_app_icon(icon, cx);
                                }
                            }))
                            .child(
                                div()
                                    .size(px(20.))
                                    .flex_none()
                                    .rounded_full()
                                    .bg(colors.rgb(0xffffff))
                                    .border_1()
                                    .border_color(colors.rgb(0xc5c7c7))
                                    .when(selected == icon, |radio| {
                                        radio.border_2().border_color(colors.rgb(0x534dff)).child(
                                            div()
                                                .m(px(2.))
                                                .size(px(12.))
                                                .rounded_full()
                                                .bg(colors.rgb(0x534dff)),
                                        )
                                    }),
                            )
                            .child(crate::app_icons::preview(icon, cx))
                            .child(
                                div()
                                    .ml(px(-12.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(icon.label())
                                    .when(selected == icon, |label| {
                                        label.child(
                                            div()
                                                .debug_selector(|| "settings-current-icon".into())
                                                .text_size(px(10.))
                                                .text_color(colors.rgb(0x1590ff))
                                                .child("CURRENT ICON"),
                                        )
                                    }),
                            ),
                    )
            }))
            .into_any_element()
    }
}
