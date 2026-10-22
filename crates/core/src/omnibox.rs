//! Turns what the user typed into the command bar into a URL to load.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// The input is an address.
    Url(String),
    /// The input is a search query; `url` is the search results page.
    Search { query: String, url: String },
}

impl Resolved {
    pub fn url(&self) -> &str {
        match self {
            Resolved::Url(url) | Resolved::Search { url, .. } => url,
        }
    }
}

const SCHEMES: &[&str] = &["http://", "https://", "file://", "about:", "chrome://", "data:"];

/// Returns `None` for blank input.
pub fn resolve(input: &str) -> Option<Resolved> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    let lower = input.to_ascii_lowercase();
    if SCHEMES.iter().any(|s| lower.starts_with(s)) {
        return Some(Resolved::Url(input.to_string()));
    }
    if !input.contains(char::is_whitespace) && looks_like_host(&lower) {
        let scheme = if is_local(&lower) { "http" } else { "https" };
        return Some(Resolved::Url(format!("{scheme}://{input}")));
    }
    Some(Resolved::Search {
        query: input.to_string(),
        url: format!("https://www.google.com/search?q={}", encode_query(input)),
    })
}

fn host_of(input: &str) -> &str {
    let host_port = input.split(['/', '?', '#']).next().unwrap_or(input);
    host_port.rsplit_once(':').map_or(host_port, |(host, port)| {
        if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) {
            host
        } else {
            host_port
        }
    })
}

fn is_local(input: &str) -> bool {
    let host = host_of(input);
    host == "localhost" || host.parse::<std::net::Ipv4Addr>().is_ok()
}

fn looks_like_host(input: &str) -> bool {
    let host = host_of(input);
    if is_local(input) {
        return true;
    }
    let mut labels = host.split('.');
    let Some(tld) = labels.next_back() else {
        return false;
    };
    let has_dot = host.contains('.');
    let labels_ok = host
        .split('.')
        .all(|l| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || c == '-'));
    has_dot && labels_ok && tld.chars().count() >= 2 && tld.chars().all(char::is_alphabetic)
}

fn encode_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for byte in query.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(input: &str) -> String {
        resolve(input).unwrap().url().to_string()
    }

    #[test]
    fn blank_input_resolves_to_nothing() {
        assert_eq!(resolve("   "), None);
    }

    #[test]
    fn addresses_get_a_scheme() {
        assert_eq!(url("github.com"), "https://github.com");
        assert_eq!(url("news.ycombinator.com/item?id=1"), "https://news.ycombinator.com/item?id=1");
        assert_eq!(url("localhost:3000"), "http://localhost:3000");
        assert_eq!(url("127.0.0.1:8080/x"), "http://127.0.0.1:8080/x");
        assert_eq!(url("пример.рф"), "https://пример.рф");
    }

    #[test]
    fn explicit_schemes_are_kept() {
        assert_eq!(url("http://example.com"), "http://example.com");
        assert_eq!(url("about:blank"), "about:blank");
    }

    #[test]
    fn everything_else_is_a_search() {
        assert_eq!(url("rust gpui"), "https://www.google.com/search?q=rust+gpui");
        assert_eq!(url("arc"), "https://www.google.com/search?q=arc");
        assert_eq!(url("v1.2"), "https://www.google.com/search?q=v1.2");
        assert_eq!(url("c++"), "https://www.google.com/search?q=c%2B%2B");
        assert_eq!(url("привет"), "https://www.google.com/search?q=%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82");
    }
}
