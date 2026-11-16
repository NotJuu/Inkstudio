//! A complete manga volume: book info (title, author, credits, …) and special pages
//! (cover, title page, credits, contents, afterword, back cover) generated from it.

use super::*;
use crate::book::{BookInfo, PageRole};

/// Roles a complete volume gets with "Make a complete book".
pub const COMPLETE: [PageRole; 5] = [
    PageRole::Cover,
    PageRole::Title,
    PageRole::Credits,
    PageRole::Afterword,
    PageRole::BackCover,
];

impl InkApp {
    pub fn open_book_info(&mut self) {
        self.book_info = self.book.as_ref().map(|b| b.info.clone());
    }

    /// Insert a special page and show it.
    pub fn insert_special(&mut self, role: PageRole) {
        let Some(b) = self.book.as_mut() else { return };
        let at = b.add_special(role, &self.fonts);
        b.current = at;
        self.active_layer = Some(b.page().ink_layer);
        self.after_page_change();
    }

    /// Add every missing special page of a complete volume.
    pub fn complete_book(&mut self) {
        let Some(b) = self.book.as_mut() else { return };
        for role in COMPLETE {
            if !b.pages.iter().any(|p| p.role == role) {
                b.add_special(role, &self.fonts);
            }
        }
        b.current = 0;
        self.active_layer = Some(b.page().ink_layer);
        self.after_page_change();
    }

    /// Rewrite the text of all special pages from the book info.
    pub fn refresh_special_pages(&mut self) {
        let Some(b) = self.book.as_mut() else { return };
        for i in 0..b.pages.len() {
            if b.pages[i].role != PageRole::Story {
                b.fill_special(i, &self.fonts);
            }
        }
        self.thumbs.clear();
        self.cache.clear();
    }

    pub(super) fn after_page_change(&mut self) {
        self.cache.clear();
        self.thumbs.clear();
        self.selected_balloon = None;
        self.selected_panel = None;
    }

    /// "Book" menu.
    pub(super) fn book_menu(&mut self, ui: &mut Ui) {
        let has = self.book.is_some();
        if ui
            .add_enabled(
                has,
                egui::Button::new(tr("Book info (title, author, credits)…")),
            )
            .clicked()
        {
            self.open_book_info();
            ui.close();
        }
        if ui
            .add_enabled(
                has,
                egui::Button::new(format!("{}  (F5)", tr("Read as manga"))),
            )
            .clicked()
        {
            self.open_reader();
            ui.close();
        }
        ui.separator();
        ui.label(RichText::new(tr("Insert special page")).color(palette(ui).text_muted));
        for role in PageRole::SPECIAL {
            if ui
                .add_enabled(has, egui::Button::new(tr(role.name())))
                .clicked()
            {
                self.insert_special(role);
                ui.close();
            }
        }
        ui.separator();
        if ui
            .add_enabled(
                has,
                egui::Button::new(tr("Make a complete book (cover … back cover)")),
            )
            .clicked()
        {
            self.complete_book();
            ui.close();
        }
        if ui
            .add_enabled(
                has,
                egui::Button::new(tr("Update special pages from book info")),
            )
            .clicked()
        {
            self.refresh_special_pages();
            ui.close();
        }
    }

    pub(super) fn book_info_dialog(&mut self, ctx: &egui::Context) {
        let p = theme::palette_ctx(ctx);
        let Some(info) = &mut self.book_info else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("book-info"))
            .frame(modal_frame(p))
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.label(RichText::new(tr("Book info")).text_style(text::title()));
                ui.label(
                    RichText::new(tr("Used on the cover, title page, credits and back cover."))
                        .color(p.text_muted),
                );
                ui.add_space(tokens::space::S4);
                egui::Grid::new("book-info-grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        let field = |ui: &mut Ui, label: &str, v: &mut String| {
                            ui.label(tr(label));
                            ui.add(egui::TextEdit::singleline(v).desired_width(340.0));
                            ui.end_row();
                        };
                        field(ui, "Title", &mut info.title);
                        field(ui, "Subtitle / volume", &mut info.subtitle);
                        field(ui, "Story by", &mut info.author);
                        field(ui, "Art by", &mut info.artist);
                        field(ui, "Publisher / circle", &mut info.publisher);
                        field(ui, "Year", &mut info.year);
                        ui.label(tr("Description"));
                        ui.add(
                            egui::TextEdit::multiline(&mut info.description)
                                .desired_width(340.0)
                                .desired_rows(3),
                        );
                        ui.end_row();
                    });
                ui.add_space(tokens::space::S3);
                ui.label(RichText::new(tr("Credits")).text_style(text::body_strong()));
                let mut remove = None;
                for (i, (role, name)) in info.credits.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(role)
                                .hint_text(tr("Role (e.g. Lettering)"))
                                .desired_width(170.0),
                        );
                        ui.add(
                            egui::TextEdit::singleline(name)
                                .hint_text(tr("Name"))
                                .desired_width(240.0),
                        );
                        if ui
                            .add(IconButton::new(icons::X, tr("Remove")).size(24.0))
                            .clicked()
                        {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    info.credits.remove(i);
                }
                if ui
                    .add(
                        Button::new(tr("Add credit"))
                            .icon(icons::PLUS)
                            .size(ButtonSize::Sm),
                    )
                    .clicked()
                {
                    info.credits.push((String::new(), String::new()));
                }
                ui.add_space(tokens::space::S5);
                ui.horizontal(|ui| {
                    if ui
                        .add(Button::new(tr("Apply")).variant(ButtonVariant::Primary))
                        .clicked()
                    {
                        apply = true;
                    }
                    if ui.add(Button::new(tr("Cancel"))).clicked()
                        || ui.input(|i| i.key_pressed(Key::Escape))
                    {
                        cancel = true;
                    }
                });
            });
        if apply {
            let info: BookInfo = self.book_info.take().unwrap();
            if let Some(b) = &mut self.book {
                if b.info != info {
                    b.info = info;
                    b.structure_dirty = true;
                    b.structure_generation += 1;
                }
            }
            self.refresh_special_pages();
        } else if cancel {
            self.book_info = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::book::{Book, PageRole, SheetKind, SheetSettings};

    #[test]
    fn special_pages_go_to_their_place_and_story_pages_are_numbered() {
        let fonts = ink_comic::Fonts::system();
        let s = SheetSettings {
            kind: SheetKind::Book,
            pages: 3,
            dpi: 72,
            ..Default::default()
        };
        let mut b = Book::new(s);
        b.info.title = "Kaze".into();
        b.info.author = "Ana".into();
        b.add_special(PageRole::BackCover, &fonts);
        b.add_special(PageRole::Credits, &fonts);
        b.add_special(PageRole::Cover, &fonts);
        let roles: Vec<PageRole> = b.pages.iter().map(|p| p.role).collect();
        assert_eq!(
            roles,
            [
                PageRole::Cover,
                PageRole::Credits,
                PageRole::Story,
                PageRole::Story,
                PageRole::Story,
                PageRole::BackCover
            ]
        );
        assert_eq!(b.page_number(0), None);
        assert_eq!(b.page_number(2), Some(1));
        assert_eq!(b.page_number(4), Some(3));
        assert!(!b.pages[0].balloons.is_empty(), "cover has its title text");
        assert!(b.pages[0].balloons.iter().any(|x| x.text.contains("Kaze")));
        assert!(b.pages[1].balloons.iter().any(|x| x.text.contains("Ana")));
    }
}
