//! Floating panels share the reference's translucent look: a native blur
//! behind a tinted fill. HUD panels (site controls, the New menu) are
//! darker; context menus use the system menu material.
use gpui::{canvas, div, hsla, prelude::*, px, Bounds, Div, Hsla, Pixels, Window};

pub(crate) const RADIUS: f32 = 14.0;
pub(crate) const MENU_RADIUS: f32 = 10.0;

/// The panel fill over the native blur; opaque where there is none.
pub(crate) fn fill() -> Hsla {
    hsla(240. / 360., 0.04, 0.13, if crate::native::HAS_BACKDROP { 0.55 } else { 0.96 })
}

pub(crate) fn menu_fill() -> Hsla {
    hsla(210. / 360., 0.04, 0.34, if crate::native::HAS_BACKDROP { 0.4 } else { 0.97 })
}

/// Places (or with `None` removes) the blur named `key` in `window`.
pub(crate) fn backdrop(window: &Window, key: &str, bounds: Option<Bounds<Pixels>>, radius: f32, material: crate::native::Material) {
    let Some(view) = crate::native::view_of(window) else { return; };
    let rect = bounds.map(|bounds| [f64::from(f32::from(bounds.origin.x)), f64::from(f32::from(bounds.origin.y)),
        f64::from(f32::from(bounds.size.width)), f64::from(f32::from(bounds.size.height))]);
    crate::native::set_backdrop(view, key, rect, f64::from(radius), material);
}

/// A HUD panel shell; the blur follows its painted bounds.
pub(crate) fn shell(id: &'static str) -> gpui::Stateful<Div> {
    div().id(id).debug_selector(move || id.into())
        .relative().rounded(px(RADIUS)).bg(fill())
        .border_1().border_color(hsla(0., 0., 1., 0.12)).shadow_lg()
        .text_color(hsla(0., 0., 1., 0.92)).font_family(".SystemUIFont")
        .child(canvas(|_, _, _| (), |bounds, _, window, _| backdrop(window, "panel", Some(bounds), RADIUS, crate::native::Material::Hud)).absolute().inset_0())
}

/// A context menu shell drawn like a native menu; `key` names its blur.
pub(crate) fn menu_shell(id: impl Into<gpui::ElementId>, key: &'static str) -> gpui::Stateful<Div> {
    div().id(id)
        .relative().rounded(px(MENU_RADIUS)).bg(menu_fill())
        .border_1().border_color(hsla(0., 0., 0., 0.55)).shadow_lg()
        .text_color(hsla(0., 0., 1., 0.9)).font_family(".SystemUIFont").text_size(px(13.))
        .child(div().absolute().inset_0().rounded(px(MENU_RADIUS - 1.)).border_1().border_color(hsla(0., 0., 1., 0.14)))
        .child(canvas(|_, _, _| (), move |bounds, _, window, _| backdrop(window, key, Some(bounds), MENU_RADIUS, crate::native::Material::Menu)).absolute().inset_0())
}
