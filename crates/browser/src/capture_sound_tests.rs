use super::*;
#[gpui::test]
fn capture_feedback_cues_follow_live_preferences_and_only_fire_once_per_completed_crop(
    cx: &mut TestAppContext,
) {
    let (view, cx, fake) = open_pages(cx);
    fake.0.borrow_mut().capture_supported = true;
    crate::feedback::CAPTURE_CUES.with(|count| count.set(0));
    for (enabled, expected) in [(false, 0), (true, 1)] {
        view.update(cx, |view, _| {
            view.edit_browser(|browser| {
                browser
                    .settings
                    .set_enabled(browser_core::settings::Toggle::SoundEffects, enabled)
            })
        });
        fake.take_calls();
        cx.simulate_keystrokes("cmd-shift-2");
        cx.run_until_parked();
        let mut overlay = current(&view, cx);
        let frame = fake.0.borrow().bounds[&1];
        select(&mut overlay, frame);
        let request = fake
            .take_calls()
            .into_iter()
            .find_map(|call| {
                if let Call::Snapshot(1, id) = call {
                    Some(id)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(crate::feedback::CAPTURE_CUES.with(|count| count.get()), 0);
        complete(&view, cx, &fake, request, image(frame));
        overlay.run_until_parked();
        assert_eq!(
            crate::feedback::CAPTURE_CUES.with(|count| count.get()),
            expected
        );
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        assert_eq!(
            crate::feedback::CAPTURE_CUES.with(|count| count.get()),
            expected,
            "polling a retained PNG must not repeat the cue"
        );
        overlay.simulate_keystrokes("escape");
        cx.run_until_parked();
    }
}
