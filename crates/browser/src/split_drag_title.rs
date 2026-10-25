//! The drag card wraps long page titles by grapheme and truncates its third line.
use gpui::{px, FontWeight, Window};
use unicode_segmentation::UnicodeSegmentation;

fn bounded(text: &str, mut fits: impl FnMut(&str) -> bool) -> String {
    let text = text.replace(['\n', '\r'], " ");
    let mut lines = Vec::new();
    let mut line = String::new();
    for grapheme in text.graphemes(true) {
        if line.is_empty() && grapheme.trim().is_empty() {
            continue;
        }
        let candidate = format!("{line}{grapheme}");
        if !line.is_empty() && !fits(&candidate) {
            if lines.len() == 2 {
                while !line.is_empty() && !fits(&format!("{line}…")) {
                    let last = line.grapheme_indices(true).last().unwrap().0;
                    line.truncate(last);
                }
                lines.push(format!("{}…", line.trim_end()));
                return lines.join("\n");
            }
            lines.push(line.trim_end().to_owned());
            line = grapheme.trim_start().to_owned();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines.join("\n")
}

pub(super) fn render(text: &str, window: &Window) -> String {
    let mut style = window.text_style().clone();
    style.font_family = ".SystemUIFont".into();
    style.font_weight = FontWeight::BOLD;
    bounded(text, |line| {
        window
            .text_system()
            .shape_line(
                line.to_owned().into(),
                px(13.),
                &[style.to_run(line.len())],
                None,
            )
            .width
            <= px(104.)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_unspaced_titles_wrap_and_ellipsis_fits_the_third_line() {
        assert_eq!(
            bounded("abcdefghijklmnop", |s| s.graphemes(true).count() <= 5),
            "abcde\nfghij\nklmn…"
        );
        assert_eq!(bounded("short", |s| s.len() <= 5), "short");
    }
    #[test]
    fn wrapping_never_splits_combined_unicode_or_emoji() {
        let glyph = "👩‍💻";
        let title = glyph.repeat(10);
        let result = bounded(&title, |s| s.graphemes(true).count() <= 3);
        assert_eq!(
            result,
            format!("{glyph}{glyph}{glyph}\n{glyph}{glyph}{glyph}\n{glyph}{glyph}…")
        );
        assert_eq!(
            bounded("e\u{301}e\u{301}", |s| s.graphemes(true).count() <= 1),
            "e\u{301}\ne\u{301}"
        );
    }
}
