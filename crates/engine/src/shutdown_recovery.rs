//! Verify an interrupted engine shutdown before reopening its persistent stores.
use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(crate) const MARKER: &str = "echo-dirty-shutdown.json";
const RECEIPT: &str = "echo-shutdown-recovery.json";

#[repr(C)]
#[derive(Default)]
struct Integrity {
    size: u32,
    sqlite_databases: u32,
    leveldb_databases: u32,
    failures: u32,
    entries_checked: u64,
}
type Verify = unsafe extern "C" fn(
    *const c_char,
    *mut Integrity,
    unsafe extern "C" fn(*mut c_void, *const c_char),
    *mut c_void,
) -> u32;
unsafe extern "C" fn diagnostic(data: *mut c_void, text: *const c_char) {
    if !text.is_null() {
        unsafe { &mut *(data.cast::<Vec<String>>()) }.push(
            unsafe { CStr::from_ptr(text) }
                .to_string_lossy()
                .into_owned(),
        );
    }
}

fn library_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    return Ok(exe
        .parent()
        .and_then(Path::parent)
        .ok_or("Missing application bundle")?
        .join("Frameworks/Chromium Embedded Framework.framework/Chromium Embedded Framework"));
    #[cfg(target_os = "windows")]
    return Ok(exe
        .parent()
        .ok_or("Missing executable directory")?
        .join("libcef.dll"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return Ok(exe
        .parent()
        .ok_or("Missing executable directory")?
        .join("libcef.so"));
}

/// A separate headless process owns Chromium's SQLite/LevelDB verifier. It exits
/// before CEF or GPUI initialization, so no live profile holds database locks.
pub fn run_verifier_process() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()?.as_os_str() != "--echo-verify-storage" {
        return None;
    }
    let result = (|| -> Result<serde_json::Value, String> {
        let root = PathBuf::from(args.next().ok_or("Missing storage root")?);
        if args.next().is_some()
            || !root.is_absolute()
            || std::fs::canonicalize(&root).map_err(|e| e.to_string())? != root
        {
            return Err("Storage verification requires one canonical absolute root".into());
        }
        #[cfg(target_os = "macos")]
        if !crate::preload_framework()? {
            return Err("CEF framework is missing".into());
        }
        let root = CString::new(crate::storage::utf8_path(&root)?).map_err(|e| e.to_string())?;
        let mut info = Integrity {
            size: std::mem::size_of::<Integrity>() as u32,
            ..Default::default()
        };
        let mut failures = Vec::<String>::new();
        let status = unsafe {
            let library = libloading::Library::new(library_path()?).map_err(|e| e.to_string())?;
            let verify = library.get::<Verify>(b"cef_echo_verify_storage_v1\0").map_err(|_| "This CEF cannot verify an interrupted shutdown; retain the profile and use the source-built distribution")?;
            verify(
                root.as_ptr(),
                &mut info,
                diagnostic,
                (&mut failures as *mut Vec<String>).cast(),
            )
        };
        Ok(
            serde_json::json!({"passed":status == 0 && info.failures == 0,"status":status,
            "sqlite_databases":info.sqlite_databases,"leveldb_databases":info.leveldb_databases,
            "entries_checked":info.entries_checked,"failures":failures}),
        )
    })();
    match result {
        Ok(report) => {
            println!("{report}");
            Some(if report["passed"] == true { 0 } else { 1 })
        }
        Err(error) => {
            println!("{}", serde_json::json!({"passed":false,"error":error}));
            Some(1)
        }
    }
}

