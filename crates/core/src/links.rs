//! Link sharing without advertising identifiers, preserving the original encoding.

pub fn clean_url(url: &str) -> String {
    if !url.to_ascii_lowercase().starts_with("https://")
        && !url.to_ascii_lowercase().starts_with("http://")
    {
        return url.into();
    }
    let (address, fragment) = url
        .split_once('#')
        .map_or((url, None), |(address, fragment)| (address, Some(fragment)));
    let Some((base, query)) = address.split_once('?') else {
        return url.into();
    };
    // Removing any query parameter could invalidate a signed download link.
    if query.split('&').any(|part| {
        matches!(
            part.split('=')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "x-amz-signature" | "x-goog-signature" | "signature" | "sig"
        )
    }) {
        return url.into();
    }
    let keep: Vec<_> = query
        .split('&')
        .filter(|part| {
            let key = part.split('=').next().unwrap_or("").to_ascii_lowercase();
            !key.starts_with("utm_")
                && !matches!(
                    key.as_str(),
                    "fbclid"
                        | "gclid"
                        | "dclid"
                        | "msclkid"
                        | "mc_cid"
                        | "mc_eid"
                        | "igshid"
                        | "twclid"
                        | "yclid"
                        | "_ga"
                        | "_gl"
                )
        })
        .collect();
    if keep.len() == query.split('&').count() {
        return url.into();
    }
    let mut clean = base.to_string();
    if !keep.is_empty() {
        clean.push('?');
        clean.push_str(&keep.join("&"));
    }
    if let Some(fragment) = fragment {
        clean.push('#');
        clean.push_str(fragment);
    }
    clean
}

pub fn markdown_link(title: &str, url: &str) -> String {
    let title = if title.trim().is_empty() { url } else { title };
    let title = title
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace(['\r', '\n'], " ");
    let clean = clean_url(url)
        .replace('(', "%28")
        .replace(')', "%29")
        .replace(' ', "%20")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('\n', "%0A")
        .replace('\r', "%0D");
    format!("[{title}]({clean})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleaning_preserves_real_query_parameters_encoding_and_fragments() {
        assert_eq!(
            clean_url(
                "https://example.com/search?q=a%2Bb&UTM_source=email&x=1&x=2&fbclid=abc#section"
            ),
            "https://example.com/search?q=a%2Bb&x=1&x=2#section"
        );
        assert_eq!(
            clean_url("https://example.com/?utm_source=x#fragment?utm_source=kept"),
            "https://example.com/#fragment?utm_source=kept"
        );
        for url in [
            "https://example.com/?q=1&token=secret",
            "https://example.com/?",
            "data:text/plain,?utm_source=test",
            "file:///tmp/a?utm_source=x",
            "https://example.com/#?utm_source=kept",
        ] {
            assert_eq!(clean_url(url), url);
        }
    }

    #[test]
    fn signed_urls_keep_their_query_intact() {
        let url = "https://example.com/file?utm_source=x&X-Amz-Signature=abc&X-Amz-Date=20260101";
        assert_eq!(clean_url(url), url);
    }

    #[test]
    fn markdown_escapes_titles_and_link_destinations() {
        assert_eq!(
            markdown_link(
                "A [page]\\title\nMore",
                "https://example.com/a(b)?utm_source=x"
            ),
            "[A \\[page\\]\\\\title More](https://example.com/a%28b%29)"
        );
        assert_eq!(
            markdown_link("", "https://example.com"),
            "[https://example.com](https://example.com)"
        );
    }
}
