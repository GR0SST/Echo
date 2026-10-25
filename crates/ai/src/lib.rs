//! User-owned AI. No Echo account, proxy, telemetry, or credential files.
//! The model/configuration API builds without HTTP or native credential stores.
mod config;
pub use config::*;
mod links;
pub use links::WebLink;
#[cfg(feature = "http")]
pub mod search;
#[cfg(feature = "http")]
pub mod client;
#[cfg(feature = "http")]
pub mod document;
pub mod credentials;
#[cfg(feature = "http")]
pub mod oauth;
#[cfg(feature = "http")]
mod sse;
