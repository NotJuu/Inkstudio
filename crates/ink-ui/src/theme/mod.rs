//! Theme: maps design tokens onto egui's `Style` and exposes the active palette.
//!
//! Widgets read colors with `theme::palette(ui)` and sizes from `tokens::*`.
//! Never write a raw `Color32` or pixel number in a widget.

pub mod tokens;

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, Style, TextStyle, Visuals,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use tokens::Palette;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Dark,
    Light,
}

impl Mode {
    pub fn palette(self) -> &'static Palette {
        match self {
            Mode::Dark => &tokens::DARK,
            Mode::Light => &tokens::LIGHT,
        }
    }
}

/// Font families beyond egui's Proportional / Monospace.
pub fn medium() -> FontFamily {
    FontFamily::Name("ui-medium".into())
}
pub fn semibold() -> FontFamily {
    FontFamily::Name("ui-semibold".into())
}

/// Font for icon glyphs (Phosphor, MIT). Never mix icons into a text string; draw them with this.
pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("phosphor".into()))
}

/// Named text styles used by widgets.
pub mod text {
    use egui::TextStyle;
    pub fn caption() -> TextStyle {
        TextStyle::Small
    }
    pub fn body() -> TextStyle {
        TextStyle::Body
    }
    pub fn body_strong() -> TextStyle {
        TextStyle::Name("body-strong".into())
    }
    pub fn label() -> TextStyle {
        TextStyle::Name("label".into())
    }
    pub fn title() -> TextStyle {
        TextStyle::Name("title".into())
    }
    pub fn heading() -> TextStyle {
        TextStyle::Heading
    }
    pub fn display() -> TextStyle {
        TextStyle::Name("display".into())
    }
}

/// Install fonts and style for `mode`. Call once at startup and again when the theme changes.
pub fn apply(ctx: &Context, mode: Mode) {
    ctx.set_fonts(font_definitions(&font_dirs()));
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, mode));
    ctx.set_theme(match mode {
        Mode::Dark => egui::Theme::Dark,
        Mode::Light => egui::Theme::Light,
    });
    let st = style(mode);
    ctx.all_styles_mut(|s| *s = st.clone());
}

/// The palette for the mode last passed to [`apply`].
pub fn palette_ctx(ctx: &Context) -> &'static Palette {
    ctx.data(|d| d.get_temp::<Mode>(egui::Id::NULL)).unwrap_or_default().palette()
}

pub fn palette(ui: &egui::Ui) -> &'static Palette {
    palette_ctx(ui.ctx())
}

/// Where to look for the open fonts (Inter, JetBrains Mono, Noto CJK).
/// The shipped app bundles them under `assets/fonts`; development falls back to system paths.
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(dir) = std::env::var("INKSTUDIO_FONT_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts"));
    for p in [
        "/usr/share/fonts/opentype/inter",
        "/usr/share/fonts/truetype/inter",
        "/usr/share/fonts/truetype/jetbrains-mono",
        "/usr/share/fonts/opentype/noto",
        "/Library/Fonts",
        "C:\\Windows\\Fonts",
    ] {
        dirs.push(PathBuf::from(p));
    }
    dirs
}

fn find(dirs: &[PathBuf], names: &[&str]) -> Option<Vec<u8>> {
    dirs.iter()
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find_map(|p: PathBuf| std::fs::read(Path::new(&p)).ok())
}

fn font_definitions(dirs: &[PathBuf]) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut add = |key: &str, files: &[&str]| -> bool {
        match find(dirs, files) {
            Some(bytes) => {
                fonts.font_data.insert(key.into(), Arc::new(FontData::from_owned(bytes)));
                true
            }
            None => false,
        }
    };
    let regular = add("inter-regular", &["Inter-Regular.otf", "Inter-Regular.ttf"]);
    let medium_ok = add("inter-medium", &["Inter-Medium.otf", "Inter-Medium.ttf"]);
    let semibold_ok = add("inter-semibold", &["Inter-SemiBold.otf", "Inter-SemiBold.ttf"]);
    let mono = add("jetbrains-mono", &["JetBrainsMono-Regular.ttf"]);
    let cjk = add("noto-cjk", &["NotoSansCJK-Regular.ttc", "NotoSansCJKjp-Regular.otf", "NotoSansJP-Regular.otf"]);
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    // egui's built-in fonts stay last in each list as a fallback.
    let base = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |primary: Option<&str>| {
        // Phosphor is NOT in the text chain: it has empty glyphs for Latin letters, and Inter
        // has its own glyphs in the Private Use Area. Icons always use `icon_font()`.
        let mut v: Vec<String> = Vec::new();
        if let Some(p) = primary {
            v.push(p.into());
        }
        if cjk {
            v.push("noto-cjk".into());
        }
        for f in &base {
            if f != "phosphor" && !v.contains(f) {
                v.push(f.clone());
            }
        }
        v
    };
    let reg = regular.then_some("inter-regular");
    fonts.families.insert(FontFamily::Proportional, family(reg));
    fonts.families.insert(medium(), family(if medium_ok { Some("inter-medium") } else { reg }));
    fonts.families.insert(semibold(), family(if semibold_ok { Some("inter-semibold") } else { reg }));
    if mono {
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "jetbrains-mono".into());
    }
    fonts
}

