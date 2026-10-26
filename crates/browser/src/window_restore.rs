//! Normal-window checkpoints use the profile's atomic session writer.
use crate::window::BrowserWindow;
use browser_core::window_restore::{RestoredWindow, WindowFrame, WindowMode};
use gpui::{point, px, size, App, Bounds, Window, WindowBounds};

pub(crate) fn placement(
    saved: &RestoredWindow,
    cx: &App,
) -> (WindowBounds, Option<gpui::DisplayId>) {
    let displays = cx.displays();
    let display = displays
        .iter()
        .find(|d| d.uuid().ok().map(|id| id.to_string()) == saved.display)
        .or_else(|| {
            displays.iter().find(|d| {
                let b = d.bounds();
                let f = saved.frame;
                f.x + f.width > f32::from(b.left())
                    && f.x < f32::from(b.right())
                    && f.y + f.height > f32::from(b.top())
                    && f.y < f32::from(b.bottom())
            })
        })
        .or_else(|| displays.first());
    let f = saved.frame;
    let mut bounds = Bounds::new(point(px(f.x), px(f.y)), size(px(f.width), px(f.height)));
    if let Some(display) = display {
        let screen = display.bounds();
        bounds.size.width = bounds.size.width.min(screen.size.width);
        bounds.size.height = bounds.size.height.min(screen.size.height);
        bounds.origin.x = bounds
            .origin
            .x
            .max(screen.left())
            .min(screen.right() - bounds.size.width);
        bounds.origin.y = bounds
            .origin
            .y
            .max(screen.top())
            .min(screen.bottom() - bounds.size.height);
    }
    (
        match saved.mode {
            WindowMode::Windowed => WindowBounds::Windowed(bounds),
            WindowMode::Maximized => WindowBounds::Maximized(bounds),
            WindowMode::Fullscreen => WindowBounds::Fullscreen(bounds),
        },
        display.map(|d| d.id()),
    )
}

impl BrowserWindow {
    pub(crate) fn attach_restoration(
        &mut self,
        saved: Option<RestoredWindow>,
        dormant: bool,
        window: &Window,
        cx: &App,
    ) {
        let id = saved.as_ref().map(|w| w.id).unwrap_or_else(|| {
            self.profile.handle.read_settings(|b| {
                b.windows
                    .iter()
                    .map(|w| w.id)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1)
            })
        });
        if let Some(saved) = &saved {
            self.sidebar_width = saved.sidebar_width;
            self.sidebar_visible = saved.sidebar_visible;
        }
        self.restore_dormant = dormant;
        self.restore_window = Some(saved.unwrap_or(RestoredWindow {
            id,
            selection: self.profile.selection(),
            sidebar_width: self.sidebar_width,
            sidebar_visible: self.sidebar_visible,
            frame: WindowFrame {
                x: 0.,
                y: 0.,
                width: 1200.,
                height: 800.,
            },
            display: None,
            mode: WindowMode::Windowed,
        }));
        self.checkpoint_bounds(window, cx);
    }

    pub(crate) fn checkpoint_bounds(&mut self, window: &Window, cx: &App) {
        if self.restore_window.is_none() {
            return;
        }
        self.checkpoint_platform_bounds(
            window.window_bounds(),
            window
                .display(cx)
                .and_then(|display| display.uuid().ok())
                .map(|id| id.to_string()),
        );
    }

    /// The native adapter supplies real bounds; the headless platform only
    /// models its fullscreen flag, so tests inject this same platform receipt.
    pub(crate) fn checkpoint_platform_bounds(
        &mut self,
        bounds: WindowBounds,
        display: Option<String>,
    ) {
        // Document fullscreen is temporary page presentation, not window intent.
        // Keep its original mode/frame even when the user quits before exiting it.
        if self.page_fullscreen.is_some() {
            self.checkpoint_window(false);
            return;
        }
        if let Some(saved) = &mut self.restore_window {
            let b = bounds.get_bounds();
            saved.frame = WindowFrame {
                x: b.origin.x.into(),
                y: b.origin.y.into(),
                width: b.size.width.into(),
                height: b.size.height.into(),
            };
            saved.mode = match bounds {
                WindowBounds::Windowed(_) => WindowMode::Windowed,
                WindowBounds::Maximized(_) => WindowMode::Maximized,
                WindowBounds::Fullscreen(_) => WindowMode::Fullscreen,
            };
            saved.display = display;
        }
        self.checkpoint_window(false);
    }

    pub(crate) fn checkpoint_window(&mut self, focused: bool) {
        let Some(saved) = &mut self.restore_window else {
            return;
        };
        saved.selection = self.profile.selection();
        saved.sidebar_width = self.sidebar_width;
        saved.sidebar_visible = self.sidebar_visible;
        self.profile
            .handle
            .checkpoint_window(saved.clone(), focused);
    }
    pub(crate) fn forget_restoration(&mut self) {
        if let Some(saved) = self.restore_window.take() {
            self.profile.handle.forget_window(saved.id);
        }
    }
}
