# Echo compatibility patch

Source: crates.io `xattr` 0.2.3, originally published by Steven Allen.
Original repository: https://github.com/Stebalien/xattr

GPUI 0.2.2 depends on zed-async-tar 0.5.0-zed, which requires xattr 0.2.
This copy changes only `src/sys/mod.rs`: Linux/Android use libc::ENODATA
for missing attributes, while macOS/BSD retain libc::ENOATTR. Current libc
no longer exports the Linux alias. No attribute handling is disabled.
Remove this patch when the transitive dependency moves to a maintained xattr.
The original MIT and Apache-2.0 licenses, source and tests are preserved.
