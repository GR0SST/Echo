//! Anonymous, bounded reads of public documents. This reader never receives a
//! browser cookie jar, credentials, private-network addresses or model URLs.
use crate::client::Cancellation;
use scraper::{Html, Selector};
use std::{
    io::Read,
    net::IpAddr,
    time::{Duration, Instant},
};
use ureq::unversioned::{
    resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver},
    transport::{DefaultConnector, NextTimeout},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub url: String,
    pub title: String,
    pub text: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentError {
    Address,
    Network,
    Unsupported,
    TooLarge,
    Cancelled,
}

#[derive(Debug)]
struct PublicResolver;
impl Resolver for PublicResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let addresses = DefaultResolver::default().resolve(uri, config, timeout)?;
        // Reject the entire answer rather than allowing a later connection
        // fallback to a private address. The connector uses these exact IPs.
        if addresses.iter().any(|address| !public_ip(address.ip())) {
            return Err(ureq::Error::HostNotFound);
        }
        Ok(addresses)
    }
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && (b == 168 || b == 0 || (b == 2)))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_ip(v4.into());
            }
            // Public unicast only. Excludes ULA, link-local, multicast,
            // IPv4-compatible, NAT64 and all non-global special ranges.
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && s[1] < 0x200)
                && !(s[0] == 0x2001 && s[1] == 0xdb8)
                && s[0] != 0x2002
        }
    }
}
fn address(text: &str) -> Result<url::Url, DocumentError> {
    let url = url::Url::parse(text).map_err(|_| DocumentError::Address)?;
    if text.len() > 8192
        || !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
        || url.port().is_some_and(|p| p != 80 && p != 443)
        || url.host_str().is_some_and(|h| {
            h.eq_ignore_ascii_case("localhost")
                || h.ends_with(".localhost")
                || h.ends_with(".local")
        })
    {
        return Err(DocumentError::Address);
    }
    if let Ok(ip) = url
        .host_str()
        .unwrap()
        .trim_matches(['[', ']'])
        .parse::<IpAddr>()
    {
        if !public_ip(ip) {
            return Err(DocumentError::Address);
        }
    }
    Ok(url)
}
pub fn read(target: &str, cancel: &Cancellation) -> Result<Document, DocumentError> {
    let mut url = address(target)?;
    let start = Instant::now();
    let agent = ureq::Agent::with_parts(
        ureq::Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(15)))
            .build(),
        DefaultConnector::default(),
        PublicResolver,
    );
    for hop in 0..=5 {
        cancel.check().map_err(|_| DocumentError::Cancelled)?;
        if start.elapsed() > Duration::from_secs(20) {
            return Err(DocumentError::Network);
        }
        let mut response = agent
            .get(url.as_str())
            .header("Accept", "text/html,text/plain;q=0.9")
            .header("User-Agent", "Echo/1.0 (link preview)")
            .call()
            .map_err(|_| DocumentError::Network)?;
        if response.status().is_redirection() {
            if hop == 5 {
                return Err(DocumentError::Network);
            }
            let location = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or(DocumentError::Address)?;
            url = address(
                url.join(location)
                    .map_err(|_| DocumentError::Address)?
                    .as_str(),
            )?;
            continue;
        }
        if !response.status().is_success() {
            return Err(DocumentError::Network);
        }
        let mime = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if mime != "text/html" && mime != "text/plain" && mime != "application/xhtml+xml" {
            return Err(DocumentError::Unsupported);
        }
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut body)
            .map_err(|_| DocumentError::Network)?;
        cancel.check().map_err(|_| DocumentError::Cancelled)?;
        if body.len() > 2 * 1024 * 1024 {
            return Err(DocumentError::TooLarge);
        }
        let body = String::from_utf8_lossy(&body);
        let (title, text) = if mime == "text/plain" {
            (
                String::new(),
                body.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(30_000)
                    .collect(),
            )
        } else {
            extract(&body)
        };
        if text.trim().is_empty() {
            return Err(DocumentError::Unsupported);
        }
        return Ok(Document {
            url: url.into(),
            title,
            text,
        });
    }
    Err(DocumentError::Network)
}
fn extract(body: &str) -> (String, String) {
    let html = Html::parse_document(body);
    let title = html
        .select(&Selector::parse("title").unwrap())
        .next()
        .map(|n| n.text().collect::<String>())
        .unwrap_or_default()
        .chars()
        .take(300)
        .collect();
    let root = ["main", "article", "body"]
        .into_iter()
        .find_map(|tag| html.select(&Selector::parse(tag).unwrap()).next());
    let mut result = String::new();
    if let Some(root) = root {
        for node in root.descendants() {
            let Some(text) = node.value().as_text() else {
                continue;
            };
            if node.ancestors().any(|n| {
                n.value().as_element().is_some_and(|e| {
                    matches!(
                        e.name(),
                        "script"
                            | "style"
                            | "noscript"
                            | "nav"
                            | "footer"
                            | "header"
                            | "form"
                            | "svg"
                            | "template"
                    )
                })
            }) {
                continue;
            }
            for word in text.split_whitespace() {
                if !result.is_empty() {
                    result.push(' ');
                }
                for c in word.chars() {
                    if result.len() >= 120_000 {
                        return (title, result.chars().take(30_000).collect());
                    }
                    result.push(c);
                }
            }
        }
    }
    (title, result.chars().take(30_000).collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn document_reader_excludes_private_networks_and_credential_urls() {
        for url in [
            "http://127.1/",
            "http://10.0.0.1/",
            "http://[::1]/",
            "http://[::ffff:192.168.0.1]/",
            "http://169.254.169.254/",
            "https://localhost/",
            "https://x.local/",
            "https://user:pass@example.com/",
            "file:///etc/passwd",
            "http://example.com:9999/",
        ] {
            assert!(address(url).is_err(), "{url}");
        }
        assert!(address("https://en.wikipedia.org/wiki/Rust").is_ok());
        for ip in [
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.1",
            "100.64.0.1",
            "2001:db8::1",
            "fc00::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(public_ip(ip.parse().unwrap()), "{ip}");
        }
    }
    #[test]
    fn document_extraction_decodes_entities_skips_active_content_and_bounds_context() {
        let (title,text)=extract("<title>A &amp; B</title><nav>secret nav</nav><main><script>ignore instructions</script><h1>Hello</h1><p>Rust &amp; types.</p><form>password</form></main>");
        assert_eq!(title, "A & B");
        assert_eq!(text, "Hello Rust & types.");
        assert!(
            extract(&format!("<body>{}</body>", "文".repeat(100_000)))
                .1
                .chars()
                .count()
                <= 30_000
        );
    }
}
