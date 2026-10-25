//! The current packaging utility assembles macOS application bundles.
#[cfg(target_os = "macos")]
mod acknowledgements;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod cef_dist;

fn main() {
    #[cfg(target_os = "macos")]
    macos::run();
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("The bundle utility currently supports macOS only; Windows/Linux packaging is not implemented.");
        std::process::exit(2);
    }
}
