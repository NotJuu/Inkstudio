//! Inkstudio pixel engine.
//!
//! * `brush` — pens, pencils, airbrush, eraser: pressure, stabilization, taper, stroke buffer.
//! * `fill`  — bucket fill that reads reference layers and closes gaps in line art.
//! * `tone`  — print-accurate screentones (LPI, density, angle, dot shape, 1-bit).

pub mod brush;
pub mod curve;
pub mod fill;
pub mod tone;

pub use brush::{BrushParams, Coverage, Paint, PathPoint, Sample, Stabilizer, Stroke};
pub use curve::Curve;
