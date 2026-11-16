//! Undo history. An entry stores the *previous* `Arc<Tile>` of every tile an action touched,
//! so recording a stroke costs one pointer per tile, not a pixel copy.

use crate::document::{Document, LayerId};
use crate::tile::{Tile, TileKey, TILE_PX};
use crate::Layer;
use std::sync::Arc;

pub enum UndoEntry {
    /// Tiles of one layer before the action. Applying swaps them with the current tiles.
    Tiles { label: String, layer: LayerId, tiles: Vec<(TileKey, Option<Arc<Tile>>)> },
    /// Whole-layer structural change (add, delete, reorder, property edit): the layer list before.
    Layers { label: String, layers: Vec<(LayerId, Layer)> },
}

impl UndoEntry {
    pub fn label(&self) -> &str {
        match self {
            UndoEntry::Tiles { label, .. } | UndoEntry::Layers { label, .. } => label,
        }
    }

    /// Approximate memory held only by this entry (tiles shared with the document cost nothing).
    fn bytes(&self) -> usize {
        match self {
            UndoEntry::Tiles { tiles, .. } => tiles.iter().filter(|(_, t)| t.is_some()).count() * TILE_PX * 4,
            UndoEntry::Layers { layers, .. } => layers.len() * 1024,
        }
    }

    /// Apply to `doc`, returning the inverse entry (for redo / undo symmetry).
    fn apply(self, doc: &mut Document) -> UndoEntry {
        match self {
            UndoEntry::Tiles { label, layer, tiles } => {
                let Some(l) = doc.layer_mut(layer) else {
                    return UndoEntry::Tiles { label, layer, tiles: Vec::new() };
                };
                let inverse = tiles
                    .into_iter()
                    .map(|(k, t)| {
                        let cur = l.raster.tile(k).cloned();
                        l.raster.put_tile(k, t);
                        (k, cur)
                    })
                    .collect();
                doc.mark_dirty_layer(layer);
                UndoEntry::Tiles { label, layer, tiles: inverse }
            }
            UndoEntry::Layers { label, layers } => {
                let cur = doc.replace_layers(layers);
                UndoEntry::Layers { label, layers: cur }
            }
        }
    }
}

pub struct History {
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
    pub max_steps: usize,
    pub max_bytes: usize,
}

impl Default for History {
    fn default() -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), max_steps: 200, max_bytes: 2 << 30 }
    }
}

impl History {
    pub fn push(&mut self, e: UndoEntry) {
        self.redo.clear();
        self.undo.push(e);
        self.trim();
    }

    fn trim(&mut self) {
        let mut bytes: usize = self.undo.iter().map(UndoEntry::bytes).sum();
        while self.undo.len() > self.max_steps || (bytes > self.max_bytes && self.undo.len() > 1) {
            let e = self.undo.remove(0);
            bytes -= e.bytes();
        }
    }

    pub fn undo(&mut self, doc: &mut Document) -> Option<String> {
        let e = self.undo.pop()?;
        let label = e.label().to_string();
        self.redo.push(e.apply(doc));
        Some(label)
    }

    pub fn redo(&mut self, doc: &mut Document) -> Option<String> {
        let e = self.redo.pop()?;
        let label = e.label().to_string();
        self.undo.push(e.apply(doc));
        Some(label)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn len(&self) -> usize {
        self.undo.len()
    }
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }
}
