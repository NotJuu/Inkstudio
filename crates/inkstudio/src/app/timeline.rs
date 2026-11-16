//! Animation timeline: transport, frame ruler with scrubbing, several tracks with show/hide and
//! rename, cel editing (new, duplicate, blank, insert/delete frame) and onion-skin settings.

use super::*;
use ink_anim::Timeline;

const HEADER_W: f32 = 168.0;
const RULER_H: f32 = 22.0;
const ROW_H: f32 = 28.0;
const FPS_PRESETS: [f32; 5] = [8.0, 12.0, 15.0, 24.0, 30.0];

#[derive(Default)]
struct Actions {
    frame: Option<usize>,
    track: Option<usize>,
    new_cel: bool,
    duplicate: bool,
    blank: bool,
    insert: bool,
    delete: bool,
    add_track: bool,
    toggle_hidden: Option<usize>,
    stop: bool,
}

impl InkApp {
    pub(super) fn timeline_pane(&mut self, ui: &mut Ui) {
        let p = palette(ui);
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        let typing = ui.ctx().egui_wants_keyboard_input();
        let Some(b) = &mut self.book else { return };
        let page = &mut b.pages[b.current];
        let Some(tl) = page.timeline.as_mut() else {
            egui::Frame::new().inner_margin(8).show(ui, |ui| {
                ui.label(RichText::new(tr("This sheet is not an animation.")).color(p.text_muted));
            });
            return;
        };
        let frame0 = page.frame;
        let track = self.anim_track.min(tl.tracks.len().saturating_sub(1));
        let mut a = Actions::default();
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                // ---- transport + timing ------------------------------------------------------------
                ui.horizontal(|ui| {
                    let last = tl.frames - 1;
                    if ui
                        .add(IconButton::new(icons::SKIP_BACK, tr("First frame (Home)")).size(26.0))
                        .clicked()
                    {
                        a.frame = Some(0);
                    }
                    if ui
                        .add(
                            IconButton::new(icons::CARET_LEFT, tr("Previous frame (,)")).size(26.0),
                        )
                        .clicked()
                    {
                        a.frame = Some(frame0.saturating_sub(1));
                    }
                    let play_icon = if self.playing {
                        icons::PAUSE
                    } else {
                        icons::PLAY
                    };
                    if ui
                        .add(
                            IconButton::new(play_icon, tr("Play / pause (Enter)"))
                                .selected(self.playing)
                                .size(26.0),
                        )
                        .clicked()
                    {
                        self.playing = !self.playing;
                    }
                    if ui
                        .add(IconButton::new(icons::CARET_RIGHT, tr("Next frame (.)")).size(26.0))
                        .clicked()
                    {
                        a.frame = Some((frame0 + 1).min(last));
                    }
                    if ui
                        .add(
                            IconButton::new(icons::SKIP_FORWARD, tr("Last frame (End)")).size(26.0),
                        )
                        .clicked()
                    {
                        a.frame = Some(last);
                    }
                    ui.separator();
                    let secs = frame0 as f32 / tl.fps;
                    ui.label(
                        RichText::new(format!("{:>3} / {}", frame0 + 1, tl.frames))
                            .text_style(text::body_strong())
                            .monospace(),
                    );
                    ui.label(
                        RichText::new(format!("{secs:.2}s"))
                            .color(p.text_muted)
                            .monospace(),
                    );
                    ui.separator();
                    egui::ComboBox::from_id_salt("fps")
                        .width(70.0)
                        .selected_text(format!("{} fps", tl.fps))
                        .show_ui(ui, |ui| {
                            for f in FPS_PRESETS {
                                ui.selectable_value(&mut tl.fps, f, format!("{f} fps"));
                            }
                        });
                    ui.add(
                        egui::DragValue::new(&mut tl.frames)
                            .range(1..=9999)
                            .suffix(tr(" frames")),
                    );
                    ui.toggle_value(&mut tl.looping, tr("Loop"));
                });
                // ---- editing + onion skin ----------------------------------------------------------
                ui.horizontal_wrapped(|ui| {
                    a.new_cel = ui
                        .add(
                            Button::new(tr("New cel"))
                                .icon(icons::PLUS)
                                .size(ButtonSize::Sm),
                        )
                        .on_hover_text(tr("Start a new drawing on this frame"))
                        .clicked();
                    a.duplicate = ui
                        .add(
                            Button::new(tr("Duplicate"))
                                .icon(icons::COPY)
                                .size(ButtonSize::Sm),
                        )
                        .on_hover_text(tr("Copy the current drawing to a new cel here"))
                        .clicked();
                    a.blank = ui
                        .add(Button::new(tr("Blank")).size(ButtonSize::Sm))
                        .on_hover_text(tr("Show nothing from this frame on"))
                        .clicked();
                    ui.separator();
                    a.insert = ui
                        .add(
                            IconButton::new(
                                icons::ARROW_LINE_RIGHT,
                                tr("Insert frame (hold the drawing one frame longer)"),
                            )
                            .size(24.0),
                        )
                        .clicked();
                    a.delete = ui
                        .add(IconButton::new(icons::ARROW_LINE_LEFT, tr("Delete frame")).size(24.0))
                        .clicked();
                    a.add_track = ui
                        .add(IconButton::new(icons::STACK_PLUS, tr("Add track")).size(24.0))
                        .clicked();
                    ui.separator();
                    ui.toggle_value(&mut tl.onion.enabled, tr("Onion skin"));
                    if tl.onion.enabled {
                        ui.label(RichText::new(tr("before")).color(p.text_muted));
                        ui.add(egui::DragValue::new(&mut tl.onion.before).range(0..=5));
                        ui.label(RichText::new(tr("after")).color(p.text_muted));
                        ui.add(egui::DragValue::new(&mut tl.onion.after).range(0..=5));
                        let mut op = tl.onion.opacity * 100.0;
                        if ui
                            .add(
                                egui::DragValue::new(&mut op)
                                    .range(5.0..=100.0)
                                    .suffix(" %"),
                            )
                            .changed()
                        {
                            tl.onion.opacity = op / 100.0;
                        }
                    }
                });
                ui.add_space(2.0);
                // ---- grid ---------------------------------------------------------------------------
                let rows = tl.tracks.len();
                let grid_h = RULER_H + ROW_H * rows as f32;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    track_headers(ui, tl, track, grid_h, &mut a, &mut self.rename_track);
                    egui::ScrollArea::horizontal()
                        .id_salt("tl-grid")
                        .show(ui, |ui| {
                            frame_grid(ui, tl, frame0, track, grid_h, &mut a);
                        });
                });
            });
        // ---- keys -----------------------------------------------------------------------------
        if !typing && ui.ui_contains_pointer() {
            ui.input(|i| {
                if i.key_pressed(Key::Home) {
                    a.frame = Some(0);
                }
                if i.key_pressed(Key::End) {
                    a.frame = Some(tl.frames - 1);
                }
            });
        }
        if !typing && ui.input(|i| i.key_pressed(Key::Comma)) {
            a.frame = Some(frame0.saturating_sub(1));
        }
        if !typing && ui.input(|i| i.key_pressed(Key::Period)) {
            a.frame = Some((frame0 + 1).min(tl.frames - 1));
        }
        // ---- apply ----------------------------------------------------------------------------
        let mut frame = a.frame.unwrap_or(frame0);
        if a.stop || a.frame.is_some() {
            self.playing = self.playing && !a.stop && a.frame.is_none();
        }
        let mut track = a.track.unwrap_or(track);
        let mut touched = false;
        if a.add_track {
            let n = tl.tracks.len();
            let name = format!("{} {}", tr("Cel"), (b'A' + (n as u8).min(25)) as char);
            track = tl.add_track(name);
            tl.new_cel(&mut page.doc, track, frame);
            touched = true;
        }
        if let Some(t) = a.toggle_hidden {
            tl.tracks[t].hidden = !tl.tracks[t].hidden;
            touched = true;
        }
        if a.new_cel {
            touched |= tl.new_cel(&mut page.doc, track, frame).is_some();
        }
        if a.duplicate {
            touched |= tl.duplicate_cel(&mut page.doc, track, frame).is_some();
        }
        if a.blank {
            tl.set_key(track, frame, None);
            touched = true;
        }
        if a.insert {
            tl.insert_frames(frame + 1, 1);
            touched = true;
        }
        if a.delete && tl.frames > 1 {
            tl.delete_frame(frame);
            frame = frame.min(tl.frames - 1);
            touched = true;
        }
        if self.playing {
            self.play_accum += dt;
            let d = tl.frame_duration();
            while self.play_accum >= d {
                self.play_accum -= d;
                frame = tl.next_frame(frame);
            }
            ui.ctx().request_repaint();
        }
        frame = frame.min(tl.frames.saturating_sub(1));
        let track_changed = track != self.anim_track;
        self.anim_track = track;
        if touched {
            page.touch();
        }
        if frame != page.frame || touched || track_changed {
            page.set_frame(frame);
            if let Some(tl) = &page.timeline {
                // Draw on the selected track; on a blank frame keep a cel of that track selected
                // so the next stroke starts a new drawing there.
                let id = tl
                    .cel_at(track, frame)
                    .or_else(|| tl.tracks.get(track).and_then(|t| t.cels.last().copied()));
                if id.is_some() {
                    self.active_layer = id;
                }
            }
        }
    }
}

