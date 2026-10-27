//! Developer ID signing and notarization for `Echo.app`.
//!
//! `ECHO_SIGN_IDENTITY="Developer ID Application: …"` signs the bundle inside
//! out with the hardened runtime; `ECHO_NOTARY_PROFILE=<notarytool profile>`
//! then notarizes it, staples the ticket and writes `target/bundle/Echo.zip`.
//! Without the identity the bundle stays unsigned, as dev builds always were.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Must match `engine::DEVELOPER_ID_SIGNED_KEY`.
const MARKER: &str = "EchoDeveloperIDSigned";

/// V8 and SwiftShader generate code at run time; Chromium's helpers need the
/// same exceptions under the hardened runtime as in Chrome's own signing.
const JIT: &[&str] = &[
    "com.apple.security.cs.allow-jit",
    "com.apple.security.cs.allow-unsigned-executable-memory",
];
const PLUGIN: &[&str] = &[
    "com.apple.security.cs.allow-jit",
    "com.apple.security.cs.allow-unsigned-executable-memory",
    "com.apple.security.cs.disable-library-validation",
];
/// The browser process runs V8 too and asks for site devices on the user's behalf.
const APP: &[&str] = &[
    "com.apple.security.cs.allow-jit",
    "com.apple.security.cs.allow-unsigned-executable-memory",
    "com.apple.security.device.audio-input",
    "com.apple.security.device.camera",
    "com.apple.security.device.bluetooth",
    "com.apple.security.device.usb",
    "com.apple.security.personal-information.location",
    "com.apple.security.print",
];

fn entitlements(keys: &[&str]) -> String {
    let body: String = keys.iter().map(|key| format!("\t<key>{key}</key>\n\t<true/>\n")).collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n{body}</dict>\n</plist>\n"
    )
}

/// Entitlements for each CEF helper, keyed by its bundle name suffix.
fn helper_entitlements(name: &str) -> &'static [&'static str] {
    match name {
        "Echo Helper (Renderer).app" | "Echo Helper (GPU).app" => JIT,
        "Echo Helper (Plugin).app" => PLUGIN,
        _ => &[],
    }
}

fn codesign(identity: &str, path: &Path, entitlements_file: Option<&Path>) {
    let mut command = Command::new("codesign");
    command.args(["--force", "--timestamp", "--options", "runtime", "--sign", identity]);
    if let Some(file) = entitlements_file {
        command.arg("--entitlements").arg(file);
    }
    let status = command.arg(path).status().expect("cannot run codesign");
    assert!(status.success(), "cannot sign {}", path.display());
}

pub(super) fn run(app: &Path) {
    let Ok(identity) = std::env::var("ECHO_SIGN_IDENTITY") else { return };
    let scratch = std::env::temp_dir().join(format!("echo-entitlements-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("cannot create entitlements directory");
    let write = |name: &str, keys: &[&str]| -> Option<PathBuf> {
        if keys.is_empty() {
            return None;
        }
        let path = scratch.join(format!("{name}.plist"));
        std::fs::write(&path, entitlements(keys)).expect("cannot write entitlements");
        Some(path)
    };

    let frameworks = app.join("Contents/Frameworks");
    let cef = frameworks.join("Chromium Embedded Framework.framework");
    let mut libraries: Vec<_> = std::fs::read_dir(cef.join("Libraries"))
        .expect("cannot read CEF libraries")
        .map(|entry| entry.expect("cannot read CEF libraries").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "dylib"))
        .collect();
    libraries.sort();
    for library in &libraries {
        codesign(&identity, library, None);
    }
    codesign(&identity, &cef, None);

    let mut helpers: Vec<_> = std::fs::read_dir(&frameworks)
        .expect("cannot read Frameworks")
        .map(|entry| entry.expect("cannot read Frameworks").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "app"))
        .collect();
    helpers.sort();
    for helper in &helpers {
        let name = helper.file_name().unwrap().to_string_lossy();
        let file = write(&name, helper_entitlements(&name));
        codesign(&identity, helper, file.as_deref());
    }
    // Sealed by the signature below; tells the engine to use the real keychain.
    let status = Command::new("plutil").args(["-replace", MARKER, "-bool", "true"])
        .arg(app.join("Contents/Info.plist")).status().expect("cannot run plutil");
    assert!(status.success(), "cannot mark the bundle as signed");
    let file = write("Echo", APP);
    codesign(&identity, app, file.as_deref());
    let _ = std::fs::remove_dir_all(&scratch);

    let status = Command::new("codesign").args(["--verify", "--deep", "--strict", "--verbose=2"]).arg(app)
        .status().expect("cannot run codesign");
    assert!(status.success(), "signature verification failed");
    println!("Signed: {identity}");

    if let Ok(profile) = std::env::var("ECHO_NOTARY_PROFILE") {
        notarize(app, &profile);
    }
}

fn zip(app: &Path, archive: &Path) {
    let _ = std::fs::remove_file(archive);
    let status = Command::new("ditto").args(["-c", "-k", "--sequesterRsrc", "--keepParent"]).arg(app).arg(archive)
        .status().expect("cannot run ditto");
    assert!(status.success(), "cannot archive the bundle");
}

fn notarize(app: &Path, profile: &str) {
    let archive = app.with_extension("zip");
    zip(app, &archive);
    let status = Command::new("xcrun")
        .args(["notarytool", "submit"]).arg(&archive)
        .args(["--keychain-profile", profile, "--wait"])
        .status().expect("cannot run notarytool");
    assert!(status.success(), "notarization failed; see `xcrun notarytool log <id> --keychain-profile {profile}`");
    let status = Command::new("xcrun").args(["stapler", "staple"]).arg(app).status().expect("cannot run stapler");
    assert!(status.success(), "cannot staple the notarization ticket");
    // Re-archive so the shipped zip carries the stapled ticket.
    zip(app, &archive);
    println!("Notarized: {}", archive.display());
}

#[test]
fn entitlements_are_a_plist_of_true_keys() {
    let plist = entitlements(JIT);
    assert!(plist.contains("<key>com.apple.security.cs.allow-jit</key>\n\t<true/>"));
    assert!(plist.ends_with("</dict>\n</plist>\n"));
    assert!(helper_entitlements("Echo Helper.app").is_empty());
}
