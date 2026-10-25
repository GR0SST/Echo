use super::*;

#[gpui::test]
fn little_archive_intervals_expire_only_the_lookup_at_their_boundaries(cx:&mut TestAppContext){
    use browser_core::settings::LittleWindowArchive::*;
    let(source,cx)=open(cx);
    source.update(cx,|w,_|w.edit_browser(|b|b.active_user_profile_mut().archive_after=None));
    let count=source.read_with(cx,|w,_|w.browser.all_tabs().count());
    for(value,hours)in[(OneHour,1),(SixHours,6),(TwelveHours,12)]{
        source.update(cx,|w,_|w.edit_browser(|b|b.settings.little_window_archive=value));
        cx.simulate_keystrokes("cmd-alt-n");cx.run_until_parked();
        let(little,mut compact)=little_root(cx);
        bar(&little,&mut compact).simulate_input("https://timer.example");bar(&little,&mut compact).simulate_keystrokes("enter");compact.run_until_parked();
        little.update(&mut compact,|w,_|w.transient.as_mut().unwrap().active=false);
        cx.executor().advance_clock(Duration::from_secs(hours*3600-1));cx.run_until_parked();
        assert!(!little.update(&mut compact,|w,cx|w.expire_little_if_due(cx)));
        cx.executor().advance_clock(Duration::from_secs(1));
        assert!(little.update(&mut compact,|w,cx|w.expire_little_if_due(cx)));
        cx.run_until_parked();
        assert_eq!(source.read_with(cx,|w,_|w.browser.all_tabs().count()),count);
    }
}
#[gpui::test]
fn little_archive_timer_observes_live_settings_and_protects_a_foreground_lookup(cx:&mut TestAppContext){
    use browser_core::settings::LittleWindowArchive::*;
    let(source,cx)=open(cx);
    source.update(cx,|w,_|w.edit_browser(|b|{b.active_user_profile_mut().archive_after=None;b.settings.little_window_archive=OneHour;}));
    cx.simulate_keystrokes("cmd-alt-n");cx.run_until_parked();
    let(little,mut compact)=little_root(cx);
    bar(&little,&mut compact).simulate_input("https://timer.example");bar(&little,&mut compact).simulate_keystrokes("enter");compact.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2*3600));cx.run_until_parked();
    assert!(!little.update(&mut compact,|w,cx|w.expire_little_if_due(cx)));
    source.update(cx,|w,_|w.edit_browser(|b|b.settings.little_window_archive=SixHours));
    little.update(&mut compact,|w,cx|w.set_little_archive_active(false,cx));
    cx.executor().advance_clock(Duration::from_secs(2*3600));cx.run_until_parked();
    assert!(!little.update(&mut compact,|w,cx|w.expire_little_if_due(cx)));
    source.update(cx,|w,_|w.edit_browser(|b|b.settings.little_window_archive=OneHour));
    cx.executor().advance_clock(Duration::from_secs(31));cx.run_until_parked();
    assert_eq!(cx.windows().into_iter().filter(|h|h.downcast::<BrowserWindow>().is_some()).count(),1);
}

#[gpui::test]
fn closing_little_retains_one_global_archive_record_but_promotion_does_not(cx: &mut TestAppContext) {
    let (source, visual) = open(cx);
    let selected = source.read_with(visual, |view, _| view.browser.space().active_tab);
    visual.simulate_keystrokes("cmd-alt-n");visual.run_until_parked();
    let (little, mut compact) = little_root(visual);
    bar(&little, &mut compact).simulate_input("https://lookup.example/archive");
    bar(&little, &mut compact).simulate_keystrokes("enter");compact.run_until_parked();
    compact.simulate_keystrokes("cmd-w");compact.run_until_parked();visual.run_until_parked();
    source.read_with(visual, |view, _| {
        assert_eq!(view.browser.space().active_tab, selected);
        assert_eq!(view.browser.archive.len(), 1);
        assert_eq!(view.browser.archive[0].origin, browser_core::ArchiveOrigin::LittleArc);
        assert_eq!(view.browser.archive[0].reason, browser_core::ArchiveReason::Manual);
        assert_eq!(view.browser.archive[0].tab.url, "https://lookup.example/archive");
    });
    visual.simulate_keystrokes("cmd-alt-n");visual.run_until_parked();
    let (little, mut compact) = little_root(visual);
    bar(&little, &mut compact).simulate_input("https://lookup.example/promoted");
    bar(&little, &mut compact).simulate_keystrokes("enter");compact.run_until_parked();
    little.update(&mut compact, |view, cx| view.promote_transient(None, cx));
    compact.run_until_parked();visual.run_until_parked();
    source.read_with(visual, |view, _| {
        assert_eq!(view.browser.archive.len(), 1, "promotion must not leave a duplicate archive record");
        assert!(view.browser.all_tabs().any(|tab| tab.url == "https://lookup.example/promoted"));
    });
}
