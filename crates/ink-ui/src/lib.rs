//! Inkstudio design system.
//!
//! * `theme`   — tokens generated from `replica/design/tokens.json` and the egui style built from them.
//! * `widgets` — the primitives every screen is built from (see `replica/design/components.md`).
//! * `gallery` — every primitive in every state, the design-system check page.
//! * `preview` — static mock of the main workspace (S03), the look-and-feel reference.
//! * `headless` — renders egui output to RGBA pixels on the CPU (screenshots and golden tests, no GPU).

pub mod gallery;
pub mod headless;
pub mod i18n;
pub mod preview;
pub mod theme;
pub mod widgets;

pub use egui_phosphor::regular as icons;
pub use i18n::tr;
