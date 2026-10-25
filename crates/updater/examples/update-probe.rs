//! Disposable old-build → new-build installer acceptance fixture; never browsers user data.
fn main() {
    echo_updater::startup();
    let version = option_env!("ECHO_PROBE_VERSION").unwrap_or("0.1.0");
    if version == "0.2.0" {
        let exe = std::env::current_exe().unwrap();
        let bundle = exe
            .ancestors()
            .find(|path| path.extension().is_some_and(|ext| ext == "app"))
            .unwrap();
        std::fs::write(bundle.parent().unwrap().join("restarted-version"), version).unwrap();
        return;
    }
    let ready = echo_updater::prepare()
        .expect("authenticated local update")
        .expect("new release");
    assert_eq!(ready.version(), "0.2.0");
    ready.arm_after_shutdown().expect("arm graceful installer");
    // Process exit models the completed browser quit barrier. The real browser
    // only reaches this operation after its engine and session shutdown gates.
}
