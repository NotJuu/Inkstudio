//! Built-in open / save dialog for `.inks` files. Works the same on every OS and needs no
//! system libraries (no GTK on Linux).

use egui::{RichText, Ui, vec2};
use ink_ui::icons;
use ink_ui::theme::{palette, text, tokens};
use ink_ui::tr;
use ink_ui::widgets::{Button, ButtonVariant, IconButton};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Open,
    Save,
}

struct Entry {
    name: String,
    dir: bool,
}

pub struct FileDialog {
    pub mode: Mode,
    pub dir: PathBuf,
    pub name: String,
    path_edit: String,
    entries: Vec<Entry>,
    confirm_overwrite: Option<PathBuf>,
    pub error: Option<String>,
    focus_name: bool,
}

pub enum Outcome {
    Open,
    Done(PathBuf),
    Cancel,
}

pub fn with_extension(p: &Path) -> PathBuf {
    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ink_io::EXTENSION)) {
        p.to_path_buf()
    } else {
        let mut s = p.as_os_str().to_owned();
        s.push(".");
        s.push(ink_io::EXTENSION);
        s.into()
    }
}

impl FileDialog {
    pub fn new(mode: Mode, dir: PathBuf, name: &str) -> Self {
        let mut d = Self { mode, dir: dir.clone(), name: name.into(), path_edit: dir.display().to_string(), entries: Vec::new(), confirm_overwrite: None, error: None, focus_name: true };
        d.refresh();
        d
    }

    pub fn go(&mut self, dir: PathBuf) {
        if dir.is_dir() {
            self.dir = dir;
            self.path_edit = self.dir.display().to_string();
            self.refresh();
        } else {
            self.error = Some(format!("{}", dir.display()));
        }
    }

    fn refresh(&mut self) {
        self.entries.clear();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let inks = Path::new(&name).extension().is_some_and(|x| x.eq_ignore_ascii_case(ink_io::EXTENSION));
                if dir || inks {
                    self.entries.push(Entry { name, dir });
                }
            }
        }
        self.entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    }

    fn target(&self) -> Option<PathBuf> {
        let n = self.name.trim();
        if n.is_empty() {
            return None;
        }
        Some(with_extension(&self.dir.join(n)))
    }

    pub fn show(&mut self, ui: &mut Ui, recent: &[PathBuf]) -> Outcome {
        let p = palette(ui);
        let mut out = Outcome::Open;
        let title = if self.mode == Mode::Open { tr("Open…") } else { tr("Save as…") };
        ui.label(RichText::new(title).text_style(text::title()));
        ui.add_space(tokens::space::S4);
        // Path bar.
        ui.horizontal(|ui| {
            if ui.add(IconButton::new(icons::ARROW_UP, tr("Up")).size(26.0)).clicked() {
                if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
                    self.go(parent);
                }
            }
            if ui.add(IconButton::new(icons::HOUSE, tr("Home")).size(26.0)).clicked() {
                self.go(crate::settings::home_dir());
            }
            let r = ui.add(egui::TextEdit::singleline(&mut self.path_edit).desired_width(560.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.go(PathBuf::from(self.path_edit.clone()));
            }
        });
        ui.add_space(tokens::space::S2);
        let mut activate: Option<usize> = None;
        let mut chosen: Option<PathBuf> = None;
        ui.horizontal_top(|ui| {
            // Folder listing.
            egui::Frame::new().fill(p.surface_sunken).stroke(egui::Stroke::new(1.0, p.border)).inner_margin(4).show(ui, |ui| {
                ui.set_width(440.0);
                ui.set_height(300.0);
                egui::ScrollArea::vertical().id_salt("files").show(ui, |ui| {
                    if self.entries.is_empty() {
                        ui.label(RichText::new(tr("No Inkstudio files here.")).color(p.text_muted));
                    }
                    for (i, e) in self.entries.iter().enumerate() {
                        let icon = if e.dir { icons::FOLDER_SIMPLE } else { icons::FILE };
                        let selected = !e.dir && self.name == e.name;
                        let r = ink_ui::widgets::list_row(ui, Some(icon), &e.name, selected);
                        if r.double_clicked() || (r.clicked() && e.dir) {
                            activate = Some(i);
                        } else if r.clicked() {
                            self.name = e.name.clone();
                        }
                    }
                });
            });
            // Recent files (open dialog).
            if self.mode == Mode::Open && !recent.is_empty() {
                ui.vertical(|ui| {
                    ui.set_width(200.0);
                    ui.label(RichText::new(tr("Recent")).text_style(text::body_strong()));
                    for r in recent {
                        let name = r.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        if ui.link(name).on_hover_text(r.display().to_string()).clicked() {
                            chosen = Some(r.clone());
                        }
                    }
                });
            }
        });
        if let Some(i) = activate {
            let e = &self.entries[i];
            let path = self.dir.join(&e.name);
            if e.dir {
                self.go(path);
            } else {
                self.name = e.name.clone();
                chosen = Some(path);
            }
        }
        ui.add_space(tokens::space::S4);
        ui.horizontal(|ui| {
            ui.label(tr("File name"));
            let r = ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(380.0));
            if self.focus_name {
                r.request_focus();
                self.focus_name = false;
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                chosen = self.target();
            }
        });
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(p.danger));
        }
        if let Some(path) = &self.confirm_overwrite.clone() {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Replace the existing file?")).color(p.warning));
                if ui.add(Button::new(tr("Replace")).variant(ButtonVariant::Danger).size(ink_ui::widgets::ButtonSize::Sm)).clicked() {
                    out = Outcome::Done(path.clone());
                }
            });
        }
        ui.add_space(tokens::space::S5);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = tokens::space::S4;
            let label = if self.mode == Mode::Open { tr("Open") } else { tr("Save") };
            if ui.add_enabled(self.target().is_some(), Button::new(label).variant(ButtonVariant::Primary).min_width(96.0)).clicked() {
                chosen = self.target();
            }
            if ui.add(Button::new(tr("Cancel")).min_width(80.0)).clicked() {
                out = Outcome::Cancel;
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            out = Outcome::Cancel;
        }
        if let Some(c) = chosen {
            let c = with_extension(&c);
            match self.mode {
                Mode::Open if c.is_file() => out = Outcome::Done(c),
                Mode::Open => self.error = Some(format!("{}", c.display())),
                Mode::Save if c.exists() && self.confirm_overwrite.as_ref() != Some(&c) => self.confirm_overwrite = Some(c),
                Mode::Save => out = Outcome::Done(c),
            }
        }
        let _ = vec2(0.0, 0.0);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_is_added_once() {
        assert_eq!(with_extension(Path::new("/x/page")), PathBuf::from("/x/page.inks"));
        assert_eq!(with_extension(Path::new("/x/page.inks")), PathBuf::from("/x/page.inks"));
        assert_eq!(with_extension(Path::new("/x/page.INKS")), PathBuf::from("/x/page.INKS"));
    }

    #[test]
    fn lists_folders_and_inks_files_only() {
        let dir = std::env::temp_dir().join(format!("inkstudio-files-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.inks"), b"").unwrap();
        std::fs::write(dir.join("b.png"), b"").unwrap();
        std::fs::write(dir.join(".hidden.inks"), b"").unwrap();
        let d = FileDialog::new(Mode::Open, dir, "");
        let names: Vec<&str> = d.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["sub", "a.inks"]);
    }
}
