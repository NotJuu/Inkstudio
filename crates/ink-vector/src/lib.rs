//! Vector layers, the vector eraser and rulers.
//!
//! * `stroke` — editable strokes stored as smoothed points with baked width, rendered with the
//!   same brush engine as raster pens (so ink looks identical), plus the three vector eraser
//!   modes: erase touched part, erase **up to the nearest intersection**, erase whole line.
//! * `ruler` — perspective (1–3 vanishing points), parallel, focus (radial), concentric,
//!   straight-line and symmetry rulers that snap freehand input.

pub mod geom;
pub mod ruler;
pub mod stroke;

pub use geom::Pt;
pub use ruler::{Ruler, RulerSnap};
pub use stroke::{EraseMode, VStroke, VectorLayer};
