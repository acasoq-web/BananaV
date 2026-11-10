//! BananaV's look: the colors of each interface type of Options ›
//! Preferences › Display (Dark, Medium, Light, White), banana yellow as the
//! accent, and the egui style made from them. The colors are read through
//! functions (`theme::panel()`), so a change of interface type shows at
//! once.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicI8, AtomicU8, AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

use crate::egui::{self, Color32, CornerRadius, Stroke};
use crate::i18n::{tr, trc};

/// Options › Preferences › Display › "Interface Type".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interface {
    #[default]
    Dark,
    Medium,
    Light,
    White,
}

impl Interface {
    pub const ALL: [Interface; 4] = [
        Interface::Dark,
        Interface::Medium,
        Interface::Light,
        Interface::White,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Interface::Dark => tr("Dark"),
            Interface::Medium => tr("Medium"),
            Interface::Light => trc("interface", "Light"),
            Interface::White => tr("White"),
        }
    }

    pub fn palette(self) -> &'static Palette {
        match self {
            Interface::Dark => &DARK,
            Interface::Medium => &MEDIUM,
            Interface::Light => &LIGHT,
            Interface::White => &WHITE,
        }
    }
}

/// The colors of an interface type.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Dark text on light panels.
    pub light: bool,
    pub banana: Color32,
    pub banana_dim: Color32,
    pub bg: Color32,
    pub panel: Color32,
    pub panel_dark: Color32,
    pub hover: Color32,
    pub separator: Color32,
    pub knob: Color32,
    pub track_video: Color32,
    pub track_audio: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    /// Text that stands out (a selected row's).
    pub text_strong: Color32,
    pub cursor: Color32,
    /// The accent of open tabs and focused lists.
    pub snap: Color32,
    pub loop_bar: Color32,
    /// The loop region's bar: a grey of its own, a step from the panels
    /// around it (a drag there makes the loop region), lighter under the
    /// pointer.
    pub loop_band: Color32,
    pub loop_band_hover: Color32,
    /// The loop region in its bar, its handles and the lines down the
    /// timeline at its ends (VEGAS's yellow), the tracks' lighter (or
    /// darker) tint within it and the room under the tracks within it.
    pub loop_strip: Color32,
    pub loop_handle: Color32,
    pub loop_tint: Color32,
    pub loop_below: Color32,
    pub marker: Color32,
    pub region: Color32,
    pub error: Color32,
    /// A track header that is selected, and selected with the keys on its
    /// list.
    pub header_selected: Color32,
    pub header_selected_focused: Color32,
    /// The open tab of a tabbed window.
    pub tab_on: Color32,
    /// The selected row of lists and the chosen category of the plug-in
    /// windows (VEGAS's blue).
    pub selection: Color32,
    pub check_border: Color32,
    /// Faint lines: rows of long lists.
    pub faint_line: Color32,
    /// The timeline's dotted grid lines (VEGAS's #505050 on its dark
    /// timeline).
    pub grid: Color32,
    /// Buttons and other widgets: at rest, under the pointer, open; their
    /// text.
    pub widget: Color32,
    pub widget_hover: Color32,
    pub widget_open: Color32,
    pub widget_text: Color32,
}

