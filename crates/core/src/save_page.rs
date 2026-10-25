//! Local page export from Chromium's retained-document MHTML snapshot.
//! No network requests, profile access or filesystem writes happen here.
use mailparse::MailHeaderMap;
use std::collections::HashMap;
use url::Url;

pub const MAX_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;
const MAX_PARTS: usize = 2048;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    HtmlOnly,
    SingleFile,
    #[default]
    Complete,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::SingleFile => "mhtml",
            _ => "html",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::HtmlOnly => "Webpage, HTML Only",
            Self::SingleFile => "Webpage, Single File",
            Self::Complete => "Webpage, Complete",
        }
    }
    pub const ALL: [Self; 3] = [Self::HtmlOnly, Self::SingleFile, Self::Complete];
}

#[derive(Debug)]
pub struct Export {
    pub document: Vec<u8>,
    /// Each path contains exactly one safe filename; the caller owns its new
    /// assets directory and publishes the main document only after these files.
    pub assets: Vec<(String, Vec<u8>)>,
}

struct Part {
    location: Option<Url>,
    content_id: Option<String>,
    mime: String,
    bytes: Vec<u8>,
    text: Option<String>,
}

pub fn suggested_name(title: &str, format: Format) -> String {
    format!(
        "{}.{}",
        safe_component(title, "Untitled"),
        format.extension()
    )
}
pub fn assets_directory(filename: &str) -> String {
    let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
    format!("{}_files", safe_component(stem, "Untitled"))
}

pub fn export(
    mhtml: &[u8],
    format: Format,
    filename: &str,
    assets_directory: &str,
) -> Result<Export, String> {
    if mhtml.len() > MAX_ARCHIVE_BYTES {
        return Err("The page archive is too large.".into());
    }
    let parsed = mailparse::parse_mail(mhtml).map_err(|_| "The page archive could not be read.")?;
    let mut parts = Vec::new();
    collect_parts(&parsed, &mut parts, 0)?;
    let start = parsed
        .ctype
        .params
        .get("start")
        .map(|value| value.trim_start_matches('<').trim_end_matches('>'));
    let source = parsed
        .headers
        .get_first_value("Snapshot-Content-Location")
        .and_then(|value| Url::parse(&value).ok());
    let root = start
        .and_then(|id| {
            parts
                .iter()
                .position(|part| part.mime == "text/html" && part.content_id.as_deref() == Some(id))
        })
        .or_else(|| {
            source.as_ref().and_then(|url| {
                parts.iter().position(|part| {
                    part.mime == "text/html" && part.location.as_ref() == Some(url)
                })
            })
        })
        .or_else(|| parts.iter().position(|part| part.mime == "text/html"))
        .ok_or("The archive has no HTML document.")?;
    if format == Format::SingleFile {
        return Ok(Export {
            document: mhtml.to_vec(),
            assets: Vec::new(),
        });
    }
    let original = parts[root]
        .location
        .clone()
        .or_else(|| {
            parsed
                .headers
                .get_first_value("Snapshot-Content-Location")
                .and_then(|s| Url::parse(&s).ok())
        })
        .ok_or("The archive has no document address.")?;
    if safe_component(assets_directory, "") != assets_directory
        || assets_directory.is_empty()
        || assets_directory == "."
        || assets_directory == ".."
    {
        return Err("Invalid assets directory.".into());
    }
    if filename.is_empty()
        || matches!(filename, "." | "..")
        || filename
            .chars()
            .any(|ch| ch.is_control() || ch == '/' || ch == '\\')
    {
        return Err("Invalid document filename.".into());
    }
    let names: Vec<_> = parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            if index == root {
                filename.to_owned()
            } else {
                resource_name(index, part)
            }
        })
        .collect();
    let mut by_url = HashMap::new();
    let mut by_id = HashMap::new();
    for (index, part) in parts.iter().enumerate() {
        if let Some(url) = &part.location {
            by_url.entry(without_fragment(url)).or_insert(index);
        }
        if let Some(id) = &part.content_id {
            by_id.entry(id.clone()).or_insert(index);
        }
    }
    let rewrite = |index: usize| -> Result<Vec<u8>, String> {
        let part = &parts[index];
        let base = part.location.clone().unwrap_or_else(|| original.clone());
        let resolver = Resolver {
            base,
            parts: &parts,
            root,
            current: index,
            complete: format == Format::Complete,
            names: &names,
            directory: assets_directory,
            by_url: &by_url,
            by_id: &by_id,
        };
        match (part.mime.as_str(), &part.text) {
            ("text/html", Some(html)) => Ok(rewrite_html(html, resolver)?.into_bytes()),
            ("text/css", Some(css)) => {
                Ok(rewrite_css(css, &|value| resolver.resolve(value)).into_bytes())
            }
            _ => Ok(part.bytes.clone()),
        }
    };
    let document = rewrite(root)?;
    let assets = if format == Format::Complete {
        parts
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != root)
            .map(|(index, _)| Ok((names[index].clone(), rewrite(index)?)))
            .collect::<Result<_, String>>()?
    } else {
        Vec::new()
    };
    Ok(Export { document, assets })
}

