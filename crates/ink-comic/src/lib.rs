//! Comic objects for manga pages.
//!
//! * `panel`   — panel frames: create, divide with a gutter, templates, hit-testing.
//! * `balloon` — speech balloons in many styles, with tails, auto-fit to their text.
//! * `text`    — lettering: horizontal and vertical (Japanese) layout, font fallback.
//! * `paint`   — anti-aliased polygon fill and outline strokes into a raster.
//!
//! Objects are kept as editable data and rendered into their layer's raster on change,
//! the same way vector layers work.

pub mod balloon;
pub mod paint;
pub mod panel;
pub mod text;

pub use balloon::{Balloon, BalloonHit, BalloonStyle, TailStyle};
pub use panel::{Panel, PanelSet, PanelTemplate};
pub use text::{Fonts, TextDir};