const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// Neutral greys, as VEGAS's dark interface (its panels #282828, the bars
/// between them #141414, the timeline #1E1E1E).
const DARK: Palette = Palette {
    light: false,
    banana: rgb(0xF7C948),
    banana_dim: rgb(0x8A7022),
    bg: rgb(0x1E1E1E),
    panel: rgb(0x282828),
    panel_dark: rgb(0x141414),
    hover: rgb(0x3A3A3A),
    separator: rgb(0x3C3C3C),
    knob: rgb(0xBBBBBB),
    track_video: rgb(0x292929),
    track_audio: rgb(0x262927),
    text: rgb(0xE6E6E6),
    text_dim: rgb(0x9A9A9A),
    text_disabled: rgb(0x5A5A5A),
    text_strong: rgb(0xFFFFFF),
    cursor: rgb(0xFFFFFF),
    snap: rgb(0x5CC8FF),
    loop_bar: rgb(0x6C8ED8),
    loop_band: rgb(0x313131),
    loop_band_hover: rgb(0x3B3B3B),
    loop_strip: rgb(0x4B4B4B),
    loop_handle: rgb(0xF0C83C),
    loop_tint: Color32::from_rgba_premultiplied(20, 20, 20, 20),
    loop_below: rgb(0x2B4D70),
    marker: rgb(0xF09A3E),
    region: rgb(0x5DC07A),
    error: rgb(0xFF7A6E),
    header_selected: rgb(0x3E3E3E),
    header_selected_focused: rgb(0x34465E),
    tab_on: rgb(0x2C6E8F),
    selection: rgb(0x3C8CDC),
    check_border: rgb(0x8C8C8C),
    faint_line: Color32::from_rgba_premultiplied(14, 14, 14, 14),
    grid: rgb(0x505050),
    widget: rgb(0x3C3C3C),
    widget_hover: rgb(0x464646),
    widget_open: rgb(0x2D2D2D),
    widget_text: rgb(0xB4B4B4),
};

const MEDIUM: Palette = Palette {
    light: false,
    banana: rgb(0xF7C948),
    banana_dim: rgb(0x9A7D26),
    bg: rgb(0x3E3E3E),
    panel: rgb(0x4B4B4B),
    panel_dark: rgb(0x363636),
    hover: rgb(0x5B5B5B),
    separator: rgb(0x646464),
    knob: rgb(0xD2D2D2),
    track_video: rgb(0x474747),
    track_audio: rgb(0x454946),
    text: rgb(0xF0F0F0),
    text_dim: rgb(0xBFBFBF),
    text_disabled: rgb(0x808080),
    text_strong: rgb(0xFFFFFF),
    cursor: rgb(0xFFFFFF),
    snap: rgb(0x6ED0FF),
    loop_bar: rgb(0x7E9EE6),
    loop_band: rgb(0x555555),
    loop_band_hover: rgb(0x5F5F5F),
    loop_strip: rgb(0x707070),
    loop_handle: rgb(0xF0C83C),
    loop_tint: Color32::from_rgba_premultiplied(20, 20, 20, 20),
    loop_below: rgb(0x365C82),
    marker: rgb(0xF5A54E),
    region: rgb(0x6FD08C),
    error: rgb(0xFF8B80),
    header_selected: rgb(0x606060),
    header_selected_focused: rgb(0x52647E),
    tab_on: rgb(0x3A7FA2),
    selection: rgb(0x3C8CDC),
    check_border: rgb(0xA8A8A8),
    faint_line: Color32::from_rgba_premultiplied(18, 18, 18, 18),
    grid: rgb(0x6E6E6E),
    widget: rgb(0x5E5E5E),
    widget_hover: rgb(0x6B6B6B),
    widget_open: rgb(0x4E4E4E),
    widget_text: rgb(0xDCDCDC),
};

const LIGHT: Palette = Palette {
    light: true,
    banana: rgb(0x5A3F00),
    banana_dim: rgb(0xD8B23E),
    bg: rgb(0x8F8F8F),
    panel: rgb(0xA0A0A0),
    panel_dark: rgb(0x888888),
    hover: rgb(0xB1B1B1),
    separator: rgb(0x757575),
    knob: rgb(0x474747),
    track_video: rgb(0x979797),
    track_audio: rgb(0x959995),
    text: rgb(0x111111),
    text_dim: rgb(0x323232),
    text_disabled: rgb(0x666666),
    text_strong: rgb(0x000000),
    cursor: rgb(0x000000),
    snap: rgb(0x175F92),
    loop_bar: rgb(0x3A5CB0),
    loop_band: rgb(0x979797),
    loop_band_hover: rgb(0xA8A8A8),
    loop_strip: rgb(0xBEBEBE),
    loop_handle: rgb(0xB88A08),
    loop_tint: Color32::from_rgba_premultiplied(0, 0, 0, 22),
    loop_below: rgb(0x7FA2C8),
    marker: rgb(0xB8620E),
    region: rgb(0x23804A),
    error: rgb(0x9E1C14),
    header_selected: rgb(0xB8B8B8),
    header_selected_focused: rgb(0x9DB2D2),
    tab_on: rgb(0x86BADA),
    selection: rgb(0x3C8CDC),
    check_border: rgb(0x474747),
    faint_line: Color32::from_black_alpha(26),
    grid: rgb(0x6E6E6E),
    widget: rgb(0xB3B3B3),
    widget_hover: rgb(0xC1C1C1),
    widget_open: rgb(0xA7A7A7),
    widget_text: rgb(0x151515),
};

