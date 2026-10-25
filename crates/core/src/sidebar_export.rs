//! Portable Markdown export of the persistent sidebar hierarchy.
use crate::{Browser, Space, folders::PinnedItem};
impl Browser {
    pub fn export_sidebar_markdown(&self) -> String {
        fn line(text: &str) -> String { text.replace(['\r', '\n'], " ") }
        fn item(browser: &Browser, space: &Space, entry: PinnedItem, depth: usize, out: &mut String) {
            if depth > crate::folders::MAX_FOLDER_DEPTH + 2 { return; }
            match entry {
                PinnedItem::Folder(id) => {
                    if let Some(folder) = space.folders.iter().find(|folder|folder.id==id) {
                        out.push_str(&format!("\n{} {}\n", "#".repeat(depth), line(&folder.name)));
                        for entry in &folder.items { item(browser,space,*entry,depth+1,out); }
                    }
                }
                PinnedItem::Tab(id) => {
                    if let Some(tab) = space.pinned.iter().find(|tab|tab.id==id) {
                        out.push_str(&format!("- {}\n",line(tab.pinned_url.as_deref().unwrap_or(&tab.url))));
                    }
                }
                PinnedItem::Split(id) => {
                    if let Some(split) = space.splits.iter().find(|split|split.id==id) {
                        for tab in &split.panes { item(browser,space,PinnedItem::Tab(*tab),depth,out); }
                    }
                }
            }
        }
        let mut out=String::new();
        for space in &self.spaces {
            if !out.is_empty() { out.push('\n'); }
            out.push_str(&format!("# {}\n",line(&space.name)));
            if let Some(profile)=self.user_profile(space.user_profile_id) {
                out.push_str("\n## Favorites\n");
                for tab in &profile.favorites { out.push_str(&format!("- {}\n",line(tab.pinned_url.as_deref().unwrap_or(&tab.url)))); }
            }
            for entry in &space.pinned_items { item(self,space,*entry,2,&mut out); }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sidebar_export_retains_nested_order_and_original_pins_without_transient_pages() {
        let mut browser=Browser::new();let now=std::time::SystemTime::now();
        let tab=browser.new_tab("https://original.example/path",now);browser.toggle_pin(tab);
        browser.navigate(tab,"https://navigated.example");
        let folder=browser.create_folder(browser.space().id,None,"Folder").unwrap();
        browser.move_item_to_folder(tab,folder);
        browser.new_tab("https://today.example",now);
        let before=crate::session::to_json(&browser);
        let text=browser.export_sidebar_markdown();
        assert!(text.contains("## Folder\n- https://original.example/path\n"));
        assert!(!text.contains("navigated.example"));assert!(!text.contains("today.example"));
        assert_eq!(before,crate::session::to_json(&browser));
    }
}
