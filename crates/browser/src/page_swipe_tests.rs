use super::*;

#[gpui::test]
fn page_swipe_feedback_preserves_focus_and_retires_with_the_document(cx:&mut TestAppContext){
    let(view,cx,fake)=open_pages(cx);
    let before=active_url(&view,cx);fake.take_calls();
    fake.send(cx,Event::PageSwipe{view:page(1),state:Some(engine::PageSwipe{back:true,progress:0.8})});
    let overlay=view.read_with(cx,|w,_|w.page_swipe.as_ref().unwrap().overlay.unwrap());
    let mut visual=VisualTestContext::from_window(overlay.into(),&cx.cx);
    visual.run_until_parked();
    assert_eq!(visual.debug_bounds("page-swipe-arrow").unwrap().size,gpui::size(gpui::px(32.),gpui::px(32.)));
    assert_eq!(active_url(&view,cx),before);
    assert!(!fake.take_calls().iter().any(|call|matches!(call,Call::FocusPage(_)|Call::Navigate(..))));
    fake.send(cx,Event::PageSwipe{view:page(1),state:None});
    cx.executor().advance_clock(Duration::from_millis(500));cx.run_until_parked();
    assert!(view.read_with(cx,|w,_|w.page_swipe.is_none()));
    fake.send(cx,Event::PageSwipe{view:page(1),state:Some(engine::PageSwipe{back:true,progress:0.8})});
    click(cx,"tab-7");cx.run_until_parked();
    assert!(view.read_with(cx,|w,_|w.page_swipe.is_none()));
    fake.send(cx,Event::PageSwipe{view:page(1),state:Some(engine::PageSwipe{back:true,progress:1.})});
    assert!(view.read_with(cx,|w,_|w.page_swipe.is_none()));
}
