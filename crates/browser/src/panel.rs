//! Dark floating panels (site controls, the New menu) share the
//! reference's translucent HUD look: a native blur behind a tinted fill.
use gpui::{canvas, div, hsla, prelude::*, px, Div, Hsla};

pub(crate) const RADIUS: f32 = 14.0;

/// The panel fill over the native blur; opaque where there is none.
pub(crate) fn fill() -> Hsla {
    hsla(240. / 360., 0.04, 0.13, if crate::native::HAS_BACKDROP { 0.55 } else { 0.96 })
}

/// A panel shell; the blur follows its painted bounds.
pub(crate) fn shell(id: &'static str) -> gpui::Stateful<Div> {
    div().id(id).debug_selector(move || id.into())
        .relative().rounded(px(RADIUS)).bg(fill())
        .border_1().border_color(hsla(0., 0., 1., 0.12)).shadow_lg()
        .text_color(hsla(0., 0., 1., 0.92)).font_family(".SystemUIFont")
        .child(canvas(|_, _, _| (), |bounds, _, window, _| {
            if let Some(view) = crate::native::view_of(window) {
                let rect = [f64::from(f32::from(bounds.origin.x)), f64::from(f32::from(bounds.origin.y)),
                    f64::from(f32::from(bounds.size.width)), f64::from(f32::from(bounds.size.height))];
                crate::native::set_backdrop(view, Some(rect), f64::from(RADIUS));
            }
        }).absolute().inset_0())
}
