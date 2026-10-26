//! Functional populated presentation assembled from existing Library metrics.
//! Exact populated Arc layout is unavailable; ARC_PARITY intentionally stays partial.
use super::*;
use browser_core::{
    media::{
        MediaAccess, MediaFolder, MediaInventory, MediaItem, MediaKind, MediaSource, MediaStamp,
    },
    UserProfileId,
};
#[cfg(not(test))]
use echo_media::file_actions::{SystemTrash, TrashPlatform};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MediaAction {
    Open,
    Reveal,
    Copy,
    Trash,
    Drag,
}
#[derive(Clone)]
pub(super) enum Row {
    Header(String),
    Tiles(Vec<MediaItem>),
}
pub(super) struct Scope {
    profile: UserProfileId,
    handle: crate::profile::ProfileHandle,
    access: MediaAccess,
    capture_key: String,
}
pub(super) struct Runtime {
    pub rows: Vec<Row>,
    model: MediaInventory,
    list: gpui::ListState,
    query: String,
    scope: Option<Scope>,
    serial: u64,
    service: Option<echo_media::Discovery>,
    seen: u64,
    thumbs: Option<echo_media::thumbnails::Thumbnails>,
    images: HashMap<(MediaStamp, u32), Arc<gpui::RenderImage>>,
    image_order: VecDeque<(MediaStamp, u32)>,
    awaiting: bool,
    _poll: Option<gpui::Task<()>>,
    _profile: Option<Subscription>,
    menu: Option<(MediaStamp, gpui::Point<gpui::Pixels>)>,
    preview_left: f32,
    undo: Option<(UserProfileId, Arc<dyn UndoFile>, MediaItem)>,
    trash: Arc<dyn MediaTrash>,
    busy: bool,
    retired: bool,
    #[cfg(test)]
    pub effects: Vec<(MediaAction, MediaStamp)>,
}
trait UndoFile: Send + Sync {
    fn restore(&self) -> Result<(), String>;
}
#[cfg(not(test))]
struct NativeUndo(echo_media::file_actions::TrashReceipt);
#[cfg(not(test))]
impl UndoFile for NativeUndo {
    fn restore(&self) -> Result<(), String> {
        SystemTrash.restore(&self.0).map_err(|e| e.to_string())
    }
}
trait MediaTrash: Send + Sync {
    fn trash(&self, stamp: &MediaStamp) -> Result<Arc<dyn UndoFile>, String>;
}
#[cfg(not(test))]
struct NativeTrash;
#[cfg(not(test))]
impl MediaTrash for NativeTrash {
    fn trash(&self, stamp: &MediaStamp) -> Result<Arc<dyn UndoFile>, String> {
        SystemTrash
            .move_to_trash(stamp)
            .map(|r| Arc::new(NativeUndo(r)) as Arc<dyn UndoFile>)
            .map_err(|e| e.to_string())
    }
}
#[cfg(test)]
struct FixtureTrash(tempfile::TempDir);
#[cfg(test)]
struct FixtureUndo {
    original: PathBuf,
    trashed: MediaStamp,
}
#[cfg(test)]
impl UndoFile for FixtureUndo {
    fn restore(&self) -> Result<(), String> {
        echo_media::validate(&self.trashed).map_err(|e| e.to_string())?;
        std::fs::hard_link(&self.trashed.path, &self.original)
            .and_then(|_| std::fs::remove_file(&self.trashed.path))
            .map_err(|e| e.to_string())
    }
}
#[cfg(test)]
impl MediaTrash for FixtureTrash {
    fn trash(&self, stamp: &MediaStamp) -> Result<Arc<dyn UndoFile>, String> {
        echo_media::validate(stamp).map_err(|e| e.to_string())?;
        let path = self
            .0
            .path()
            .join(stamp.path.file_name().ok_or("Missing filename")?);
        std::fs::hard_link(&stamp.path, &path)
            .and_then(|_| std::fs::remove_file(&stamp.path))
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(FixtureUndo {
            original: stamp.path.clone(),
            trashed: echo_media::stamp(&path).map_err(|e| e.to_string())?,
        }))
    }
}
impl Runtime {
    pub fn new() -> Self {
        Self {
            rows: vec![],
            model: Default::default(),
            list: gpui::ListState::new(0, gpui::ListAlignment::Top, px(180.)),
            query: String::new(),
            scope: None,
            serial: 0,
            service: None,
            seen: 0,
            thumbs: None,
            images: HashMap::new(),
            image_order: VecDeque::new(),
            awaiting: false,
            _poll: None,
            _profile: None,
            menu: None,
            preview_left: 305.,
            undo: None,
            #[cfg(test)]
            trash: Arc::new(FixtureTrash(tempfile::tempdir().unwrap())),
            #[cfg(not(test))]
            trash: Arc::new(NativeTrash),
            busy: false,
            retired: false,
            #[cfg(test)]
            effects: vec![],
        }
    }
}
impl MediaView {
    /// Retained windows outlive CEF shutdown; retire read-only producers first.
    pub(crate) fn retire_media_for_quit(&mut self) {
        if self.runtime.retired { return; }
        self.runtime.retired = true;
        let discovery = self.runtime.service.is_some();
        let thumbnails = self.runtime.thumbs.is_some();
        let ready_images = self.runtime.images.len();
        self.runtime._poll = None;
        self.runtime._profile = None;
        self.invalidate_media();
        eprintln!("[media-shutdown] producers-cancelled discovery={discovery} thumbnails={thumbnails} ready_images={ready_images}");
    }
    pub(crate) fn configure_media(
        &mut self,
        profile: UserProfileId,
        handle: crate::profile::ProfileHandle,
        access: MediaAccess,
        captures: browser_core::captures::Captures,
        directory: Option<PathBuf>,
        left: f32,
        cx: &mut Context<Self>,
    ) {
        if self.runtime.retired { return; }
        self.runtime.preview_left = left;
        let key = serde_json::to_string(&captures).unwrap_or_default();
        if self.runtime.scope.as_ref().is_some_and(|s| {
            s.profile == profile
                && s.handle.same_workspace(&handle)
                && s.access == access
                && s.capture_key == key
        }) {
            return;
        }
        let had_selection = self.has_media_selection();
        self.invalidate_media();
        self.runtime._profile = Some(handle.observe_settings(cx));
        self.runtime.scope = Some(Scope {
            profile,
            handle,
            access: access.clone(),
            capture_key: key,
        });
        let approved = echo_media::ApprovedFolders::from_system_selection(access.paths());
        let owned = directory.map(|d| (d, profile, captures));
        if approved.is_some() || owned.is_some() {
            self.runtime.service = Some(echo_media::Discovery::start_sources(approved, owned));
        }
        self.runtime.thumbs = Some(Default::default());
        if self.runtime._poll.is_none() {
            self.runtime._poll = Some(cx.spawn(async move |owner, cx| loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                if owner.update(cx, |this, cx| this.poll_media(cx)).is_err() {
                    break;
                }
            }));
        }
        if had_selection {
            cx.emit(MediaIntent::SelectionChanged(false));
        }
        cx.notify();
    }
    fn invalidate_media(&mut self) {
        self.runtime.serial = self.runtime.serial.wrapping_add(1);
        self.runtime.service = None;
        self.runtime.scope = None;
        self.runtime.seen = 0;
        self.runtime.model.replace(vec![]);
        self.runtime.rows.clear();
        self.runtime.list.reset(0);
        self.runtime.images.clear();
        self.runtime.image_order.clear();
        self.runtime.thumbs = None;
        self.runtime.menu = None;
    }
    pub(super) fn check_media_access(&mut self, cx: &mut Context<Self>) {
        let revoked = self
            .runtime
            .scope
            .as_ref()
            .is_some_and(|s| s.handle.media_access(s.profile).as_ref() != Some(&s.access));
        if revoked {
            let selected = self.has_media_selection();
            self.invalidate_media();
            if selected {
                cx.emit(MediaIntent::SelectionChanged(false));
            }
            cx.notify();
        }
    }
    fn poll_media(&mut self, cx: &mut Context<Self>) {
        self.check_media_access(cx);
        let snapshot = self
            .runtime
            .service
            .as_ref()
            .and_then(|s| s.snapshot_since(self.runtime.seen));
        if let Some((generation, snapshot)) = snapshot {
            self.runtime.seen = generation;
            self.replace_media_items(snapshot.items, cx);
        }
        if self.runtime.awaiting {
            self.runtime.awaiting = false;
            cx.notify();
        }
    }
    pub(crate) fn has_media_selection(&self) -> bool {
        self.runtime.model.selected().is_some()
    }
    pub(crate) fn replace_media_items(&mut self, items: Vec<MediaItem>, cx: &mut Context<Self>) {
        let selected = self.has_media_selection();
        if !self.runtime.model.replace(items) {
            return;
        }
        self.runtime.rows.clear();
        self.runtime.query.clear();
        if selected != self.has_media_selection() {
            cx.emit(MediaIntent::SelectionChanged(self.has_media_selection()));
        }
        let live: std::collections::HashSet<_> = self
            .runtime
            .model
            .items()
            .iter()
            .map(|i| i.stamp.clone())
            .collect();
        self.runtime
            .images
            .retain(|(stamp, _), _| live.contains(stamp));
        self.runtime
            .image_order
            .retain(|(stamp, _)| live.contains(stamp));
        if self
            .runtime
            .menu
            .as_ref()
            .is_some_and(|(stamp, _)| !live.contains(stamp))
        {
            self.runtime.menu = None;
        }
        cx.notify();
    }
    pub(super) fn media_query_changed(&mut self, query: &str, cx: &mut Context<Self>) {
        self.runtime.menu = None;
        if self
            .runtime
            .model
            .selected()
            .is_some_and(|i| !i.matches(query))
        {
            self.runtime.model.clear_selection();
            cx.emit(MediaIntent::SelectionChanged(false));
        }
        self.runtime.rows.clear();
        self.runtime.query.clear();
    }
    pub(super) fn rebuild_media_rows(&mut self, query: &str) {
        if query == self.runtime.query && !self.runtime.rows.is_empty() {
            return;
        }
        self.runtime.query = query.to_owned();
        let mut sections: Vec<(String, Vec<MediaItem>)> = vec![("Captures".into(), vec![])];
        if let Some(scope) = &self.runtime.scope {
            for folder in [
                MediaFolder::Downloads,
                MediaFolder::Desktop,
                MediaFolder::Documents,
            ] {
                if let browser_core::media::FolderGrant::Granted(path) = scope.access.grant(folder)
                {
                    sections.push((
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        vec![],
                    ));
                }
            }
        }
        if sections.len() == 1 {
            sections.push(("Media".into(), vec![]));
        }
        for item in self
            .runtime
            .model
            .items()
            .iter()
            .filter(|i| i.matches(query))
        {
            let index = if matches!(item.source, MediaSource::Capture { .. }) {
                0
            } else {
                self.runtime
                    .scope
                    .as_ref()
                    .and_then(|scope| {
                        [
                            MediaFolder::Downloads,
                            MediaFolder::Desktop,
                            MediaFolder::Documents,
                        ]
                        .into_iter()
                        .filter_map(|f| {
                            if let browser_core::media::FolderGrant::Granted(path) =
                                scope.access.grant(f)
                            {
                                Some(path)
                            } else {
                                None
                            }
                        })
                        .enumerate()
                        .find(|(_, path)| item.stamp.path.starts_with(path))
                        .map(|(n, _)| n + 1)
                    })
                    .unwrap_or(1)
            };
            if let Some((_, items)) = sections.get_mut(index) {
                items.push(item.clone());
            }
        }
        self.runtime.rows = sections
            .into_iter()
            .filter(|(_, items)| !items.is_empty())
            .flat_map(|(name, items)| {
                std::iter::once(Row::Header(name)).chain(
                    items
                        .chunks(2)
                        .map(|c| Row::Tiles(c.to_vec()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        self.runtime.list.reset(self.runtime.rows.len());
    }
    fn media_image(&mut self, stamp: &MediaStamp, edge: u32) -> Option<Arc<gpui::RenderImage>> {
        let key = (stamp.clone(), edge);
        if let Some(image) = self.runtime.images.get(&key) {
            return Some(image.clone());
        }
        let thumbs = self.runtime.thumbs.as_ref()?;
        let Some(image) = thumbs.request(stamp, edge) else {
            self.runtime.awaiting = thumbs.pending_count() > 0;
            return None;
        };
        let mut pixels = image.rgba.to_vec();
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let image = Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::from_raw(image.width, image.height, pixels)?,
        )]));
        while self.runtime.image_order.len() >= 32 {
            if let Some(old) = self.runtime.image_order.pop_front() {
                self.runtime.images.remove(&old);
            }
        }
        self.runtime.image_order.push_back(key.clone());
        self.runtime.images.insert(key, image.clone());
        Some(image)
    }
    fn select_media(&mut self, stamp: &MediaStamp, window: &mut Window, cx: &mut Context<Self>) {
        if self.runtime.model.select(stamp) {
            self.focus.focus(window);
            cx.emit(MediaIntent::SelectionChanged(true));
            cx.notify();
        }
    }
    fn clear_media_selection(&mut self, cx: &mut Context<Self>) {
        self.runtime.model.clear_selection();
        self.runtime.menu = None;
        cx.emit(MediaIntent::SelectionChanged(false));
        cx.notify();
    }
    pub(crate) fn media_action(
        &mut self,
        stamp: MediaStamp,
        action: MediaAction,
        cx: &mut Context<Self>,
    ) {
        self.runtime.menu = None;
        let Some(item) = self
            .runtime
            .model
            .items()
            .iter()
            .find(|i| i.stamp == stamp)
            .cloned()
        else {
            return;
        };
        if let Err(error) = echo_media::validate(&stamp) {
            cx.emit(MediaIntent::Error(error.to_string()));
            return;
        }
        #[cfg(test)]
        {
            self.runtime.effects.push((action, stamp.clone()));
            if action != MediaAction::Trash {
                cx.notify();
                return;
            }
        }
        match action {
            MediaAction::Open => cx.open_with_system(&stamp.path),
            MediaAction::Reveal => cx.reveal_path(&stamp.path),
            MediaAction::Copy => {
                if let Err(error) = crate::download_files::copy_file(&stamp.path, cx) {
                    cx.emit(MediaIntent::Error(error));
                }
            }
            MediaAction::Drag => {}
            MediaAction::Trash => {
                if self.runtime.busy {
                    return;
                }
                let Some(profile) = self.runtime.scope.as_ref().map(|s| s.profile) else {
                    return;
                };
                self.runtime.busy = true;
                let serial = self.runtime.serial;
                let files = self.runtime.trash.clone();
                let task = cx.background_spawn(async move { files.trash(&stamp) });
                cx.spawn(async move |owner, cx| {
                    let result = task.await;
                    let _ = owner.update(cx, |this, cx| {
                        this.runtime.busy = false;
                        match result {
                            Ok(receipt) => {
                                this.runtime.undo = Some((profile, receipt, item.clone()));
                                if this.runtime.serial == serial
                                    && this
                                        .runtime
                                        .scope
                                        .as_ref()
                                        .is_some_and(|s| s.profile == profile)
                                {
                                    let remaining = this
                                        .runtime
                                        .model
                                        .items()
                                        .iter()
                                        .filter(|i| i.stamp != item.stamp)
                                        .cloned()
                                        .collect();
                                    this.replace_media_items(remaining, cx);
                                    if let Some(service) = &this.runtime.service {
                                        service.refresh();
                                    }
                                    cx.emit(MediaIntent::Trashed(profile));
                                }
                            }
                            Err(error) => cx.emit(MediaIntent::Error(error)),
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
    }
    pub(crate) fn undo_trash(&mut self, profile: UserProfileId, cx: &mut Context<Self>) -> bool {
        if self.runtime.busy
            || !self
                .runtime
                .undo
                .as_ref()
                .is_some_and(|(owner, _, _)| *owner == profile)
        {
            return false;
        }
        let Some((profile, receipt, item)) = self.runtime.undo.take() else {
            return false;
        };
        self.runtime.busy = true;
        let serial = self.runtime.serial;
        let retry = receipt.clone();
        let path = item.stamp.path.clone();
        let task = cx.background_spawn(async move {
            receipt.restore()?;
            echo_media::stamp(&path).map_err(|e| e.to_string())
        });
        cx.spawn(async move |owner, cx| {
            let result = task.await;
            let _ = owner.update(cx, |this, cx| {
                this.runtime.busy = false;
                match result {
                    Ok(stamp) => {
                        if this.runtime.serial == serial
                            && this
                                .runtime
                                .scope
                                .as_ref()
                                .is_some_and(|s| s.profile == profile)
                        {
                            // Restoring a receipt is an explicit user action. A
                            // revoked folder must still remain absent from Media.
                            let visible =
                                this.runtime.scope.as_ref().is_some_and(|scope| {
                                    match item.source {
                                        MediaSource::Capture { profile: owner, .. } => {
                                            owner == profile
                                        }
                                        MediaSource::Folder => scope
                                            .access
                                            .paths()
                                            .iter()
                                            .any(|root| stamp.path.starts_with(root)),
                                    }
                                });
                            if visible {
                                let mut items = this.runtime.model.items().to_vec();
                                items.push(MediaItem { stamp, ..item });
                                this.replace_media_items(items, cx);
                            }
                            if let Some(service) = &this.runtime.service {
                                service.refresh();
                            }
                            cx.emit(MediaIntent::UndoCompleted);
                        }
                    }
                    Err(error) => {
                        this.runtime.undo = Some((profile, retry, item));
                        cx.emit(MediaIntent::Error(error));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        true
    }
    #[cfg(test)]
    pub(crate) fn test_media_menu_open(&self) -> bool {
        self.runtime.menu.is_some()
    }
    #[cfg(test)]
    pub(crate) fn test_media_items(&mut self, items: Vec<MediaItem>, cx: &mut Context<Self>) {
        self.runtime.service = None;
        self.replace_media_items(items, cx);
    }
    #[cfg(test)]
    pub(crate) fn test_media_effects(&self) -> Vec<(MediaAction, MediaStamp)> {
        self.runtime.effects.clone()
    }
    #[cfg(test)]
    pub(crate) fn test_media_scanning(&self) -> bool {
        self.runtime.service.is_some()
    }
    #[cfg(test)]
    pub(crate) fn test_media_retired(&self) -> bool {
        self.runtime.retired && self.runtime.service.is_none() && self.runtime.thumbs.is_none()
            && self.runtime._poll.is_none() && self.runtime._profile.is_none()
    }
    #[cfg(test)]
    pub(crate) fn test_media_names(&self) -> Vec<String> {
        self.runtime
            .rows
            .iter()
            .filter_map(|r| {
                if let Row::Tiles(items) = r {
                    Some(items)
                } else {
                    None
                }
            })
            .flatten()
            .map(|i| i.name.clone())
            .collect()
    }
    pub(super) fn render_populated(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = self.colors;
        let selected = self.runtime.model.selected().cloned();
        div().id("library-media").debug_selector(||"library-media".into()).relative().size_full().track_focus(&self.focus).text_color(colors.text)
   .on_key_down(cx.listener(|this,event:&gpui::KeyDownEvent,window,cx| {
    if !this.focus.is_focused(window) {return;}
    let key=event.keystroke.key.as_str();let cmd=event.keystroke.modifiers.platform;
    if key=="escape" {if this.runtime.menu.take().is_none() {this.clear_media_selection(cx);}cx.notify();cx.stop_propagation();return;}
    if let Some(item)=this.runtime.model.selected().cloned() {if key=="enter" || (cmd&&key=="c") {this.media_action(item.stamp,if key=="enter"{MediaAction::Open}else{MediaAction::Copy},cx);cx.stop_propagation();return;}}
    let delta=match key {"left"=>-1,"right"=>1,"up"=>-2,"down"=>2,_=>return};
    let visible:Vec<_>=this.runtime.rows.iter().filter_map(|r|if let Row::Tiles(items)=r{Some(items)}else{None}).flatten().map(|i|i.stamp.clone()).collect();if visible.is_empty(){return;}
    let index=this.runtime.model.selected().and_then(|s|visible.iter().position(|stamp|stamp==&s.stamp)).map(|i|(i as isize+delta).clamp(0,visible.len() as isize-1) as usize).unwrap_or(0);
    this.select_media(&visible[index],window,cx);if let Some(row)=this.runtime.rows.iter().position(|r|matches!(r,Row::Tiles(items) if items.iter().any(|i|i.stamp==visible[index]))) {this.runtime.list.scroll_to_reveal_item(row);}cx.stop_propagation();
   }))
   .child(div().absolute().left_0().top_0().bottom_0().w(px(305.)).child(self.render_media_search(cx))
    .child(div().absolute().left_0().right_0().top(px(56.)).bottom(px(10.)).child(gpui::list(self.runtime.list.clone(),cx.processor(|this,index,_,cx|this.render_media_row(index,cx))).size_full())))
   .when(selected.is_some(),|view|view.child(self.render_media_preview(selected.unwrap(),cx)))
   .when(self.runtime.menu.is_some(),|view|view.child(self.render_media_menu(cx))).into_any_element()
    }
    fn render_media_search(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .id("library-media-search")
            .debug_selector(|| "library-media-search".into())
            .absolute()
            .left(px(8.))
            .right(px(8.))
            .top(px(10.))
            .h(px(36.))
            .rounded(px(10.))
            .bg(self.colors.control)
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.focus_search(window, cx);
                    cx.emit(MediaIntent::SearchFocused);
                    window.prevent_default();
                }),
            )
            .child(crate::icons::control(crate::icons::Icon::Search))
            .child(div().flex_1().min_w_0().child(self.input.clone()))
            .into_any_element()
    }
    fn render_media_row(&mut self, index: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(row) = self.runtime.rows.get(index).cloned() else {
            return div().into_any_element();
        };
        match row {
            Row::Header(name) => div()
                .h(px(32.))
                .px(px(12.))
                .flex()
                .items_center()
                .text_size(px(12.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(self.colors.muted)
                .child(name)
                .into_any_element(),
            Row::Tiles(items) => {
                let mut row = div().h(px(136.)).px(px(12.)).flex().gap(px(8.));
                for (column, item) in items.into_iter().enumerate() {
                    let stamp = item.stamp.clone();
                    let right = stamp.clone();
                    let selected = self
                        .runtime
                        .model
                        .selected()
                        .is_some_and(|s| s.stamp == stamp);
                    let image = self.media_image(&stamp, 160);
                    let placeholder = image.is_none();
                    let color = self.colors;
                    let mut tile = div()
                        .id(("media-tile", index * 2 + column))
                        .debug_selector({
                            let name = format!("media-item-{}", item.name);
                            move || name.clone().into()
                        })
                        .w(px(136.5))
                        .h(px(128.))
                        .rounded(px(8.))
                        .p(px(4.))
                        .border_1()
                        .border_color(if selected {
                            gpui::rgb(0x6258ff).into()
                        } else {
                            gpui::transparent_black()
                        })
                        .when(selected, |v| v.bg(color.control))
                        .hover(move |v| v.bg(color.hover))
                        .cursor_pointer()
                        .on_click(
                            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                this.select_media(&stamp, window, cx);
                                if event.click_count() == 2 {
                                    this.media_action(stamp.clone(), MediaAction::Open, cx);
                                }
                            }),
                        )
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                this.select_media(&right, window, cx);
                                this.runtime.menu = Some((
                                    right.clone(),
                                    gpui::point(
                                        (event.position.x - px(112.)).max(px(4.)).min(px(550.)),
                                        event
                                            .position
                                            .y
                                            .min(window.viewport_size().height - px(180.)),
                                    ),
                                ));
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        )
                        .child(
                            div()
                                .h(px(90.))
                                .w_full()
                                .rounded(px(6.))
                                .overflow_hidden()
                                .bg(color.control)
                                .flex()
                                .items_center()
                                .justify_center()
                                .when_some(image, |v, image| {
                                    v.child(
                                        gpui::img(image)
                                            .size_full()
                                            .object_fit(gpui::ObjectFit::Contain),
                                    )
                                })
                                .when(placeholder, |v| {
                                    v.child(crate::icons::ionicon(
                                        if item.kind == MediaKind::Video {
                                            "videocam-outline"
                                        } else {
                                            "image-outline"
                                        },
                                        28.,
                                        1.,
                                    ))
                                }),
                        )
                        .child(
                            div()
                                .mt(px(5.))
                                .h(px(18.))
                                .text_size(px(12.))
                                .line_height(px(18.))
                                .truncate()
                                .child(item.name),
                        );
                    tile = crate::media_drag::attach(tile, item.stamp, cx);
                    row = row.child(tile);
                }
                row.into_any_element()
            }
        }
    }
    fn render_media_preview(
        &mut self,
        item: MediaItem,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let colors = self.colors;
        let image = self.media_image(&item.stamp, 512);
        let mut actions = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .h(px(48.))
            .flex_none();
        for (action, label) in [
            (MediaAction::Open, "Open"),
            (MediaAction::Reveal, "Show in Finder"),
            (MediaAction::Copy, "Copy"),
            (MediaAction::Trash, "Move to Trash"),
        ] {
            let stamp = item.stamp.clone();
            actions = actions.child(
                div()
                    .id(("media-preview-action", action as usize))
                    .debug_selector(move || format!("media-action-{action:?}").into())
                    .px(px(8.))
                    .h(px(28.))
                    .rounded(px(6.))
                    .bg(colors.control)
                    .hover(move |s| s.bg(colors.hover))
                    .flex()
                    .items_center()
                    .text_size(px(12.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if action == MediaAction::Trash {
                            this.focus.focus(window);
                        }
                        this.media_action(stamp.clone(), action, cx)
                    }))
                    .child(label),
            );
        }
        super::render_preview_at(
            colors.preview,
            Some(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(44.))
                            .flex_none()
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(item.name),
                            )
                            .child(
                                div()
                                    .id("media-preview-close")
                                    .debug_selector(|| "media-preview-close".into())
                                    .size(px(28.))
                                    .rounded(px(6.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(move |s| s.bg(colors.hover))
                                    .cursor_pointer()
                                    .on_click(
                                        cx.listener(|this, _, _, cx| {
                                            this.clear_media_selection(cx)
                                        }),
                                    )
                                    .child(crate::icons::control(crate::icons::Icon::Close)),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .p(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when_some(image, |v, image| {
                                v.child(
                                    gpui::img(image)
                                        .size_full()
                                        .object_fit(gpui::ObjectFit::Contain),
                                )
                            })
                            .child(
                                div()
                                    .absolute()
                                    .bottom(px(52.))
                                    .text_size(px(12.))
                                    .text_color(colors.muted)
                                    .when(item.kind == MediaKind::Video, |v| {
                                        v.child("Video · Open to play")
                                    }),
                            ),
                    )
                    .child(actions)
                    .into_any_element(),
            ),
            self.runtime.preview_left,
        )
    }
    fn render_media_menu(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some((stamp, position)) = self.runtime.menu.clone() else {
            return div().into_any_element();
        };
        let colors = self.colors;
        div()
            .id("media-context-menu")
            .debug_selector(|| "media-context-menu".into())
            .absolute()
            .left(position.x)
            .top(position.y)
            .w(px(220.))
            .p(px(4.))
            .rounded(px(8.))
            // Use the existing context-menu backing, not the translucent
            // empty-document tint: file thumbnails must not show through text.
            .bg(if colors.text.l > 0.5 {
                crate::panel::menu_fill(colors.preview.h, false)
            } else {
                gpui::rgb(0xfbf8ff).into()
            })
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.runtime.menu = None;
                cx.notify();
            }))
            .border_1()
            .border_color(colors.control)
            .shadow_lg()
            .children(
                [
                    (MediaAction::Open, "Open"),
                    (MediaAction::Reveal, "Show in Finder"),
                    (MediaAction::Copy, "Copy"),
                    (MediaAction::Trash, "Move to Trash"),
                ]
                .into_iter()
                .map(|(action, label)| {
                    let stamp = stamp.clone();
                    div()
                        .id(("media-menu-action", action as usize))
                        .debug_selector(move || format!("media-menu-{action:?}").into())
                        .h(px(28.))
                        .px(px(10.))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .hover(move |s| s.bg(colors.hover))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.media_action(stamp.clone(), action, cx)
                        }))
                        .child(label)
                }),
            )
            .into_any_element()
    }
}
