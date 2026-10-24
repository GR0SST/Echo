//! Window-local recent sidebar items. Previewing never changes the document
//! selection or recency; only an actual visit is recorded.
use crate::{Browser, TabId};
use std::collections::HashSet;

pub const VISIBLE_LIMIT: usize = 5;

#[derive(Clone, Debug, Default)]
pub struct RecentItems {
    // Keep a group's members so its surviving pane retains its place if the
    // anchor closes. Older visits can fill a vacant slot after an archive.
    visits: Vec<Vec<TabId>>,
}

impl RecentItems {
    pub fn record(&mut self, browser: &Browser) {
        let live: HashSet<_> = browser.all_tabs().map(|tab| tab.id).collect();
        for members in &mut self.visits {
            members.retain(|id| live.contains(id));
        }
        self.visits.retain(|members| !members.is_empty());
        let Some(active) = browser.active_tab() else {
            return;
        };
        let members = browser.related_tabs(active.id);
        self.visits
            .retain(|old| !old.iter().any(|id| members.contains(id)));
        self.visits.insert(0, members);
    }

    /// Explicitly unloaded Pinned/Favorite items remain in the model, but are
    /// no longer open recent items in this window.
    pub fn forget(&mut self, tabs: &[TabId]) {
        for members in &mut self.visits {
            members.retain(|id| !tabs.contains(id));
        }
        self.visits.retain(|members| !members.is_empty());
    }

    pub fn visible(&self, browser: &Browser) -> Vec<TabId> {
        let live: HashSet<_> = browser.all_tabs().map(|tab| tab.id).collect();
        let mut seen = HashSet::new();
        self.visits
            .iter()
            .filter_map(|members| {
                let tab = members.iter().find(|tab| live.contains(tab))?;
                let anchor = browser.item_anchor(*tab);
                seen.insert(anchor).then_some(anchor)
            })
            .take(VISIBLE_LIMIT)
            .collect()
    }
}