/// Ruler corner + one header per track: eye, name (double-click renames).
fn track_headers(
    ui: &mut Ui,
    tl: &mut Timeline,
    selected: usize,
    grid_h: f32,
    a: &mut Actions,
    rename: &mut Option<(usize, String)>,
) {
    let p = palette(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(HEADER_W, grid_h), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        Rect::from_min_size(rect.min, vec2(HEADER_W, RULER_H)),
        0,
        p.bg,
    );
    painter.text(
        pos2(rect.left() + 8.0, rect.top() + RULER_H / 2.0),
        Align2::LEFT_CENTER,
        tr("Tracks"),
        egui::FontId::proportional(11.0),
        p.text_muted,
    );
    let mut commit = None;
    for (i, t) in tl.tracks.iter_mut().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.left(), rect.top() + RULER_H + i as f32 * ROW_H),
            vec2(HEADER_W, ROW_H),
        );
        let sel = i == selected;
        painter.rect_filled(r, 0, if sel { p.accent_subtle } else { p.surface });
        painter.line_segment(
            [r.left_bottom(), r.right_bottom()],
            Stroke::new(1.0, p.border),
        );
        let eye = Rect::from_min_size(r.min + vec2(4.0, 3.0), vec2(22.0, 22.0));
        let er = ui
            .interact(eye, ui.id().with(("eye", i)), egui::Sense::click())
            .on_hover_text(tr("Show / hide track"));
        painter.text(
            eye.center(),
            Align2::CENTER_CENTER,
            if t.hidden {
                icons::EYE_SLASH
            } else {
                icons::EYE
            },
            theme::icon_font(15.0),
            if t.hidden { p.text_muted } else { p.text },
        );
        if er.clicked() {
            a.toggle_hidden = Some(i);
        }
        let name_r = Rect::from_min_max(pos2(eye.right() + 4.0, r.top()), r.max);
        match rename {
            Some((ri, name)) if *ri == i => {
                let resp = ui.put(
                    name_r.shrink2(vec2(2.0, 3.0)),
                    egui::TextEdit::singleline(name),
                );
                resp.request_focus();
                if resp.lost_focus() {
                    commit = Some((i, name.clone()));
                }
            }
            _ => {
                let nr = ui
                    .interact(name_r, ui.id().with(("track", i)), egui::Sense::click())
                    .on_hover_text(tr("Click to draw on this track · double-click to rename"));
                painter.text(
                    pos2(name_r.left(), r.center().y),
                    Align2::LEFT_CENTER,
                    &t.name,
                    egui::FontId::proportional(13.0),
                    if t.hidden { p.text_muted } else { p.text },
                );
                painter.text(
                    pos2(r.right() - 6.0, r.center().y),
                    Align2::RIGHT_CENTER,
                    t.cels.len().to_string(),
                    egui::FontId::proportional(10.0),
                    p.text_muted,
                );
                if nr.double_clicked() {
                    *rename = Some((i, t.name.clone()));
                } else if nr.clicked() {
                    a.track = Some(i);
                }
            }
        }
    }
    if let Some((i, name)) = commit {
        if !name.trim().is_empty() {
            tl.tracks[i].name = name.trim().to_string();
        }
        *rename = None;
    }
}

