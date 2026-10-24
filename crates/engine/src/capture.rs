//! Live source access is independent of audible playback and saved permissions.
//! CEF invokes the display/lifespan callbacks on its UI thread.
use std::{cell::RefCell, collections::HashMap};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MediaCaptureState {
    pub video: bool,
    pub audio: bool,
}

#[derive(Default)]
struct Registry(HashMap<i32, MediaCaptureState>);
impl Registry {
    fn created(&mut self, browser: i32) { self.0.insert(browser, MediaCaptureState::default()); }
    fn changed(&mut self, browser: i32, state: MediaCaptureState) -> bool {
        let Some(current) = self.0.get_mut(&browser) else { return false; };
        if *current == state { return false; }
        *current = state;
        true
    }
    fn current(&self, browser: i32) -> MediaCaptureState { self.0.get(&browser).copied().unwrap_or_default() }
    fn destroyed(&mut self, browser: i32) { self.0.remove(&browser); }
}

thread_local! { static STATES: RefCell<Registry> = RefCell::default(); }
pub(crate) fn created(browser: i32) { STATES.with_borrow_mut(|states| states.created(browser)); }
pub(crate) fn changed(browser: i32, state: MediaCaptureState) -> bool { STATES.with_borrow_mut(|states| states.changed(browser, state)) }
pub(crate) fn current(browser: i32) -> MediaCaptureState { STATES.with_borrow(|states| states.current(browser)) }
pub(crate) fn destroyed(browser: i32) { STATES.with_borrow_mut(|states| states.destroyed(browser)); }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_transitions_are_independent_per_browser_and_duplicate_callbacks_are_quiet() {
        let mut states = Registry::default();
        states.created(1); states.created(2);
        for state in [
            MediaCaptureState { audio: true, video: false },
            MediaCaptureState { audio: false, video: true },
            MediaCaptureState { audio: true, video: true },
            MediaCaptureState::default(),
        ] {
            assert!(states.changed(1, state));
            assert!(!states.changed(1, state));
            assert_eq!(states.current(1), state);
            assert_eq!(states.current(2), MediaCaptureState::default());
        }
    }

    #[test]
    fn closed_browsers_reject_late_access_and_recreated_ids_start_clean() {
        let mut states = Registry::default();
        let active = MediaCaptureState { video: true, audio: true };
        assert!(!states.changed(1, active));
        states.created(1);
        assert!(states.changed(1, active));
        states.destroyed(1);
        assert!(!states.changed(1, active));
        states.created(1);
        assert_eq!(states.current(1), MediaCaptureState::default());
    }
}
