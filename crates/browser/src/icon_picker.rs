//! The sidebar icon popover shares overlay lifetime and focus restoration with
//! other browser popups, but never renders the Command Bar's URL/text editor.
use crate::{icon_catalog::Catalog, icons::IconTarget, window::{BrowserWindow, CommandBar, Target}};
use gpui::{actions, canvas, div, hsla, point, prelude::*, px, AnyElement, App, Bounds, Context, Focusable, KeyBinding};

const CONTEXT: &str = "IconPicker";
const WIDTH: f32 = 272.;
const HEIGHT: f32 = 272.;
const CELL: f32 = 31.;
const EMOJI_ROW: f32 = 33.;
actions!(icon_picker, [NextRow, PreviousRow, Confirm, Dismiss]);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-t", crate::window::OpenCommandBar, Some(CONTEXT)),
        KeyBinding::new("cmd-l", crate::window::EditCurrentUrl, Some(CONTEXT)),
        KeyBinding::new("down", NextRow, Some(CONTEXT)),
        KeyBinding::new("up", PreviousRow, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
    ]);
}

impl BrowserWindow {
    pub(crate) fn track_icon_anchor(&self, target: IconTarget) -> AnyElement {
        let anchors = self.icon_anchors.clone();
        let interactive = self.sidebar_page_interactive;
        canvas(|_, _, _| (), move |bounds, _, _, _| {
            if interactive {
                let bounds = if matches!(target, IconTarget::Tab(_) | IconTarget::Folder(_)) {
                    Bounds::new(point(bounds.origin.x + px(9.), bounds.center().y - px(9.)), gpui::size(px(18.), px(18.)))
                } else { bounds };
                anchors.borrow_mut().insert(target, bounds);
            }
        }).absolute().inset_0().into_any_element()
    }

    pub(crate) fn custom_icon(&self, target: IconTarget) -> Option<&str> {
        match target {
            IconTarget::Tab(id) => self.browser.item_icon(id),
            IconTarget::Space(id) => self.browser.spaces.iter().find(|space| space.id == id).and_then(|space| space.custom_icon.as_deref()),
            IconTarget::Folder(id) => self.browser.folder(id).and_then(|folder| folder.custom_icon.as_deref()),
        }
    }

    fn set_picker_icon(&mut self, icon: Option<&str>, cx: &mut Context<Self>) {
        let Some(Target::Icon(target)) = self.command_bar.as_ref().map(|bar| bar.target) else { return; };
        self.edit_browser(|browser| match target {
            IconTarget::Tab(id) => browser.set_item_icon(id, icon),
            IconTarget::Space(id) => browser.set_space_icon(id, icon),
            IconTarget::Folder(id) => browser.set_folder_icon(id, icon),
        });
        cx.notify();
    }

    fn select_picker_icon(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(crate::window::Suggestion::SaveIcon { icon }) = self.suggestions(cx).get(index) {
            self.set_picker_icon(icon.as_deref(), cx);
        }
    }

