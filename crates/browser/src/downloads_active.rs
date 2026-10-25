//! Ephemeral transfer presentation. IDs are local UI keys, never CEF IDs or
//! reconstructed ownership. The host resolves every intent against its service.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TransferId(pub(crate) u64);
impl TransferId {
    pub(crate) fn allocate() -> Option<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .ok()
            .map(Self)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferRow {
    pub id: TransferId,
    pub profile: UserProfileId,
    pub title: String,
    pub subtitle: String,
    pub progress: Option<f32>,
    pub canceled: bool,
}
#[derive(Default)]
pub(super) struct Transfers {
    pub rows: Vec<TransferRow>,
    selected: Option<TransferId>,
    hovered: Option<TransferId>,
    menu: Option<TransferId>,
}
impl<Token: Clone + Eq + 'static> DownloadsView<Token> {
    pub(crate) fn set_transfers(&mut self, rows: Vec<TransferRow>, cx: &mut Context<Self>) {
        if self.transfers.rows == rows {
            return;
        }
        self.transfers.rows = rows;
        let contains = |id| self.transfers.rows.iter().any(|r| r.id == id);
        if self.transfers.selected.is_some_and(|id| !contains(id)) {
            self.transfers.selected = None;
        }
        if self.transfers.hovered.is_some_and(|id| !contains(id)) {
            self.transfers.hovered = None;
        }
        if self.transfers.menu.is_some_and(|id| !contains(id)) {
            self.transfers.menu = None;
        }
        cx.notify();
    }
    pub(crate) fn has_transfer(&self, id: TransferId, profile: UserProfileId) -> bool {
        self.transfers
            .rows
            .iter()
            .any(|r| r.id == id && r.profile == profile)
    }
    pub(crate) fn set_transfer_menu(&mut self, id: Option<TransferId>, cx: &mut Context<Self>) {
        if self.transfers.menu != id {
            self.transfers.menu = id;
            cx.notify();
        }
    }
    pub(super) fn transfer_elements(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.transfers.rows.is_empty() {
            return Vec::new();
        }
        let mut elements = vec![div()
            .h(px(27.))
            .px(px(12.))
            .pt(px(4.))
            .flex()
            .items_center()
            .text_size(px(12.))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(self.colors.muted)
            .line_height(px(16.))
            .child("Today")
            .into_any_element()];
        for row in &self.transfers.rows {
            let id = row.id;
            let profile = row.profile;
            let canceled = row.canceled;
            let menu = self.transfers.menu == Some(id);
            let hovered = menu || self.transfers.hovered == Some(id);
            let group = gpui::SharedString::from(format!("transfer-{}", id.0));
            let artwork = if canceled {
                div()
                    .relative()
                    .left(px(2.))
                    .size(px(14.))
                    .rounded(px(3.))
                    .bg(self.colors.muted.opacity(0.5))
                    .into_any_element()
            } else {
                progress_icon(row.progress)
            };
            elements.push(
                div()
                    .id(gpui::SharedString::from(format!("transfer-row-{}", id.0)))
                    .debug_selector(move || format!("downloads-transfer-{}", id.0))
                    .group(group.clone())
                    .relative()
                    .flex_none()
                    .h(px(56.))
                    .w(px(DOWNLOADS_COLUMN_WIDTH - 17.))
                    .mx(px(8.))
                    .px(px(8.))
                    .rounded(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .hover(|s| s.bg(self.colors.hover))
                    .when(menu || self.transfers.selected == Some(id), |el| {
                        el.bg(self.colors.selected)
                    })
                    .on_hover(cx.listener(move |this, active: &bool, _, cx| {
                        if *active {
                            this.transfers.hovered = Some(id);
                        } else if this.transfers.hovered == Some(id) {
                            this.transfers.hovered = None;
                        }
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.transfers.selected = Some(id);
                        if !canceled {
                            cx.emit(DownloadsIntent::<Token>::TransferOpened { id, profile });
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex_none()
                            .size(px(36.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(artwork),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .pr(px(if hovered { 44. } else { 16. }))
                            .child(
                                div()
                                    .debug_selector(move || {
                                        format!("downloads-transfer-title-{}", id.0)
                                    })
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_size(px(14.))
                                    .line_height(px(18.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(self.colors.text.opacity(0.8))
                                    .when(canceled, |el| el.text_color(self.colors.muted))
                                    .child(SidebarTitle::new(row.title.clone())),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .text_color(self.colors.muted)
                                    .child(SidebarTitle::new(row.subtitle.clone())),
                            ),
                    )
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("transfer-menu-{}", id.0)))
                            .debug_selector(move || format!("downloads-transfer-overflow-{}", id.0))
                            .absolute()
                            .right(px(12.))
                            .top(px(12.))
                            .size(px(32.))
                            .rounded(px(8.))
                            .invisible()
                            .group_hover(group, |s| s.visible())
                            .when(menu, |el| el.visible().bg(self.colors.selected))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(self.colors.selected))
                            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.transfers.selected = Some(id);
                                cx.emit(DownloadsIntent::<Token>::TransferMenuRequested {
                                    id,
                                    profile,
                                    position: event.position(),
                                });
                                cx.notify();
                            }))
                            .child(crate::icons::more()),
                    )
                    .into_any_element(),
            );
        }
        elements
    }
}
/// Reference-measured 24 px blue disk, fixed arrow and genuine received-progress arc.
/// This palette belongs to the download glyph, independent of Space materials.
pub(crate) fn progress_icon(progress: Option<f32>) -> AnyElement {
    render_progress_icon(progress, 24., false)
}
pub(crate) fn progress_icon_sized(progress: Option<f32>, size: f32) -> AnyElement {
    render_progress_icon(progress, size, true)
}
fn render_progress_icon(progress: Option<f32>, size: f32, light: bool) -> AnyElement {
    let scale = size / 24.;
    div()
        .relative()
        .flex_none()
        .size(px(size))
        .rounded_full()
        .bg(if light {
            gpui::rgb(0xecedfe)
        } else {
            gpui::rgb(0x1f2afe)
        })
        .shadow(vec![gpui::BoxShadow {
            color: gpui::rgba(0x8a93ff80).into(),
            offset: gpui::point(px(0.), px(0.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
        }])
        .border_1()
        .border_color(gpui::rgba(0x9ba2ffbb))
        .child(
            gpui::canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let offset = bounds.origin;
                    let pos = |x: f32, y: f32| offset + gpui::point(px(x * scale), px(y * scale));
                    let mut arrow = gpui::PathBuilder::stroke(px(2. * scale));
                    arrow.move_to(pos(11., 6.5));
                    arrow.line_to(pos(11., 15.5));
                    arrow.move_to(pos(8., 12.5));
                    arrow.line_to(pos(11., 15.5));
                    arrow.line_to(pos(14., 12.5));
                    if let Ok(path) = arrow.build() {
                        window.paint_path(
                            path,
                            if light {
                                gpui::rgb(0x1f2afe).into()
                            } else {
                                gpui::white()
                            },
                        );
                    }
                    if let Some(progress) = progress.filter(|p| p.is_finite() && *p > 0.) {
                        let steps = (progress.clamp(0., 1.) * 64.).ceil().max(1.) as usize;
                        let mut arc = gpui::PathBuilder::stroke(px(2. * scale));
                        for i in 0..=steps {
                            let angle = -std::f32::consts::FRAC_PI_2
                                + std::f32::consts::TAU * progress.clamp(0., 1.) * i as f32
                                    / steps as f32;
                            let point = pos(12. + 10.8 * angle.cos(), 12. + 10.8 * angle.sin());
                            if i == 0 {
                                arc.move_to(point);
                            } else {
                                arc.line_to(point);
                            }
                        }
                        if let Ok(path) = arc.build() {
                            window.paint_path(
                                path,
                                if light {
                                    gpui::rgb(0x1f2afe).into()
                                } else {
                                    gpui::white()
                                },
                            );
                        }
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .into_any_element()
}
