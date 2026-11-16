//! Inkstudio design system.
//!
//! * `theme`   — tokens generated from `replica/design/tokens.json` and the egui style built from them.
//! * `widgets` — the primitives every screen is built from (see `replica/design/components.md`).
//! * `gallery` — every primitive in every state, the design-system check page.
//! * `headless` — renders egui output to RGBA pixels on the CPU (screenshots and golden tests, no GPU).

pub mod gallery;
pub mod headless;
pub mod theme;
pub mod widgets;

pub use egui_phosphor::regular as icons;
