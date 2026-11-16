//! UI primitives. Specs: `replica/design/components.md`.
//!
//! Shared rules for every widget here:
//! * colors from `theme::palette`, sizes from `theme::tokens`, never raw values;
//! * interactive widgets sense clicks, so they take keyboard focus (Tab / Shift+Tab)
//!   and activate with Space / Enter;
//! * a 2px `focus-ring` outline is drawn when focused;
//! * every widget reports `WidgetInfo` (role, label, value, enabled) to AccessKit;
//!   icon-only widgets require a label, which is also their tooltip.

mod button;
mod color_wheel;
mod feedback;
mod field;
mod overlay;
mod panel;
mod row;
mod swatch;
mod tabs;
mod timeline;

pub use button::{Button, ButtonSize, ButtonVariant, IconButton};
pub use color_wheel::{Hsv, color_wheel};
pub use feedback::{ToastKind, progress_bar, toast};
pub use field::{Unit, slider_field, unit_field};
pub use overlay::{dual_stroke_line, handle};
pub use panel::{DialogResult, dialog_frame, palette_header};
pub use row::{LayerKind, LayerRowEvent, LayerRowState, layer_row, list_row};
pub use swatch::{color_pair, swatch};
pub use tabs::tab_strip;
pub use timeline::{CelCell, timeline_track};

use crate::theme::{palette, tokens};
use egui::{Rect, Response, StrokeKind, Ui};

/// Draw the standard focus ring around `rect` if `resp` has keyboard focus.
pub(crate) fn focus_ring(ui: &Ui, resp: &Response, rect: Rect, radius: f32) {
    if resp.has_focus() {
        let p = palette(ui);
        ui.painter().rect_stroke(
            rect.expand(1.0),
            egui::CornerRadius::same(radius as u8 + 1),
            egui::Stroke::new(tokens::stroke::FOCUS, p.focus_ring),
            StrokeKind::Outside,
        );
    }
}