fn collect_parts(
    mail: &mailparse::ParsedMail<'_>,
    parts: &mut Vec<Part>,
    depth: usize,
) -> Result<(), String> {
    if depth > 16 || parts.len() >= MAX_PARTS {
        return Err("The archive contains too many resources.".into());
    }
    if !mail.subparts.is_empty() {
        for child in &mail.subparts {
            collect_parts(child, parts, depth + 1)?;
        }
        return Ok(());
    }
    let mime = mail.ctype.mimetype.to_ascii_lowercase();
    let bytes = mail
        .get_body_raw()
        .map_err(|_| "A page resource could not be decoded.")?;
    let text = matches!(mime.as_str(), "text/html" | "text/css").then(|| {
        mail.get_body()
            .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).into_owned())
    });
    parts.push(Part {
        location: mail
            .headers
            .get_first_value("Content-Location")
            .and_then(|s| Url::parse(&s).ok()),
        content_id: mail.headers.get_first_value("Content-ID").map(|s| {
            s.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_owned()
        }),
        mime,
        bytes,
        text,
    });
    Ok(())
}
fn safe_component(value: &str, fallback: &str) -> String {
    let value: String = value
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                ch
            }
        })
        .take(100)
        .collect();
    let value = value.trim().trim_matches('.').trim();
    if value.is_empty() {
        fallback.into()
    } else {
        value.into()
    }
}
fn resource_name(index: usize, part: &Part) -> String {
    let base = part
        .location
        .as_ref()
        .and_then(|url| url.path_segments()?.next_back())
        .filter(|s| !s.is_empty())
        .unwrap_or("resource");
    let mut base = safe_component(base, "resource");
    if !base.contains('.') {
        base.push_str(match part.mime.as_str() {
            "text/html" => ".html",
            "text/css" => ".css",
            "image/png" => ".png",
            "image/jpeg" => ".jpg",
            "image/svg+xml" => ".svg",
            "image/gif" => ".gif",
            "image/webp" => ".webp",
            "application/javascript" | "text/javascript" => ".js",
            "font/woff2" => ".woff2",
            _ => ".bin",
        });
    }
    format!("{index}_{base}")
}
fn without_fragment(url: &Url) -> String {
    let mut url = url.clone();
    url.set_fragment(None);
    url.into()
}
fn encoded_component(value: &str) -> String {
    let mut url = Url::parse("file:///").unwrap();
    url.path_segments_mut().unwrap().push(value);
    url.path().trim_start_matches('/').to_owned()
}
struct Resolver<'a> {
    base: Url,
    parts: &'a [Part],
    root: usize,
    current: usize,
    complete: bool,
    names: &'a [String],
    directory: &'a str,
    by_url: &'a HashMap<String, usize>,
    by_id: &'a HashMap<String, usize>,
}
impl Resolver<'_> {
    fn resolve(&self, value: &str) -> String {
        let value = value.trim();
        if value.is_empty()
            || value.starts_with('#')
            || value.starts_with("data:")
            || value.starts_with("javascript:")
        {
            return value.into();
        }
        let cid = value.strip_prefix("cid:").and_then(|id| self.by_id.get(id));
        let resolved = cid
            .and_then(|index| self.parts[*index].location.clone())
            .or_else(|| self.base.join(value).ok());
        let index = cid.copied().or_else(|| {
            resolved
                .as_ref()
                .and_then(|url| self.by_url.get(&without_fragment(url)).copied())
        });
        if self.complete {
            if let Some(index) = index {
                let name = encoded_component(&self.names[index]);
                let mut target = match (self.current == self.root, index == self.root) {
                    (true, true) | (false, false) => name,
                    (false, true) => format!("../{name}"),
                    (true, false) => format!("{}/{name}", encoded_component(self.directory)),
                };
                if let Some(fragment) = resolved.as_ref().and_then(|url| url.fragment()) {
                    target.push('#');
                    target.push_str(fragment);
                }
                return target;
            }
        }
        resolved.map_or_else(|| value.into(), String::from)
    }
}
fn rewrite_html(html: &str, mut resolver: Resolver<'_>) -> Result<String, String> {
    // The original base applies to links and CSS before the offline base is
    // removed. This also keeps HTML-only navigation at the original website.
    let initial = resolver.base.clone();
    lol_html::rewrite_str(
        html,
        lol_html::RewriteStrSettings {
            element_content_handlers: vec![lol_html::element!("base[href]", |element| {
                if let Some(href) = element.get_attribute("href") {
                    if let Ok(base) = initial.join(&href) {
                        resolver.base = base;
                    }
                }
                Ok(())
            })],
            ..Default::default()
        },
    )
    .map_err(|_| "The document base could not be read.")?;
    let mut style_buffer = String::new();
    let output=lol_html::rewrite_str(html,lol_html::RewriteStrSettings {
        element_content_handlers: vec![
            lol_html::element!("base",|element| {element.remove();Ok(())}),
            lol_html::element!("[href], [src], [poster], object[data], [background], [action], [srcset], [style]",|element| {
                for attribute in ["href","src","poster","background","action"] {
                    if let Some(value)=element.get_attribute(attribute) {element.set_attribute(attribute,&resolver.resolve(&value))?;}
                }
                if element.tag_name()=="object" {if let Some(value)=element.get_attribute("data") {element.set_attribute("data",&resolver.resolve(&value))?;}}
                if let Some(value)=element.get_attribute("srcset") {element.set_attribute("srcset",&rewrite_srcset(&value,&|value|resolver.resolve(value)))?;}
                if let Some(value)=element.get_attribute("style") {element.set_attribute("style",&rewrite_css(&value,&|value|resolver.resolve(value)))?;}
                Ok(())
            }),
            lol_html::element!("meta[charset]",|element| {element.set_attribute("charset","utf-8")?;Ok(())}),
            lol_html::element!("meta[http-equiv]",|element| {
                if element.get_attribute("http-equiv").is_some_and(|value|value.eq_ignore_ascii_case("content-type")) {element.set_attribute("content","text/html; charset=utf-8")?;}
                Ok(())
            }),
            lol_html::text!("style",|text| {
            style_buffer.push_str(text.as_str());
            if text.last_in_text_node() {
                let rewritten=rewrite_css(&style_buffer,&|value|resolver.resolve(value));style_buffer.clear();
                text.replace(&rewritten,lol_html::html_content::ContentType::Html);
            } else {text.remove();}
            Ok(())
        })],..Default::default()
    }).map_err(|_|"The HTML document could not be exported.")?;
    Ok(output)
}

