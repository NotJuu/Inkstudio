//! On-canvas overlays (S11 transform, S12 frames, S13 balloon tails, S15 rulers).
//! Overlays sit on arbitrary artwork, so nothing relies on a single color:
//! lines are drawn twice (dark under light), handles are white with a dark border.

use crate::theme::{palette, tokens};
use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Stroke, StrokeKind, Ui, vec2};

/// A 1px line visible on both black and white art: a 3px dark under-stroke plus the colored line.
pub fn dual_stroke_line(painter: &Painter, a: Pos2, b: Pos2, color: Color32) {
    painter.line_segment([a, b], Stroke::new(3.0, Color32::from_black_alpha(140)));
    painter.line_segment([a, b], Stroke::new(tokens::stroke::OVERLAY, color));
}

/// A square drag handle centred on `at`. `hot` = hovered or dragged (drawn filled with focus-ring).
/// The hit area is `handle-size + 2 × hit-slop`, larger than what is drawn.
pub fn handle(ui: &Ui, at: Pos2, hot: bool) -> Rect {
    let p = palette(ui);
    let s = tokens::layout::HANDLE_SIZE;
    let r = Rect::from_center_size(at, vec2(s, s));
    let fill = if hot { p.focus_ring } else { p.overlay_handle };
    ui.painter().rect(
        r,
        CornerRadius::same(1),
        fill,
        Stroke::new(1.0, p.overlay_handle_border),
        StrokeKind::Inside,
    );
    r.expand(tokens::layout::HIT_SLOP)
}
