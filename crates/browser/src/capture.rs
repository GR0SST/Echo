//! Page capture chrome. Reference Display-P3 colors are converted to sRGB for GPUI. Screenshot/document work stays behind Pages/engine.
use crate::{
    icons::{self, Icon},
    pages::PageIdentity,
    text_input::TextInput,
    window::{BrowserWindow, Overlay, OverlayKind, PopupFocus},
};
use gpui::{
    actions, div, point, prelude::*, px, size, AnyElement, App, Bounds, ClipboardItem, Context,
    CursorStyle, Entity, FocusHandle, Focusable, KeyBinding, MouseButton, Pixels, Point, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::{
    io::{Cursor, Write},
    sync::Arc,
    time::{Duration, SystemTime},
};
actions!(capture, [DismissCapture]);
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", DismissCapture, Some("Capture")),
        KeyBinding::new("cmd-w", DismissCapture, Some("Capture")),
    ]);
}
#[derive(Clone, Debug, PartialEq)]
struct Scope {
    identity: PageIdentity,
    epoch: u64,
    zoom: u16,
    url: String,
    frame: engine::Bounds,
}
pub(crate) struct Capture {
    scope: Scope,
    pub overlay: Option<WindowHandle<Overlay>>,
    focus: FocusHandle,
    pointer: Point<Pixels>,
    anchor: Option<Point<Pixels>>,
    hover: Option<Bounds<Pixels>>,
    selected: Option<Bounds<Pixels>>,
    query: Option<engine::ScriptTicket>,
    queried: Option<Point<Pixels>>,
    request: Option<u64>,
    png: Option<Arc<Vec<u8>>>,
    dimensions: (u32, u32),
    saved: bool,
    destination: Entity<TextInput>,
}
impl Capture {
    pub(crate) fn is_selecting(&self) -> bool {
        self.selected.is_none()
    }
}

