//! Select the optional machine-local CEF without changing the portable default.
use std::path::{Path, PathBuf};

const VERSION: &str = "154.0.34+g14c5a08+chromium-154.0.8037.98";

pub(crate) fn select() -> Result<Option<PathBuf>, String> {
    let explicit = std::env::var_os("ECHO_CEF_DIST").filter(|s| !s.is_empty());
    let fallback = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/cef");
    if explicit.is_none() {
        if let Some(configured) = std::env::var_os("CEF_PATH") {
            if std::fs::canonicalize(&configured).ok() != std::fs::canonicalize(&fallback).ok() {
                return Ok(None); // Preserve an explicit CEF_PATH override.
            }
        }
    }
    let candidate = explicit.clone().map(PathBuf::from).or_else(|| {
        // This distribution is macOS arm64 only. Never select it for another host.
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            std::env::var_os("HOME").map(|home| PathBuf::from(home)
                .join("Documents/Bot/cef-src/echo-extensions-dist"))
        } else { None }
    });
    let Some(path) = candidate else { return Ok(None); };
    if explicit.is_none() && !path.exists() { return Ok(None); }
    validate(&path)?;
    Ok(Some(path.canonicalize().map_err(|e| e.to_string())?))
}

fn validate(path: &Path) -> Result<(), String> {
    let check = || -> Result<(), Box<dyn std::error::Error>> {
        let header = std::fs::read_to_string(path.join("include/cef_version.h"))?;
        if !header.contains(&format!("#define CEF_VERSION \"{VERSION}\"")) {
            return Err("CEF version mismatch".into());
        }
        let archive: serde_json::Value = serde_json::from_slice(&std::fs::read(path.join("archive.json"))?)?;
        if archive["type"] != "minimal" || archive["name"] != format!("cef_binary_{VERSION}_macosarm64_minimal.tar.bz2") {
            return Err("CEF archive metadata mismatch".into());
        }
        for entry in ["CMakeLists.txt", "cmake", "libcef_dll",
            "Chromium Embedded Framework.framework/Chromium Embedded Framework"] {
            std::fs::metadata(path.join(entry))?;
        }
        Ok(())
    };
    check().map_err(|e| format!("Invalid local CEF distribution {}: {e}", path.display()))
}
