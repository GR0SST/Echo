//! Display-only URL shortening for the link status pill.
use unicode_segmentation::UnicodeSegmentation;

/// Keep the host and final path segment recognizable when a link is too wide.
/// The measurement comes from the same font used to draw the label. This
/// string is never used for navigation, persistence or the clipboard.
pub(crate) fn elide(target: &str, available: f32, mut measure: impl FnMut(&str) -> f32) -> String {
    let width = if available.is_finite() { available.max(0.) } else { 0. };
    let standard = target.strip_prefix("https://").or_else(|| target.strip_prefix("http://"));
    let text: String = standard.unwrap_or(target).chars().filter(|ch| !ch.is_control()).collect();
    if measure(&text) <= width { return text; }
    if standard.is_some() {
        // CEF supplies canonical URLs, so encoded slashes remain inside their
        // segments. Query/fragment delimiters must not be treated as path.
        let authority_end = text.find(['/', '?', '#']).unwrap_or(text.len());
        let suffix_start = text[authority_end..].find(['?', '#']).map_or(text.len(), |index| authority_end + index);
        let path = &text[authority_end..suffix_start];
        if measure(&text[..suffix_start]) > width && path.starts_with('/') {
            let components: Vec<_> = path[1..].split('/').collect();
            let trailing = components.last() == Some(&"") && components.len() > 1;
            let last = components.len() - 1 - usize::from(trailing);
            if last > 0 {
                let filename = format!("{}{}", components[last], if trailing { "/" } else { "" });
                let build = |count: usize| {
                    let mut result = format!("{}/", &text[..authority_end]);
                    for component in &components[..count] { result.push_str(component); result.push('/'); }
                    result.push_str("…/");
                    result.push_str(&filename);
                    result
                };
                let mut low = 0;
                let mut high = last;
                if measure(&build(0)) <= width {
                    // Retain as much of the leading path as fits, reserving the
                    // final segment. The remaining query is tail-elided.
                    while low < high {
                        let count = (low + high).div_ceil(2);
                        if measure(&build(count)) <= width { low = count; }
                        else { high = count - 1; }
                    }
                    let candidate = build(low) + &text[suffix_start..];
                    return tail(&candidate, width, &mut measure);
                }
            }
        }
    }
    tail(&text, width, &mut measure)
}

fn tail(text: &str, width: f32, measure: &mut impl FnMut(&str) -> f32) -> String {
    if measure(text) <= width { return text.into(); }
    if measure("…") > width { return String::new(); }
    let boundaries: Vec<_> = text.grapheme_indices(true).map(|(index, _)| index).chain([text.len()]).collect();
    let mut low = 0;
    let mut high = boundaries.len() - 1;
    while low < high {
        let count = (low + high).div_ceil(2);
        let candidate = format!("{}…", &text[..boundaries[count]]);
        if measure(&candidate) <= width { low = count; }
        else { high = count - 1; }
    }
    format!("{}…", &text[..boundaries[low]])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn width(text: &str) -> f32 { text.graphemes(true).count() as f32 }

    #[test]
    fn fitting_targets_retain_the_exact_path_query_and_fragment() {
        assert_eq!(elide("https://example.org/a%2Fb?utm_source=test#part", 100., width),
            "example.org/a%2Fb?utm_source=test#part");
        assert_eq!(elide("mailto:name@example.org", 100., width), "mailto:name@example.org");
    }

    #[test]
    fn long_paths_keep_the_host_and_final_segment_including_a_trailing_slash() {
        for ending in ["report.pdf", "report/"] {
            let target = format!("https://example.org/first/second/third/fourth/{ending}");
            let label = elide(&target, 32., width);
            assert!(label.starts_with("example.org/") && label.contains("/…/"), "{label}");
            assert!(label.ends_with(ending), "{label}");
            assert!(width(&label) <= 32.);
        }
    }

    #[test]
    fn a_query_that_does_not_fit_gets_an_explicit_tail_ellipsis() {
        let label = elide("https://example.org/short?very-long-query=123456789", 30., width);
        assert_eq!(label, "example.org/short?very-long-q…");
        let label = elide("https://example.org/first/second/third/report?signature=123456789", 32., width);
        assert!(label.starts_with("example.org/") && label.contains("/…/report"), "{label}");
        assert!(label.ends_with('…') && width(&label) <= 32.);
    }

    #[test]
    fn narrow_and_nonstandard_targets_shorten_without_broken_graphemes() {
        let family = "👨‍👩‍👧‍👦";
        assert_eq!(elide(&format!("data:{family}more"), 7., width), format!("data:{family}…"));
        for limit in [0., 0.5, 1., 4., 10., 20., 40., 100.] {
            for target in ["https://example.org/a/b/c", "file:///tmp/long-file.txt", "https://example.org/?a=/b/c"] {
                assert!(width(&elide(target, limit, width)) <= limit);
            }
        }
    }
}
