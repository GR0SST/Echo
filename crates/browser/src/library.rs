//! In-window Library presentation. The host owns navigation, pages and mutations.
use browser_core::{Browser, SpaceId, UserProfileId};
use gpui::{div, prelude::*, px, AnyElement, App, Hsla, Window};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LibrarySection {
    Media,
    Downloads,
    Easels,
    Spaces,
    Boosts,
    ArchivedTabs,
}

impl LibrarySection {
    pub(crate) const ALL: [Self; 6] = [
        Self::Media,
        Self::Downloads,
        Self::Easels,
        Self::Spaces,
        Self::Boosts,
        Self::ArchivedTabs,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Media => "Media",
            Self::Downloads => "Downloads",
            Self::Easels => "Easels",
            Self::Spaces => "Spaces",
            Self::Boosts => "Boosts",
            Self::ArchivedTabs => "Archived Tabs",
        }
    }
    pub(crate) fn selector(self) -> &'static str {
        match self {
            Self::Media => "library-nav-media",
            Self::Downloads => "library-nav-downloads",
            Self::Easels => "library-nav-easels",
            Self::Spaces => "library-nav-spaces",
            Self::Boosts => "library-nav-boosts",
            Self::ArchivedTabs => "library-nav-archive",
        }
    }
}

/// Every event carries its original target, rather than reading active_space
/// after a click or menu has changed selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LibraryIntent {
    Navigate(LibrarySection),
    SelectSpace(SpaceId),
    BeginSpaceCreation {
        origin: SpaceId,
    },
    OpenProfile {
        space: SpaceId,
        profile: UserProfileId,
    },
    EditTheme(SpaceId),
    Overflow(SpaceId),
}

pub(crate) type LibraryEvents = Rc<dyn Fn(LibraryIntent, &mut Window, &mut App)>;

#[derive(Clone, Debug)]
pub(crate) struct LibrarySpace {
    pub id: SpaceId,
    pub profile: UserProfileId,
    pub name: String,
    pub profile_name: String,
    pub icon: Option<String>,
    pub hue: f32,
}

