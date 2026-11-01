//! Local, profile-owned Easels: a canvas of shapes, text and images that
//! scrolls vertically. The document is `WIDTH` points wide and scales with
//! the page, so objects keep their layout at any window size.
use serde::{Deserialize, Serialize};

/// The document width in points; every frame is measured in these points.
pub const WIDTH: f32 = 1000.;
/// An Easel's address; the page is drawn by the browser, not the engine.
pub const URL_PREFIX: &str = "echo://easel/";
pub const DEFAULT_TITLE: &str = "Untitled Easel";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EaselId(u64);
impl EaselId {
    pub fn url(self) -> String {
        format!("{URL_PREFIX}{}", self.0)
    }
    /// The Easel an address opens, if it names one.
    pub fn from_url(url: &str) -> Option<Self> {
        url.strip_prefix(URL_PREFIX)?.trim_end_matches('/').parse().ok().map(Self)
    }
}

/// Arc's Easel palette names; the browser picks the exact pigments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Color {
    Black,
    Yellow,
    Pink,
    Red,
    LightGreen,
    DarkGreen,
    White,
    Gray,
    Purple,
    LightBlue,
    DarkBlue,
}
impl Color {
    /// Swatch order: two rows, as in the color picker.
    pub const ALL: [Self; 11] = [
        Self::Black, Self::Yellow, Self::Pink, Self::Red, Self::LightGreen, Self::DarkGreen,
        Self::White, Self::Gray, Self::Purple, Self::LightBlue, Self::DarkBlue,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeKind {
    Rectangle,
    Circle,
    Arrow,
    Freehand,
}
impl ShapeKind {
    pub fn default_color(self) -> Color {
        match self {
            Self::Rectangle => Color::LightGreen,
            Self::Circle => Color::Red,
            Self::Arrow => Color::LightBlue,
            Self::Freehand => Color::Black,
        }
    }
}

/// Text sizes from the smallest; the editor steps through them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParagraphStyle {
    Body,
    H2,
    H1,
    H0,
    Ultra,
}
impl ParagraphStyle {
    const ORDER: [Self; 5] = [Self::Body, Self::H2, Self::H1, Self::H0, Self::Ultra];
    pub fn larger(self) -> Option<Self> {
        Self::ORDER.iter().position(|style| *style == self).and_then(|index| Self::ORDER.get(index + 1)).copied()
    }
    pub fn smaller(self) -> Option<Self> {
        Self::ORDER.iter().position(|style| *style == self).and_then(|index| index.checked_sub(1)).map(|index| Self::ORDER[index])
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FontFamily {
    #[default]
    Clean,
    Hip,
    Fancy,
    Mono,
}
impl FontFamily {
    pub fn next(self) -> Self {
        match self { Self::Clean => Self::Hip, Self::Hip => Self::Fancy, Self::Fancy => Self::Mono, Self::Mono => Self::Clean }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// In document points from the top left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Frame {
    /// The frame spanning two corners in any order.
    pub fn spanning(a: Point, b: Point) -> Self {
        Self { x: a.x.min(b.x), y: a.y.min(b.y), width: (a.x - b.x).abs(), height: (a.y - b.y).abs() }
    }
    pub fn contains(&self, point: Point, slop: f32) -> bool {
        point.x >= self.x - slop && point.x <= self.x + self.width + slop
            && point.y >= self.y - slop && point.y <= self.y + self.height + slop
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Content {
    #[serde(rename_all = "camelCase")]
    Text { string: String, paragraph_style: ParagraphStyle, #[serde(default)] font_family: FontFamily },
    /// Arrow ends and freehand points are fractions of the frame, so a
    /// resize stretches them with it.
    Shape { kind: ShapeKind, color: Color, #[serde(default, skip_serializing_if = "Vec::is_empty")] points: Vec<Point> },
    /// A copy kept in the profile, so the original may move or disappear.
    Image { file: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: u64,
    pub frame: Frame,
    pub content: Content,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Easel {
    pub id: EaselId,
    pub title: String,
    /// Objects in paint order; later ones lie on top.
    #[serde(default)]
    pub objects: Vec<Object>,
    #[serde(default)]
    next_object: u64,
}
impl Easel {
    pub fn add(&mut self, frame: Frame, content: Content) -> u64 {
        self.next_object += 1;
        let id = self.next_object;
        self.objects.push(Object { id, frame, content });
        id
    }
    pub fn object(&self, id: u64) -> Option<&Object> {
        self.objects.iter().find(|object| object.id == id)
    }
    pub fn object_mut(&mut self, id: u64) -> Option<&mut Object> {
        self.objects.iter_mut().find(|object| object.id == id)
    }
    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.objects.len();
        self.objects.retain(|object| object.id != id);
        self.objects.len() != before
    }
    /// The topmost object under `point`.
    pub fn hit(&self, point: Point, slop: f32) -> Option<u64> {
        self.objects.iter().rev().find(|object| object.frame.contains(point, slop)).map(|object| object.id)
    }
    /// The lowest edge of any object, in points.
    pub fn bottom(&self) -> f32 {
        self.objects.iter().map(|object| object.frame.bottom()).fold(0., f32::max)
    }
    /// Keeps object ids unique after an undo restores older objects.
    pub fn set_objects(&mut self, objects: Vec<Object>) {
        self.next_object = self.next_object.max(objects.iter().map(|object| object.id).max().unwrap_or(0));
        self.objects = objects;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Easels {
    #[serde(default)]
    entries: Vec<Easel>,
    #[serde(default)]
    next_id: u64,
}
impl Easels {
    /// Newest first, as Library lists them.
    pub fn iter(&self) -> impl Iterator<Item = &Easel> {
        self.entries.iter().rev()
    }
    pub fn create(&mut self) -> EaselId {
        self.next_id += 1;
        let id = EaselId(self.next_id);
        self.entries.push(Easel { id, title: DEFAULT_TITLE.into(), objects: Vec::new(), next_object: 0 });
        id
    }
    pub fn get(&self, id: EaselId) -> Option<&Easel> {
        self.entries.iter().find(|easel| easel.id == id)
    }
    pub fn get_mut(&mut self, id: EaselId) -> Option<&mut Easel> {
        self.entries.iter_mut().find(|easel| easel.id == id)
    }
    pub fn delete(&mut self, id: EaselId) -> bool {
        let before = self.entries.len();
        self.entries.retain(|easel| easel.id != id);
        self.entries.len() != before
    }
    /// A blank title falls back to the default, as an empty field does in Arc.
    pub fn rename(&mut self, id: EaselId, title: &str) -> bool {
        let Some(easel) = self.get_mut(id) else { return false; };
        let title = title.trim();
        easel.title = if title.is_empty() { DEFAULT_TITLE.into() } else { title.into() };
        true
    }
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let mut ids = std::collections::HashSet::new();
        for easel in &self.entries {
            if easel.id.0 == 0 || easel.id.0 > self.next_id || !ids.insert(easel.id) {
                return Err("invalid Easel identity");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easels_round_trip_with_their_objects() {
        let mut easels = Easels::default();
        let id = easels.create();
        let easel = easels.get_mut(id).unwrap();
        let shape = easel.add(Frame { x: 10., y: 20., width: 100., height: 50. }, Content::Shape { kind: ShapeKind::Rectangle, color: Color::LightGreen, points: Vec::new() });
        easel.add(Frame { x: 0., y: 0., width: 300., height: 30. }, Content::Text { string: "Hello".into(), paragraph_style: ParagraphStyle::Body, font_family: FontFamily::Clean });
        assert!(easels.rename(id, "  Plans "));
        let json = serde_json::to_string(&easels).unwrap();
        let restored: Easels = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, easels);
        assert_eq!(restored.get(id).unwrap().title, "Plans");
        assert_eq!(restored.get(id).unwrap().hit(Point { x: 50., y: 40. }, 0.), Some(shape));
        assert!(restored.validate().is_ok());
    }

    #[test]
    fn urls_name_easels_and_blank_titles_fall_back() {
        let mut easels = Easels::default();
        let id = easels.create();
        assert_eq!(EaselId::from_url(&id.url()), Some(id));
        assert_eq!(EaselId::from_url("echo://history"), None);
        easels.rename(id, "   ");
        assert_eq!(easels.get(id).unwrap().title, DEFAULT_TITLE);
    }

    #[test]
    fn the_topmost_object_wins_and_ids_survive_undo() {
        let mut easels = Easels::default();
        let id = easels.create();
        let easel = easels.get_mut(id).unwrap();
        let frame = Frame { x: 0., y: 0., width: 10., height: 10. };
        let below = easel.add(frame, Content::Shape { kind: ShapeKind::Circle, color: Color::Red, points: Vec::new() });
        let saved = easel.objects.clone();
        let above = easel.add(frame, Content::Shape { kind: ShapeKind::Circle, color: Color::Red, points: Vec::new() });
        assert_eq!(easel.hit(Point { x: 5., y: 5. }, 0.), Some(above));
        easel.set_objects(saved);
        assert_eq!(easel.hit(Point { x: 5., y: 5. }, 0.), Some(below));
        assert!(easel.add(frame, Content::Image { file: "a.png".into() }) > above);
    }

    #[test]
    fn text_sizes_step_within_their_range() {
        assert_eq!(ParagraphStyle::Body.larger(), Some(ParagraphStyle::H2));
        assert_eq!(ParagraphStyle::Ultra.larger(), None);
        assert_eq!(ParagraphStyle::Body.smaller(), None);
        assert_eq!(ParagraphStyle::H0.smaller(), Some(ParagraphStyle::H1));
    }
}
