//! Inkstudio document model.
//!
//! Performance design (see `replica/architecture.md` §1):
//! * pixels live in sparse 256×256 tiles; a missing tile is fully transparent, so an empty
//!   600 dpi page costs no memory;
//! * tiles are shared through `Arc` and copied only when written (copy-on-write), so an undo
//!   step keeps just the tiles a stroke touched, and a snapshot of a whole layer is O(tiles);
//! * compositing runs per tile in parallel (rayon), and only for tiles that changed.

pub mod blend;
pub mod color;
pub mod document;
pub mod layer;
pub mod tile;
pub mod undo;

pub use color::Rgba;
pub use document::{Document, LayerId};
pub use layer::{Layer, LayerProps, Raster};
pub use tile::{TILE, Tile, TileKey};
pub use undo::{History, UndoEntry};