/// Frame ruler and cells; click or drag anywhere to scrub, click a row to select its track.
fn frame_grid(
    ui: &mut Ui,
    tl: &Timeline,
    frame: usize,
    selected: usize,
    grid_h: f32,
    a: &mut Actions,
) {
    let p = palette(ui);
    let cw = (tokens::layout::TIMELINE_CELL_W + 6.0).max(20.0);
    let w = cw * tl.frames as f32;
    let (rect, resp) = ui.allocate_exact_size(
        vec2(w.max(ui.available_width()), grid_h),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    let fps = tl.fps.round().max(1.0) as usize;
    // Onion range.
    if tl.onion.enabled {
        let lo = frame.saturating_sub(tl.onion.before);
        let hi = (frame + tl.onion.after).min(tl.frames - 1);
        let r = Rect::from_min_max(
            pos2(rect.left() + lo as f32 * cw, rect.top()),
            pos2(rect.left() + (hi + 1) as f32 * cw, rect.top() + RULER_H),
        );
        painter.rect_filled(r, 0, Color32::from_rgba_unmultiplied(120, 170, 255, 40));
    }
    // Ruler.
    painter.rect_filled(Rect::from_min_size(rect.min, vec2(w, RULER_H)), 0, p.bg);
    for f in 0..tl.frames {
        let x = rect.left() + f as f32 * cw;
        let second = f % fps == 0;
        let tick = if second {
            RULER_H
        } else if (f + 1) % 5 == 0 {
            RULER_H * 0.45
        } else {
            RULER_H * 0.25
        };
        painter.line_segment(
            [
                pos2(x, rect.top() + RULER_H - tick),
                pos2(x, rect.top() + RULER_H),
            ],
            Stroke::new(1.0, if second { p.text_muted } else { p.border }),
        );
        if f == 0 || (f + 1) % 5 == 0 || second {
            painter.text(
                pos2(x + 3.0, rect.top() + 2.0),
                Align2::LEFT_TOP,
                (f + 1).to_string(),
                egui::FontId::proportional(10.0),
                if second { p.text } else { p.text_muted },
            );
        }
    }
    // Tracks.
    for (ti, t) in tl.tracks.iter().enumerate() {
        let y = rect.top() + RULER_H + ti as f32 * ROW_H;
        let row = Rect::from_min_size(pos2(rect.left(), y), vec2(w, ROW_H));
        painter.rect_filled(
            row,
            0,
            if ti == selected {
                p.surface
            } else {
                p.surface_sunken
            },
        );
        let mut f = 0;
        while f < tl.frames {
            let x = rect.left() + f as f32 * cw;
            if f % fps == 0 {
                painter.line_segment([pos2(x, y), pos2(x, y + ROW_H)], Stroke::new(1.0, p.border));
            }
            if tl.is_key(ti, f) {
                if let Some(ci) = tl.cel_index(ti, f) {
                    // Exposure block: key cell plus held frames.
                    let mut end = f + 1;
                    while end < tl.frames && !tl.is_key(ti, end) {
                        end += 1;
                    }
                    let block = Rect::from_min_max(
                        pos2(x + 1.0, y + 4.0),
                        pos2(rect.left() + end as f32 * cw - 1.0, y + ROW_H - 4.0),
                    );
                    let fill = if t.hidden {
                        p.border
                    } else if ti == selected {
                        p.accent
                    } else {
                        p.accent_subtle
                    };
                    painter.rect_filled(
                        block,
                        3,
                        fill.gamma_multiply(if ti == selected { 0.55 } else { 1.0 }),
                    );
                    painter.circle_filled(pos2(x + cw / 2.0, y + ROW_H / 2.0), 3.0, p.text);
                    if block.width() > cw * 1.5 {
                        painter.text(
                            pos2(x + cw, y + ROW_H / 2.0),
                            Align2::LEFT_CENTER,
                            (ci + 1).to_string(),
                            egui::FontId::proportional(11.0),
                            p.text,
                        );
                    }
                    f = end;
                    continue;
                } else {
                    painter.text(
                        pos2(x + cw / 2.0, y + ROW_H / 2.0),
                        Align2::CENTER_CENTER,
                        "×",
                        egui::FontId::proportional(12.0),
                        p.text_muted,
                    );
                }
            }
            f += 1;
        }
        painter.line_segment(
            [row.left_bottom(), row.right_bottom()],
            Stroke::new(1.0, p.border),
        );
    }
    // Playhead.
    let px = rect.left() + frame as f32 * cw;
    painter.rect_stroke(
        Rect::from_min_size(pos2(px, rect.top()), vec2(cw, RULER_H)),
        2,
        Stroke::new(1.5, p.danger),
        egui::StrokeKind::Inside,
    );
    painter.line_segment(
        [
            pos2(px + cw / 2.0, rect.top() + RULER_H),
            pos2(px + cw / 2.0, rect.bottom()),
        ],
        Stroke::new(2.0, p.danger),
    );
    // Scrub / select.
    if let Some(pos) = resp
        .interact_pointer_pos()
        .filter(|_| resp.clicked() || resp.dragged())
    {
        let f = (((pos.x - rect.left()) / cw).floor().max(0.0) as usize).min(tl.frames - 1);
        a.frame = Some(f);
        a.stop = true;
        if pos.y > rect.top() + RULER_H && resp.clicked() {
            let t = ((pos.y - rect.top() - RULER_H) / ROW_H) as usize;
            if t < tl.tracks.len() {
                a.track = Some(t);
            }
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}
