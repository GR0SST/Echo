//! Searchable Unicode emoji and portable Ionicons; no reference-app assets.
use std::sync::LazyLock;

pub(crate) static EMOJI: LazyLock<Vec<[String; 4]>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/emoji/catalog.json")).expect("valid emoji catalog")
});

// Order and filled variants measured in the reference Space icon popover.
pub(crate) const SPACE_SYMBOLS: &[&str] = &[
    "star", "bookmark", "heart", "flag", "flash", "triangle", "medical", "notifications",
    "bulb", "boat", "grid", "apps", "layers", "server", "albums", "copy",
    "folder", "car-sport", "calendar", "mail", "checkbox", "document", "book", "chatbubble-ellipses",
    "people", "terminal", "construct", "square", "egg", "circle", "moon", "sunny",
    "planet", "leaf", "cloud", "paw", "basket", "gift", "bed", "restaurant",
    "barbell", "airplane", "musical-notes", "color-palette", "videocam", "bandage", "code", "baseball",
    "cloud-outline", "map", "bug", "pizza", "skull", "newspaper", "thumbs-up", "train",
];

pub(crate) const FOLDER_SYMBOLS: &[&str] = &[
    "star", "bookmark", "heart", "flash", "triangle", "medical", "notifications", "bulb",
    "chatbubble-ellipses", "people", "construct", "egg", "circle", "moon", "planet", "leaf",
    "cloud", "paw", "restaurant", "airplane", "musical-notes", "videocam", "code", "skull",
];

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) enum Catalog {
    #[default]
    Emoji,
    Symbols,
}

#[derive(Default)]
pub(crate) struct Picker {
    pub folder: bool,
    pub keyboard: bool,
    pub catalog: Catalog,
    pub category: Option<String>,
}

#[derive(Clone)]
pub(crate) struct Entry {
    pub value: String,
    pub label: String,
    pub category: String,
}

pub(crate) fn symbol_value(name: &str) -> String {
    if name == "circle" { "shape:circle".into() } else { format!("ion:{name}") }
}

impl Picker {
    pub fn entries(&self, search: &str) -> Vec<Entry> {
        let query = search.trim().to_lowercase();
        match self.catalog {
            Catalog::Emoji => EMOJI.iter().filter(|row| {
                (query.is_empty() || row[0] == query || row[1].contains(&query) || row[3].contains(&query))
            }).map(|row| Entry { value: row[0].clone(), label: row[1].clone(), category: row[2].clone() }).collect(),
            Catalog::Symbols => (if self.folder { FOLDER_SYMBOLS } else { SPACE_SYMBOLS }).iter()
                .map(|name| Entry { value: symbol_value(name), label: name.replace('-', " "), category: String::new() }).collect(),
        }
    }

    pub fn rows(&self, search: &str) -> Vec<Vec<Entry>> {
        let mut rows: Vec<Vec<Entry>> = Vec::new();
        let group_categories = self.catalog == Catalog::Emoji && search.trim().is_empty();
        for entry in self.entries(search) {
            if rows.last().is_none_or(|row| row.len() == 8 || (group_categories && row[0].category != entry.category)) {
                rows.push(Vec::new());
            }
            rows.last_mut().unwrap().push(entry);
        }
        rows
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_symbols_have_portable_geometry() {
        assert_eq!(SPACE_SYMBOLS.len(), 56);
        for name in SPACE_SYMBOLS.iter().chain(FOLDER_SYMBOLS.iter()) {
            assert!(browser_core::icons::valid_icon(&symbol_value(name)), "{name}");
        }
    }
    #[test]
    fn searches_names_aliases_tags_and_literal_emoji() {
        let picker = Picker::default();
        for query in ["compass", "🧭", "family", "scientist"] {
            assert!(!picker.entries(query).is_empty(), "{query}");
        }
        assert!(picker.entries("no-such-emoji-12345").is_empty());
        let picker = Picker { catalog: Catalog::Symbols, category: None, keyboard: false, folder: false };
        assert!(picker.entries("heart").iter().any(|entry| entry.value == "ion:heart"));
        assert_eq!(picker.entries("ignored in symbol mode").len(), 56);
    }
}