    fn picker_catalog(&mut self, catalog: Catalog, cx: &mut Context<Self>) {
        self.last_icon_catalog = catalog;
        if let Some(bar) = &mut self.command_bar {
            bar.icon_picker.catalog = catalog;
            bar.icon_picker.category = None;
            bar.icon_picker.keyboard = false;
            bar.selected = 0;
            bar.rows_scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }

    fn picker_category(&mut self, category: &'static str, cx: &mut Context<Self>) {
        if let Some(bar) = &mut self.command_bar {
            bar.icon_picker.category = Some(category.into());
            bar.icon_picker.keyboard = false;
            bar.selected = 0;
            bar.input.update(cx, |input, cx| input.set_text("", cx));
        }
        let browser = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = browser.update(cx, |this, cx| {
                if let Some(bar) = &mut this.command_bar {
                    let rows = bar.icon_picker.rows("");
                    let row = rows.iter().position(|row| row[0].category == category).unwrap_or(0);
                    bar.rows_scroll.set_offset(point(px(0.), px(-(row as f32 * EMOJI_ROW))));
                }
                cx.notify();
            });
        });
    }

    fn picker_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(bar) = &self.command_bar else { return; };
        let rows = bar.icon_picker.rows(bar.input.read(cx).text());
        let count: usize = rows.iter().map(Vec::len).sum();
        if let Some(bar) = &mut self.command_bar {
            if count > 0 {
                let mut starts = Vec::new();
                let mut start = 0;
                for row in &rows { starts.push(start); start += row.len(); }
                let row = starts.iter().rposition(|start| *start <= bar.selected).unwrap_or(0);
                let column = bar.selected - starts[row];
                bar.selected = if !bar.icon_picker.keyboard { 0 }
                    else if delta.abs() == 8 {
                        let next = (row as isize + delta.signum()).clamp(0, rows.len() as isize - 1) as usize;
                        starts[next] + column.min(rows[next].len() - 1)
                    } else { (bar.selected as isize + delta).clamp(0, count as isize - 1) as usize };
                bar.icon_picker.keyboard = true;
                let row = starts.iter().rposition(|start| *start <= bar.selected).unwrap_or(0);
                let row_height = if bar.icon_picker.catalog == Catalog::Emoji { EMOJI_ROW } else { CELL };
                let visible_height = if bar.icon_picker.folder { 93. } else if bar.icon_picker.catalog == Catalog::Emoji { 154. } else { 217. };
                let offset = -f32::from(bar.rows_scroll.offset().y);
                if row as f32 * row_height < offset { bar.rows_scroll.set_offset(point(px(0.), px(-(row as f32 * row_height)))); }
                else if (row + 1) as f32 * row_height > offset + visible_height { bar.rows_scroll.set_offset(point(px(0.), px(visible_height - (row + 1) as f32 * row_height))); }
            }
        }
        cx.notify();
    }

    pub(crate) fn render_icon_picker(&self, bar: &CommandBar, width: f32, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let Target::Icon(target) = bar.target else { return div().into_any_element(); };
        let anchor = bar.icon_anchor.unwrap_or(Bounds::new(point(px(17.), px(145.)), gpui::size(px(18.), px(18.))));
        let preferred_left = f32::from(anchor.origin.x) - 3.;
        let left = preferred_left.clamp(14., (width - WIDTH - 14.).max(14.));
        let folder = matches!(target, IconTarget::Folder(_));
        let panel_height = if folder { 144. } else { HEIGHT };
        let below = f32::from(anchor.bottom()) + 15.;
        let above = below + panel_height > height - 8.;
        let top = if above { (f32::from(anchor.origin.y) - panel_height - 15.).max(8.) } else { below };
        let emoji = bar.icon_picker.catalog == Catalog::Emoji;
        let row_height = if emoji { EMOJI_ROW } else { CELL };
        let current = self.custom_icon(target);
        let catalog_rows = bar.icon_picker.rows(bar.input.read(cx).text());
        let mut rows = Vec::new();
        let mut first_index = 0;
        for chunk in &catalog_rows {
            let start = first_index;
            first_index += chunk.len();
            rows.push(div().flex().h(px(row_height)).flex_none().children(chunk.iter().enumerate().map(|(column, entry)| {
                let index = start + column;
                let label = entry.label.clone();
                let selected = current == Some(entry.value.as_str()) || (bar.icon_picker.keyboard && bar.selected == index);
                div().id(("icon-cell", index)).debug_selector(move || format!("icon-cell-{index}"))
                    .w(px(CELL)).h(px(row_height)).flex_none().rounded(px(4.)).flex().items_center().justify_center()
                    .when(selected, |this| this.bg(hsla(0., 0., 1., 0.2)))
                    .hover(|style| style.bg(hsla(0., 0., 1., 0.16)))
                    .tooltip(move |_, cx| cx.new(|_| IconTooltip(label.clone())).into())
                    .on_click(cx.listener(move |this, _, _, cx| this.select_picker_icon(index, cx)))
                    .child(if emoji { div().text_size(px(20.)).line_height(px(24.)).child(entry.value.clone()).into_any_element() }
                        else if entry.value == "shape:circle" { div().size(px(12.)).rounded_full().bg(gpui::white()).into_any_element() }
                        else { crate::icons::ionicon(entry.value.strip_prefix("ion:").unwrap_or_default(), 14., 1.) })
            })));
        }
        let categories = [
            ("Smileys & Emotion", "happy-outline"), ("Animals & Nature", "sunny-outline"),
            ("Food & Drink", "cafe-outline"), ("Activities", "ribbon-outline"),
            ("Travel & Places", "bus-outline"), ("Objects", "gift-outline"),
            ("Symbols", "heart-outline"), ("Flags", "flag-outline"),
        ];
        let arrow_x = (f32::from(anchor.center().x) - left).clamp(12., WIDTH - 12.);
        let panel = div().id("icon-picker").debug_selector(|| "icon-picker".into()).key_context(CONTEXT)
            .when(!emoji, |this| this.track_focus(&bar.input.read(cx).focus_handle(cx)))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if this.command_bar.as_ref().is_some_and(|bar| bar.icon_picker.keyboard) {
                    let delta = match event.keystroke.key.as_str() { "left" => -1, "right" => 1, _ => return };
                    this.picker_move(delta, cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &crate::window::OpenCommandBar, window, cx| this.open_command_bar(Target::NewTab, "", window, cx)))
            .on_action(cx.listener(|this, _: &crate::window::EditCurrentUrl, window, cx| {
                let (target, text) = this.browser.active_tab().map(|tab| (Target::Tab(tab.id), tab.url.clone())).unwrap_or((Target::NewTab, String::new()));
                this.open_command_bar(target, &text, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextRow, _, cx| this.picker_move(8, cx)))
            .on_action(cx.listener(|this, _: &PreviousRow, _, cx| this.picker_move(-8, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| { let index = this.command_bar.as_ref().map_or(0, |bar| bar.selected); this.confirm(index, cx); }))
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| this.close_command_bar(cx)))
            .on_click(|_, _, cx| cx.stop_propagation())
            .absolute().left(px(left)).top(px(top)).w(px(WIDTH)).h(px(panel_height)).rounded(px(20.))
            .bg(hsla(240. / 360., 0.12, 0.18, if crate::native::HAS_BACKDROP { 0.86 } else { 1. }))
            .border_1().border_color(hsla(0., 0., 0., 0.8)).shadow_2xl()
            .font_family(".SystemUIFont").text_size(px(12.)).text_color(gpui::white())
            .child(canvas(|_, _, _| (), |bounds, _, window, _| crate::panel::backdrop(window, "command-bar", Some(bounds), 20., crate::native::Material::Menu)).absolute().inset_0())
            .child(div().absolute().inset_0().rounded(px(19.)).border_1().border_color(hsla(0., 0., 1., 0.15)))
            .child(canvas(|_, _, _| (), move |bounds, _, window, _| {
                let mut path = gpui::PathBuilder::fill();
                let y = if above { f32::from(bounds.size.height) } else { 0. };
                let dy = if above { 10. } else { -10. };
                path.move_to(bounds.origin + point(px(arrow_x - 10.), px(y)));
                path.line_to(bounds.origin + point(px(arrow_x), px(y + dy)));
                path.line_to(bounds.origin + point(px(arrow_x + 10.), px(y)));
                path.close();
                if let Ok(path) = path.build() { window.paint_path(path, hsla(240. / 360., 0.12, 0.18, 1.)); }
            }).absolute().inset_0())
            .when(folder, |this| this.child(div().absolute().top(px(13.)).left_0().w_full().h(px(24.)).flex().items_center().justify_center().gap(px(8.))
                .child(div().font_weight(gpui::FontWeight::SEMIBOLD).text_size(px(13.)).child("Icon"))
                .when(current.is_some(), |this| this.child(div().id("icon-picker-remove").debug_selector(|| "icon-picker-remove".into())
                    .size(px(24.)).rounded(px(4.)).flex().items_center().justify_center().hover(|style| style.bg(hsla(0., 0., 1., 0.12)))
                    .tooltip(|_, cx| cx.new(|_| IconTooltip("Remove icon".into())).into())
                    .on_click(cx.listener(|this, _, _, cx| this.set_picker_icon(None, cx)))
                    .child(crate::icons::ionicon("trash-outline", 16., 1.))))))
            .when(!folder, |this| this.child(div().absolute().top(px(13.)).left_0().w_full().h(px(24.)).flex().items_center().justify_center().gap(px(6.))
                .child(div().flex().h(px(24.)).rounded(px(5.)).bg(hsla(0., 0., 1., 0.1))
                    .children([(Catalog::Emoji, "Emoji", "icon-picker-emoji"), (Catalog::Symbols, "Icon", "icon-picker-symbols")].into_iter().map(|(catalog, label, selector)| {
                        div().id(selector).debug_selector(move || selector.into()).w(px(57.)).h_full().rounded(px(5.)).flex().items_center().justify_center()
                            .when(catalog == bar.icon_picker.catalog, |this| this.bg(gpui::rgb(0x1685ff)))
                            .on_click(cx.listener(move |this, _, _, cx| this.picker_catalog(catalog, cx))).child(label)
                    })))
                .when(current.is_some(), |this| this.child(div().id("icon-picker-remove").debug_selector(|| "icon-picker-remove".into())
                    .size(px(24.)).rounded(px(4.)).flex().items_center().justify_center().hover(|style| style.bg(hsla(0., 0., 1., 0.12)))
                    .tooltip(|_, cx| cx.new(|_| IconTooltip("Remove icon".into())).into())
                    .on_click(cx.listener(|this, _, _, cx| this.set_picker_icon(None, cx)))
                    .child(crate::icons::ionicon("trash-outline", 16., 1.))))))
            .when(emoji, |this| this.child(div().absolute().top(px(46.)).left(px(8.)).right(px(8.)).h(px(28.)).rounded(px(14.))
                .border_3().border_color(gpui::rgb(0x327fa9)).bg(hsla(0., 0., 0.08, 0.8)).flex().items_center().gap(px(4.)).px(px(5.))
                .child(div().text_color(hsla(0., 0., 1., 0.5)).child(crate::icons::ionicon("search-outline", 13., 1.)))
                .child(div().debug_selector(|| "icon-picker-search".into()).flex_1().min_w_0().child(bar.input.clone()))
                .when(!bar.input.read(cx).text().is_empty(), |this| this.child(div().id("icon-picker-clear").debug_selector(|| "icon-picker-clear".into())
                    .on_click(cx.listener(|this, _, _, cx| { if let Some(bar) = &this.command_bar { bar.input.update(cx, |input, cx| input.set_text("", cx)); } }))
                    .text_color(hsla(0., 0., 1., 0.5)).child(crate::icons::ionicon("close-circle", 14., 1.))))))
            .child(div().id("icon-picker-grid").debug_selector(|| "icon-picker-grid".into()).absolute().top(px(if folder { 42. } else if emoji { 75. } else { 47. })).left(px(11.)).w(px(248.)).h(px(if folder { 93. } else if emoji { 154. } else { 217. }))
                .overflow_y_scroll().track_scroll(&bar.rows_scroll).flex().flex_col().children(rows))
            .when(emoji, |this| this.child(div().absolute().left(px(10.)).right(px(10.)).bottom(px(12.)).h(px(31.)).border_t_1().border_color(hsla(0., 0., 1., 0.12))
                .pt(px(4.)).flex().items_center().children(categories.into_iter().enumerate().map(|(index, (category, icon))| {
                    div().id(("icon-category", index)).debug_selector(move || format!("icon-category-{index}"))
                        .w(px(CELL)).h(px(24.)).flex().items_center().justify_center().rounded(px(4.))
                        .text_color(hsla(0., 0., 1., 0.45)).hover(|style| style.bg(hsla(0., 0., 1., 0.1)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.picker_category(category, cx);
                        })).child(crate::icons::ionicon(icon, 14., 1.))
                }))));
        div().id("icon-picker-backdrop").debug_selector(|| "icon-picker-backdrop".into()).size_full()
            .on_click(cx.listener(|this, _, _, cx| this.close_command_bar(cx))).child(panel).into_any_element()
    }
}

struct IconTooltip(String);
impl gpui::Render for IconTooltip {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px(px(8.)).py(px(4.)).rounded(px(5.)).bg(gpui::rgb(0x303039))
            .text_color(gpui::white()).text_size(px(11.)).child(self.0.clone())
    }
}
