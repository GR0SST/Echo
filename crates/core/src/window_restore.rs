//! Persistent normal-window presentation, stored in the workspace checkpoint.
use crate::windows::WindowSelection;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RestoredWindow {
    pub id: u64,
    pub selection: WindowSelection,
    pub sidebar_width: f32,
    pub sidebar_visible: bool,
    pub frame: WindowFrame,
    pub display: Option<String>,
    pub mode: WindowMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowFrame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RestoredWindow {
    pub fn valid(&self) -> bool {
        self.id > 0
            && self.sidebar_width.is_finite()
            && (164. ..=500.).contains(&self.sidebar_width)
            && [
                self.frame.x,
                self.frame.y,
                self.frame.width,
                self.frame.height,
            ]
            .iter()
            .all(|v| v.is_finite())
            && (100. ..=32768.).contains(&self.frame.width)
            && (100. ..=32768.).contains(&self.frame.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session, Browser};
    #[test]
    fn checkpoint_round_trip_keeps_independent_space_selections_and_split_focus() {
        let mut b = Browser::new();
        let now = std::time::SystemTime::now();
        let first = b.new_tab("https://first.example", now);
        let pane = b.new_tab("https://pane.example", now);
        b.add_split(first, pane, crate::splits::Placement::Right, now)
            .unwrap();
        b.focus_split_pane(0, now);
        let a = b.window_selection();
        b.add_space("Second", 20.);
        b.switch_space(1);
        let second = b.new_tab("https://second.example", now);
        let c = b.window_selection();
        for (id, selection, width, visible) in [(1, a, 228., false), (2, c, 310., true)] {
            b.windows.push(RestoredWindow {
                id,
                selection,
                sidebar_width: width,
                sidebar_visible: visible,
                frame: WindowFrame {
                    x: 100. * id as f32,
                    y: 100.,
                    width: 1000.,
                    height: 760.,
                },
                display: Some("display".into()),
                mode: WindowMode::Windowed,
            });
        }
        let loaded = session::from_json(&session::to_json(&b)).unwrap();
        assert_eq!(loaded.windows, b.windows);
        assert_eq!(
            loaded
                .window_projection(&loaded.windows[0].selection)
                .active_split()
                .unwrap()
                .panes,
            vec![first, pane]
        );
        assert_eq!(
            loaded
                .window_projection(&loaded.windows[0].selection)
                .active_tab()
                .unwrap()
                .id,
            first
        );
        assert_eq!(
            loaded
                .window_projection(&loaded.windows[1].selection)
                .active_tab()
                .unwrap()
                .id,
            second
        );
    }
    #[test]
    fn window_selection_retains_its_own_folder_and_pinned_expansion() {
        let mut b = Browser::new();
        let folder = b.create_folder(b.space().id, None, "Folder").unwrap();
        let expanded = b.window_selection();
        b.set_folder_collapsed(folder, true);
        b.spaces[0].pinned_collapsed = true;
        let collapsed = b.window_selection();
        let restored = session::from_json(&session::to_json(&b)).unwrap();
        let a = restored.window_projection(&expanded);
        let c = restored.window_projection(&collapsed);
        assert!(!a.folder(folder).unwrap().collapsed && !a.space().pinned_collapsed);
        assert!(c.folder(folder).unwrap().collapsed && c.space().pinned_collapsed);
    }
    #[test]
    fn invalid_or_duplicate_window_geometry_preserves_tab_inventory() {
        let mut b = Browser::new();
        b.new_tab("https://keep.example", std::time::SystemTime::now());
        let w = RestoredWindow {
            id: 1,
            selection: b.window_selection(),
            sidebar_width: 228.,
            sidebar_visible: true,
            frame: WindowFrame {
                x: 0.,
                y: 0.,
                width: 1000.,
                height: 760.,
            },
            display: None,
            mode: WindowMode::Windowed,
        };
        b.windows = vec![w.clone(), w];
        b.windows[0].frame.width = -1.;
        let restored = session::from_json(&session::to_json(&b)).unwrap();
        assert_eq!(restored.windows.len(), 1);
        assert_eq!(restored.all_tabs().count(), 1);
    }
}