const WHITE: Palette = Palette {
    light: true,
    banana: rgb(0x5A3F00),
    banana_dim: rgb(0xE6C458),
    bg: rgb(0xBFBFBF),
    panel: rgb(0xD0D0D0),
    panel_dark: rgb(0xB8B8B8),
    hover: rgb(0xE1E1E1),
    separator: rgb(0xA2A2A2),
    knob: rgb(0x505050),
    track_video: rgb(0xC4C4C4),
    track_audio: rgb(0xC2C6C3),
    text: rgb(0x111111),
    text_dim: rgb(0x3E3E3E),
    text_disabled: rgb(0x8A8A8A),
    text_strong: rgb(0x000000),
    cursor: rgb(0x000000),
    snap: rgb(0x175F92),
    loop_bar: rgb(0x3A5CB0),
    loop_band: rgb(0xC5C5C5),
    loop_band_hover: rgb(0xD6D6D6),
    loop_strip: rgb(0xEBEBEB),
    loop_handle: rgb(0xB88A08),
    loop_tint: Color32::from_rgba_premultiplied(0, 0, 0, 16),
    loop_below: rgb(0xA9C5E2),
    marker: rgb(0xB8620E),
    region: rgb(0x23804A),
    error: rgb(0x9E1C14),
    header_selected: rgb(0xE4E4E4),
    header_selected_focused: rgb(0xBACFEB),
    tab_on: rgb(0xA8D0E8),
    selection: rgb(0x3C8CDC),
    check_border: rgb(0x606060),
    faint_line: Color32::from_black_alpha(20),
    grid: rgb(0x9C9C9C),
    widget: rgb(0xDBDBDB),
    widget_hover: rgb(0xE8E8E8),
    widget_open: rgb(0xCFCFCF),
    widget_text: rgb(0x151515),
};

/// The interface type shown (an index in [`Interface::ALL`]).
static INTERFACE: AtomicU8 = AtomicU8::new(0);
/// Options › Preferences › Display › "Button Highlight Color" (RGBA).
static BUTTON_HIGHLIGHT: AtomicU32 = AtomicU32::new(u32::from_be_bytes(DEFAULT_HIGHLIGHT));
/// Options › Preferences › Display › "Icon color strength" (0–100 %).
static ICON_STRENGTH: AtomicU8 = AtomicU8::new(100);

/// BananaV's highlight of active toolbar buttons: dim banana.
pub const DEFAULT_HIGHLIGHT: [u8; 4] = [0x6C, 0x5A, 0x24, 0xFF];

pub fn set_interface(kind: Interface) {
    let i = Interface::ALL.iter().position(|&k| k == kind).unwrap_or(0);
    INTERFACE.store(i as u8, Ordering::Relaxed);
}

pub fn interface() -> Interface {
    Interface::ALL[INTERFACE.load(Ordering::Relaxed) as usize % Interface::ALL.len()]
}

pub fn set_button_highlight(rgba: [u8; 4]) {
    BUTTON_HIGHLIGHT.store(u32::from_be_bytes(rgba), Ordering::Relaxed);
}

