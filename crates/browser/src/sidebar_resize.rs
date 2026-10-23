//! Window-local GPUI resizing. No workspace or session mutation is involved.
use super::*;
use crate::sidebar_resize_state::{SidebarResizeOutcome, SidebarResizeState};

/// The exact native hit-strip width remains a visual acceptance gate.
const RESIZE_HIT_WIDTH: f32 = 4.;

pub(super) struct SidebarResizeGesture {
    state: SidebarResizeState,
    activation_id: u64,
    activation_child: Option<WindowHandle<Overlay>>,
    live_width: f32,
    floating: bool,
    focus: Option<FocusHandle>,
    page_focus: Option<crate::pages::ResizePageFocus>,
    popup_generation: u64,
}
impl BrowserWindow {
    pub(crate) fn sidebar_width(&self) -> f32 {
        self.sidebar_render_width.unwrap_or_else(|| {
            self.sidebar_resize
                .as_ref()
                .map_or(self.sidebar_width, |resize| resize.live_width)
        })
    }

    /// Window-level listeners retain the gesture outside the stripe and even
    /// outside the root hitbox. They do nothing when no resize is active.
    pub(super) fn render_sidebar_resize_tracking(&self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let moving = owner.clone();
                window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture {
                        return;
                    }
                    let _ = moving.update(cx, |this, cx| {
                        if this.sidebar_resize.is_some() && !event.dragging() {
                            this.cancel_sidebar_resize(window, cx);
                        } else {
                            this.move_sidebar_resize(f32::from(event.position.x), window, cx);
                        }
                    });
                });
                let ending = owner.clone();
                window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture
                        || event.button != gpui::MouseButton::Left
                    {
                        return;
                    }
                    let _ = ending.update(cx, |this, cx| {
                        this.end_sidebar_resize(f32::from(event.position.x), window, cx)
                    });
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .w(px(0.))
        .h(px(0.))
        .into_any_element()
    }

    pub(super) fn render_sidebar_resize_handle(
        &self,
        floating: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let right = if floating {
            self.sidebar_width()
        } else {
            self.sidebar_width() * self.sidebar_amount()
        };
        div()
            .id("sidebar-resize-handle")
            .debug_selector(|| "sidebar-resize-handle".into())
            .absolute()
            .top_0()
            .bottom_0()
            .left(px((right - RESIZE_HIT_WIDTH).max(0.)))
            .w(px(RESIZE_HIT_WIDTH))
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .capture_any_mouse_down(cx.listener(
                move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if event.button != gpui::MouseButton::Left {
                        return;
                    }
                    // Preserve an existing input responder. Native page focus is
                    // temporarily routed to the GPUI root for drag cancellation.
                    window.prevent_default();
                    cx.stop_propagation();
                    if event.click_count >= 2 {
                        this.cancel_sidebar_resize(window, cx);
                        this.sidebar_width = SidebarResizeState::reset(this.sidebar_visible).width;
                        this.queue_floating_resize(cx);
                        cx.notify();
                    } else {
                        this.begin_sidebar_resize(
                            f32::from(event.position.x),
                            floating,
                            window,
                            cx,
                        );
                    }
                },
            ))
            .into_any_element()
    }

    fn begin_sidebar_resize(
        &mut self,
        pointer_x: f32,
        floating: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_resize.is_some() || crate::quit::is_quitting() {
            return;
        }
        self.sidebar_resize_focus_on_activation = None;
        let focus = window.focused(cx);
        let page_focus = self
            .pages
            .as_ref()
            .map(|pages| pages.borrow().capture_resize_focus());
        if page_focus
            .as_ref()
            .is_some_and(|focus| focus.has_native_focus())
        {
            if let Some(pages) = &self.pages {
                let mut pages = pages.borrow_mut();
                pages.suppress_resize_focus();
                pages.focus_ui();
            }
            window.focus(&self.focus_handle);
        }
        self.sidebar_reveal = None;
        // Pager positions are fractional Space positions; retaining them keeps
        // identity and animation progress when pixel travel changes.
        self.pager.drag = None;
        self.sidebar_resize_generation = self.sidebar_resize_generation.wrapping_add(1);
        self.sidebar_resize = Some(SidebarResizeGesture {
            activation_id: self.sidebar_resize_generation,
            activation_child: floating
                .then(|| {
                    self.floating_sidebar
                        .as_ref()
                        .and_then(|sidebar| sidebar.overlay)
                })
                .flatten(),
            state: SidebarResizeState::begin(
                self.sidebar_width,
                !floating && self.sidebar_visible,
                pointer_x,
            ),
            live_width: self.sidebar_width,
            floating,
            focus,
            page_focus,
            popup_generation: self.popup_focus_generation,
        });
        cx.notify();
    }

    pub(super) fn move_sidebar_resize(
        &mut self,
        pointer_x: f32,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(resize) = &mut self.sidebar_resize else {
            return false;
        };
        let outcome = resize.state.update(pointer_x);
        resize.live_width = outcome.width;
        if !resize.floating {
            self.sidebar_visible = outcome.visible;
        }
        self.queue_floating_resize(cx);
        cx.notify();
        true
    }

    pub(super) fn end_sidebar_resize(
        &mut self,
        pointer_x: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(resize) = self.sidebar_resize.take() else {
            return;
        };
        let outcome = resize.state.finish(pointer_x);
        self.apply_resize_outcome(outcome, resize.floating);
        self.queue_floating_resize(cx);
        self.restore_sidebar_resize_focus(resize, window, cx);
        cx.notify();
    }

    pub(super) fn capture_sidebar_resize_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_resize.is_none() || event.keystroke.key != "escape" {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        self.cancel_sidebar_resize(window, cx);
    }

    fn cancel_sidebar_resize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(resize) = self.sidebar_resize.take() else {
            return;
        };
        self.apply_resize_outcome(resize.state.cancel(), resize.floating);
        self.queue_floating_resize(cx);
        self.restore_sidebar_resize_focus(resize, window, cx);
        cx.notify();
    }

    fn apply_resize_outcome(&mut self, outcome: SidebarResizeOutcome, floating: bool) {
        self.sidebar_width = outcome.width;
        if !floating {
            self.sidebar_visible = outcome.visible;
        }
        self.sidebar_reveal = None;
    }

    /// Active commands cancel without waiting for a future activation. Release
    /// suppression now, then restore through the same validated responder path
    /// after this command has finished changing the host layout.
    pub(super) fn cancel_sidebar_resize_for_active_command(&mut self, cx: &mut Context<Self>) {
        let Some(resize) = self.sidebar_resize.take() else {
            return;
        };
        self.apply_resize_outcome(resize.state.cancel(), resize.floating);
        self.sidebar_resize_focus_on_activation = None;
        if let Some(pages) = &self.pages {
            pages.borrow_mut().end_resize_focus_suppression();
        }
        self.queue_floating_resize(cx);
        let generation = self.sidebar_resize_generation;
        let owner = cx.entity().downgrade();
        let main = self.main_window;
        cx.defer(move |cx| {
            if crate::quit::is_quitting() {
                return;
            }
            let _ = main.update(cx, |_, window, cx| {
                if !window.is_window_active() {
                    return;
                }
                let _ = owner.update(cx, |this, cx| {
                    if this.sidebar_resize_generation == generation && this.sidebar_resize.is_none()
                    {
                        this.restore_sidebar_resize_focus(resize, window, cx);
                    }
                });
            });
        });
        cx.notify();
    }

    /// Teardown never leaves a deferred focus restoration for a closing host.
    pub(super) fn discard_sidebar_resize(&mut self) {
        self.abandon_sidebar_resize();
        self.sidebar_resize_focus_on_activation = None;
        if let Some(pages) = &self.pages {
            pages.borrow_mut().end_resize_focus_suppression();
        }
    }

    /// Root and floating sidebar are one resize interaction hierarchy. AppKit
    /// sends the root's resign-key before the owned child becomes key, so check
    /// both exact windows after the paired notifications have had a frame to run.
    /// Never use App::active_window here: on macOS it reports mainWindow.
    pub(super) fn on_sidebar_resize_activation(
        &mut self,
        source: AnyWindowHandle,
        active: bool,
        cx: &mut Context<Self>,
    ) {
        if active {
            return;
        }
        let Some(resize) = &self.sidebar_resize else {
            return;
        };
        let main = self.main_window;
        let child = resize.activation_child;
        if source != main && child.map(|child| child.into()) != Some(source) {
            return;
        }
        if !resize.floating {
            if source == main {
                self.abandon_sidebar_resize();
                self.queue_floating_resize(cx);
                cx.notify();
            }
            return;
        }
        let activation_id = resize.activation_id;
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(PAGER_FRAME).await;
            if crate::quit::is_quitting() {
                return;
            }
            let Ok(Some(current_child)) = owner.update(cx, |this, _| {
                this.sidebar_resize
                    .as_ref()
                    .filter(|resize| resize.activation_id == activation_id && resize.floating)
                    .map(|_| {
                        this.floating_sidebar
                            .as_ref()
                            .filter(|sidebar| !sidebar.closing)
                            .and_then(|sidebar| sidebar.overlay)
                    })
            }) else {
                return;
            };
            // Each native window is queried after releasing the BrowserWindow
            // borrow; its key-status callback state is the cross-platform seam.
            let main_active = main
                .update(cx, |_, window, _| window.is_window_active())
                .unwrap_or(false);
            let child_matches = current_child == child && child.is_some();
            let child_active = child
                .filter(|_| child_matches)
                .and_then(|child| {
                    child
                        .update(cx, |_, window, _| window.is_window_active())
                        .ok()
                })
                .unwrap_or(false);
            let _ =
                owner.update(cx, |this, cx| {
                    if this.sidebar_resize.as_ref().is_some_and(|resize| {
                        resize.activation_id == activation_id && resize.floating
                    }) && (!child_matches || (!main_active && !child_active))
                    {
                        this.abandon_sidebar_resize();
                        this.queue_floating_resize(cx);
                        cx.notify();
                    }
                });
        })
        .detach();
    }

    /// Deactivation/minimization cancels without activating a responder.
    pub(super) fn abandon_sidebar_resize(&mut self) {
        if let Some(resize) = self.sidebar_resize.take() {
            self.apply_resize_outcome(resize.state.cancel(), resize.floating);
            if self.popup_focus_generation == resize.popup_generation
                && self.sidebar_root_may_take_focus()
            {
                if let Some(snapshot) = resize
                    .page_focus
                    .filter(|snapshot| snapshot.has_native_focus())
                {
                    self.sidebar_resize_focus_on_activation =
                        Some((snapshot, resize.popup_generation));
                }
            }
            if self.sidebar_resize_focus_on_activation.is_none() {
                if let Some(pages) = &self.pages {
                    pages.borrow_mut().end_resize_focus_suppression();
                }
            }
        }
        self.sidebar_resize_generation = self.sidebar_resize_generation.wrapping_add(1);
    }

    pub(super) fn restore_abandoned_resize_focus(&mut self) {
        let Some((snapshot, generation)) = self.sidebar_resize_focus_on_activation.take() else {
            return;
        };
        if let Some(pages) = &self.pages {
            pages.borrow_mut().end_resize_focus_suppression();
        }
        if generation != self.popup_focus_generation
            || !self.sidebar_root_may_take_focus()
            || !self.page_content_visible()
            || crate::quit::is_quitting()
        {
            return;
        }
        if let Some(pages) = &self.pages {
            pages.borrow_mut().restore_resize_focus(snapshot);
        }
    }

    fn restore_sidebar_resize_focus(
        &mut self,
        resize: SidebarResizeGesture,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pages) = &self.pages {
            pages.borrow_mut().end_resize_focus_suppression();
        }
        if self.popup_focus_generation != resize.popup_generation || crate::quit::is_quitting() {
            return;
        }
        // UI inputs were never blurred by the handle. Do not re-focus an old
        // editor if another control was activated while dragging.
        let Some(snapshot) = resize.page_focus else {
            return;
        };
        if !snapshot.has_native_focus() {
            return;
        }
        if !self.sidebar_root_may_take_focus() || !self.page_content_visible() {
            return;
        }
        if !self.focus_handle.is_focused(window) {
            return;
        }
        if let Some(pages) = &self.pages {
            if pages.borrow_mut().restore_resize_focus(snapshot) {
                return;
            }
        }
        // A retired native target is never replaced with another document.
        if let Some(focus) = resize.focus {
            if focus.is_focused(window) {
                window.focus(&focus);
            }
        }
    }

    pub(super) fn queue_floating_resize(&mut self, cx: &mut Context<Self>) {
        self.sidebar_resize_generation = self.sidebar_resize_generation.wrapping_add(1);
        let generation = self.sidebar_resize_generation;
        let Some(overlay) = self
            .floating_sidebar
            .as_ref()
            .filter(|sidebar| !sidebar.closing)
            .and_then(|sidebar| sidebar.overlay)
        else {
            return;
        };
        let owner = cx.entity().downgrade();
        cx.defer(move |cx| {
            if crate::quit::is_quitting() {
                return;
            }
            let Ok(Some(width)) = owner.update(cx, |this, _| {
                (this.sidebar_resize_generation == generation
                    && this.floating_sidebar.as_ref().is_some_and(|sidebar| {
                        !sidebar.closing && sidebar.overlay == Some(overlay)
                    }))
                .then(|| {
                    this.sidebar_width() + 2. * FLOATING_SIDEBAR_INSET + FLOATING_SIDEBAR_SHADOW
                })
            }) else {
                return;
            };
            // No entity/PageStore borrow survives into the child update.
            let _ = overlay.update(cx, |_, window, _| {
                window.resize(size(px(width), window.viewport_size().height));
            });
        });
    }
}