fn cr(r: f32) -> CornerRadius {
    CornerRadius::same(r as u8)
}

fn shadow((x, y, blur, spread, color): (i8, i8, u8, u8, Color32)) -> Shadow {
    Shadow { offset: [x, y], blur, spread, color }
}

pub fn style(mode: Mode) -> Style {
    use tokens::{control_h, font, layout, motion, pad_x, radius, space, stroke};
    let p = mode.palette();
    let mut s = Style::default();

    s.text_styles = [
        (TextStyle::Small, FontId::new(font::CAPTION, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(font::BODY, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(font::BODY, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(font::BODY, FontFamily::Monospace)),
        (TextStyle::Heading, FontId::new(font::HEADING, semibold())),
        (text::body_strong(), FontId::new(font::BODY_STRONG, medium())),
        (text::label(), FontId::new(font::LABEL, medium())),
        (text::title(), FontId::new(font::TITLE, semibold())),
        (text::display(), FontId::new(font::DISPLAY, semibold())),
    ]
    .into();

    let sp = &mut s.spacing;
    sp.item_spacing = egui::vec2(space::S4, space::S2);
    sp.button_padding = egui::vec2(pad_x::MD, space::S1);
    sp.interact_size = egui::vec2(control_h::SM, control_h::SM);
    sp.window_margin = Margin::same(space::S5 as i8);
    sp.menu_margin = Margin::same(space::S2 as i8);
    sp.indent = space::S6;
    sp.icon_width = font::BODY + 2.0;
    sp.icon_spacing = space::S2;
    sp.slider_width = 120.0;
    sp.combo_width = 120.0;
    sp.scroll.bar_width = 8.0;

    s.animation_time = motion::BASE;
    s.interaction.tooltip_delay = 0.5;

    let mut v = match mode {
        Mode::Dark => Visuals::dark(),
        Mode::Light => Visuals::light(),
    };
    v.override_text_color = Some(p.text);
    v.panel_fill = p.surface;
    v.window_fill = p.surface_raised;
    v.window_stroke = Stroke::new(stroke::HAIRLINE, p.border_strong);
    v.window_corner_radius = cr(radius::LG);
    v.window_shadow = shadow(tokens::shadow::FLOATING);
    v.popup_shadow = shadow(tokens::shadow::POPUP);
    v.menu_corner_radius = cr(radius::MD);
    v.extreme_bg_color = p.surface_sunken; // text fields, slider rails
    v.faint_bg_color = p.surface_hover;
    v.code_bg_color = p.surface_sunken;
    v.hyperlink_color = p.accent_text;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.selection.bg_fill = p.accent_subtle;
    v.selection.stroke = Stroke::new(stroke::FOCUS, p.focus_ring);
    v.text_cursor.stroke = Stroke::new(stroke::FOCUS, p.accent_text);
    v.window_highlight_topmost = false;
    v.striped = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(stroke::HAIRLINE, p.border);
    w.noninteractive.fg_stroke = Stroke::new(stroke::HAIRLINE, p.text);
    w.inactive.bg_fill = p.track; // slider rails, checkbox boxes: visible on every surface
    w.inactive.weak_bg_fill = p.surface_raised;
    w.inactive.bg_stroke = Stroke::new(stroke::HAIRLINE, p.border_input);
    w.inactive.fg_stroke = Stroke::new(stroke::HAIRLINE, p.text);
    w.hovered.bg_fill = p.surface_hover;
    w.hovered.weak_bg_fill = p.surface_hover;
    w.hovered.bg_stroke = Stroke::new(stroke::HAIRLINE, p.border_input);
    w.hovered.fg_stroke = Stroke::new(stroke::HAIRLINE, p.text);
    w.active.bg_fill = p.surface_active;
    w.active.weak_bg_fill = p.surface_active;
    w.active.bg_stroke = Stroke::new(stroke::FOCUS, p.focus_ring);
    w.active.fg_stroke = Stroke::new(stroke::HAIRLINE, p.text);
    w.open.bg_fill = p.surface_active;
    w.open.weak_bg_fill = p.surface_active;
    w.open.bg_stroke = Stroke::new(stroke::HAIRLINE, p.border_strong);
    w.open.fg_stroke = Stroke::new(stroke::HAIRLINE, p.text);
    for wv in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        wv.corner_radius = cr(radius::MD);
        wv.expansion = 0.0;
    }
    s.visuals = v;
    let _ = layout::WINDOW_MIN_W; // layout tokens are used by the app shell
    s
}
