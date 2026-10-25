//! Floating panels share the reference's translucent look: a native blur
//! behind a tinted fill. HUD panels (site controls, the New menu) are
//! darker; context menus use the system menu material.
use gpui::{canvas, div, hsla, prelude::*, px, Bounds, Div, Hsla, Pixels, Window};

pub(crate) const RADIUS: f32 = 14.0;
pub(crate) const MENU_RADIUS: f32 = 12.0;

/// The panel fill over the native blur; opaque where there is none.
pub(crate) fn fill() -> Hsla {
    hsla(240. / 360., 0.04, 0.13, if crate::native::HAS_BACKDROP { 0.55 } else { 0.96 })
}

pub(crate) fn menu_fill(hue: f32, blurred: bool) -> Hsla {
    hsla(hue, 0.08, 0.17, if blurred { 0.5 } else { 1. })
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
pub(crate) fn menu_shell(id: impl Into<gpui::ElementId>, key: &'static str, hue: f32, installed: std::rc::Rc<std::cell::Cell<bool>>) -> gpui::Stateful<Div> {
    menu_shell_at(id, key, hue, installed, None, std::rc::Rc::new(std::cell::Cell::new(true)))
}

pub(crate) fn menu_shell_at(id: impl Into<gpui::ElementId>, key: &'static str, hue: f32, installed: std::rc::Rc<std::cell::Cell<bool>>, parent: Option<usize>, alive: std::rc::Rc<std::cell::Cell<bool>>) -> gpui::Stateful<Div> {
    menu_shell_impl(id,key,hue,installed,parent,alive,None)
}

pub(crate) fn menu_shell_at_appearance(id: impl Into<gpui::ElementId>, key: &'static str, hue: f32, installed: std::rc::Rc<std::cell::Cell<bool>>, parent: Option<usize>, alive: std::rc::Rc<std::cell::Cell<bool>>, dark: bool) -> gpui::Stateful<Div> {
    menu_shell_impl(id,key,hue,installed,parent,alive,Some(dark))
}

fn menu_shell_impl(id: impl Into<gpui::ElementId>, key: &'static str, hue: f32, installed: std::rc::Rc<std::cell::Cell<bool>>, parent: Option<usize>, alive: std::rc::Rc<std::cell::Cell<bool>>, appearance: Option<bool>) -> gpui::Stateful<Div> {
    let calibrated=appearance.is_some();let dark=appearance.unwrap_or(true);
    div().id(id)
        .relative().rounded(px(MENU_RADIUS)).bg(if parent.is_some() && installed.get() { if !calibrated{hsla(0.,0.,0.45,0.38)}else if dark {hsla(250./360.,0.18,0.28,0.38)}else{gpui::Hsla::from(gpui::rgb(0xfbf8ff)).opacity(0.99)} } else if dark { menu_fill(hue, installed.get()) } else {gpui::Hsla::from(gpui::rgb(0xfbf8ff))})
        .shadow_lg()
        .text_color(hsla(0., 0., if dark {1.}else{0.}, 0.85)).font_family(".SystemUIFont").when(calibrated,|r|r.font_weight(gpui::FontWeight::NORMAL)).text_size(px(13.))
        .child(div().absolute().inset_0().rounded(px(MENU_RADIUS - 1.)).border_1().border_color(hsla(0., 0., if dark {1.}else{0.}, 0.10)))
        .child(canvas(|_, _, _| (), move |bounds, _, window, cx| {
            use crate::material::*;
            if !alive.get() { return; }
            let blurred = parent.or_else(|| crate::native::view_of(window).map(|v|v as usize)).is_some_and(|view| {
                (if parent.is_some() { crate::native::set_content_backdrop } else { crate::native::set_backdrop_configured })(view as *mut std::ffi::c_void, key,
                    Some([f32::from(bounds.origin.x) as f64, f32::from(bounds.origin.y) as f64,
                        f32::from(bounds.size.width) as f64, f32::from(bounds.size.height) as f64]),
                    MENU_RADIUS as f64, BackdropConfig {
                        appearance: if dark {EffectAppearance::Dark}else{EffectAppearance::Light}, state: EffectState::Active,
                        blending: if parent.is_some() { EffectBlending::WithinWindow } else { EffectBlending::BehindWindow }, emphasized: false,
                        effect_alpha: 1., material: EffectMaterial::Menu,
                    }) == BackdropInstallation::Installed
            });
            if installed.replace(blurred) != blurred {
                // A refresh requested while painting is ignored by GPUI. Rebuild
                // the fill after this frame, even if the pointer stays still.
                window.defer(cx, |window, _| window.refresh());
            }
        }).absolute().inset_0())
}