fn rewrite_srcset(value: &str, resolve: &impl Fn(&str) -> String) -> String {
    let mut rest = value;
    let mut result = String::new();
    while !rest.is_empty() {
        let leading = rest.len()
            - rest
                .trim_start_matches(|ch: char| ch.is_ascii_whitespace() || ch == ',')
                .len();
        result.push_str(&rest[..leading]);
        rest = &rest[leading..];
        if rest.is_empty() {
            break;
        }
        let end = rest
            .find(|ch: char| ch.is_ascii_whitespace())
            .unwrap_or(rest.len());
        let url = &rest[..end];
        let clean = url.trim_end_matches(',');
        result.push_str(&resolve(clean));
        result.push_str(&url[clean.len()..]);
        rest = &rest[end..];
        if clean.len() != url.len() {
            continue;
        }
        let mut parens = 0;
        let end = rest
            .char_indices()
            .find_map(|(index, ch)| {
                if ch == '(' {
                    parens += 1;
                } else if ch == ')' {
                    parens -= 1;
                } else if ch == ',' && parens == 0 {
                    return Some(index + 1);
                }
                None
            })
            .unwrap_or(rest.len());
        result.push_str(&rest[..end]);
        rest = &rest[end..];
    }
    result
}
fn rewrite_css(value: &str, resolve: &impl Fn(&str) -> String) -> String {
    fn quoted(value: &str) -> String {
        let mut output = String::new();
        cssparser::serialize_string(value, &mut output).unwrap();
        output
    }
    fn collect<'i>(
        parser: &mut cssparser::Parser<'i, '_>,
        replacements: &mut Vec<(usize, usize, String)>,
        resolve: &impl Fn(&str) -> String,
    ) -> Result<(), cssparser::ParseError<'i, ()>> {
        let mut keyword = 0;
        while !parser.is_exhausted() {
            let start = parser.position().byte_index();
            let token = parser.next_including_whitespace_and_comments()?.clone();
            match token {
                cssparser::Token::AtKeyword(name) if name.eq_ignore_ascii_case("import") => {
                    keyword = 1
                }
                cssparser::Token::AtKeyword(name) if name.eq_ignore_ascii_case("charset") => {
                    keyword = 2
                }
                cssparser::Token::QuotedString(url) if keyword != 0 => {
                    replacements.push((
                        start,
                        parser.position().byte_index(),
                        quoted(&if keyword == 1 {
                            resolve(&url)
                        } else {
                            "utf-8".into()
                        }),
                    ));
                    keyword = 0;
                }
                cssparser::Token::UnquotedUrl(url) => {
                    replacements.push((
                        start,
                        parser.position().byte_index(),
                        format!("url({})", quoted(&resolve(&url))),
                    ));
                    keyword = 0;
                }
                cssparser::Token::Function(name) if name.eq_ignore_ascii_case("url") => {
                    let url = parser.parse_nested_block(|nested| {
                        nested
                            .expect_string_cloned()
                            .map_err(Into::<cssparser::ParseError<'i, ()>>::into)
                    });
                    if let Ok(url) = url {
                        replacements.push((
                            start,
                            parser.position().byte_index(),
                            format!("url({})", quoted(&resolve(&url))),
                        ));
                    }
                    keyword = 0;
                }
                cssparser::Token::Function(_)
                | cssparser::Token::ParenthesisBlock
                | cssparser::Token::SquareBracketBlock
                | cssparser::Token::CurlyBracketBlock => {
                    let _ =
                        parser.parse_nested_block(|nested| collect(nested, replacements, resolve));
                    keyword = 0;
                }
                cssparser::Token::WhiteSpace(_) | cssparser::Token::Comment(_) => {}
                _ => keyword = 0,
            }
        }
        Ok(())
    }
    let mut input = cssparser::ParserInput::new(value);
    let mut parser = cssparser::Parser::new(&mut input);
    let mut replacements = Vec::new();
    let _ = collect(&mut parser, &mut replacements, resolve);
    let mut output = String::new();
    let mut cursor = 0;
    for (start, end, replacement) in replacements {
        if start >= cursor && end <= value.len() {
            output.push_str(&value[cursor..start]);
            output.push_str(&replacement);
            cursor = end;
        }
    }
    output.push_str(&value[cursor..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let parts = [
            (
                "text/html; charset=utf-8",
                "https://save.test/docs/page.html",
                "root",
                r##"<!doctype html><html><head><base href="../"><meta charset="utf-8"><link rel="stylesheet" href="css/theme.css"><style>.hero {background: url('images/icon.png')}</style></head><body><img src="images/icon.png" srcset="images/icon.png 1x, data:image/png;base64,AAAA 2x"><iframe src="cid:child"></iframe><a href="help#topic">Help</a></body></html>"##,
            ),
            (
                "text/css; charset=utf-8",
                "https://save.test/css/theme.css",
                "css",
                r##"@import "extra.css"; .hero {background: url(../images/icon.png)} .text::after {content: "url(do-not-rewrite)"}"##,
            ),
            (
                "image/png",
                "https://save.test/images/icon.png",
                "image",
                "binary image",
            ),
            (
                "text/html; charset=utf-8",
                "https://save.test/frame/index.html",
                "child",
                r##"<html><body><img src="../images/icon.png"><a href="../help">Help</a></body></html>"##,
            ),
            (
                "text/css; charset=utf-8",
                "https://save.test/css/extra.css",
                "extra",
                "body {color: red}",
            ),
        ];
        let mut result="MIME-Version: 1.0\r\nSnapshot-Content-Location: https://save.test/docs/page.html\r\nContent-Type: multipart/related; boundary=echo; start=\"<root>\"\r\n\r\n".to_owned();
        for (mime, location, id, body) in parts {
            result.push_str(&format!("--echo\r\nContent-Type: {mime}\r\nContent-Location: {location}\r\nContent-ID: <{id}>\r\n\r\n{body}\r\n"));
        }
        result.push_str("--echo--\r\n");
        result.into_bytes()
    }
    #[test]
    fn complete_page_keeps_frames_styles_images_and_offline_relative_references() {
        let result = export(&fixture(), Format::Complete, "Demo.html", "Demo_files").unwrap();
        let html = String::from_utf8(result.document).unwrap();
        assert!(html.contains("Demo_files/1_theme.css"), "{html}");
        assert!(html.contains("Demo_files/2_icon.png"));
        assert!(html.contains("Demo_files/3_index.html"));
        assert!(html.contains("https://save.test/help#topic"));
        assert!(html.contains("data:image/png;base64,AAAA 2x"));
        assert!(!html.contains("<base"));
        let assets: HashMap<_, _> = result.assets.into_iter().collect();
        let css = String::from_utf8(assets["1_theme.css"].clone()).unwrap();
        assert!(css.contains("@import \"4_extra.css\""), "{css}");
        assert!(css.contains("url(\"2_icon.png\")"), "{css}");
        assert!(css.contains("\"url(do-not-rewrite)\""));
        let frame = String::from_utf8(assets["3_index.html"].clone()).unwrap();
        assert!(frame.contains("src=\"2_icon.png\""), "{frame}");
        assert_eq!(assets["2_icon.png"], b"binary image");
    }
    #[test]
    fn html_only_retains_website_addresses_and_single_file_preserves_mime_bytes() {
        let mime = fixture();
        let result = export(&mime, Format::HtmlOnly, "Demo.html", "Demo_files").unwrap();
        assert!(result.assets.is_empty());
        let html = String::from_utf8(result.document).unwrap();
        for url in [
            "https://save.test/css/theme.css",
            "https://save.test/images/icon.png",
            "https://save.test/frame/index.html",
        ] {
            assert!(html.contains(url), "{html}");
        }
        let single = export(&mime, Format::SingleFile, "Demo.mhtml", "Demo_files").unwrap();
        assert_eq!(single.document, mime);
        assert!(single.assets.is_empty());
    }
    #[test]
    fn resource_names_cannot_escape_the_new_assets_directory() {
        let mime = String::from_utf8(fixture()).unwrap().replace(
            "https://save.test/images/icon.png",
            "https://save.test/../../../../evil.png",
        );
        let result = export(mime.as_bytes(), Format::Complete, "Demo.html", "Demo_files").unwrap();
        assert!(result
            .assets
            .iter()
            .all(|(name, _)| !name.contains('/') && !name.contains('\\') && name != ".."));
        assert!(export(&fixture(), Format::Complete, "Demo.html", "../outside").is_err());
        assert!(export(
            &fixture(),
            Format::Complete,
            "../outside.html",
            "Demo_files"
        )
        .is_err());
        assert!(export(
            b"not an archive",
            Format::Complete,
            "Demo.html",
            "Demo_files"
        )
        .is_err());
    }
    #[test]
    fn css_urls_inside_nested_functions_are_rewritten_without_touching_comments_or_strings() {
        let source = r##"/* url(no) */ a {background: image-set(url("a.png") 1x, url(b.png) 2x); content:"url(no)"}"##;
        let result = rewrite_css(source, &|value| format!("saved/{value}"));
        assert!(result.contains("url(\"saved/a.png\")"), "{result}");
        assert!(result.contains("url(\"saved/b.png\")"), "{result}");
        assert!(result.contains("/* url(no) */"));
        assert!(result.contains("content:\"url(no)\""));
    }
    #[test]
    fn a_multipart_start_selects_the_main_document_instead_of_an_earlier_frame() {
        let mime = String::from_utf8(fixture())
            .unwrap()
            .replace("start=\"<root>\"", "start=\"<child>\"");
        let result = export(
            mime.as_bytes(),
            Format::HtmlOnly,
            "Child.html",
            "Child_files",
        )
        .unwrap();
        let html = String::from_utf8(result.document).unwrap();
        assert!(!html.contains("<iframe"));
        assert!(html.contains("https://save.test/images/icon.png"));
    }
    #[test]
    fn titles_and_resource_directory_names_keep_unicode_without_path_components() {
        assert_eq!(
            suggested_name("Folder / 🧭 report", Format::SingleFile),
            "Folder _ 🧭 report.mhtml"
        );
        assert_eq!(suggested_name("...", Format::HtmlOnly), "Untitled.html");
        assert_eq!(assets_directory("A report.html"), "A report_files");
    }
}
