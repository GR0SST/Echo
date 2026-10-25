//! The current packaging utility assembles macOS application bundles.
#[cfg(target_os = "macos")]
mod acknowledgements;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod cef_dist;

#[cfg(any(not(target_os = "macos"), test))]
mod native_bundle;

fn main() {
    #[cfg(target_os = "macos")]
    macos::run();
    #[cfg(not(target_os = "macos"))]
    native_bundle::run();
}
