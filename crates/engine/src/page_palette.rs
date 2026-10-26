//! Opt-in Space colors exposed to the main web document, as in Arc.
use cef::*;

pub const VARIABLES: [&str; 12] = [
    "--arc-palette-foregroundPrimary",
    "--arc-palette-foregroundSecondary",
    "--arc-palette-foregroundTertiary",
    "--arc-palette-maxContrastColor",
    "--arc-palette-minContrastColor",
    "--arc-palette-backgroundExtra",
    "--arc-palette-background",
    "--arc-palette-focus",
    "--arc-palette-hover",
    "--arc-palette-cutoutColor",
    "--arc-palette-title",
    "--arc-palette-subtitle",
];
/// sRGB RRGGBBAA values in VARIABLES order. Contains no profile or document data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagePalette(pub [u32; 12]);

pub(crate) fn apply(browser: &Browser, palette: Option<PagePalette>) -> bool {
    let Some(frame) = browser.main_frame() else {
        return false;
    };
    let url = CefString::from(&frame.url()).to_string();
    if !url::Url::parse(&url).is_ok_and(|url| matches!(url.scheme(), "http" | "https")) {
        return true;
    }
    let css = palette
        .map(|palette| {
            let properties = VARIABLES
                .iter()
                .zip(palette.0)
                .map(|(name, color)| format!("{name}:#{color:08X};"))
                .collect::<String>();
            format!(":root{{{properties}}}")
        })
        .unwrap_or_default();
    let script = format!(
        r#"(() => {{
const key = Symbol.for('echo.space.palette.sheet');
const css = {};
let sheet = document[key];
if (!css) {{
 if (sheet) {{ document.adoptedStyleSheets = document.adoptedStyleSheets.filter(value => value !== sheet); delete document[key]; }}
 return;
}}
if (!sheet) {{ sheet = new CSSStyleSheet(); document[key] = sheet; }}
sheet.replaceSync(css);
if (!document.adoptedStyleSheets.includes(sheet)) {{ document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet]; }}
}})();"#,
        serde_json::to_string(&css).unwrap()
    );
    frame.execute_java_script(
        Some(&CefString::from(script.as_str())),
        Some(&CefString::from("echo-space-palette")),
        1,
    );
    true
}
