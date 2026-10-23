//! Lossless local-platform download paths. Foreign native encodings fail closed;
//! local-only sessions never transcode arbitrary bytes/surrogates through UTF-8.
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(tag = "encoding", content = "units", rename_all = "snake_case")]
enum Native {
    UnixBytes(Vec<u8>),
    WindowsWide(Vec<u16>),
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Wire {
    Text(String),
    Native(Native),
}
fn encode(value: &OsStr) -> Wire {
    if let Some(text) = value.to_str() {
        return Wire::Text(text.to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Wire::Native(Native::UnixBytes(value.as_bytes().to_vec()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        Wire::Native(Native::WindowsWide(value.encode_wide().collect()))
    }
}
fn decode<E: Error>(wire: Wire) -> Result<OsString, E> {
    match wire {
        Wire::Text(text) => Ok(text.into()),
        Wire::Native(Native::UnixBytes(bytes)) => {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStringExt;
                Ok(OsString::from_vec(bytes))
            }
            #[cfg(not(unix))]
            {
                let _ = bytes;
                Err(E::custom(
                    "Unix path encoding is unsupported on this platform",
                ))
            }
        }
        Wire::Native(Native::WindowsWide(wide)) => {
            #[cfg(windows)]
            {
                use std::os::windows::ffi::OsStringExt;
                Ok(OsString::from_wide(&wide))
            }
            #[cfg(not(windows))]
            {
                let _ = wide;
                Err(E::custom(
                    "Windows path encoding is unsupported on this platform",
                ))
            }
        }
    }
}
pub(super) mod path {
    use super::*;
    pub fn serialize<S: Serializer>(value: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        encode(value.as_os_str()).serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        decode::<D::Error>(Wire::deserialize(deserializer)?).map(PathBuf::from)
    }
}
pub(super) mod os_string {
    use super::*;
    pub fn serialize<S: Serializer>(value: &OsStr, serializer: S) -> Result<S::Ok, S::Error> {
        encode(value).serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<OsString, D::Error> {
        decode::<D::Error>(Wire::deserialize(deserializer)?)
    }
}
