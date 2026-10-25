//! Website/browser arbitration, independent of CEF and native UI lifetimes.
use crate::Shortcut;
use std::{
    collections::HashMap,
    ops::Deref,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShortcutPriority {
    PreferWebsites,
    UseBoth,
    #[default]
    PreferBrowser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EventIdentity(pub u64, pub u64);
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EventStamp {
    pub identity: Option<EventIdentity>,
    pub milliseconds: Option<u32>,
}

pub(crate) struct Configuration {
    keys: Vec<Shortcut>,
    priorities: Vec<(Shortcut, ShortcutPriority)>,
    pub revision: u64,
}
impl Configuration {
    pub fn new(keys: Vec<Shortcut>) -> Self {
        Self {
            keys,
            priorities: Vec::new(),
            revision: 0,
        }
    }
    pub fn set_keys(&mut self, keys: Vec<Shortcut>) {
        if self.keys == keys {
            return;
        }
        self.keys = keys;
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn set_priorities(&mut self, priorities: Vec<(Shortcut, ShortcutPriority)>) {
        if self.priorities == priorities {
            return;
        }
        self.priorities = priorities;
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn priority(&self, key: &Shortcut) -> ShortcutPriority {
        self.priorities
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, policy)| *policy)
            .unwrap_or_default()
    }
    #[cfg(test)]
    pub fn clear(&mut self) {
        self.set_keys(Vec::new());
    }
}
impl Deref for Configuration {
    type Target = [Shortcut];
    fn deref(&self) -> &Self::Target {
        &self.keys
    }
}

// Arc's second-press window is independent of the system double-click interval.
// Native boundary qualification is recorded in PROFILES_SETTINGS_REFERENCE.md.
pub(crate) const DOUBLE_PRESS: Duration = Duration::from_secs(6);
struct Press {
    key: Shortcut,
    stamp: EventStamp,
    at: Instant,
}
struct Pending {
    key: Shortcut,
    forced: bool,
}
#[derive(Default)]
pub(crate) struct State {
    revision: u64,
    last: Option<Press>,
    pending: HashMap<EventIdentity, Pending>,
    synthetic: Option<Pending>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Before {
    Browser,
    Website,
    Both,
}
impl State {
    pub fn reset(&mut self) {
        self.last = None;
        self.pending.clear();
        self.synthetic = None;
    }
    fn revision(&mut self, revision: u64) {
        if self.revision != revision {
            self.reset();
            self.revision = revision;
        }
    }
    pub fn before(
        &mut self,
        revision: u64,
        key: Shortcut,
        policy: ShortcutPriority,
        stamp: EventStamp,
        repeat: bool,
        now: Instant,
    ) -> Before {
        self.revision(revision);
        // A busy renderer can acknowledge an unhandled key much later. Bound
        // retained events by count rather than silently dropping its fallback
        // because the renderer took longer than an arbitrary timeout.
        if self.pending.len() >= 128 {
            self.pending.clear();
        }
        if policy == ShortcutPriority::PreferBrowser {
            return Before::Browser;
        }
        let forced = policy == ShortcutPriority::UseBoth
            && !repeat
            && self.last.as_ref().is_some_and(|last| {
                let elapsed = match (stamp.milliseconds, last.stamp.milliseconds) {
                    (Some(now), Some(previous)) => {
                        Duration::from_millis(now.wrapping_sub(previous) as u64)
                    }
                    _ => now.saturating_duration_since(last.at),
                };
                last.key == key && elapsed <= DOUBLE_PRESS
            });
        if policy == ShortcutPriority::UseBoth && !repeat && !forced {
            self.last = Some(Press {
                key: key.clone(),
                stamp,
                at: now,
            });
        } else if forced {
            self.last = None;
        }
        let pending = Pending { key, forced };
        if let Some(identity) = stamp.identity {
            self.pending.insert(identity, pending);
        } else {
            self.synthetic = Some(pending);
        }
        if forced {
            Before::Both
        } else {
            Before::Website
        }
    }
    /// Returns true only if the renderer's unhandled event still needs its
    /// browser action. A forced second press has already queued that action,
    /// while the same second key is deliberately still delivered to the site.
    pub fn unhandled(&mut self, revision: u64, key: &Shortcut, stamp: EventStamp) -> bool {
        self.revision(revision);
        let pending = if let Some(identity) = stamp.identity {
            self.pending.remove(&identity)
        } else {
            self.synthetic.take()
        };
        let Some(pending) = pending.filter(|pending| pending.key == *key) else {
            return false;
        };
        if !pending.forced
            && self
                .last
                .as_ref()
                .is_some_and(|last| last.stamp.identity == stamp.identity)
        {
            self.last = None;
        }
        !pending.forced
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key() -> Shortcut {
        Shortcut {
            cmd: true,
            shift: true,
            key: "c".into(),
            ..Default::default()
        }
    }
    fn stamp(id: u64, ms: u32) -> EventStamp {
        EventStamp {
            identity: Some(EventIdentity(id, 0)),
            milliseconds: Some(ms),
        }
    }
    #[test]
    fn browser_preempts_but_website_policy_waits_for_the_matching_unhandled_event() {
        let mut state = State::default();
        let now = Instant::now();
        let key = key();
        assert_eq!(
            state.before(
                0,
                key.clone(),
                ShortcutPriority::PreferBrowser,
                stamp(1, 100),
                false,
                now
            ),
            Before::Browser
        );
        assert_eq!(
            state.before(
                0,
                key.clone(),
                ShortcutPriority::PreferWebsites,
                stamp(2, 200),
                false,
                now
            ),
            Before::Website
        );
        assert!(!state.unhandled(0, &key, stamp(1, 100)));
        assert!(state.unhandled(0, &key, stamp(2, 200)));
        assert!(!state.unhandled(0, &key, stamp(2, 200)));
    }
    #[test]
    fn both_delivers_both_keys_to_the_site_and_dispatches_the_browser_once_even_with_late_acks() {
        let mut state = State::default();
        let now = Instant::now();
        let key = key();
        assert_eq!(
            state.before(
                0,
                key.clone(),
                ShortcutPriority::UseBoth,
                stamp(1, 100),
                false,
                now
            ),
            Before::Website
        );
        assert_eq!(
            state.before(
                0,
                key.clone(),
                ShortcutPriority::UseBoth,
                stamp(2, 150),
                false,
                now
            ),
            Before::Both
        );
        // The first unhandled acknowledgement may arrive after the second
        // press. Native identities prevent suppressing the wrong key event.
        assert!(state.unhandled(0, &key, stamp(1, 100)));
        assert!(!state.unhandled(0, &key, stamp(2, 150)));
    }
    #[test]
    fn slow_renderer_retains_website_fallback_and_reconfiguration_rejects_old_acks() {
        let mut state = State::default();
        let now = Instant::now();
        let key = key();
        state.before(
            0,
            key.clone(),
            ShortcutPriority::PreferWebsites,
            stamp(1, 100),
            false,
            now,
        );
        state.before(
            0,
            key.clone(),
            ShortcutPriority::PreferWebsites,
            stamp(2, 10_100),
            false,
            now + Duration::from_secs(10),
        );
        assert!(state.unhandled(0, &key, stamp(1, 100)));
        assert!(!state.unhandled(1, &key, stamp(2, 10_100)));
    }
    #[test]
    fn both_matches_the_isolated_arc_boundary_observations() {
        let now = Instant::now();
        let key = key();
        for (gap, expected) in [(5_633, Before::Both), (6_038, Before::Website)] {
            let mut state = State::default();
            state.before(
                0,
                key.clone(),
                ShortcutPriority::UseBoth,
                stamp(1, 1_000),
                false,
                now,
            );
            assert_eq!(
                state.before(
                    0,
                    key.clone(),
                    ShortcutPriority::UseBoth,
                    stamp(2, 1_000 + gap),
                    false,
                    now
                ),
                expected
            );
        }
    }
    #[test]
    fn both_uses_native_input_time_and_preserves_its_window_across_browser_shortcuts() {
        let mut state = State::default();
        let now = Instant::now();
        let key = key();
        state.before(
            0,
            key.clone(),
            ShortcutPriority::UseBoth,
            stamp(1, u32::MAX - 20),
            false,
            now,
        );
        assert_eq!(
            state.before(
                0,
                key.clone(),
                ShortcutPriority::UseBoth,
                stamp(2, 10),
                false,
                now + Duration::from_secs(20)
            ),
            Before::Both
        );
        state.before(
            0,
            key.clone(),
            ShortcutPriority::UseBoth,
            stamp(3, 100),
            false,
            now,
        );
        let expired = 101 + DOUBLE_PRESS.as_millis() as u32;
        assert_eq!(
            state.before(
                0,
                key.clone(),
                ShortcutPriority::UseBoth,
                stamp(4, expired),
                false,
                now
            ),
            Before::Website
        );
        let other = Shortcut {
            cmd: true,
            key: "l".into(),
            ..Default::default()
        };
        assert_eq!(
            state.before(
                0,
                other,
                ShortcutPriority::PreferBrowser,
                stamp(5, expired + 10),
                false,
                now
            ),
            Before::Browser
        );
        assert_eq!(
            state.before(
                0,
                key.clone(),
                ShortcutPriority::UseBoth,
                stamp(6, expired + 20),
                false,
                now
            ),
            Before::Both
        );
        assert_eq!(
            state.before(
                0,
                key.clone(),
                ShortcutPriority::UseBoth,
                stamp(7, expired + 30),
                false,
                now
            ),
            Before::Website
        );
        assert_eq!(
            state.before(
                1,
                key.clone(),
                ShortcutPriority::UseBoth,
                stamp(8, expired + 40),
                false,
                now
            ),
            Before::Website
        );
        assert_eq!(
            state.before(
                1,
                key,
                ShortcutPriority::UseBoth,
                stamp(9, expired + 50),
                true,
                now
            ),
            Before::Website
        );
    }
}