/// The fill of an active toolbar button.
pub fn button_highlight() -> Color32 {
    let [r, g, b, a] = BUTTON_HIGHLIGHT.load(Ordering::Relaxed).to_be_bytes();
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

pub fn set_icon_strength(percent: u8) {
    ICON_STRENGTH.store(percent.min(100), Ordering::Relaxed);
}

/// `color` as icons show it: its saturation scaled by the icon color
/// strength (grey at 0 %).
pub fn icon_color(color: Color32) -> Color32 {
    desaturate(color, ICON_STRENGTH.load(Ordering::Relaxed))
}

/// `color` with its saturation scaled to `percent`: the color mixed with
/// its own grey (Rec. 709 luma), alpha kept.
pub fn desaturate(color: Color32, percent: u8) -> Color32 {
    if percent >= 100 {
        return color;
    }
    let k = percent as f32 / 100.0;
    let [r, g, b, a] = color.to_array();
    let luma = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
    let mix = |c: u8| (luma + (c as f32 - luma) * k).round().clamp(0.0, 255.0) as u8;
    // Premultiplied channels stay within alpha: the grey of a premultiplied
    // color is premultiplied too.
    Color32::from_rgba_premultiplied(mix(r), mix(g), mix(b), a)
}

fn palette() -> &'static Palette {
    shown(interface(), brightness())
}

