//! Builds `target/bundle/Echo.app`: the browser executable, Chromium
//! Embedded Framework and the helper apps CEF needs on macOS.
//!
//! Usage: `cargo run -p bundle`, then `open target/bundle/Echo.app`.
//! Set `ECHO_SIGN_IDENTITY` (and `ECHO_NOTARY_PROFILE`) to sign; see `sign.rs`.

use std::path::Path;
use crate::acknowledgements;

use cef::build_util::mac::{build_bundle, bundle, BundleInfo};

pub(super) fn run() {
    // Cargo's configured CEF_PATH remains the CDN fallback. The bundle utility
    // selects before its nested Cargo builds and runtime framework resolution.
    match crate::cef_dist::select() {
        Ok(Some(path)) => {
            eprintln!("Using local Echo CEF: {}", path.display());
            std::env::set_var("CEF_PATH", path);
        }
        Ok(None) => {}
        Err(error) => { eprintln!("{error}"); std::process::exit(1); }
    }
    let info = BundleInfo::new(
        "Echo",
        "dev.echo.browser",
        "Echo",
        "English",
        semver::Version::parse(&std::env::var("ECHO_RELEASE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into())).expect("invalid ECHO_RELEASE_VERSION"),
    );
    // Replace, never overwrite in place: macOS kills processes whose mapped
    // executable changes underneath them (e.g. a running copy of the app).
    let app = Path::new("target/bundle/Echo.app");
    if app.exists() {
        std::fs::remove_dir_all(app).expect("cannot remove the old bundle");
    }
    let release = std::env::args().any(|arg| arg == "--release");
    let result = if release {
        for executable in ["Echo", "browser_helper"] {
            let status = std::process::Command::new("cargo").args(["build", "--locked", "--release", "--bin", executable]).status().expect("cannot run cargo");
            assert!(status.success(), "release compilation failed");
        }
        bundle(Path::new("target/bundle"), &std::env::var_os("CARGO_TARGET_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| "target".into()).join("release"), "Echo", "browser_helper", None, info)
    } else { build_bundle(Path::new("target/bundle"), "Echo", info) };
    match result {
        Ok(path) => {
            install_version(&path);
            install_icon(&path);
            install_scripting(&path);
            install_icon_notices(&path);
            acknowledgements::install(&path);
            crate::sign::run(&path);
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

fn install_version(app: &Path) {
    let version = std::env::var("ECHO_RELEASE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into());
    let build = std::env::var("ECHO_BUILD_NUMBER").unwrap_or_else(|_| "1".into());
    assert!(!build.is_empty() && build.bytes().all(|b| b.is_ascii_digit()), "ECHO_BUILD_NUMBER must be numeric");
    for (key, value) in [("CFBundleShortVersionString", version), ("CFBundleVersion", build)] {
        assert!(std::process::Command::new("plutil").args(["-replace", key, "-string", &value])
            .arg(app.join("Contents/Info.plist")).status().expect("cannot set bundle version").success());
    }
}
