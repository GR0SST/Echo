//! Builds `target/bundle/Echo.app`: the browser executable, Chromium
//! Embedded Framework and the helper apps CEF needs on macOS.
//!
//! Usage: `cargo run -p bundle`, then `open target/bundle/Echo.app`.

use std::path::Path;
mod acknowledgements;

use cef::build_util::mac::{build_bundle, BundleInfo};

fn main() {
    let info = BundleInfo::new(
        "Echo",
        "dev.echo.browser",
        "Echo",
        "English",
        semver::Version::new(0, 1, 0),
    );
    // Replace, never overwrite in place: macOS kills processes whose mapped
    // executable changes underneath them (e.g. a running copy of the app).
    let app = Path::new("target/bundle/Echo.app");
    if app.exists() {
        std::fs::remove_dir_all(app).expect("cannot remove the old bundle");
    }
    match build_bundle(Path::new("target/bundle"), "Echo", info) {
        Ok(path) => {
            install_icon(&path);
            install_scripting(&path);
            install_icon_notices(&path);
            acknowledgements::install(&path);
            println!("Bundle: {}", path.display());
        }
        Err(error) => {
            eprintln!("Bundling failed: {error}");
            std::process::exit(1);
        }
    }
}

fn install_icon_notices(app: &Path) {
    let notices = app.join("Contents/Resources/ThirdPartyNotices");
    std::fs::create_dir_all(&notices).expect("cannot create third-party notices");
    for (name, text) in [
        ("Gemoji.txt", include_str!("../../browser/assets/emoji/LICENSE")),
        ("Ionicons.txt", include_str!("../../browser/assets/ionicons/LICENSE")),
    ] {
        std::fs::write(notices.join(name), text).expect("cannot install icon catalog license");
    }
}

/// Adds the app icon (`assets/Echo.icns`, built by `scripts/make_icon.py`).
fn install_icon(app: &Path) {
    let icon = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/Echo.icns");
    let resources = app.join("Contents/Resources");
    std::fs::create_dir_all(&resources).expect("cannot create the bundle's Resources");
    std::fs::copy(&icon, resources.join("Echo.icns")).expect("cannot copy the app icon");
    let plist = app.join("Contents/Info.plist");
    let status = std::process::Command::new("plutil")
        .args(["-replace", "CFBundleIconFile", "-string", "Echo"])
        .arg(&plist)
        .status()
        .expect("cannot run plutil");
    assert!(status.success(), "cannot set the bundle icon");
    for (key, description) in [
        ("NSCameraUsageDescription", "Websites may use your camera for features on their site."),
        ("NSMicrophoneUsageDescription", "Websites may use your microphone for features on their site."),
        ("NSLocationUsageDescription", "Websites may use your location for features on their site."),
        ("NSLocationWhenInUseUsageDescription", "Websites may use your location for features on their site."),
    ] {
        let status = std::process::Command::new("plutil").args(["-replace",key,"-string",description])
            .arg(&plist).status().expect("cannot set site permission description");
        assert!(status.success(), "cannot set site permission description");
    }
    let status = std::process::Command::new("plutil")
        .args(["-insert", "CFBundleURLTypes", "-json",
            r#"[{"CFBundleURLName":"Web URLs","CFBundleTypeRole":"Viewer","CFBundleURLSchemes":["http","https"]}]"#])
        .arg(&plist).status().expect("cannot register web URL handling");
    assert!(status.success(), "cannot register web URL handling");
}

fn install_scripting(app: &Path) {
    let resources = app.join("Contents/Resources");
    std::fs::write(resources.join("Echo.sdef"), include_bytes!("../assets/Echo.sdef")).expect("cannot install scripting dictionary");
    let plist = app.join("Contents/Info.plist");
    for (key, kind, value) in [("NSAppleScriptEnabled", "-bool", "true"), ("OSAScriptingDefinition", "-string", "Echo.sdef")] {
        let status = std::process::Command::new("plutil").args(["-replace", key, kind, value]).arg(&plist).status().expect("cannot register scripting dictionary");
        assert!(status.success(), "cannot register scripting dictionary");
    }
}
