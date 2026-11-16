use crate::theme::{palette, text, tokens};
use egui::{
    Align2, CornerRadius, Rect, Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType, pos2, vec2,
};

/// One frame cell of a track: a new cel drawing starts here, a hold continues the previous one,
/// or the frame is blank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CelCell {
    Blank,
    Start(u16),
    Hold,
}

/// A timeline track row (S21): name column + frame cells + playhead.
/// Keyboard: the row is one tab stop; Left/Right move the current frame; Space plays (handled by shell).
/// Returns the clicked frame, if any.
pub fn timeline_track(
    ui: &mut Ui,
    name: &str,
    cells: &[CelCell],
    current: usize,
    selected: bool,
) -> Option<usize> {
    let p = palette(ui);
    let cw = tokens::layout::TIMELINE_CELL_W;
    let ch = tokens::layout::TIMELINE_CELL_H;
    let name_w = 120.0;
    let w = name_w + cw * cells.len() as f32;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ch), Sense::click());
    resp.widget_info(|| {
        WidgetInfo::labeled(
            WidgetType::Other,
            ui.is_enabled(),
            format!("Track {name}, frame {}", current + 1),
        )
    });
    let mut clicked = None;
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            if pos.x > rect.left() + name_w {
                clicked = Some(((pos.x - rect.left() - name_w) / cw) as usize);
            }
        }
    }
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let name_rect = Rect::from_min_size(rect.min, vec2(name_w, ch));
        painter.rect_filled(
            name_rect,
            CornerRadius::ZERO,
            if selected { p.accent_subtle } else { p.surface },
        );
        painter.text(
            pos2(name_rect.left() + 8.0, name_rect.center().y),
            Align2::LEFT_CENTER,
            name,
            text::body().resolve(ui.style()),
            p.text,
        );
        for (i, c) in cells.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(rect.left() + name_w + i as f32 * cw, rect.top()),
                vec2(cw, ch),
            );
            let bg = if i % 2 == 0 { p.surface_sunken } else { p.bg };
            painter.rect_filled(r, CornerRadius::ZERO, bg);
            match c {
                CelCell::Blank => {}
                CelCell::Start(n) => {
                    painter.rect_filled(
                        r.shrink2(vec2(1.0, 3.0)),
                        CornerRadius::same(2),
                        p.accent_subtle,
                    );
                    painter.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        n.to_string(),
                        text::caption().resolve(ui.style()),
                        p.text,
                    );
                }
                CelCell::Hold => {
                    painter.rect_filled(
                        r.shrink2(vec2(0.0, 3.0)),
                        CornerRadius::ZERO,
                        p.accent_subtle,
                    );
                    painter.line_segment(
                        [pos2(r.left(), r.center().y), pos2(r.right(), r.center().y)],
                        Stroke::new(1.0, p.text_muted),
                    );
                }
            }
            painter.line_segment(
                [r.right_top(), r.right_bottom()],
                Stroke::new(1.0, p.border),
            );
        }
        let x = rect.left() + name_w + current as f32 * cw;
        painter.rect_stroke(
            Rect::from_min_size(pos2(x, rect.top()), vec2(cw, ch)),
            CornerRadius::ZERO,
            Stroke::new(2.0, p.focus_ring),
            StrokeKind::Inside,
        );
        super::focus_ring(ui, &resp, rect, 0.0);
    }
    clicked
}
