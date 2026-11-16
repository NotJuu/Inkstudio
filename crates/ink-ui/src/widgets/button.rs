use super::focus_ring;
use crate::theme::{palette, text, tokens};
use egui::{
    Align2, Color32, CornerRadius, Response, Sense, Stroke, StrokeKind, Ui, Widget, WidgetInfo,
    WidgetText, WidgetType, vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    Primary,
    #[default]
    Secondary,
    Ghost,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl ButtonSize {
    fn height(self) -> f32 {
        use tokens::control_h;
        match self {
            ButtonSize::Sm => control_h::SM,
            ButtonSize::Md => control_h::MD,
            ButtonSize::Lg => control_h::LG,
        }
    }
    fn pad(self) -> f32 {
        use tokens::pad_x;
        match self {
            ButtonSize::Sm => pad_x::SM,
            ButtonSize::Md => pad_x::MD,
            ButtonSize::Lg => pad_x::LG,
        }
    }
}

/// Text button. Dialog footers, start screen, launchers.
pub struct Button {
    label: String,
    icon: Option<&'static str>,
    variant: ButtonVariant,
    size: ButtonSize,
    loading: bool,
    min_width: f32,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            variant: ButtonVariant::Secondary,
            size: ButtonSize::Md,
            loading: false,
            min_width: 0.0,
        }
    }
    pub fn primary(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Primary)
    }
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }
    pub fn size(mut self, s: ButtonSize) -> Self {
        self.size = s;
        self
    }
    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }
    /// Shows a spinner, ignores clicks, keeps the label for screen readers.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }
}

impl Widget for Button {
    fn ui(self, ui: &mut Ui) -> Response {
        let p = palette(ui);
        let enabled = ui.is_enabled() && !self.loading;
        let galley = WidgetText::from(self.label.clone()).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            text::body_strong(),
        );
        let h = self.size.height();
        let icon_w = if self.icon.is_some() {
            tokens::icon::MD + tokens::space::S3
        } else {
            0.0
        };
        let w = (galley.size().x + icon_w + 2.0 * self.size.pad())
            .max(self.min_width)
            .max(h);
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(w, h), sense);
        let label = self.label.clone();
        resp.widget_info(|| {
            let mut info = WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), &label);
            if self.loading {
                info.label = Some(format!("{label}, working"));
            }
            info
        });

        if ui.is_rect_visible(rect) {
            let hovered = enabled && resp.hovered();
            let pressed = enabled && resp.is_pointer_button_down_on();
            let (fill, fg, border) = match self.variant {
                ButtonVariant::Primary => (
                    if pressed || hovered {
                        p.accent_hover
                    } else {
                        p.accent
                    },
                    p.on_accent,
                    Color32::TRANSPARENT,
                ),
                ButtonVariant::Danger => (p.danger_solid, p.on_danger, Color32::TRANSPARENT),
                ButtonVariant::Secondary => (
                    if pressed {
                        p.surface_active
                    } else if hovered {
                        p.surface_hover
                    } else {
                        p.surface_raised
                    },
                    p.text,
                    p.border_strong,
                ),
                ButtonVariant::Ghost => (
                    if pressed {
                        p.surface_active
                    } else if hovered {
                        p.surface_hover
                    } else {
                        Color32::TRANSPARENT
                    },
                    p.text,
                    Color32::TRANSPARENT,
                ),
            };
            let (fill, fg) = if ui.is_enabled() {
                (fill, fg)
            } else {
                (fill.gamma_multiply(0.5), p.text_disabled)
            };
            let r = CornerRadius::same(tokens::radius::MD as u8);
            ui.painter().rect(
                rect,
                r,
                fill,
                Stroke::new(tokens::stroke::HAIRLINE, border),
                StrokeKind::Inside,
            );
            if self.loading {
                let s = tokens::icon::MD;
                let spin = egui::Rect::from_center_size(rect.center(), vec2(s, s));
                egui::Spinner::new().size(s).color(fg).paint_at(ui, spin);
            } else {
                let content_w = galley.size().x + icon_w;
                let left = rect.center().x - content_w / 2.0;
                if let Some(i) = self.icon {
                    let c = egui::pos2(left + tokens::icon::MD / 2.0, rect.center().y);
                    ui.painter().text(
                        c,
                        Align2::CENTER_CENTER,
                        i,
                        crate::theme::icon_font(tokens::icon::MD),
                        fg,
                    );
                }
                let pos = egui::pos2(left + icon_w, rect.center().y - galley.size().y / 2.0);
                ui.painter().galley(pos, galley, fg);
            }
            focus_ring(ui, &resp, rect, tokens::radius::MD);
        }
        resp
    }
}

/// Square icon-only button for the tool column and command bar.
/// `label` is required: it is the tooltip and the accessible name.
pub struct IconButton {
    icon: &'static str,
    label: String,
    selected: Option<bool>,
    size: f32,
}

impl IconButton {
    pub fn new(icon: &'static str, label: impl Into<String>) -> Self {
        Self {
            icon,
            label: label.into(),
            selected: None,
            size: tokens::control_h::LG,
        }
    }
    /// Makes it a toggle (tool selection, eye, lock).
    pub fn selected(mut self, on: bool) -> Self {
        self.selected = Some(on);
        self
    }
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
}

impl Widget for IconButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let p = palette(ui);
        let (rect, resp) = ui.allocate_exact_size(vec2(self.size, self.size), Sense::click());
        let kind = if self.selected.is_some() {
            WidgetType::Checkbox
        } else {
            WidgetType::Button
        };
        resp.widget_info(|| match self.selected {
            Some(on) => WidgetInfo::selected(kind, ui.is_enabled(), on, &self.label),
            None => WidgetInfo::labeled(kind, ui.is_enabled(), &self.label),
        });
        if ui.is_rect_visible(rect) {
            let on = self.selected.unwrap_or(false);
            let fill = if on {
                p.accent_subtle
            } else if resp.is_pointer_button_down_on() {
                p.surface_active
            } else if resp.hovered() {
                p.surface_hover
            } else {
                Color32::TRANSPARENT
            };
            let r = CornerRadius::same(tokens::radius::MD as u8);
            ui.painter().rect_filled(rect, r, fill);
            if on {
                // Selection indicator bar: visible on any surface (accent-indicator role).
                let bar = egui::Rect::from_min_size(
                    rect.left_top() + vec2(0.0, 4.0),
                    vec2(2.0, rect.height() - 8.0),
                );
                ui.painter()
                    .rect_filled(bar, CornerRadius::same(1), p.accent_indicator);
            }
            let fg = if !ui.is_enabled() {
                p.text_disabled
            } else if on {
                p.accent_text
            } else {
                p.text
            };
            let icon_size = if self.size >= tokens::control_h::LG {
                tokens::icon::LG
            } else {
                tokens::icon::MD
            };
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                self.icon,
                crate::theme::icon_font(icon_size),
                fg,
            );
            focus_ring(ui, &resp, rect, tokens::radius::MD);
        }
        resp.on_hover_text(&self.label)
    }
}
