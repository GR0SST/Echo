//! CoreText-style wrapping for the reference's short static paragraphs.
//! GPUI 0.2 counts trailing spaces and treats NBSP as a break candidate.
use gpui::{div, prelude::*, px, AnyElement, FontWeight, Pixels, StyledText, Window};
use std::ops::Range;

/// Replacing a delimiter preserves byte offsets for attributed ranges. The
/// renderer disables its second wrapping pass; invisible trailing spaces may
/// extend beyond the line's ink, as they do in the reference. Complex scripts
/// and overlong words retain GPUI's normal fallback behavior.
fn wrap(
    text: &str,
    width: Pixels,
    mut measure: impl FnMut(Range<usize>) -> Pixels,
) -> Option<String> {
    if width <= px(0.)
        || text
            .chars()
            .any(|c| (c.is_alphabetic() && c as u32 > 0x052f) || c as u32 >= 0x1f000)
    {
        return None;
    }
    let mut output = text.as_bytes().to_vec();
    let mut paragraph_start = 0;
    for paragraph in text.split_inclusive('\n') {
        let paragraph_body = paragraph.strip_suffix('\n').unwrap_or(paragraph);
        let mut line_start = paragraph_start;
        let mut word_start = paragraph_start;
        for word in paragraph_body.split_inclusive(' ') {
            let end = word_start + word.trim_end_matches(' ').len();
            if end > word_start {
                if measure(word_start..end) > width {
                    return None;
                }
                if word_start > line_start && measure(line_start..end) > width {
                    debug_assert_eq!(output[word_start - 1], b' ');
                    output[word_start - 1] = b'\n';
                    line_start = word_start;
                }
            }
            word_start += word.len();
        }
        paragraph_start += paragraph.len();
    }
    Some(String::from_utf8(output).expect("only ASCII delimiters changed"))
}

pub(crate) fn render(
    text: String,
    bold: Option<Range<usize>>,
    width: Pixels,
    lift: Pixels,
    window: &Window,
) -> AnyElement {
    let mut style = window.text_style().clone();
    style.font_family = ".SystemUIFont".into();
    style.font_weight = FontWeight::NORMAL;
    let wrapped = wrap(&text, width, |range| {
        let mut runs = Vec::new();
        if let Some(emphasis) = bold
            .as_ref()
            .filter(|b| b.start < range.end && b.end > range.start)
        {
            let start = emphasis.start.max(range.start);
            let end = emphasis.end.min(range.end);
            if start > range.start {
                runs.push(style.to_run(start - range.start));
            }
            let mut emphasized = style.clone();
            emphasized.font_weight = FontWeight::BOLD;
            runs.push(emphasized.to_run(end - start));
            if end < range.end {
                runs.push(style.to_run(range.end - end));
            }
        } else {
            runs.push(style.to_run(range.len()));
        }
        window
            .text_system()
            .shape_line(text[range].to_owned().into(), px(14.), &runs, None)
            .width
    });
    let manual = wrapped.is_some();
    let text = wrapped.unwrap_or(text);
    let label = StyledText::new(text).with_highlights(bold.into_iter().map(|range| {
        (
            range,
            gpui::HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..Default::default()
            },
        )
    }));
    div()
        .w_full()
        .relative()
        .top(lift)
        .font_family(".SystemUIFont")
        .font_weight(FontWeight::NORMAL)
        .text_size(px(14.))
        .when(manual, |paragraph| paragraph.whitespace_nowrap())
        .child(label)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn measured(text: &str, width: f32) -> String {
        wrap(text, px(width), |range| {
            px(text[range].chars().count() as f32)
        })
        .unwrap()
    }
    #[test]
    fn a_trailing_space_does_not_push_the_preceding_word_onto_the_next_line() {
        assert_eq!(measured("aaaa bbbb c", 9.), "aaaa bbbb\nc");
        assert_eq!(measured("aaaa  bbb c", 9.), "aaaa  bbb\nc");
    }
    #[test]
    fn nonbreaking_groups_and_paragraph_separation_survive_wrapping() {
        assert_eq!(
            measured("aaa —\u{a0}so\n\nnext", 5.),
            "aaa\n—\u{a0}so\n\nnext"
        );
    }
}