fn viewport(frame: engine::Bounds) -> Bounds<Pixels> {
    Bounds::new(
        point(px(frame.x as f32), px(frame.y as f32)),
        size(px(frame.width as f32), px(frame.height as f32)),
    )
}
fn drag_rect(a: Point<Pixels>, b: Point<Pixels>, frame: Bounds<Pixels>) -> Bounds<Pixels> {
    let at = |p: Point<Pixels>| {
        point(
            p.x.max(frame.left()).min(frame.right()),
            p.y.max(frame.top()).min(frame.bottom()),
        )
    };
    let (a, b) = (at(a), at(b));
    Bounds::new(
        point(a.x.min(b.x), a.y.min(b.y)),
        size((b.x - a.x).abs(), (b.y - a.y).abs()),
    )
}
pub(crate) fn crop_png(
    png: &[u8],
    rect: Bounds<Pixels>,
    frame: engine::Bounds,
) -> Option<(Vec<u8>, u32, u32)> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?;
    if image.width() == 0 || image.height() == 0 || frame.width <= 0. || frame.height <= 0. {
        return None;
    }
    let x = ((f64::from(f32::from(rect.left())) - frame.x) / frame.width * f64::from(image.width()))
        .floor()
        .max(0.) as u32;
    let y = ((f64::from(f32::from(rect.top())) - frame.y) / frame.height
        * f64::from(image.height()))
    .floor()
    .max(0.) as u32;
    let right = ((f64::from(f32::from(rect.right())) - frame.x) / frame.width
        * f64::from(image.width()))
    .ceil()
    .min(f64::from(image.width())) as u32;
    let bottom = ((f64::from(f32::from(rect.bottom())) - frame.y) / frame.height
        * f64::from(image.height()))
    .ceil()
    .min(f64::from(image.height())) as u32;
    let (width, height) = (right.checked_sub(x)?, bottom.checked_sub(y)?);
    if width == 0 || height == 0 {
        return None;
    }
    let mut out = Cursor::new(Vec::new());
    image
        .crop_imm(x, y, width, height)
        .write_to(&mut out, image::ImageFormat::Png)
        .ok()?;
    Some((out.into_inner(), width, height))
}
impl BrowserWindow {
    fn capture_scope(&self) -> Option<Scope> {
        if !self.page_content_visible() {
            return None;
        }
        let tab = self.browser.active_tab()?;
        let pages = self.pages.as_ref()?.borrow();
        let identity = pages.identity_for_tab(tab.id)?;
        let epoch = pages.page_document_epoch(tab.id)?;
        let frame = pages
            .surface_layout()
            .into_iter()
            .find(|(id, _)| *id == tab.id)?
            .1;
        Some(Scope {
            identity,
            epoch,
            zoom: pages.zoom_percent(tab.id),
            url: tab.url.clone(),
            frame,
        })
    }
    fn capture_valid(&self, scope: &Scope) -> bool {
        self.capture_scope().as_ref() == Some(scope)
    }
    pub(crate) fn capture_available(&self) -> bool {
        self.capture_scope().is_some()
            && self
                .pages
                .as_ref()
                .is_some_and(|pages| pages.borrow().supports_snapshots())
    }
    pub(crate) fn capture_page(&mut self, cx: &mut Context<Self>) {
        if self.capture.is_some() || self.save_prompt_active {
            return;
        }
        let Some(scope) = self.capture_scope() else {
            return;
        };
        if !self
            .pages
            .as_ref()
            .is_some_and(|pages| pages.borrow().supports_snapshots())
        {
            return;
        }
        self.close_popups_for_window_operation(false, cx);
        self.suspend_popup_focus_for_utility();
        let frame = viewport(scope.frame);
        let focus = cx.focus_handle();
        let destination = cx.new(|cx| TextInput::new("Send To...", cx));
        self.capture = Some(Capture {
            scope,
            overlay: None,
            focus,
            pointer: frame.center(),
            anchor: None,
            hover: None,
            selected: None,
            query: None,
            queried: None,
            request: None,
            png: None,
            dimensions: (0, 0),
            saved: false,
            destination,
        });
        if let Some(pages) = &self.pages {
            pages.borrow_mut().focus_ui();
        }
        let browser = cx.entity();
        let main = self.main_window;
        let parent = self.native_view;
        cx.defer(move |cx| {
            if !browser
                .read(cx)
                .capture
                .as_ref()
                .is_some_and(|capture| browser.read(cx).capture_valid(&capture.scope))
            {
                return;
            }
            let Ok(bounds) = main.update(cx, |_, window, _| window.bounds()) else {
                return;
            };
            if let Ok(handle) = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    kind: WindowKind::Normal,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                |window, cx| {
                    if let (Some(parent), Some(child)) = (parent, crate::native::view_of(window)) {
                        crate::native::attach_overlay(parent, child, true);
                    }
                    if let Some(capture) = &browser.read(cx).capture {
                        capture.focus.focus(window);
                    }
                    cx.new(|cx| Overlay::new(browser.downgrade(), OverlayKind::Capture, window, cx))
                },
            ) {
                browser.update(cx, |view, _| {
                    if let Some(capture) = &mut view.capture {
                        capture.overlay = Some(handle);
                    }
                });
            }
        });
        cx.spawn(async move |view, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(30))
                .await;
            let Ok(keep) = view.update(cx, |view, cx| view.poll_capture(cx)) else {
                break;
            };
            if !keep {
                break;
            }
        })
        .detach();
        cx.notify();
    }
    pub(crate) fn close_capture(&mut self, focus: PopupFocus, cx: &mut Context<Self>) {
        let Some(capture) = self.capture.take() else {
            return;
        };
        self.close_popup_overlay(capture.overlay, focus, false, cx);
        cx.notify();
    }
    fn poll_capture(&mut self, cx: &mut Context<Self>) -> bool {
        self.refresh_profile_projection();
        let Some(mut capture) = self.capture.take() else {
            return false;
        };
        if !self.capture_valid(&capture.scope) || crate::quit::is_quitting() {
            self.capture = Some(capture);
            self.close_capture(PopupFocus::OnActivation, cx);
            return false;
        }
        if let Some(result) = capture.query.as_ref().and_then(|ticket| ticket.take()) {
            capture.query = None;
            if let Ok(json) = result {
                if let Ok(values) = serde_json::from_str::<[f32; 4]>(&json) {
                    if values.iter().all(|v| v.is_finite()) && values[2] > 0. && values[3] > 0. {
                        let frame = viewport(capture.scope.frame);
                        let start = frame.origin + point(px(values[0]), px(values[1]));
                        let end = start + point(px(values[2]), px(values[3]));
                        capture.hover = Some(drag_rect(start, end, frame));
                        cx.notify();
                    }
                }
            }
        }
        if capture.selected.is_none()
            && capture.anchor.is_none()
            && capture.query.is_none()
            && capture.queried != Some(capture.pointer)
        {
            let frame = viewport(capture.scope.frame);
            let p = capture.pointer - frame.origin;
            let zoom = f32::from(capture.scope.zoom) / 100.;
            let source=format!("JSON.stringify((()=>{{const e=document.elementFromPoint({},{});if(!e)return null;const r=e.getBoundingClientRect();return[r.x*{zoom},r.y*{zoom},r.width*{zoom},r.height*{zoom}]}})())",f32::from(p.x)/zoom,f32::from(p.y)/zoom);
            capture.queried = Some(capture.pointer);
            capture.query = self.pages.as_ref().and_then(|pages| {
                pages
                    .borrow_mut()
                    .evaluate_script(
                        capture.scope.identity.tab,
                        capture.scope.identity.profile,
                        &capture.scope.url,
                        &source,
                    )
                    .ok()
            });
        }
        if let Some(request) = capture.request {
            let pages = self.pages.as_ref().unwrap().borrow();
            if let Some((got, png)) = pages
                .snapshot(capture.scope.identity.tab)
                .filter(|(id, _)| *id == request)
            {
                let _ = got;
                capture.request = None;
                if let Some((png, width, height)) = capture
                    .selected
                    .and_then(|rect| crop_png(&png, rect, capture.scope.frame))
                {
                    if std::env::var_os("BROWSER_SCRIPT").is_some() {
                        eprintln!("[capture-ready] {width}x{height}");
                    }
                    capture.png = Some(Arc::new(png));
                    crate::feedback::capture(self,cx);
                    capture.dimensions = (width, height);
                    let overlay = capture.overlay;
                    let input = capture.destination.clone();
                    let owner = cx.weak_entity();
                    cx.defer(move |cx| {
                        let ready = owner
                            .read_with(cx, |view, _| {
                                view.capture.as_ref().is_some_and(|state| {
                                    state.png.is_some() && state.overlay == overlay
                                })
                            })
                            .unwrap_or(false);
                        if ready {
                            if let Some(overlay) = overlay {
                                let _ = overlay.update(cx, |_, window, cx| {
                                    input.read(cx).focus_handle(cx).focus(window)
                                });
                            }
                        }
                    });
                    cx.notify();
                } else {
                    drop(pages);
                    self.capture = Some(capture);
                    self.close_capture(PopupFocus::Restore, cx);
                    return false;
                }
            } else if !pages.snapshot_pending(capture.scope.identity.tab, request) {
                drop(pages);
                self.capture = Some(capture);
                self.close_capture(PopupFocus::Restore, cx);
                return false;
            }
        }
        self.capture = Some(capture);
        true
    }
    fn capture_down(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(capture) = &mut self.capture {
            if capture.selected.is_none() && viewport(capture.scope.frame).contains(&p) {
                capture.anchor = Some(p);
                capture.pointer = p;
                cx.notify();
            }
        }
    }
    fn capture_move(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(capture) = &mut self.capture {
            if capture.selected.is_none() {
                capture.pointer = p;
                cx.notify();
            }
        }
    }
    fn capture_up(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        if std::env::var_os("BROWSER_SCRIPT").is_some() {
            eprintln!("[capture-up] {:?}", p);
        }
        let Some(capture) = &mut self.capture else {
            return;
        };
        let Some(anchor) = capture.anchor.take() else {
            return;
        };
        let drag = drag_rect(anchor, p, viewport(capture.scope.frame));
        let rect = if drag.size.width > px(3.) && drag.size.height > px(3.) {
            Some(drag)
        } else {
            capture.hover
        };
        let Some(rect) = rect.filter(|r| r.size.width > px(1.) && r.size.height > px(1.)) else {
            return;
        };
        capture.selected = Some(rect);
        capture.request = self.pages.as_ref().and_then(|pages| {
            pages
                .borrow_mut()
                .request_fresh_snapshot(capture.scope.identity.tab)
        });
        if capture.request.is_none() {
            self.close_capture(PopupFocus::Restore, cx);
        }
        cx.notify();
    }
    fn capture_copy(&mut self, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        let Some(capture) = &self.capture else {
            return;
        };
        if !self.capture_valid(&capture.scope) {
            return;
        };
        if let Some(png) = &capture.png {
            cx.write_to_clipboard(ClipboardItem::new_image(&gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                png.as_ref().clone(),
            )));
        }
    }
    fn capture_save_library(&mut self, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        let Some(capture) = &self.capture else {
            return;
        };
        if capture.saved || !self.capture_valid(&capture.scope) {
            return;
        };
        let Some(png) = capture.png.clone() else {
            return;
        };
        let scope = capture.scope.clone();
        let dimensions = capture.dimensions;
        let title = self
            .browser
            .active_tab()
            .map(|t| t.title.clone())
            .unwrap_or_default();
        let Some(root) = self.profile.handle.storage_directory(cx) else {
            return;
        };
        let directory = root.join("Captures").join(format!(
            "profile-{}",
            serde_json::to_string(&scope.identity.profile).unwrap()
        ));
        let published = (|| -> std::io::Result<std::path::PathBuf> {
            std::fs::create_dir_all(&directory)?;
            let mut file = tempfile::Builder::new()
                .prefix("Capture ")
                .suffix(".png")
                .tempfile_in(&directory)?;
            file.write_all(&png)?;
            file.as_file().sync_all()?;
            let (_, path) = file.keep().map_err(|e| e.error)?;
            Ok(path)
        })();
        let Ok(path) = published else {
            return;
        };
        let filename = path.file_name().unwrap().to_string_lossy().into_owned();
        let id = self.edit_browser(|browser| {
            browser
                .user_profile_mut(scope.identity.profile)?
                .captures
                .add(
                    "Capture".into(),
                    title,
                    scope.url,
                    filename,
                    dimensions.0,
                    dimensions.1,
                    SystemTime::now(),
                )
        });
        if id.is_some() {
            if let Some(capture) = &mut self.capture {
                capture.saved = true;
            }
            cx.notify();
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
    fn capture_save_file(&mut self, cx: &mut Context<Self>) {
        self.refresh_profile_projection();
        let Some(capture) = &self.capture else {
            return;
        };
        let Some(png) = capture.png.clone() else {
            return;
        };
        if !self.capture_valid(&capture.scope) {
            return;
        }
        self.save_prompt_active = true;
        self.close_capture(PopupFocus::OnActivation, cx);
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let directory = dirs::download_dir().unwrap_or_else(crate::profile_dir);
            let choice = cx.prompt_for_new_path(&directory, Some("Capture.png"));
            cx.spawn(async move |cx| {
                let chosen = choice.await;
                let _ = owner.update(cx, |view, cx| {
                    view.save_prompt_active = false;
                    cx.notify();
                });
                if let Ok(Ok(Some(path))) = chosen {
                    let _ = std::fs::write(path, png.as_ref());
                }
            })
            .detach();
        });
    }

    fn capture_retake(&mut self, cx: &mut Context<Self>) {
        if let Some(capture) = &mut self.capture {
            capture.selected = None;
            capture.png = None;
            capture.request = None;
            capture.saved = false;
            capture.queried = None;
            cx.notify();
        }
    }
    pub(crate) fn render_capture(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(capture) = &self.capture else {
            return div().into_any_element();
        };
        let frame = viewport(capture.scope.frame);
        let selecting = capture.is_selecting();
        let rect = capture
            .selected
            .or_else(|| capture.anchor.map(|a| drag_rect(a, capture.pointer, frame)))
            .or(capture.hover);
        let focus = capture.focus.clone();
        let pointer = capture.pointer;
        let menu_input = capture.destination.clone();
        let saved = capture.saved;
        let ready = capture.png.is_some();
        let full = window.bounds().size;
        let outline = rect.map(|r| {
            Bounds::new(
                r.origin - point(px(4.), px(4.)),
                r.size + size(px(8.), px(8.)),
            )
        });
        let mask = gpui::black().opacity(0.25);
        let mut root = div()
            .id("page-capture")
            .debug_selector(|| "page-capture".into())
            .key_context("Capture")
            .track_focus(&focus)
            .relative()
            .size_full()
            .font_family(".SystemUIFont")
            .cursor(CursorStyle::None)
            .on_action(cx.listener(|view, _: &DismissCapture, _, cx| {
                view.close_capture(PopupFocus::Restore, cx)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &gpui::MouseDownEvent, _, cx| {
                    view.capture_down(event.position, cx)
                }),
            )
            .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, _, cx| {
                view.capture_move(event.position, cx)
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, event: &gpui::MouseUpEvent, _, cx| {
                    view.capture_up(event.position, cx)
                }),
            );
        root = root.child(mask_canvas(outline, mask));
        if let Some(r) = outline {
            // A transparent box's shadow also paints beneath its interior.
            // Clip four copies outside the hole so the selected page stays clear.
            let clips = [
                Bounds::new(point(px(0.), px(0.)), size(full.width, r.top().max(px(0.)))),
                Bounds::new(
                    point(px(0.), r.top()),
                    size(r.left().max(px(0.)), r.size.height),
                ),
                Bounds::new(
                    point(r.right(), r.top()),
                    size((full.width - r.right()).max(px(0.)), r.size.height),
                ),
                Bounds::new(
                    point(px(0.), r.bottom()),
                    size(full.width, (full.height - r.bottom()).max(px(0.))),
                ),
            ];
            for clip in clips {
                root = root.child(
                    div()
                        .absolute()
                        .left(clip.left())
                        .top(clip.top())
                        .w(clip.size.width)
                        .h(clip.size.height)
                        .overflow_hidden()
                        .child(
                            div()
                                .absolute()
                                .left(r.left() - clip.left())
                                .top(r.top() - clip.top())
                                .w(r.size.width)
                                .h(r.size.height)
                                .rounded(px(6.))
                                .shadow(vec![gpui::BoxShadow {
                                    color: gpui::black().opacity(0.35),
                                    offset: point(px(0.), px(5.)),
                                    blur_radius: px(24.),
                                    spread_radius: px(0.),
                                }]),
                        ),
                );
            }
            root = root.child(
                div()
                    .absolute()
                    .left(r.left())
                    .top(r.top())
                    .w(r.size.width)
                    .h(r.size.height)
                    .rounded(px(6.))
                    .border_4()
                    .border_color(gpui::rgb(0x171c2e)),
            );
        }
        if selecting {
            root = root.child(
                div()
                    .absolute()
                    .left(frame.center().x - px(177.5))
                    .top(frame.bottom() - px(102.))
                    .w(px(355.))
                    .h(px(67.))
                    .rounded(px(16.))
                    .p(px(1.))
                    .bg(gpui::linear_gradient(
                        90.,
                        gpui::linear_color_stop(gpui::rgb(0xffe4e4), 0.),
                        gpui::linear_color_stop(gpui::rgb(0xbbc7fe), 1.),
                    ))
                    .child(
                        div()
                            .size_full()
                            .rounded(px(15.))
                            .bg(gpui::white())
                            .text_color(gpui::rgb(0x6335af))
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("Click or drag to capture a portion of this page"),
                    ),
            );
            root = root.child(
                div()
                    .absolute()
                    .left(pointer.x - px(11.))
                    .top(pointer.y - px(9.))
                    .child(camera()),
            );
        } else if ready {
            let r = rect.unwrap();
            let height = px(229.);
            let x = if r.right() + px(286.) <= full.width {
                r.right() + px(14.)
            } else {
                (r.left() - px(286.)).max(px(10.))
            };
            let y = (r.top() - px(4.))
                .max(px(10.))
                .min((full.height - height - px(10.)).max(px(10.)));
            let row = |label: &'static str, icon: AnyElement| {
                div()
                    .id(label)
                    .debug_selector(move || format!("capture-{label}"))
                    .h(px(40.))
                    .py(px(4.))
                    .child(
                        div()
                            .h_full()
                            .rounded(px(5.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .hover(|s| s.bg(gpui::rgb(0x3a3868)).text_color(gpui::rgb(0xd6d7fe)))
                            .child(div().text_color(gpui::rgb(0x58566d)).child(icon))
                            .child(label),
                    )
            };
            let save_icon = div()
                .size(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .size(px(14.))
                        .rounded_full()
                        .border_1()
                        .border_color(gpui::rgb(0x58566d))
                        .when(saved, |v| {
                            v.bg(gpui::rgb(0xd6d7fe))
                                .border_color(gpui::rgb(0xd6d7fe))
                                .text_color(gpui::rgb(0x635f9b))
                                .child(icons::control_sized(Icon::Check, 12.))
                        }),
                )
                .into_any_element();
            root = root.child(
                div()
                    .id("capture-menu")
                    .debug_selector(|| "capture-menu".into())
                    .absolute()
                    .left(x)
                    .top(y)
                    .w(px(273.))
                    .rounded(px(7.))
                    .px(px(10.))
                    .pt(px(13.))
                    .pb(px(4.))
                    .bg(gpui::rgb(0x130f2e))
                    .text_color(gpui::rgb(0xe8e7ea))
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .cursor_default()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .h(px(31.))
                            .mb(px(7.))
                            .rounded(px(5.))
                            .px(px(10.))
                            .bg(gpui::rgb(0x24203d))
                            .child(menu_input),
                    )
                    .child(
                        row(
                            if saved {
                                "Saved to Library"
                            } else {
                                "Save to Library"
                            },
                            save_icon,
                        )
                        .on_click(cx.listener(|view, _, window, cx| {
                            if let Some(state) = &view.capture {
                                state.focus.focus(window);
                            }
                            view.capture_save_library(cx)
                        })),
                    )
                    .child(
                        row("Copy", icons::ionicon("copy-outline", 18., 1.)).on_click(cx.listener(
                            |view, _, window, cx| {
                                if let Some(state) = &view.capture {
                                    state.focus.focus(window);
                                }
                                view.capture_copy(cx)
                            },
                        )),
                    )
                    .child(
                        row("Save…", icons::ionicon("download", 18., 1.)).on_click(cx.listener(
                            |view, _, window, cx| {
                                if let Some(state) = &view.capture {
                                    state.focus.focus(window);
                                }
                                view.capture_save_file(cx)
                            },
                        )),
                    )
                    .child(
                        div()
                            .h(px(14.))
                            .flex()
                            .items_center()
                            .child(div().h(px(1.)).w_full().bg(gpui::white().opacity(0.08))),
                    )
                    .child(
                        row("Retake", icons::control(Icon::Reload)).on_click(cx.listener(
                            |view, _, window, cx| {
                                if let Some(state) = &view.capture {
                                    state.focus.focus(window);
                                }
                                view.capture_retake(cx)
                            },
                        )),
                    ),
            );
        }
        root.into_any_element()
    }
}
fn mask_canvas(hole: Option<Bounds<Pixels>>, color: gpui::Hsla) -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let mut path = gpui::PathBuilder::fill();
            path.move_to(bounds.origin);
            path.line_to(point(bounds.right(), bounds.top()));
            path.line_to(point(bounds.right(), bounds.bottom()));
            path.line_to(point(bounds.left(), bounds.bottom()));
            path.close();
            if let Some(r) = hole {
                let radius = 6_f32
                    .min(f32::from(r.size.width) / 2.)
                    .min(f32::from(r.size.height) / 2.);
                let (l, t, right, bottom) = (
                    f32::from(r.left()),
                    f32::from(r.top()),
                    f32::from(r.right()),
                    f32::from(r.bottom()),
                );
                let mut points = Vec::with_capacity(52);
                for (x, y, from) in [
                    (l + radius, t + radius, std::f32::consts::PI),
                    (right - radius, t + radius, 1.5 * std::f32::consts::PI),
                    (right - radius, bottom - radius, 0.),
                    (l + radius, bottom - radius, std::f32::consts::FRAC_PI_2),
                ] {
                    for i in 0..=12 {
                        let angle = from + std::f32::consts::FRAC_PI_2 * i as f32 / 12.;
                        points.push(
                            bounds.origin
                                + point(px(x + radius * angle.cos()), px(y + radius * angle.sin())),
                        );
                    }
                }
                // Opposite winding subtracts the rounded selection from the mask.
                points.reverse();
                if let Some(first) = points.first() {
                    path.move_to(*first);
                    for p in &points[1..] {
                        path.line_to(*p);
                    }
                    path.close();
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

fn camera() -> AnyElement {
    gpui::canvas(
        |_, _, _| (),
        |bounds, _, window, _| {
            let dark = gpui::rgb(0x1b2931);
            let light = gpui::rgb(0xfcfcfc);
            let at = |x: f32, y: f32, w: f32, h: f32, r: f32, color: gpui::Hsla| {
                let mut q = gpui::fill(
                    Bounds::new(bounds.origin + point(px(x), px(y)), size(px(w), px(h))),
                    color,
                );
                q.corner_radii = px(r).into();
                q
            };
            window.paint_quad(at(0., 3., 21., 14., 3., light.into()));
            window.paint_quad(at(5., 0.5, 10., 5.5, 2., light.into()));
            window.paint_quad(at(1., 4., 19., 11., 2., dark.into()));
            window.paint_quad(at(6., 1., 8., 5., 1., dark.into()));
            window.paint_quad(at(6., 5., 9., 9., 4.5, light.into()));
            window.paint_quad(at(7.5, 6.5, 6., 6., 3., dark.into()));
        },
    )
    .size(px(22.))
    .into_any_element()
}
