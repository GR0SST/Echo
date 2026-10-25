//! Load the macOS allocator-bearing framework before AppKit starts worker threads.
use cef::library_loader::LibraryLoader;
use std::{path::Path, sync::OnceLock};

// Never unload a process-wide allocator while dispatch/CEF workers can free data.
static FRAMEWORK: OnceLock<Result<LibraryLoader, String>> = OnceLock::new();

pub(super) fn preload() -> Result<bool, String> {
    unsafe extern "C" {
        fn pthread_main_np() -> std::ffi::c_int;
    }
    if unsafe { pthread_main_np() } == 0 {
        return Err("Chromium framework must be loaded on the main thread".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let Some(framework) = framework_path(&exe) else {
        return Ok(false);
    };
    if !framework.is_file() {
        return Err(format!(
            "Chromium framework is incomplete: {}",
            framework.display()
        ));
    }
    FRAMEWORK
        .get_or_init(|| {
            let loader = LibraryLoader::new(&exe, false);
            if loader.load() {
                Ok(loader)
            } else {
                Err("cannot load Chromium Embedded Framework".into())
            }
        })
        .as_ref()
        .map(|_| true)
        .map_err(Clone::clone)
}

fn framework_path(exe: &Path) -> Option<std::path::PathBuf> {
    let root = exe
        .parent()?
        .parent()?
        .join("Frameworks/Chromium Embedded Framework.framework");
    root.exists()
        .then(|| root.join("Chromium Embedded Framework"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framework_preload_rejects_a_worker_before_touching_the_allocator() {
        let result = std::thread::spawn(preload).join().unwrap();
        assert_eq!(
            result.unwrap_err(),
            "Chromium framework must be loaded on the main thread"
        );
    }
    #[test]
    fn absent_framework_keeps_unbundled_ui_available() {
        let root = std::env::temp_dir().join(format!("echo-framework-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("MacOS")).unwrap();
        assert!(framework_path(&root.join("MacOS/Echo")).is_none());
        let dir = root.join("Frameworks/Chromium Embedded Framework.framework");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            framework_path(&root.join("MacOS/Echo")),
            Some(dir.join("Chromium Embedded Framework"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
