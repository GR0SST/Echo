//! Builds `target/bundle/Echo.app`: the browser executable, Chromium
//! Embedded Framework and the helper apps CEF needs on macOS.
//!
//! Usage: `cargo run -p bundle`, then `open target/bundle/Echo.app`.

use std::path::Path;

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
            println!("Bundle: {}", path.display());
        }
        Err(error) => {
            eprintln!("Bundling failed: {error}");
            std::process::exit(1);
        }
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
}
