//! User-owned AI. No Echo account, proxy, telemetry, or credential files.
//! The model/configuration API builds without HTTP or native credential stores.
mod config;
pub use config::*;
#[cfg(feature = "http")]
pub mod client;
pub mod credentials;
#[cfg(feature = "http")]
pub mod oauth;
#[cfg(feature = "http")]
mod sse;
