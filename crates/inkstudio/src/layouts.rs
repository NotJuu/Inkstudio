//! Workspace layouts: built-in arrangements for different kinds of work, plus user layouts.
//!
//! Every layout keeps its own dock state: rearranging palettes in "Animation" doesn't change
//! "Manga". Built-ins can be reset to their default; user layouts are saved under a name.

use crate::app::Pane;
use crate::book::SheetKind;
use egui_dock::{DockState, NodeIndex};

pub const STANDARD: &str = "standard";
pub const CUSTOM_PREFIX: &str = "custom:";

/// (id, English display name) of the built-in layouts, in menu order.
pub const BUILTIN: [(&str, &str); 7] = [
    ("standard", "Standard"),
    ("manga", "Manga & inking"),
    ("lettering", "Lettering & balloons"),
    ("animation", "Animation"),
    ("illustration", "Illustration & painting"),
    ("concept", "Concept art"),
    ("focus", "Canvas focus"),
];

pub fn is_builtin(id: &str) -> bool {
    BUILTIN.iter().any(|(b, _)| *b == id)
}

pub fn custom_id(name: &str) -> String {
    format!("{CUSTOM_PREFIX}{name}")
}

/// Display name (untranslated) for an id.
pub fn display_name(id: &str) -> &str {
    BUILTIN
        .iter()
        .find(|(b, _)| *b == id)
        .map_or_else(|| id.strip_prefix(CUSTOM_PREFIX).unwrap_or(id), |(_, n)| *n)
}

/// The layout a sheet kind starts with.
pub fn for_kind(kind: SheetKind) -> &'static str {
    match kind {
        SheetKind::MangaPage | SheetKind::Book => "manga",
        SheetKind::Animation => "animation",
        SheetKind::Illustration | SheetKind::Webtoon => "illustration",
        SheetKind::ConceptArt => "concept",
    }
}

pub fn kind_key(kind: SheetKind) -> &'static str {
    match kind {
        SheetKind::Illustration => "illustration",
        SheetKind::MangaPage => "manga_page",
        SheetKind::Book => "book",
        SheetKind::Webtoon => "webtoon",
        SheetKind::Animation => "animation",
        SheetKind::ConceptArt => "concept_art",
    }
}

/// Default dock state of a built-in layout.
pub fn builtin(id: &str) -> Option<DockState<Pane>> {
    let mut d = DockState::new(vec![Pane::Canvas]);
    let s = d.main_surface_mut();
    let root = NodeIndex::root();
    match id {
        "standard" => {
            let [center, left] = s.split_left(root, 0.17, vec![Pane::SubTool]);
            s.split_below(left, 0.55, vec![Pane::ToolProperty]);
            let [center, right] = s.split_right(center, 0.79, vec![Pane::Color, Pane::Navigator]);
            s.split_below(right, 0.36, vec![Pane::Layers]);
            s.split_below(center, 0.80, vec![Pane::Pages, Pane::Timeline]);
        }
        // Inking pages: brushes left, big layer list right, page strip below.
        "manga" => {
            let [center, left] = s.split_left(root, 0.17, vec![Pane::SubTool]);
            s.split_below(left, 0.60, vec![Pane::ToolProperty]);
            let [center, right] = s.split_right(center, 0.78, vec![Pane::Color, Pane::Navigator]);
            s.split_below(right, 0.30, vec![Pane::Layers]);
            s.split_below(center, 0.84, vec![Pane::Pages]);
        }
        // Lettering: wide balloon picker and a tall text editor on the left.
        "lettering" => {
            let [center, left] = s.split_left(root, 0.24, vec![Pane::SubTool]);
            s.split_below(left, 0.42, vec![Pane::ToolProperty]);
            let [center, right] = s.split_right(center, 0.80, vec![Pane::Layers, Pane::Navigator]);
            let _ = right;
            s.split_below(center, 0.84, vec![Pane::Pages]);
        }
        // Animation: a tall timeline, color and cels on the right.
        "animation" => {
            let [center, left] = s.split_left(root, 0.16, vec![Pane::SubTool]);
            s.split_below(left, 0.58, vec![Pane::ToolProperty]);
            let [center, right] = s.split_right(center, 0.79, vec![Pane::Color]);
            s.split_below(right, 0.40, vec![Pane::Layers, Pane::Navigator]);
            s.split_below(center, 0.70, vec![Pane::Timeline]);
        }
        // Painting: a large color palette, layers below, no page strip or timeline.
        "illustration" => {
            let [center, left] = s.split_left(root, 0.16, vec![Pane::SubTool]);
            s.split_below(left, 0.55, vec![Pane::ToolProperty]);
            let [_center, right] = s.split_right(center, 0.75, vec![Pane::Color]);
            s.split_below(right, 0.45, vec![Pane::Layers, Pane::Navigator]);
        }
        // Concept art: big color tools and navigator on the right, brushes left, nothing below.
        "concept" => {
            let [center, left] = s.split_left(root, 0.15, vec![Pane::SubTool]);
            s.split_below(left, 0.55, vec![Pane::ToolProperty]);
            let [_center, right] = s.split_right(center, 0.74, vec![Pane::Color]);
            let [_r, lower] = s.split_below(right, 0.50, vec![Pane::Layers]);
            s.split_below(lower, 0.55, vec![Pane::Navigator]);
        }
        // Maximum canvas: one narrow tab stack on the right.
        "focus" => {
            s.split_right(
                root,
                0.84,
                vec![Pane::Color, Pane::Layers, Pane::Navigator, Pane::SubTool],
            );
        }
        _ => return None,
    }
    Some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_has_the_canvas_and_roundtrips() {
        for (id, _) in BUILTIN {
            let d = builtin(id).unwrap_or_else(|| panic!("{id}"));
            assert!(d.find_tab(&Pane::Canvas).is_some(), "{id}");
            let back: DockState<Pane> = ron::from_str(&ron::to_string(&d).unwrap()).unwrap();
            assert!(back.find_tab(&Pane::Canvas).is_some());
        }
        assert!(
            builtin("animation")
                .unwrap()
                .find_tab(&Pane::Timeline)
                .is_some()
        );
        assert!(
            builtin("manga")
                .unwrap()
                .find_tab(&Pane::Timeline)
                .is_none()
        );
        assert!(builtin("manga").unwrap().find_tab(&Pane::Pages).is_some());
        assert!(builtin("nope").is_none());
    }

    #[test]
    fn names_and_kinds() {
        assert_eq!(display_name("custom:My setup"), "My setup");
        assert_eq!(display_name("animation"), "Animation");
        assert_eq!(for_kind(SheetKind::Book), "manga");
        assert!(is_builtin("focus") && !is_builtin(&custom_id("focus")));
    }
}
