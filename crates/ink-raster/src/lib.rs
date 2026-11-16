//! Inkstudio pixel engine.
//!
//! * `brush` — the stroke engine: pressure, speed, stabilization, taper, stroke buffer.
//! * `tip` — tip shapes (SDF), paper grain, scatter.
//! * `presets` — the brush catalog (manga-focused).
//! * `fill`  — bucket fill that reads reference layers and closes gaps in line art.
//! * `tone`  — print-accurate screentones (LPI, density, angle, dot shape, 1-bit).

pub mod brush;
pub mod curve;
pub mod fill;
pub mod presets;
pub mod tip;
pub mod tone;

pub use brush::{BrushParams, Coverage, Paint, PathPoint, Sample, Stabilizer, Stroke};
pub use curve::Curve;
pub use tip::{Grain, Scatter, TipShape};