/// A frozen navigation order while Control is held. It is independent of the
/// browser's current page, so cycling is reversible until the user commits.
#[derive(Clone, Debug)]
pub struct RecentSelection {
    items: Vec<TabId>,
    selected: usize,
}
impl RecentSelection {
    pub fn new(mut items: Vec<TabId>, backwards: bool) -> Option<Self> {
        items.truncate(VISIBLE_LIMIT);
        if items.len() < 2 {
            return None;
        }
        let selected = if backwards { items.len() - 1 } else { 1 };
        Some(Self { items, selected })
    }
    pub fn items(&self) -> &[TabId] {
        &self.items
    }
    pub fn selected_index(&self) -> usize {
        self.selected
    }
    pub fn selected(&self) -> Option<TabId> {
        self.items.get(self.selected).copied()
    }
    pub fn select(&mut self, index: usize) {
        if index < self.items.len() {
            self.selected = index;
        }
    }
    pub fn advance(&mut self, delta: isize) {
        if !self.items.is_empty() {
            self.selected =
                (self.selected as isize + delta).rem_euclid(self.items.len() as isize) as usize;
        }
    }
    pub fn reconcile(&mut self, browser: &Browser) {
        let selected = self.selected().map(|id| browser.item_anchor(id));
        let live: HashSet<_> = browser.all_tabs().map(|tab| tab.id).collect();
        let mut seen = HashSet::new();
        self.items = self
            .items
            .iter()
            .filter(|id| live.contains(id))
            .map(|id| browser.item_anchor(*id))
            .filter(|id| seen.insert(*id))
            .collect();
        self.selected = selected
            .and_then(|id| self.items.iter().position(|item| *item == id))
            .unwrap_or_else(|| self.selected.min(self.items.len().saturating_sub(1)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;
    fn visit(browser: &mut Browser, recent: &mut RecentItems, url: &str) -> TabId {
        let id = browser.new_tab(url, NOW);
        recent.record(browser);
        id
    }

    #[test]
    fn cycling_previews_mru_order_without_selecting_or_reordering_tabs() {
        let mut browser = Browser::new();
        let mut recent = RecentItems::default();
        let a = visit(&mut browser, &mut recent, "https://a.test");
        let b = visit(&mut browser, &mut recent, "https://b.test");
        let c = visit(&mut browser, &mut recent, "https://c.test");
        browser.activate_tab(a, NOW);
        recent.record(&browser);
        assert_eq!(recent.visible(&browser), [a, c, b]);
        let mut selection = RecentSelection::new(recent.visible(&browser), false).unwrap();
        assert_eq!(selection.selected(), Some(c));
        selection.advance(1);
        assert_eq!(selection.selected(), Some(b));
        selection.advance(1);
        assert_eq!(selection.selected(), Some(a));
        selection.advance(-1);
        assert_eq!(selection.selected(), Some(b));
        assert_eq!(browser.active_tab().unwrap().id, a);
        assert_eq!(
            browser
                .space()
                .today
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            [c, b, a]
        );
        browser.activate_sidebar_item(selection.selected().unwrap(), NOW);
        recent.record(&browser);
        assert_eq!(recent.visible(&browser), [b, a, c]);
    }

    #[test]
    fn only_five_items_cycle_and_older_visits_fill_an_archived_slot() {
        let mut browser = Browser::new();
        let mut recent = RecentItems::default();
        let ids: Vec<_> = (0..6)
            .map(|i| visit(&mut browser, &mut recent, &format!("https://{i}.test")))
            .collect();
        assert_eq!(
            recent.visible(&browser),
            [ids[5], ids[4], ids[3], ids[2], ids[1]]
        );
        let mut selection = RecentSelection::new(recent.visible(&browser), false).unwrap();
        selection.advance(4);
        assert_eq!(selection.selected(), Some(ids[5]));
        browser.close_tab(ids[4], NOW);
        recent.record(&browser);
        assert_eq!(
            recent.visible(&browser),
            [ids[5], ids[3], ids[2], ids[1], ids[0]]
        );
    }

    #[test]
    fn visits_are_local_to_each_window_of_the_shared_profile() {
        let mut browser = Browser::new();
        let a = browser.new_tab("https://a.test", NOW);
        let mut a_window = browser.window_selection();
        let mut b_window = browser.new_window_selection();
        let mut a_recent = RecentItems::default();
        let mut b_recent = RecentItems::default();
        a_recent.record(&browser.window_projection(&a_window));
        let b = browser.edit_in_window(&mut b_window, |browser| {
            browser.new_tab("https://b.test", NOW)
        });
        b_recent.record(&browser.window_projection(&b_window));
        assert_eq!(a_recent.visible(&browser), [a]);
        assert_eq!(b_recent.visible(&browser), [b]);
        browser.edit_in_window(&mut a_window, |browser| browser.activate_tab(b, NOW));
        a_recent.record(&browser.window_projection(&a_window));
        assert_eq!(a_recent.visible(&browser), [b, a]);
        assert_eq!(b_recent.visible(&browser), [b]);
    }

    #[test]
    fn split_panes_are_one_recent_item_and_a_survivor_keeps_its_recency() {
        let mut browser = Browser::new();
        let mut recent = RecentItems::default();
        let a = visit(&mut browser, &mut recent, "https://a.test");
        let b = visit(&mut browser, &mut recent, "https://b.test");
        browser
            .add_split(a, b, crate::splits::Placement::Right, NOW)
            .unwrap();
        recent.record(&browser);
        let c = visit(&mut browser, &mut recent, "https://c.test");
        assert_eq!(recent.visible(&browser), [c, a]);
        assert!(browser.close_split_pane(a, NOW));
        recent.record(&browser);
        assert_eq!(recent.visible(&browser), [c, b]);
    }

    #[test]
    fn an_unloaded_pin_is_excluded_until_visited_again() {
        let mut browser = Browser::new();
        let mut recent = RecentItems::default();
        let a = visit(&mut browser, &mut recent, "https://a.test");
        browser.toggle_pin(a);
        let b = visit(&mut browser, &mut recent, "https://b.test");
        browser.close_tab(a, NOW);
        recent.forget(&[a]);
        recent.record(&browser);
        assert!(browser.all_tabs().any(|tab| tab.id == a));
        assert_eq!(recent.visible(&browser), [b]);
        browser.activate_tab(a, NOW);
        recent.record(&browser);
        assert_eq!(recent.visible(&browser), [a, b]);
    }

    #[test]
    fn reverse_starts_at_the_oldest_visible_item_and_stale_candidates_are_safe() {
        let mut browser = Browser::new();
        let mut recent = RecentItems::default();
        let a = visit(&mut browser, &mut recent, "https://a.test");
        assert!(RecentSelection::new(recent.visible(&browser), false).is_none());
        let b = visit(&mut browser, &mut recent, "https://b.test");
        let c = visit(&mut browser, &mut recent, "https://c.test");
        let mut selection = RecentSelection::new(recent.visible(&browser), true).unwrap();
        assert_eq!(selection.selected(), Some(a));
        browser.close_tab(a, NOW);
        selection.reconcile(&browser);
        assert_eq!(selection.selected(), Some(b));
        browser.close_tab(b, NOW);
        browser.close_tab(c, NOW);
        selection.reconcile(&browser);
        selection.advance(1);
        assert_eq!(selection.selected(), None);
    }
}
