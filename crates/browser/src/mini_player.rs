//! Arc's background-media strip: a stationary transport with rows revealed
//! above it, then a delayed collapse. CEF media and audio stay behind Pages.
use crate::{pages::PageIdentity, window::BrowserWindow};
use browser_core::TabId;
use gpui::{canvas, div, prelude::*, px, AnyElement, Bounds, Context, Pixels, Render, Task};
use std::{cell::Cell, collections::HashSet, rc::Rc, time::{Duration, SystemTime}};

// Reference screenshots are Display P3; authored GPUI colors below use sRGB.
const ROW: f32 = 33.;
#[derive(Default)]
pub(crate) struct MiniPlayer {
    pub tab: Option<TabId>,
    hovered: bool,
    volume_open: bool,
    header: f32,
    volume: f32,
    generation: u64,
    animation: Option<Task<()>>,
    collapse: Option<Task<()>>,
    pointer: Option<Task<()>>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    playing: HashSet<TabId>,
    dismissed: HashSet<TabId>,
    slider: Rc<Cell<Option<Bounds<Pixels>>>>,
    drag: Option<(TabId, PageIdentity, Bounds<Pixels>)>,
}
impl MiniPlayer {
    pub fn record(&mut self, tab: TabId, state: &engine::MediaState) {
        if state.playing {
            if self.playing.insert(tab) { self.tab=Some(tab); self.dismissed.remove(&tab); }
        } else { self.playing.remove(&tab); }
        if !state.active {
            self.dismissed.remove(&tab);
            if self.tab==Some(tab) {
                self.tab=None; self.hovered=false; self.volume_open=false;
                self.header=0.; self.volume=0.; self.drag=None;
                self.animation=None; self.collapse=None;
            }
        }
    }
}
struct MediaTooltip(&'static str);
impl Render for MediaTooltip {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div().pl(px(5.)).pt(px(5.)).child(div().id("media-tooltip").debug_selector(||"media-tooltip".into())
            .rounded(px(14.)).rounded_tl(px(2.)).border_1().border_color(gpui::rgb(0x1f224d))
            .px(px(9.)).py(px(6.)).bg(gpui::rgb(0x090d4e)).font_family(".SystemUIFont")
            .text_color(gpui::rgb(0xd0d2f5)).text_size(px(12.)).line_height(px(14.))
            .font_weight(gpui::FontWeight::BOLD).child(self.0))
    }
}
impl BrowserWindow {
    pub(crate) fn mini_player_source(&self) -> Option<(TabId,engine::MediaState)> {
        let pages=self.pages.as_ref()?.borrow();
        let eligible=|id:TabId| {
            if self.mini_player.dismissed.contains(&id) || !self.browser.all_tabs().any(|tab|tab.id==id) { return None; }
            let state=pages.owned_media(id)?;
            (state.active && !state.video && !pages.is_visible(id)).then_some((id,state))
        };
        self.mini_player.tab.and_then(eligible).or_else(||self.browser.all_tabs().find_map(|tab|eligible(tab.id)))
    }
    fn animate_mini_player(&mut self, header: f32, volume: f32, cx: &mut Context<Self>) {
        let from=(self.mini_player.header,self.mini_player.volume);
        self.mini_player.animation=None;
        self.mini_player.generation=self.mini_player.generation.wrapping_add(1);
        let generation=self.mini_player.generation;
        if crate::native::prefers_reduced_motion() || from==(header,volume) {
            self.mini_player.header=header;self.mini_player.volume=volume;cx.notify();return;
        }
        // Window-scoped Arc capture: about 200 ms, fast initial travel and
        // a short settling tail. Each revealed row measures 33 points.
        let started=cx.background_executor().now();
        self.mini_player.animation=Some(cx.spawn(async move |this,cx|loop{
            cx.background_executor().timer(Duration::from_millis(8)).await;
            let t=(cx.background_executor().now().duration_since(started).as_secs_f32()/0.2).min(1.);
            let eased=1.-(1.-t).powi(3);
            let keep=this.update(cx,|this,cx|{
                if this.mini_player.generation!=generation{return false;}
                this.mini_player.header=from.0+(header-from.0)*eased;
                this.mini_player.volume=from.1+(volume-from.1)*eased;
                cx.notify();t<1.
            }).unwrap_or(false);
            if !keep{break;}
        }));
    }
    fn hover_mini_player(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.mini_player.hovered==hovered{return;}
        self.mini_player.hovered=hovered;
        self.mini_player.collapse=None;
        if hovered {
            if self.mini_player.pointer.is_none() {
                if let Some(parent)=self.native_view {
                    self.mini_player.pointer=Some(cx.spawn(async move |this,cx|loop {
                        cx.background_executor().timer(Duration::from_millis(32)).await;
                        let keep=this.update(cx,|this,cx| {
                            if this.mini_player_source().is_none(){this.mini_player.hovered=false;this.mini_player.pointer=None;return false;}
                            if let (Some([x,y]),Some(bounds))=(crate::native::pointer_position(parent),this.mini_player.bounds.get()) {
                                this.hover_mini_player(bounds.contains(&gpui::point(px(x),px(y))),cx);
                            }
                            let keep=this.mini_player.hovered||this.mini_player.header>0.||this.mini_player.drag.is_some();
                            if !keep {this.mini_player.pointer=None;}
                            keep
                        }).unwrap_or(false);
                        if !keep {break;}
                    }));
                }
            }
            self.animate_mini_player(1.,if self.mini_player.volume_open{1.}else{0.},cx);
        }else{
            self.queue_mini_collapse(cx);
        }
    }
    fn queue_mini_collapse(&mut self, cx: &mut Context<Self>) {
        // A drag can outlive the initial hover-exit delay. Restart it when
        // capture ends so the expanded card cannot remain stuck open.
        if !self.mini_player.hovered {
            self.mini_player.collapse=Some(cx.spawn(async move |this,cx|{
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let _=this.update(cx,|this,cx|{
                    if !this.mini_player.hovered && this.mini_player.drag.is_none(){
                        this.mini_player.volume_open=false;
                        this.animate_mini_player(0.,0.,cx);
                    }
                });
            }));
        }
    }
    pub(crate) fn return_to_media_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        if !self.pages.as_ref().is_some_and(|pages|pages.borrow().media_identity(id).is_some()) { return; }
        let profile=self.browser.user_profile_id_for_tab(id);
        let space=self.browser.spaces.iter().position(|space|space.pinned.iter().chain(&space.today).any(|tab|tab.id==id))
            .or_else(||(profile!=Some(self.browser.active_user_profile_id())).then(||self.browser.spaces.iter().position(|space|Some(space.user_profile_id)==profile)).flatten());
        if let Some(index)=space {self.edit_browser(|browser|browser.switch_space(index));}
        if profile!=Some(self.browser.active_user_profile_id()){return;}
        self.edit_browser(|browser|browser.activate_tab(id,SystemTime::now()));
        self.select_sidebar_page(crate::sidebar_pages::SidebarPage::Space(self.browser.space().id),cx);
        self.settle_pager(cx);
        if let Some(pages)=&self.pages {pages.borrow_mut().request_page_focus();}
        self.mini_player.hovered=false;self.mini_player.volume_open=false;
        self.mini_player.header=0.;self.mini_player.volume=0.;self.mini_player.drag=None;
        self.mini_player.animation=None;self.mini_player.collapse=None;
        cx.notify();
    }
    fn move_mini_volume(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some((tab,identity,bounds))=self.mini_player.drag else{return;};
        let Some(pages)=&self.pages else{return;};
        if pages.borrow().media_identity(tab)!=Some(identity){self.mini_player.drag=None;return;}
        let width=(f32::from(bounds.size.width)-20.).max(1.);
        let volume=((x-f32::from(bounds.left())-10.)/width).clamp(0.,1.);
        pages.borrow_mut().set_media_volume(tab,volume);
        cx.notify();
    }
    fn render_mini_tracking(&self, cx: &mut Context<Self>) -> AnyElement {
        let owner=cx.entity().downgrade();
        canvas(|_,_,_|(),move|_,_,window,_|{
            let moving=owner.clone();
            window.on_mouse_event(move|event:&gpui::MouseMoveEvent,phase,_,cx|{
                if phase!=gpui::DispatchPhase::Capture{return;}
                let _=moving.update(cx,|this,cx|{
                    if this.mini_player.drag.is_some(){
                        if event.dragging(){this.move_mini_volume(f32::from(event.position.x),cx);}
                        else{this.mini_player.drag=None;this.queue_mini_collapse(cx);}
                    }
                });
            });
            let ending=owner.clone();
            window.on_mouse_event(move|event:&gpui::MouseUpEvent,phase,_,cx|{
                if phase!=gpui::DispatchPhase::Capture||event.button!=gpui::MouseButton::Left{return;}
                let _=ending.update(cx,|this,cx|{
                    if this.mini_player.drag.is_some(){this.move_mini_volume(f32::from(event.position.x),cx);this.mini_player.drag=None;this.queue_mini_collapse(cx);}
                });
            });
        }).absolute().size(px(0.)).into_any_element()
    }
    pub(crate) fn render_mini_player(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id,state)=self.mini_player_source()?;
        let tab=self.browser.all_tabs().find(|tab|tab.id==id)?;
        let identity=self.pages.as_ref()?.borrow().media_identity(id)?;
        let volume=self.pages.as_ref()?.borrow().media_volume(id);
        let ink=gpui::Hsla::from(gpui::rgb(0xaaaab5));
        let button=|name:&'static str,label:&'static str,enabled:bool,icon:AnyElement| {
            div().flex_1().h(px(ROW)).flex().items_center().justify_center()
                // A playback transition discards the old tooltip rather than
                // leaving “Pause” visible over the newly paused control.
                .child(div().id((name,usize::from(name=="media-toggle" && state.playing))).debug_selector(move||name.into()).size(px(24.)).rounded(px(4.))
                    .flex().items_center().justify_center().text_color(ink.opacity(if enabled{1.}else{0.32}))
                    .when(enabled,|this|this.hover(|style|style.bg(gpui::hsla(0.,0.,1.,0.075)).text_color(gpui::rgb(0xafb0ba))))
                    .tooltip(move|_,cx|cx.new(|_|MediaTooltip(label)).into())
                    .on_click(cx.listener(move|this,_,_,cx|{
                        cx.stop_propagation();
                        let Some((current,current_state))=this.mini_player_source()else{return;};
                        if current!=id||!this.pages.as_ref().is_some_and(|pages|pages.borrow().media_identity(id)==Some(identity)){return;}
                        match name{
                            "media-return"=>this.return_to_media_tab(id,cx),
                            "media-volume"=>{
                                this.mini_player.volume_open=!this.mini_player.volume_open;
                                this.mini_player.collapse=None;
                                this.animate_mini_player(1.,if this.mini_player.volume_open{1.}else{0.},cx);
                            }
                            _=>{
                                let action=match name{
                                    "media-previous" if current_state.previous=>engine::MediaAction::Previous,
                                    "media-next" if current_state.next=>engine::MediaAction::Next,
                                    "media-toggle"=>engine::MediaAction::Toggle,
                                    _=>return,
                                };
                                if let Some(pages)=&this.pages{pages.borrow_mut().media_action(id,action);}
                            }
                        }
                    })).child(icon))
        };
        let bounds=self.mini_player.slider.clone();
        let paint_bounds=bounds.clone();
        let slider=div().id("media-volume-slider").debug_selector(||"media-volume-slider".into()).relative().w_full().h(px(ROW))
            .on_mouse_down(gpui::MouseButton::Left,cx.listener(move|this,event:&gpui::MouseDownEvent,_,cx|{
                cx.stop_propagation();
                if !this.pages.as_ref().is_some_and(|pages|pages.borrow().media_identity(id)==Some(identity)){return;}
                if let Some(bounds)=bounds.get(){this.mini_player.drag=Some((id,identity,bounds));this.mini_player.collapse=None;this.move_mini_volume(f32::from(event.position.x),cx);}
            }))
            .child(canvas(move|bounds,_,_|{paint_bounds.set(Some(bounds));},move|bounds,_,window,_|{
                let x=bounds.left()+px(10.)+px((f32::from(bounds.size.width)-20.).max(0.)*volume);
                let y=px(f32::from(bounds.center().y).round());
                window.paint_quad(gpui::fill(Bounds::new(gpui::point(bounds.left(),y-px(3.)),gpui::size(bounds.size.width,px(6.))),gpui::rgb(0x20203e)).corner_radii(px(3.)));
                window.paint_quad(gpui::fill(Bounds::new(gpui::point(bounds.left(),y-px(3.)),gpui::size(x-bounds.left(),px(6.))),gpui::rgb(0xedeeff)).corner_radii(px(3.)));
                window.paint_quad(gpui::fill(Bounds::new(gpui::point(x-px(10.),y-px(8.)),gpui::size(px(20.),px(16.))),gpui::rgb(0xdadade)).corner_radii(px(8.)));
            }).size_full());
        let profile=self.browser.user_profile_id_for_tab(id).unwrap_or(self.browser.active_user_profile_id());
        let card_bounds=self.mini_player.bounds.clone();
        let card=div().id("mini-player").debug_selector(||"mini-player".into()).absolute().bottom_0().left_0().right_0()
            .h(px(ROW*(1.+self.mini_player.header+self.mini_player.volume))).rounded(px(8.))
            .bg(gpui::rgb(0x050627)).flex().flex_col().overflow_hidden()
            .child(canvas(move|bounds,_,_|{card_bounds.set(Some(bounds));},|_,_,_,_|{}).absolute().size_full())
            .on_hover(cx.listener(|this,hovered:&bool,_,cx|this.hover_mini_player(*hovered,cx)))
            .child(div().relative().h(px(ROW*self.mini_player.header)).flex_none().overflow_hidden()
                .child(div().absolute().bottom_0().left_0().right_0().h(px(ROW)).flex().items_center().pl(px(14.)).pr(px(10.)).gap(px(8.))
                    .child(div().id("media-title").debug_selector(||"media-title".into()).flex_1().min_w_0().truncate().text_size(px(13.)).font_weight(gpui::FontWeight::MEDIUM).text_color(gpui::rgb(0xcdcdd4)).child(crate::sidebar_title::SidebarTitle::new(tab.title.clone())))
                    .child(div().id("media-dismiss").debug_selector(||"media-dismiss".into()).size(px(24.)).flex().items_center().justify_center().rounded(px(4.)).text_color(ink)
                        .hover(|style|style.bg(gpui::hsla(0.,0.,1.,0.075)).text_color(gpui::rgb(0xafb0ba)))
                        .on_click(cx.listener(move|this,_,_,cx|{cx.stop_propagation();if !this.pages.as_ref().is_some_and(|pages|pages.borrow().media_identity(id)==Some(identity)){return;}
                            this.mini_player.dismissed.insert(id);this.mini_player.hovered=false;this.mini_player.drag=None;
                            this.mini_player.header=0.;this.mini_player.volume=0.;this.mini_player.volume_open=false;
                            this.mini_player.animation=None;this.mini_player.collapse=None;cx.notify();}))
                        .child(crate::icons::control_sized(crate::icons::Icon::Close,22.)))))
            .child(div().flex().h(px(ROW)).flex_none()
                .child(button("media-return","Back to Tab",true,self.render_media_favicon(tab,profile)))
                .child(button("media-previous","Previous Track",state.previous,crate::icons::media_transport("previous")))
                .child(button("media-toggle",if state.playing{"Pause"}else{"Play"},true,crate::icons::media_transport(if state.playing{"pause"}else{"play"})))
                .child(button("media-next","Next Track",state.next,crate::icons::media_transport("next")))
                .child(button("media-volume","Adjust Tab Volume",true,crate::icons::media_speaker(volume,tab.muted))))
            .child(div().h(px(ROW*self.mini_player.volume)).flex_none().overflow_hidden().px(px(8.)).child(slider));
        Some(div().relative().h(px(ROW)).flex_none().mx(px(8.)).mb(px(-8.)).child(card).child(self.render_mini_tracking(cx)).into_any_element())
    }
}
