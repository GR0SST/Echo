//! Provider-independent Today grouping. A late model answer is a conditional
//! edit of the captured tabs, never authority to create, close, or move pages.
use crate::{Browser, SpaceId, TabId, UserProfileId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodayGroup {
    pub id: u64,
    pub title: String,
    pub tabs: Vec<TabId>,
    #[serde(default)]
    pub collapsed: bool,
}
#[derive(Clone, Debug)]
pub enum TodayRow {
    Group(TodayGroup),
    Tab(TabId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TidyTab {
    pub id: TabId,
    pub title: String,
    pub url: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TidySnapshot {
    pub space: SpaceId,
    pub profile: UserProfileId,
    pub tabs: Vec<TidyTab>,
    pub(crate) order: Vec<TabId>,
    signatures: Vec<TidyTab>,
    groups: Vec<TodayGroup>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TidyResult {
    pub groups: Vec<TidyResultGroup>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TidyResultGroup {
    pub title: String,
    pub tabs: Vec<TabId>,
}

impl Browser {
    pub fn can_tidy_tabs(&self, space: SpaceId) -> bool {
        !self.is_private()
            && !self.is_blank()
            && self.settings.max.enabled(echo_ai::MaxFeature::TidyTabs)
            && self.spaces.iter().find(|s| s.id == space).is_some_and(|s| {
                s.today
                    .iter()
                    .filter(|t| self.item_anchor(t.id) == t.id)
                    .take(7)
                    .count()
                    > 6
            })
    }
    pub(crate) fn repair_today_groups(&mut self) {
        let mut group_ids = HashSet::new();
        let allowed: HashMap<_, HashSet<_>> = self
            .spaces
            .iter()
            .map(|s| {
                (
                    s.id,
                    s.today
                        .iter()
                        .filter(|t| self.item_anchor(t.id) == t.id)
                        .map(|t| t.id)
                        .collect(),
                )
            })
            .collect();
        for space in &mut self.spaces {
            let mut used = HashSet::new();
            space.today_groups.retain_mut(|g| {
                if g.id == 0
                    || g.id == u64::MAX
                    || !group_ids.insert(g.id)
                    || g.title.trim().is_empty()
                    || g.title.chars().count() > 80
                    || g.title.chars().any(char::is_control)
                {
                    return false;
                }
                g.tabs
                    .retain(|id| allowed[&space.id].contains(id) && used.insert(*id));
                if g.tabs.is_empty() {
                    return false;
                }
                self.next_id = self.next_id.max(g.id + 1);
                true
            });
        }
    }
    pub fn tidy_snapshot(&self, space: SpaceId) -> Option<TidySnapshot> {
        if !self.can_tidy_tabs(space) {
            return None;
        }
        let space = self.spaces.iter().find(|s| s.id == space)?;
        let signatures: Vec<_> = space
            .today
            .iter()
            .map(|t| TidyTab {
                id: t.id,
                title: t.title.clone(),
                url: t.url.clone(),
            })
            .collect();
        let tabs: Vec<_> = signatures
            .iter()
            .filter(|t| self.item_anchor(t.id) == t.id)
            .cloned()
            .collect();
        (tabs.len() > 6).then(|| TidySnapshot {
            space: space.id,
            profile: space.user_profile_id,
            order: signatures.iter().map(|t| t.id).collect(),
            signatures,
            tabs,
            groups: space.today_groups.clone(),
        })
    }

    /// Invalid, duplicate and stale answers are all-or-nothing no-ops. The
    /// provider may group only supplied whole sidebar items; ungrouped items
    /// retain their relative order. Split panes keep their existing identities.
    pub fn apply_tidy_tabs(&mut self, snapshot: &TidySnapshot, result: TidyResult) -> bool {
        if self.tidy_snapshot(snapshot.space).as_ref() != Some(snapshot) || result.groups.is_empty()
        {
            return false;
        }
        let allowed: HashSet<_> = snapshot.tabs.iter().map(|t| t.id).collect();
        let mut used = HashSet::new();
        if result.groups.iter().any(|g| {
            g.title.trim().is_empty()
                || g.title.chars().count() > 80
                || g.title.chars().any(char::is_control)
                || g.tabs.len() < 2
                || g.tabs
                    .iter()
                    .any(|id| !allowed.contains(id) || !used.insert(*id))
        }) {
            return false;
        }
        let mut anchors: Vec<_> = result
            .groups
            .iter()
            .flat_map(|g| g.tabs.iter().copied())
            .collect();
        anchors.extend(
            snapshot
                .tabs
                .iter()
                .map(|t| t.id)
                .filter(|id| !used.contains(id)),
        );
        let ranks: HashMap<_, _> = anchors
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();
        let ordered: HashMap<_, _> = snapshot
            .order
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, (ranks[&self.item_anchor(*id)], i)))
            .collect();
        let groups: Vec<_> = result
            .groups
            .into_iter()
            .map(|g| TodayGroup {
                id: self.next_id(),
                title: g.title.trim().into(),
                tabs: g.tabs,
                collapsed: false,
            })
            .collect();
        let space = self
            .spaces
            .iter_mut()
            .find(|s| s.id == snapshot.space)
            .unwrap();
        let before = space.today_groups.clone();
        space.today.sort_by_key(|t| ordered[&t.id]);
        let after_order = space.today.iter().map(|t| t.id).collect();
        space.today_groups = groups.clone();
        self.sidebar_history.record_tidy(
            snapshot.space,
            snapshot.profile,
            before,
            groups,
            snapshot.order.clone(),
            after_order,
        );
        true
    }

    pub fn toggle_today_group(&mut self, space: SpaceId, group: u64) -> bool {
        let Some(group) = self
            .spaces
            .iter_mut()
            .find(|s| s.id == space)
            .and_then(|s| s.today_groups.iter_mut().find(|g| g.id == group))
        else {
            return false;
        };
        group.collapsed = !group.collapsed;
        true
    }
    pub fn remove_today_group_header(&mut self, space: SpaceId, group: u64) -> bool {
        let Some(space) = self.spaces.iter_mut().find(|s| s.id == space) else {
            return false;
        };
        let old = space.today_groups.len();
        space.today_groups.retain(|g| g.id != group);
        space.today_groups.len() != old
    }
    pub fn today_rows(&self, space: SpaceId) -> Vec<TodayRow> {
        let Some(state) = self.spaces.iter().find(|s| s.id == space) else {
            return Vec::new();
        };
        let groups = self.today_groups(space);
        let membership: HashMap<_, _> = groups
            .iter()
            .enumerate()
            .flat_map(|(i, g)| g.tabs.iter().map(move |id| (*id, i)))
            .collect();
        let mut shown = HashSet::new();
        let mut rows = Vec::new();
        for tab in state
            .today
            .iter()
            .filter(|t| self.item_anchor(t.id) == t.id)
        {
            if let Some(index) = membership.get(&tab.id) {
                if shown.insert(*index) {
                    let group = &groups[*index];
                    rows.push(TodayRow::Group(group.clone()));
                    if !group.collapsed {
                        rows.extend(group.tabs.iter().map(|id| TodayRow::Tab(*id)));
                    }
                }
            } else {
                rows.push(TodayRow::Tab(tab.id));
            }
        }
        rows
    }
    pub fn today_groups(&self, space: SpaceId) -> Vec<TodayGroup> {
        let Some(space) = self.spaces.iter().find(|s| s.id == space) else {
            return Vec::new();
        };
        let ids: HashSet<_> = space
            .today
            .iter()
            .filter(|t| self.item_anchor(t.id) == t.id)
            .map(|t| t.id)
            .collect();
        let mut used = HashSet::new();
        space
            .today_groups
            .iter()
            .filter_map(|g| {
                let mut group = g.clone();
                group.tabs.retain(|id| ids.contains(id) && used.insert(*id));
                (!group.tabs.is_empty()).then_some(group)
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TidyEdit {
    pub space: SpaceId,
    pub profile: UserProfileId,
    pub before: Vec<TodayGroup>,
    pub after: Vec<TodayGroup>,
    pub before_order: Vec<TabId>,
    pub after_order: Vec<TabId>,
}
impl TidyEdit {
    pub(crate) fn apply(&self, redo: bool, browser: &mut Browser) -> bool {
        let (source, target, order) = if redo {
            (&self.before, &self.after, &self.after_order)
        } else {
            (&self.after, &self.before, &self.before_order)
        };
        let Some(space) = browser
            .spaces
            .iter_mut()
            .find(|s| s.id == self.space && s.user_profile_id == self.profile)
        else {
            return false;
        };
        let same = |a: &[TodayGroup], b: &[TodayGroup]| {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(a, b)| a.id == b.id && a.title == b.title && a.tabs == b.tabs)
        };
        if !same(&space.today_groups, source) {
            return false;
        }
        // Replace only known slots, preserving newly opened tabs and retaining
        // live titles/URLs. Missing or moved pages are never resurrected.
        let rank: HashMap<_, _> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let mut sorted: Vec<_> = space
            .today
            .iter()
            .filter(|t| rank.contains_key(&t.id))
            .cloned()
            .collect();
        sorted.sort_by_key(|t| rank[&t.id]);
        let mut sorted = sorted.into_iter();
        for tab in &mut space.today {
            if rank.contains_key(&tab.id) {
                *tab = sorted.next().unwrap();
            }
        }
        space.today_groups = target.clone();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;
    fn fixture() -> (Browser, TidySnapshot) {
        let mut b = Browser::new();
        b.settings
            .max
            .set_enabled(echo_ai::MaxFeature::TidyTabs, true);
        for i in 0..8 {
            b.new_tab(format!("https://example.test/{i}"), NOW);
        }
        let snapshot = b.tidy_snapshot(b.space().id).unwrap();
        (b, snapshot)
    }
    fn result(s: &TidySnapshot) -> TidyResult {
        TidyResult {
            groups: vec![
                TidyResultGroup {
                    title: "Rust".into(),
                    tabs: vec![s.tabs[3].id, s.tabs[1].id],
                },
                TidyResultGroup {
                    title: "Cooking".into(),
                    tabs: vec![s.tabs[6].id, s.tabs[2].id],
                },
            ],
        }
    }
    #[test]
    fn tidy_is_an_atomic_persisted_edit_that_keeps_every_page_and_selection() {
        let (mut b, s) = fixture();
        let active = b.space().active_tab;
        let ids: HashSet<_> = b.all_tabs().map(|t| t.id).collect();
        assert!(b.apply_tidy_tabs(&s, result(&s)));
        assert_eq!(b.space().active_tab, active);
        assert_eq!(b.all_tabs().map(|t| t.id).collect::<HashSet<_>>(), ids);
        let loaded = crate::session::from_json(&crate::session::to_json(&b)).unwrap();
        assert_eq!(loaded.today_groups(s.space), b.today_groups(s.space));
        assert_eq!(b.space().today[0].id, s.tabs[3].id);
        assert_eq!(b.space().today[1].id, s.tabs[1].id);
        assert_eq!(b.sidebar_undo_action(), Some("Tidy Tabs"));
        b.set_title(s.tabs[3].id, "Live title after tidying");
        let extra = b.new_tab("https://extra.test", NOW);
        assert!(b.undo_sidebar(NOW));
        assert!(b.today_groups(s.space).is_empty());
        assert_eq!(b.space().today[0].id, extra);
        assert_eq!(
            b.all_tabs().find(|t| t.id == s.tabs[3].id).unwrap().title,
            "Live title after tidying"
        );
        assert!(b.redo_sidebar(NOW));
        assert_eq!(b.space().today[0].id, extra);
        assert_eq!(b.today_groups(s.space).len(), 2);
    }
    #[test]
    fn tidy_rejects_stale_duplicate_unknown_and_malformed_model_output() {
        let (mut b, s) = fixture();
        let before = crate::session::to_json(&b);
        for bad in [
            TidyResult {
                groups: vec![TidyResultGroup {
                    title: "Repeated".into(),
                    tabs: vec![s.tabs[0].id, s.tabs[0].id],
                }],
            },
            TidyResult {
                groups: vec![TidyResultGroup {
                    title: "Unknown".into(),
                    tabs: vec![s.tabs[0].id, TabId(99999)],
                }],
            },
            TidyResult {
                groups: vec![TidyResultGroup {
                    title: "Bad\nlabel".into(),
                    tabs: vec![s.tabs[0].id, s.tabs[1].id],
                }],
            },
        ] {
            assert!(!b.apply_tidy_tabs(&s, bad));
            assert_eq!(crate::session::to_json(&b), before);
        }
        b.set_title(s.tabs[0].id, "Changed while awaiting AI");
        assert!(!b.apply_tidy_tabs(&s, result(&s)));
        assert!(b.today_groups(s.space).is_empty());
        let fresh = b.tidy_snapshot(s.space).unwrap();
        b.settings
            .max
            .set_enabled(echo_ai::MaxFeature::TidyTabs, false);
        assert!(!b.apply_tidy_tabs(&fresh, result(&fresh)));
    }
    #[test]
    fn closed_or_moved_members_are_never_resurrected_by_tidy_undo() {
        let (mut b, s) = fixture();
        assert!(b.apply_tidy_tabs(&s, result(&s)));
        let moved = s.tabs[3].id;
        let other = b.add_space("Other", 20.);
        assert!(b.move_tab(moved, other, crate::Section::Today));
        assert!(b.undo_sidebar(NOW));
        assert!(b.redo_sidebar(NOW));
        assert!(b
            .spaces
            .iter()
            .find(|s| s.id == other)
            .unwrap()
            .today
            .iter()
            .any(|t| t.id == moved));
        assert!(!b
            .today_groups(s.space)
            .iter()
            .any(|g| g.tabs.contains(&moved)));
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;
    #[test]
    fn tidy_keeps_whole_splits_and_never_accepts_an_individual_child_id() {
        let now = std::time::SystemTime::UNIX_EPOCH;
        let mut b = Browser::new();
        b.settings
            .max
            .set_enabled(echo_ai::MaxFeature::TidyTabs, true);
        let ids: Vec<_> = (0..9)
            .map(|i| b.new_tab(format!("https://example.test/{i}"), now))
            .collect();
        b.add_split(ids[0], ids[1], crate::splits::Placement::Right, now)
            .unwrap();
        let panes = b.split_for(ids[0]).unwrap().panes.clone();
        let snapshot = b.tidy_snapshot(b.space().id).unwrap();
        assert!(!snapshot.tabs.iter().any(|t| t.id == ids[1]));
        assert!(!b.apply_tidy_tabs(
            &snapshot,
            TidyResult {
                groups: vec![TidyResultGroup {
                    title: "Bad".into(),
                    tabs: vec![ids[1], ids[3]]
                }]
            }
        ));
        assert!(b.apply_tidy_tabs(
            &snapshot,
            TidyResult {
                groups: vec![TidyResultGroup {
                    title: "Docs".into(),
                    tabs: vec![ids[0], ids[3]]
                }]
            }
        ));
        assert_eq!(b.split_for(ids[0]).unwrap().panes, panes);
        assert_eq!(b.all_tabs().count(), 9);
        assert!(b.undo_sidebar(now));
        assert_eq!(b.split_for(ids[0]).unwrap().panes, panes);
    }
}