fn verify_in_child(root: &Path) -> Result<serde_json::Value, String> {
    let output = root.join(format!(
        ".echo-storage-verification-{}.json",
        std::process::id()
    ));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--echo-verify-storage")
            .arg(root)
            .stdout(Stdio::from(file))
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Storage integrity verification exceeded its 60-second limit; dirty-shutdown marker retained".into());
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let bytes = std::fs::read(&output).map_err(|e| e.to_string())?;
        let parsed = serde_json::from_slice::<serde_json::Value>(&bytes);
        if !status.success() {
            return Err(match parsed {
                Ok(report) => format!(
                    "Storage integrity verification failed; profile and marker retained: {report}"
                ),
                Err(_) => format!(
                    "Storage verifier exited {status}; profile and dirty-shutdown marker retained"
                ),
            });
        }
        let report = parsed
            .map_err(|e| format!("Invalid storage verification report; marker retained: {e}"))?;
        if report["passed"] != true {
            return Err(format!(
                "Storage integrity verification failed; profile and marker retained: {report}"
            ));
        }
        Ok(report)
    })();
    let _ = std::fs::remove_file(output);
    result
}

pub(crate) fn recover(root: &Path) -> Result<(), String> {
    recover_with(root, verify_in_child)
}
fn recover_with(
    root: &Path,
    verify: impl FnOnce(&Path) -> Result<serde_json::Value, String>,
) -> Result<(), String> {
    let marker = root.join(MARKER);
    let metadata = match std::fs::symlink_metadata(&marker) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 65536 {
        return Err("Invalid dirty-shutdown marker; profile retained".into());
    }
    let previous: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&marker).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Invalid dirty-shutdown marker: {e}"))?;
    if previous["schema"] != 1 {
        return Err("Unsupported dirty-shutdown marker; profile retained".into());
    }
    eprintln!(
        "[engine-recovery] previous network shutdown was incomplete; verifying cookie and site storage"
    );
    let integrity = verify(root)?;
    if integrity["passed"] != true {
        return Err("Storage integrity did not pass; profile and marker retained".into());
    }
    let report = serde_json::json!({"schema":1,"previous_shutdown":previous,"integrity":integrity});
    let temporary = root.join(format!(".echo-recovery-{}.tmp", std::process::id()));
    let result = (|| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(report.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, root.join(RECEIPT)).map_err(|e| e.to_string())?;
        std::fs::remove_file(&marker).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        std::fs::File::open(root)
            .and_then(|file| file.sync_all())
            .map_err(|e| e.to_string())?;
        eprintln!(
            "[engine-recovery] cookie and site-storage integrity verified; receipt={}",
            root.join(RECEIPT).display()
        );
        Ok(())
    })();
    let _ = std::fs::remove_file(temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "echo-recovery-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn clean_start_never_runs_a_verifier() {
        let root = root();
        recover_with(&root, |_| panic!("clean start must not inspect stores")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn failed_verification_retains_marker_and_every_store() {
        let root = root();
        std::fs::write(root.join(MARKER), r#"{"schema":1}"#).unwrap();
        std::fs::write(root.join("Cookies"), b"sentinel").unwrap();
        assert!(recover_with(&root, |_| Err("corrupt fixture".into())).is_err());
        assert!(root.join(MARKER).exists());
        assert!(!root.join(RECEIPT).exists());
        assert_eq!(std::fs::read(root.join("Cookies")).unwrap(), b"sentinel");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn verified_start_archives_reason_and_only_then_clears_marker() {
        let root = root();
        std::fs::write(root.join(MARKER), r#"{"schema":1,"network_pid":42}"#).unwrap();
        recover_with(&root, |_| {
            Ok(serde_json::json!({"passed":true,"sqlite_databases":1,"leveldb_databases":2}))
        })
        .unwrap();
        assert!(!root.join(MARKER).exists());
        let receipt: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join(RECEIPT)).unwrap()).unwrap();
        assert_eq!(receipt["previous_shutdown"]["network_pid"], 42);
        assert_eq!(receipt["integrity"]["leveldb_databases"], 2);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_marker_cannot_be_erased_by_a_successful_verifier() {
        let root = root();
        std::fs::write(root.join(MARKER), b"incomplete").unwrap();
        assert!(
            recover_with(&root, |_| panic!(
                "invalid marker must stop before native IO"
            ))
            .is_err()
        );
        assert!(root.join(MARKER).exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
