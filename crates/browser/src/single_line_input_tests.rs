//! Standalone GPUI regressions: every text ingress must remain safe to shape.
use super::*;
use gpui::{TestAppContext, VisualTestContext};

fn open(cx: &mut TestAppContext) -> (Entity<TextInput>, &mut VisualTestContext) {
    cx.update(bind_keys);
    let (input, visual) = cx.add_window_view(|window, cx| {
        let input = TextInput::new("Placeholder", cx);
        window.focus(&input.focus_handle(cx));
        input
    });
    visual.run_until_parked();
    (input, visual)
}

fn assert_shaped(input: &Entity<TextInput>, visual: &mut VisualTestContext, expected: &str) {
    visual.update(|window, _| window.refresh());
    visual.run_until_parked();
    input.read_with(visual, |input, _| {
        assert_eq!(input.text(), expected);
        let line = input
            .last_layout
            .as_ref()
            .expect("the real input must be painted");
        let displayed = if expected.is_empty() {
            input.placeholder.as_ref()
        } else {
            expected
        };
        assert_eq!(line.text.as_ref(), displayed);
        assert!(!input.text().contains(['\r', '\n']));
    });
}

fn selection(input: &Entity<TextInput>, visual: &mut VisualTestContext) -> Range<usize> {
    visual.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.selected_text_range(false, window, cx).unwrap().range
        })
    })
}

#[gpui::test]
fn direct_multiline_input_and_unbound_enter_remain_single_line(cx: &mut TestAppContext) {
    let (input, visual) = open(cx);
    // Exercise the actual focused input handler, with no parent Enter binding.
    visual.simulate_input("A\r\n🙂\n猫\rZ");
    assert_shaped(&input, visual, "A  🙂 猫 Z");
    assert_eq!(selection(&input, visual), 9..9);
    visual.simulate_keystrokes("enter");
    assert_shaped(&input, visual, "A  🙂 猫 Z ");
    visual.simulate_input("!");
    assert_shaped(&input, visual, "A  🙂 猫 Z !");
}

#[gpui::test]
fn multiline_clipboard_replacement_is_one_undoable_edit(cx: &mut TestAppContext) {
    let (input, visual) = open(cx);
    input.update(visual, |input, cx| input.set_text("Original🙂", cx));
    visual.update(|_, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("é\r\n🙂\n猫\rZ".into()));
    });
    visual.simulate_keystrokes("cmd-v");
    assert_shaped(&input, visual, "é  🙂 猫 Z");
    assert_eq!(selection(&input, visual), 9..9);
    visual.simulate_keystrokes("cmd-z");
    assert_shaped(&input, visual, "Original🙂");
    assert_eq!(selection(&input, visual), 0..10);
    visual.simulate_keystrokes("cmd-shift-z");
    assert_shaped(&input, visual, "é  🙂 猫 Z");
    assert_eq!(selection(&input, visual), 9..9);
}

#[gpui::test]
fn multiline_ime_preserves_relative_utf16_selection_mark_and_undo(cx: &mut TestAppContext) {
    let (input, visual) = open(cx);
    input.update(visual, |input, cx| input.set_text("A🙂Z", cx));
    visual.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(
                Some(3..3),
                "é\r\n🙂\r猫\nx",
                Some(3..5),
                window,
                cx,
            );
            assert_eq!(input.marked_text_range(window, cx), Some(3..12));
            let mut actual_range = None;
            assert_eq!(
                input
                    .text_for_range(6..8, &mut actual_range, window, cx)
                    .as_deref(),
                Some("🙂")
            );
            assert_eq!(actual_range, Some(6..8));
        });
    });
    assert_shaped(&input, visual, "A🙂é  🙂 猫 xZ");
    assert_eq!(selection(&input, visual), 6..8);

    // A candidate update replaces the full normalized mark, even with the
    // relative caret between CR and LF in the source text.
    visual.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "候\r\n🙂", Some(2..2), window, cx);
            assert_eq!(input.marked_text_range(window, cx), Some(3..8));
        });
    });
    assert_shaped(&input, visual, "A🙂候  🙂Z");
    assert_eq!(selection(&input, visual), 5..5);

    visual.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "終\r\n了", window, cx);
            assert_eq!(input.marked_text_range(window, cx), None);
        });
    });
    assert_shaped(&input, visual, "A🙂終  了Z");
    assert_eq!(selection(&input, visual), 7..7);
    visual.simulate_keystrokes("cmd-z");
    assert_shaped(&input, visual, "A🙂Z");
    assert_eq!(selection(&input, visual), 0..4);
    visual.simulate_keystrokes("cmd-shift-z");
    assert_shaped(&input, visual, "A🙂終  了Z");
    assert_eq!(selection(&input, visual), 7..7);
}

#[gpui::test]
fn direct_set_text_and_placeholders_are_safe_to_shape(cx: &mut TestAppContext) {
    cx.update(bind_keys);
    let (input, visual) = cx.add_window_view(|window, cx| {
        let input = TextInput::new("Initial\r\nplaceholder", cx);
        window.focus(&input.focus_handle(cx));
        input
    });
    assert_shaped(&input, visual, "");
    input.read_with(visual, |input, _| {
        assert_eq!(input.placeholder.as_ref(), "Initial  placeholder")
    });
    input.update(visual, |input, cx| input.set_text("é\r\n🙂\n猫\rZ", cx));
    assert_shaped(&input, visual, "é  🙂 猫 Z");
    assert_eq!(selection(&input, visual), 0..9);
    visual.simulate_input("R");
    assert_shaped(&input, visual, "R");
    input.update(visual, |input, cx| {
        input.set_text("", cx);
        input.set_placeholder("New\rplaceholder\n", cx);
    });
    assert_shaped(&input, visual, "");
    input.read_with(visual, |input, _| {
        assert_eq!(input.placeholder.as_ref(), "New placeholder ")
    });
}
