use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!("echo-session-startup-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn path(&self) -> PathBuf { self.0.join("session.json") }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

#[gpui::test]
fn invalid_workspace_never_produces_a_normal_owner_or_overwrites_the_source(cx: &mut gpui::TestAppContext) {
    for json in [
        r#"{"format_version":2,"profiles":{},"default_user_profile_id":1,"next_user_profile_id":2,"spaces":[]}"#,
        r#"{"spaces":[{"user_profile_id":2}]}"#,
        "{truncated",
    ] {
        let fixture = Fixture::new();
        let path = fixture.path();
        std::fs::write(&path, json).unwrap();
        cx.update(|cx| {
            assert!(new_window::source_from_session(&path, cx).is_err());
            assert!(cx.try_global::<new_window::NormalProfile>().is_none());
            assert!(cx.windows().is_empty());
        });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), json);
        assert!(!path.with_extension("json.broken").exists());
        assert!(!path.with_extension("json.tmp").exists());
        assert!(!fixture.0.join("Chromium").exists());
    }
}

#[test]
fn only_missing_session_is_first_launch_and_existing_unreadable_state_is_preserved() {
    let fixture = Fixture::new();
    let path = fixture.path();
    assert!(!load_session(&path).unwrap().spaces.is_empty());
    assert!(!path.exists());
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("keep"), "original").unwrap();
    assert!(load_session(&path).is_err());
    assert_eq!(std::fs::read_to_string(path.join("keep")).unwrap(), "original");
    assert!(!path.with_extension("json.broken").exists());
}
