//! The Space header stays fixed while Pinned and Today scroll together.
use super::*;

pub(super) struct DragScroll {
    space: SpaceId,
    item: DragIdentity,
    pointer: Rc<Cell<gpui::Point<gpui::Pixels>>>,
    running: Rc<Cell<bool>>,
    _task: Task<()>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DragIdentity {
    Tab(TabId),
    Folder(browser_core::folders::FolderId),
}

#[derive(Clone)]
enum ScrollPayload {
    Tab(crate::tab_drag::TabDrag),
    Folder(crate::folder_drag::FolderDrag),
}

impl ScrollPayload {
    fn identity(&self) -> DragIdentity {
        match self {
            Self::Tab(drag) => DragIdentity::Tab(drag.id),
            Self::Folder(drag) => DragIdentity::Folder(drag.id),
        }
    }

    fn accepted_by(&self, window: &BrowserWindow, space: SpaceId) -> bool {
        match self {
            Self::Tab(drag) => window.accepts_tab_drag(drag),
            Self::Folder(drag) => window.accepts_folder_drag(drag, space),
        }
    }
}

// Native reference: a held row starts scrolling within approximately 20 px
// of either edge; 11 px inside the bottom yields about 60 logical px/s.
// The acceleration at the very edge remains subject to native comparison.
fn velocity(bounds: Bounds<gpui::Pixels>, pointer: gpui::Point<gpui::Pixels>) -> f32 {
    if !bounds.contains(&pointer) {
        return 0.;
    }
    let top = f32::from(pointer.y - bounds.top());
    let bottom = f32::from(bounds.bottom() - pointer.y);
    let speed = |distance: f32| (60. / 121.) * (22. - distance).max(0.).powi(2);
    if top < 22. {
        speed(top)
    } else if bottom < 22. {
        -speed(bottom)
    } else {
        0.
    }
}

impl BrowserWindow {
    pub(crate) fn cancel_sidebar_drag_scroll(&mut self) {
        self.sidebar_drag_scroll = None;
    }

    pub(super) fn update_sidebar_drag_scroll(
        &mut self,
        drag: &crate::tab_drag::TabDrag,
        space: SpaceId,
        pointer: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.update_sidebar_payload_scroll(ScrollPayload::Tab(drag.clone()), space, pointer, cx);
    }

    pub(super) fn update_sidebar_folder_scroll(
        &mut self,
        drag: &crate::folder_drag::FolderDrag,
        space: SpaceId,
        pointer: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.update_sidebar_payload_scroll(ScrollPayload::Folder(drag.clone()), space, pointer, cx);
    }

    fn update_sidebar_payload_scroll(
        &mut self,
        drag: ScrollPayload,
        space: SpaceId,
        pointer: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(scrolls) = self.sidebar_scrolls.get(&space) else {
            return;
        };
        #[cfg(debug_assertions)]
        if std::env::var_os("ECHO_TRACE_SIDEBAR_DRAG").is_some() {
            eprintln!(
                "[sidebar-drag] pointer={pointer:?} bounds={:?} velocity={} offset={:?}",
                scrolls.list.bounds(),
                velocity(scrolls.list.bounds(), pointer),
                scrolls.list.offset()
            );
        }
        if self.browser.space().id != space
            || !drag.accepted_by(self, space)
            || velocity(scrolls.list.bounds(), pointer) == 0.
        {
            self.sidebar_drag_scroll = None;
            return;
        }
        if let Some(active) = self.sidebar_drag_scroll.as_ref().filter(|active| {
            active.space == space && active.item == drag.identity() && active.running.get()
        }) {
            active.pointer.set(pointer);
            return;
        }
        let scroll = scrolls.list.clone();
        let pointer = Rc::new(Cell::new(pointer));
        let held_pointer = pointer.clone();
        let drag = drag.clone();
        let item = drag.identity();
        let running = Rc::new(Cell::new(true));
        let task_running = running.clone();
        let task = cx.spawn(async move |this, cx| {
            // The reference waits about 200 ms before sustained edge scrolling.
            cx.background_executor()
                .timer(Duration::from_millis(200))
                .await;
            let mut previous = cx.background_executor().now();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let now = cx.background_executor().now();
                let elapsed = now
                    .saturating_duration_since(previous)
                    .as_secs_f32()
                    .min(0.05);
                previous = now;
                let keep = this
                    .update(cx, |this, cx| {
                        if !cx.has_active_drag()
                            || this.browser.space().id != space
                            || !drag.accepted_by(this, space)
                        {
                            return false;
                        }
                        let speed = velocity(scroll.bounds(), held_pointer.get());
                        if speed == 0. {
                            return false;
                        }
                        let mut offset = scroll.offset();
                        let old = offset.y;
                        offset.y = (offset.y + px(speed * elapsed))
                            .clamp(-scroll.max_offset().height, px(0.));
                        if old != offset.y {
                            scroll.set_offset(offset);
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
            task_running.set(false);
        });
        self.sidebar_drag_scroll = Some(DragScroll {
            space,
            item,
            pointer,
            running,
            _task: task,
        });
    }
}