/// Options › Preferences › Display: "Interface brightness" (BananaV's own):
/// the interface type's greys lighter (up to 40 %) or darker.
static BRIGHTNESS: AtomicI8 = AtomicI8::new(0);
/// The palettes made for a type and a brightness (−40 to 40), each once
/// and kept for the program's life.
static MADE: [[OnceLock<&'static Palette>; 81]; 4] = [const { [const { OnceLock::new() }; 81] }; 4];

/// The brightness from now on, −40 to 40 (%).
pub fn set_brightness(percent: i8) {
    BRIGHTNESS.store(percent.clamp(-40, 40), Ordering::Relaxed);
}

pub fn brightness() -> i8 {
    BRIGHTNESS.load(Ordering::Relaxed)
}

/// `kind`'s palette at `brightness` (%): its greys — backgrounds, panels,
/// tracks, widgets, lines — toward white or black; text and accents as
/// they are.
pub fn shown(kind: Interface, brightness: i8) -> &'static Palette {
    if brightness == 0 {
        return kind.palette();
    }
    let i = Interface::ALL.iter().position(|&k| k == kind).unwrap_or(0);
    let b = brightness.clamp(-40, 40);
    MADE[i % 4][(i32::from(b) + 40) as usize]
        .get_or_init(|| Box::leak(Box::new(brightened(kind.palette(), b))))
}

/// `p` with its greys moved by `percent`: toward white (three quarters of
/// the way at most) or black.
fn brightened(p: &Palette, percent: i8) -> Palette {
    let k = f32::from(percent.clamp(-40, 40)) / 100.0;
    let shade = |c: Color32| {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        let f = |v: u8| {
            let v = f32::from(v);
            let out = if k >= 0.0 {
                v + (255.0 - v) * k * 0.75
            } else {
                v * (1.0 + k)
            };
            out.round().clamp(0.0, 255.0) as u8
        };
        Color32::from_rgba_unmultiplied(f(r), f(g), f(b), a)
    };
    Palette {
        bg: shade(p.bg),
        panel: shade(p.panel),
        panel_dark: shade(p.panel_dark),
        hover: shade(p.hover),
        separator: shade(p.separator),
        track_video: shade(p.track_video),
        track_audio: shade(p.track_audio),
        header_selected: shade(p.header_selected),
        header_selected_focused: shade(p.header_selected_focused),
        check_border: shade(p.check_border),
        faint_line: shade(p.faint_line),
        grid: shade(p.grid),
        widget: shade(p.widget),
        widget_hover: shade(p.widget_hover),
        widget_open: shade(p.widget_open),
        loop_band: shade(p.loop_band),
        loop_band_hover: shade(p.loop_band_hover),
        loop_strip: shade(p.loop_strip),
        ..*p
    }
}

pub fn banana() -> Color32 {
    palette().banana
}
pub fn banana_dim() -> Color32 {
    palette().banana_dim
}
pub fn bg() -> Color32 {
    palette().bg
}
pub fn panel() -> Color32 {
    palette().panel
}
pub fn panel_dark() -> Color32 {
    palette().panel_dark
}
pub fn hover() -> Color32 {
    palette().hover
}
pub fn separator() -> Color32 {
    palette().separator
}
pub fn knob() -> Color32 {
    palette().knob
}
pub fn track_video() -> Color32 {
    palette().track_video
}
pub fn track_audio() -> Color32 {
    palette().track_audio
}
pub fn text() -> Color32 {
    palette().text
}
pub fn text_dim() -> Color32 {
    palette().text_dim
}
pub fn text_disabled() -> Color32 {
    palette().text_disabled
}
pub fn text_strong() -> Color32 {
    palette().text_strong
}
pub fn cursor() -> Color32 {
    palette().cursor
}
pub fn snap() -> Color32 {
    palette().snap
}
pub fn loop_bar() -> Color32 {
    palette().loop_bar
}
pub fn loop_band() -> Color32 {
    palette().loop_band
}
pub fn loop_band_hover() -> Color32 {
    palette().loop_band_hover
}
pub fn loop_strip() -> Color32 {
    palette().loop_strip
}
pub fn loop_handle() -> Color32 {
    palette().loop_handle
}
pub fn loop_tint() -> Color32 {
    palette().loop_tint
}
pub fn loop_below() -> Color32 {
    palette().loop_below
}
pub fn marker() -> Color32 {
    palette().marker
}
pub fn region() -> Color32 {
    palette().region
}
pub fn error() -> Color32 {
    palette().error
}
pub fn header_selected() -> Color32 {
    palette().header_selected
}
pub fn header_selected_focused() -> Color32 {
    palette().header_selected_focused
}
pub fn tab_on() -> Color32 {
    palette().tab_on
}
pub fn selection() -> Color32 {
    palette().selection
}
pub fn check_border() -> Color32 {
    palette().check_border
}
pub fn faint_line() -> Color32 {
    palette().faint_line
}

pub fn grid() -> Color32 {
    palette().grid
}

/// The edit cursor and the selected events while Auto Ripple is on.
pub fn ripple() -> Color32 {
    rgb(0xFF6A3D)
}
/// The outline of the selected events the user picked (clicked), as
/// VEGAS's: yellow in every look.
pub fn event_picked() -> Color32 {
    rgb(0xFFFF5A)
}
/// The outline of the events selected along with a picked one, for being
/// in its group: VEGAS's blue.
pub fn event_grouped() -> Color32 {
    rgb(0x3C8CDC)
}
/// The outline of the events linked to a selected one (sync links), as
/// VEGAS's: purple.
pub fn event_sync_linked() -> Color32 {
    rgb(0xB45AE6)
}
/// The speaker of audio generators' previews.
pub fn clip_audio() -> Color32 {
    rgb(0x2F7A64)
}
/// The bar over prerendered parts of the timeline.
pub fn prerendered() -> Color32 {
    rgb(0x3A86FF)
}

/// Sets egui's style from the interface type shown.
pub fn apply(ctx: &egui::Context) {
    apply_kind(ctx, interface());
}

/// Sets egui's style from the interface type `kind`.
pub fn apply_kind(ctx: &egui::Context, kind: Interface) {
    let p = shown(kind, brightness());
    // One style slot, holding the visuals of the interface type.
    ctx.set_theme(egui::Theme::Dark);
    ctx.global_style_mut(|style| {
        let mut v = if p.light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        v.panel_fill = p.panel;
        v.window_fill = p.panel;
        v.window_stroke = Stroke::new(1.0, p.separator);
        v.extreme_bg_color = p.panel_dark;
        v.faint_bg_color = p.bg;
        v.code_bg_color = p.panel_dark;
        v.error_fg_color = p.error;
        v.selection.bg_fill = p.banana_dim;
        v.selection.stroke = Stroke::new(1.0, p.banana);
        v.hyperlink_color = p.banana;
        let w = &mut v.widgets;
        w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
        w.noninteractive.bg_stroke = Stroke::new(1.0, p.separator);
        w.noninteractive.bg_fill = p.panel;
        w.noninteractive.weak_bg_fill = p.panel;
        w.inactive.weak_bg_fill = p.widget;
        w.inactive.bg_fill = p.widget;
        w.inactive.fg_stroke = Stroke::new(1.0, p.widget_text);
        w.hovered.weak_bg_fill = p.widget_hover;
        w.hovered.bg_fill = p.widget_hover;
        w.hovered.bg_stroke = Stroke::new(1.0, p.banana_dim);
        w.hovered.fg_stroke = Stroke::new(1.5, p.text_strong);
        w.active.bg_fill = p.banana_dim;
        w.active.weak_bg_fill = p.banana_dim;
        w.active.fg_stroke = Stroke::new(2.0, p.text_strong);
        w.open.weak_bg_fill = p.widget_open;
        w.open.bg_fill = p.panel;
        w.open.fg_stroke = Stroke::new(1.0, p.text);
        v.slider_trailing_fill = true;
        let r = CornerRadius::same(3);
        w.noninteractive.corner_radius = r;
        w.inactive.corner_radius = r;
        w.hovered.corner_radius = r;
        w.active.corner_radius = r;
        style.visuals = v;
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.slider_width = 140.0;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grey_keeps_its_brightness_and_full_strength_keeps_the_color() {
        let red = Color32::from_rgb(0xE0, 0x40, 0x30);
        assert_eq!(desaturate(red, 100), red);
        let grey = desaturate(red, 0);
        let [r, g, b, a] = grey.to_array();
        assert_eq!((r, g), (g, b), "grey");
        assert_eq!(a, 255);
        let luma = 0.2126 * 224.0 + 0.7152 * 64.0 + 0.0722 * 48.0;
        assert!((r as f32 - luma).abs() <= 1.0);
        let half = desaturate(red, 50).to_array();
        assert!(half[0] < 0xE0 && half[0] > r, "halfway: {half:?}");
    }

    #[test]
    fn text_stands_out_from_the_panels_of_every_interface_type() {
        // Relative luminance contrast (WCAG) of text on panels.
        fn lum(c: Color32) -> f32 {
            let f = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            let [r, g, b, _] = c.to_array();
            0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
        }
        fn contrast(a: Color32, b: Color32) -> f32 {
            let (x, y) = (lum(a), lum(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        }
        for kind in Interface::ALL {
            let p = kind.palette();
            for (what, fg, bg, least) in [
                ("text on panels", p.text, p.panel, 4.5),
                ("text on the dark panels", p.text, p.panel_dark, 4.5),
                ("dim text on panels", p.text_dim, p.panel, 3.0),
                ("button text", p.widget_text, p.widget, 4.5),
                ("accent text", p.banana, p.panel, 3.0),
                ("the cursor on the tracks", p.cursor, p.track_video, 3.0),
            ] {
                let c = contrast(fg, bg);
                assert!(c >= least, "{kind:?}: {what} {c:.2}");
            }
        }
    }

    #[test]
    fn the_interface_type_is_kept_by_name() {
        for kind in Interface::ALL {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(serde_json::from_str::<Interface>(&json).unwrap(), kind);
        }
        assert_eq!(serde_json::to_string(&Interface::Dark).unwrap(), "\"Dark\"");
    }

    #[test]
    fn brightness_moves_the_greys_and_keeps_text_and_accents() {
        let base = shown(Interface::Dark, 0);
        assert!(std::ptr::eq(base, Interface::Dark.palette()));
        let lighter = shown(Interface::Dark, 40);
        let darker = shown(Interface::Dark, -40);
        let luma = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
        assert!(luma(lighter.panel) > luma(base.panel));
        assert!(luma(darker.panel) < luma(base.panel));
        assert!(luma(lighter.track_video) > luma(base.track_video));
        assert_eq!(lighter.text, base.text);
        assert_eq!(darker.banana, base.banana);
        assert_eq!(darker.loop_handle, base.loop_handle);
        // The same palette each time (made once), within the range.
        assert!(std::ptr::eq(lighter, shown(Interface::Dark, 40)));
        assert!(std::ptr::eq(
            shown(Interface::Light, 99),
            shown(Interface::Light, 40)
        ));
    }
}
