//! Windows/Linux runtime staging. Platform installers are produced by Velopack.
use std::{fs, path::Path, process::Command};

pub(crate) fn run() {
    let release = std::env::args().any(|arg| arg == "--release");
    for name in ["Echo", "browser_helper"] {
        let mut command = Command::new("cargo");
        command.args(["build", "--locked", "--bin", name]);
        if release {
            command.arg("--release");
        }
        assert!(
            command.status().expect("cannot run cargo").success(),
            "native compilation failed"
        );
    }
    let out = Path::new("target/bundle/Echo");
    if out.exists() {
        fs::remove_dir_all(out).expect("cannot replace staging directory");
    }
    fs::create_dir_all(out).expect("cannot create staging directory");
    let cef = cef::sys::get_cef_dir().expect("CEF runtime directory");
    for entry in fs::read_dir(cef).expect("cannot read CEF distribution") {
        let entry = entry.expect("CEF entry");
        // CEF's headers/build system are SDK inputs, not runtime files.
        if matches!(
            entry.file_name().to_str(),
            Some(
                "include"
                    | "cmake"
                    | "libcef_dll"
                    | "CMakeLists.txt"
                    | "build.json"
                    | "qualification.json"
                    | "cleanup.json"
            )
        ) {
            continue;
        }
        copy_tree(&entry.path(), &out.join(entry.file_name())).expect("cannot copy CEF runtime");
    }
    let target_root = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "target".into());
    for name in ["Echo", "browser_helper"] {
        let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        fs::copy(
            target_root
                .join(if release { "release" } else { "debug" })
                .join(&file),
            out.join(file),
        )
        .expect("cannot copy browser executable");
    }
    let resources = out.join("resources");
    fs::create_dir_all(&resources).expect("cannot create resources");
    fs::write(
        resources.join("Privacy.html"),
        include_str!("../../browser/assets/privacy.html"),
    )
    .expect("cannot install privacy document");
    println!("Native bundle: {}", out.display());
}
fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else {
        fs::copy(source, destination)?;
    }
    Ok(())
}
