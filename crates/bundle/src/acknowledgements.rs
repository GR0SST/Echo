//! Generate credits from the dependencies actually resolved for Echo, never a
//! reference browser's acknowledgements. Preserve upstream license text verbatim.
use std::{collections::BTreeMap, path::Path, process::Command};

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(super) fn install(app: &Path) {
    let compiler = Command::new("rustc")
        .arg("-vV")
        .output()
        .expect("cannot identify target platform");
    let compiler = String::from_utf8(compiler.stdout).expect("invalid rustc version");
    let host = compiler
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("missing compiler host");
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--offline",
            "--filter-platform",
            host,
        ])
        .output()
        .expect("cannot inspect dependency acknowledgements");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("invalid cargo metadata");
    let members: std::collections::HashSet<_> = metadata["workspace_members"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|id| id.as_str())
        .collect();
    let mut packages = BTreeMap::new();
    for package in metadata["packages"].as_array().unwrap() {
        if members.contains(package["id"].as_str().unwrap()) {
            continue;
        }
        packages.insert(
            format!(
                "{} {}",
                package["name"].as_str().unwrap(),
                package["version"].as_str().unwrap()
            ),
            package,
        );
    }
    let mut html = String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Acknowledgements</title></head><body><h1>Acknowledgements</h1><p>This project makes use of the following third party libraries:</p>");
    for (name, package) in packages {
        html.push_str(&format!(
            "<h2>{}</h2><p>License: {}</p>",
            escape(&name),
            escape(
                package["license"]
                    .as_str()
                    .unwrap_or("See upstream notices")
            )
        ));
        let manifest = Path::new(package["manifest_path"].as_str().unwrap());
        let directory = manifest.parent().unwrap();
        let mut files = std::collections::BTreeSet::new();
        if let Some(file) = package["license_file"].as_str() {
            files.insert(directory.join(file));
        }
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_ascii_uppercase();
                if entry.path().is_file()
                    && (name.starts_with("LICENSE")
                        || name.starts_with("COPYING")
                        || name.starts_with("NOTICE"))
                {
                    files.insert(entry.path());
                }
            }
        }
        for file in files {
            if let Ok(text) = std::fs::read_to_string(&file) {
                html.push_str(&format!(
                    "<h3>{}</h3><pre>{}</pre>",
                    escape(&file.file_name().unwrap().to_string_lossy()),
                    escape(&text)
                ));
            }
        }
    }
    for (name, text) in [
        ("Gemoji", include_str!("../../browser/assets/emoji/LICENSE")),
        (
            "Ionicons",
            include_str!("../../browser/assets/ionicons/LICENSE"),
        ),
    ] {
        html.push_str(&format!(
            "<h2>{}</h2><pre>{}</pre>",
            escape(name),
            escape(text)
        ));
    }
    html.push_str("</body></html>");
    let resources = app.join("Contents/Resources");
    std::fs::write(
        resources.join("Privacy.html"),
        include_str!("../../browser/assets/privacy.html"),
    )
    .expect("cannot install privacy policy");
    std::fs::write(resources.join("Acknowledgements.html"), html)
        .expect("cannot install acknowledgements");
    std::fs::copy(
        cef::sys::get_cef_dir().expect("CEF distribution path unavailable").join("CREDITS.html"),
        resources.join("ChromiumCredits.html"),
    )
    .expect("cannot install Chromium acknowledgements");
}

#[test]
fn upstream_notice_text_cannot_become_html() {
    assert_eq!(
        escape("<a href=\"x\">&'"),
        "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
    );
}