/// Snapshot metadata only. Favorites, tabs, folders and their interactions are
/// rendered by the host's existing sidebar renderer, never copied here.
#[derive(Clone, Debug)]
pub(crate) struct LibraryProjection {
    pub origin: SpaceId,
    pub spaces: Vec<LibrarySpace>,
}
impl LibraryProjection {
    pub(crate) fn from_browser(browser: &Browser, origin: SpaceId) -> Option<Self> {
        if browser.is_blank()
            || browser.is_private()
            || !browser.spaces.iter().any(|space| space.id == origin)
        {
            return None;
        }
        let spaces = browser
            .spaces
            .iter()
            .map(|space| {
                Some(LibrarySpace {
                    id: space.id,
                    profile: space.user_profile_id,
                    name: space.name.clone(),
                    profile_name: browser.user_profile(space.user_profile_id)?.name().into(),
                    icon: space.custom_icon.clone(),
                    hue: space.hue,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self { origin, spaces })
    }
}

/// Opt in only after wiring a real action. Easels is always presentation-only.
#[derive(Clone, Copy, Default)]
pub(crate) struct LibraryActions {
    pub profile: bool,
    pub theme: bool,
    pub overflow: bool,
    pub media: bool,
    pub downloads: bool,
    pub boosts: bool,
    pub archive: bool,
}
impl LibraryActions {
    fn navigable(self, section: LibrarySection) -> bool {
        match section {
            LibrarySection::Spaces => true,
            LibrarySection::Media => self.media,
            LibrarySection::Downloads => self.downloads,
            LibrarySection::Boosts => self.boosts,
            LibrarySection::ArchivedTabs => self.archive,
            LibrarySection::Easels => false,
        }
    }
}

/// Colors come from the host's existing theme; this module defines no new palette.
#[derive(Clone, Copy)]
pub(crate) struct LibraryColors {
    pub background: Hsla,
    pub card_surface: fn(f32) -> Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
}

fn action(
    selector: String,
    intent: LibraryIntent,
    enabled: bool,
    events: LibraryEvents,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(gpui::SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .when(enabled, |el| {
            el.cursor_pointer().on_click(move |_, window, cx| {
                cx.stop_propagation();
                events(intent, window, cx);
            })
        })
        .when(!enabled, |el| el.opacity(0.45))
}

/// Render at the root browser-content bounds, not inside the ordinary sidebar
/// or an overlay window. `render_sidebar` supplies Favorites/Pinned/Today using
/// the captured Space and Profile IDs, with all native tab/menu/drag hooks.
/// The callback must return a shrinkable element (no fixed sidebar width).
/// `other_section` is host-owned; Media's actual inventory is outside this slice.
pub(crate) fn render_library(
    projection: &LibraryProjection,
    section: LibrarySection,
    colors: LibraryColors,
    actions: LibraryActions,
    events: LibraryEvents,
    mut render_sidebar: impl FnMut(&LibrarySpace, LibraryEvents) -> AnyElement,
    other_section: Option<AnyElement>,
) -> AnyElement {
    // The captured rail has six 96 × 80 icon tiles at a 100 px pitch.
    // Its 580 px group is centered as a whole, not pinned below the titlebar.
    // At short heights, retain the observed tile geometry and scroll the rail
    // instead of shrinking controls or centering inaccessible negative content.
    let mut nav = div()
        .id("library-nav-scroll")
        .debug_selector(|| "library-nav-scroll".into())
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(px(112.))
        .overflow_y_scroll()
        .overflow_x_hidden();
    nav.style().restrict_scroll_to_axis = Some(true);
    let nav = nav.child(
        div()
            .w_full()
            .h(px(580.))
            .min_h_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .debug_selector(|| "library-nav-group".into())
                    .flex_none()
                    .w(px(96.))
                    .h(px(580.))
                    .flex()
                    .flex_col()
                    .gap(px(20.))
                    .children(LibrarySection::ALL.into_iter().map(|item| {
                        action(
                            item.selector().into(),
                            LibraryIntent::Navigate(item),
                            actions.navigable(item),
                            events.clone(),
                        )
                        .relative()
                        .flex_none()
                        .w(px(96.))
                        .h(px(80.))
                        .rounded(px(12.))
                        // Unsupported routes remain visible without invented actions.
                        .opacity(1.)
                        .text_color(colors.muted)
                        .when(item == section, |el| {
                            el.text_color(colors.text).child(
                                div()
                                    .debug_selector(|| "library-nav-selected-fill".into())
                                    .absolute()
                                    .inset_0()
                                    .rounded(px(12.))
                                    .bg(colors.selected),
                            )
                        })
                        .when(actions.navigable(item), |el| {
                            el.hover(move |s| s.bg(colors.hover))
                        })
                        .child(
                            div()
                                .debug_selector(move || format!("{}-icon", item.selector()))
                                .absolute()
                                .left(px(36.))
                                .top(px(16.))
                                .size(px(24.))
                                .flex_none()
                                .child(navigation_icon(item, 1.)),
                        )
                        .child(
                            div()
                                .debug_selector(move || format!("{}-label", item.selector()))
                                .absolute()
                                .left_0()
                                .right_0()
                                .top(px(48.))
                                .h(px(12.))
                                .text_center()
                                .text_size(px(12.))
                                .line_height(px(12.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .child(item.label()),
                        )
                    })),
            ),
    );
    let mut root = div()
        .id("library")
        .debug_selector(|| "library".into())
        .relative()
        .size_full()
        .bg(colors.background)
        .text_color(colors.text)
        .child(nav);
    if section == LibrarySection::Spaces {
        let cards = projection
            .spaces
            .iter()
            .enumerate()
            .map(|(index, space)| {
                let sidebar = render_sidebar(space, events.clone());
                render_card(index, space, colors, actions, events.clone(), sidebar)
            })
            .collect::<Vec<_>>();
        let mut scroll = div()
            .id("library-spaces-scroll")
            .debug_selector(|| "library-spaces-scroll".into())
            .absolute()
            .left(px(112.))
            .right_0()
            .top(px(48.))
            .bottom(px(48.))
            .overflow_x_scroll()
            .overflow_y_hidden();
        // GPUI otherwise maps a vertical wheel delta onto this horizontal
        // strip as well as scrolling the nested sidebar body.
        scroll.style().restrict_scroll_to_axis = Some(true);
        root = root.child(
            scroll.child(
                div()
                    .flex()
                    .w(px(104. + projection.spaces.len() as f32 * 264.))
                    .h_full()
                    .items_center()
                    .gap(px(24.))
                    .px(px(28.))
                    .children(cards)
                    .child(
                        action(
                            "library-new-space".into(),
                            LibraryIntent::BeginSpaceCreation {
                                origin: projection.origin,
                            },
                            true,
                            events,
                        )
                        .flex_none()
                        .size(px(48.))
                        .rounded_full()
                        .bg(colors.selected)
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |s| s.bg(colors.hover))
                        .child(crate::icons::control(crate::icons::Icon::Plus)),
                    ),
            ),
        );
    } else if let Some(content) = other_section {
        root = root.child(
            div()
                .absolute()
                .left(px(112.))
                .right_0()
                .top_0()
                .bottom_0()
                .child(content),
        );
    }
    root.into_any_element()
}

/// Unbranded, fixed 24 px section artwork. No font-dependent control glyphs.
fn navigation_icon(section: LibrarySection, scale: f32) -> AnyElement {
    let contours = match section {
        LibrarySection::Media => vec![
            vec![
                (2., 4.),
                (17., 2.),
                (18., 7.),
                (5., 7.),
                (5., 17.),
                (3., 17.),
                (2., 4.),
            ],
            nav_rect(5., 7., 17., 14., 1.5),
            vec![(7., 18.), (11., 13.), (14., 16.), (17., 12.), (20., 16.)],
            nav_circle(10., 11., 1.),
        ],
        LibrarySection::Downloads => vec![
            nav_circle(12., 12., 9.),
            vec![(12., 6.), (12., 17.)],
            vec![(8., 13.), (12., 17.), (16., 13.)],
        ],
        LibrarySection::Easels => vec![
            nav_rect(3., 3., 18., 18., 2.),
            vec![(3., 7.), (21., 7.)],
            vec![
                (14., 10.),
                (10., 9.),
                (9., 11.),
                (14., 15.),
                (13., 17.),
                (8., 13.),
                (8., 16.),
                (11., 19.),
            ],
        ],
        LibrarySection::Spaces => vec![
            vec![(4., 5.), (14., 2.), (19., 18.), (9., 21.), (4., 5.)],
            nav_rect(10., 5., 11., 17., 2.),
        ],
        LibrarySection::Boosts => vec![
            vec![
                (5., 12.),
                (3., 6.),
                (4., 4.),
                (17., 1.),
                (20., 3.),
                (23., 15.),
                (21., 18.),
                (14., 20.),
            ],
            vec![(7., 10.), (10., 6.), (17., 13.), (13., 17.), (7., 10.)],
            vec![
                (10., 14.),
                (7., 17.),
                (5., 16.),
                (2., 19.),
                (2., 21.),
                (4., 22.),
                (7., 19.),
                (7., 17.),
            ],
            vec![(17., 5.), (17., 9.)],
            vec![(15., 7.), (19., 7.)],
        ],
        LibrarySection::ArchivedTabs => vec![
            nav_rect(3., 4., 18., 4., 1.),
            nav_rect(5., 8., 14., 13., 1.5),
            vec![(9., 11.), (15., 11.)],
        ],
    };
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for points in &contours {
                let mut path = gpui::PathBuilder::stroke(px(1.4 * scale));
                for (index, &(x, y)) in points.iter().enumerate() {
                    let point = bounds.origin + gpui::point(px(x * scale), px(y * scale));
                    if index == 0 {
                        path.move_to(point);
                    } else {
                        path.line_to(point);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, window.text_style().color);
                }
            }
        },
    )
    .size(px(24. * scale))
    .flex_none()
    .into_any_element()
}

fn nav_circle(x: f32, y: f32, radius: f32) -> Vec<(f32, f32)> {
    (0..=32)
        .map(|index| {
            let angle = index as f32 * std::f32::consts::TAU / 32.;
            (x + radius * angle.cos(), y + radius * angle.sin())
        })
        .collect()
}

fn nav_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Vec<(f32, f32)> {
    let mut points = Vec::new();
    for (cx, cy, start) in [
        (x + width - radius, y + radius, -1.),
        (x + width - radius, y + height - radius, 0.),
        (x + radius, y + height - radius, 1.),
        (x + radius, y + radius, 2.),
    ] {
        for index in 0..=6 {
            let angle = (start + index as f32 / 6.) * std::f32::consts::FRAC_PI_2;
            points.push((cx + radius * angle.cos(), cy + radius * angle.sin()));
        }
    }
    points.push(points[0]);
    points
}

fn render_card(
    index: usize,
    space: &LibrarySpace,
    colors: LibraryColors,
    actions: LibraryActions,
    events: LibraryEvents,
    sidebar: AnyElement,
) -> AnyElement {
    let header = action(
        format!("library-space-select-{index}"),
        LibraryIntent::SelectSpace(space.id),
        true,
        events.clone(),
    )
    .flex()
    .flex_none()
    .min_w_0()
    .w_full()
    .items_center()
    .gap(px(8.))
    .h(px(32.))
    .child(match &space.icon {
        Some(icon) => crate::icons::glyph(icon),
        None => div()
            .flex_none()
            .size(px(7.))
            .rounded_full()
            .bg(colors.text)
            .into_any_element(),
    })
    .child(
        div()
            .debug_selector(move || format!("library-space-name-{index}"))
            .flex_1()
            .min_w_0()
            .truncate()
            .child(space.name.clone()),
    );
    let profile = action(
        format!("library-space-profile-{index}"),
        LibraryIntent::OpenProfile {
            space: space.id,
            profile: space.profile,
        },
        actions.profile,
        events.clone(),
    )
    .flex()
    .items_center()
    .min_w_0()
    .max_w(px(180.))
    .h(px(24.))
    .rounded_full()
    .px(px(8.))
    .py(px(4.))
    .bg(colors.selected)
    .text_size(px(11.))
    .child(
        div()
            .debug_selector(move || format!("library-space-profile-name-{index}"))
            .min_w_0()
            .truncate()
            .child(space.profile_name.clone()),
    );
    let theme = action(
        format!("library-space-theme-{index}"),
        LibraryIntent::EditTheme(space.id),
        actions.theme,
        events.clone(),
    )
    .size(px(28.))
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .child(crate::icons::control(crate::icons::Icon::SiteControls));
    div()
        .id(("library-space", index))
        .debug_selector(move || format!("library-space-{index}"))
        .flex_none()
        .w(px(240.))
        .h_full()
        .rounded_xl()
        .overflow_hidden()
        // The host supplies its established Space theme mapping.
        .bg((colors.card_surface)(space.hue))
        .flex()
        .flex_col()
        .p(px(12.))
        .gap(px(8.))
        .child(header)
        .child(
            div()
                .flex()
                .flex_none()
                .min_w_0()
                .h(px(28.))
                .gap(px(8.))
                .items_center()
                .justify_between()
                .child(profile)
                .child(theme),
        )
        .child(
            div()
                .debug_selector(move || format!("library-space-body-{index}"))
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(sidebar),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(28.))
                .flex_none()
                // Handle appearance only: reorder is not implemented by this module.
                .child(
                    div()
                        .debug_selector(move || format!("library-space-handle-{index}"))
                        .flex_none()
                        .w(px(24.))
                        .h(px(3.))
                        .rounded_full()
                        .bg(colors.muted),
                )
                .child(
                    action(
                        format!("library-space-overflow-{index}"),
                        LibraryIntent::Overflow(space.id),
                        actions.overflow,
                        events,
                    )
                    .size(px(28.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(crate::icons::more()),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;

#[path = "library_media.rs"]
pub(crate) mod media;
