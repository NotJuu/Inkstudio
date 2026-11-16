//! Crash recovery: autosave writes `<config>/recovery/<id>.inks` plus a small JSON note.
//! Copies are removed when the book is saved or its changes are discarded; whatever is left
//! after a crash is offered on the next start.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Note {
    pub id: u64,
    pub name: String,
    /// The project file the work belongs to, if it was ever saved.
    pub original: Option<PathBuf>,
    /// Unix seconds.
    pub saved_at: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub note: Note,
    pub file: PathBuf,
}

pub fn dir(config: &Path) -> PathBuf {
    config.join("recovery")
}

pub fn file(config: &Path, id: u64) -> PathBuf {
    dir(config).join(format!("{id:016x}.inks"))
}

fn note_path(config: &Path, id: u64) -> PathBuf {
    dir(config).join(format!("{id:016x}.json"))
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn write_note(config: &Path, note: &Note) -> std::io::Result<()> {
    std::fs::create_dir_all(dir(config))?;
    std::fs::write(
        note_path(config, note.id),
        serde_json::to_vec(note).map_err(std::io::Error::other)?,
    )
}

pub fn remove(config: &Path, id: u64) {
    let _ = std::fs::remove_file(file(config, id));
    let _ = std::fs::remove_file(note_path(config, id));
}

/// Recovery copies left behind, newest first (notes without a project file are skipped).
pub fn list(config: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir(config)) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "json") {
            if let Some(note) = std::fs::read(&p)
                .ok()
                .and_then(|b| serde_json::from_slice::<Note>(&b).ok())
            {
                let f = file(config, note.id);
                if f.is_file() {
                    out.push(Entry { note, file: f });
                }
            }
        }
    }
    out.sort_by(|a, b| b.note.saved_at.cmp(&a.note.saved_at));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_list_remove() {
        let cfg = std::env::temp_dir().join(format!("inkstudio-recovery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cfg);
        for (id, t) in [(1u64, 100u64), (2, 300)] {
            write_note(
                &cfg,
                &Note {
                    id,
                    name: format!("w{id}"),
                    original: None,
                    saved_at: t,
                },
            )
            .unwrap();
            std::fs::write(file(&cfg, id), b"x").unwrap();
        }
        // A note whose file is missing is ignored.
        write_note(
            &cfg,
            &Note {
                id: 3,
                name: "ghost".into(),
                original: None,
                saved_at: 999,
            },
        )
        .unwrap();
        let l = list(&cfg);
        assert_eq!(l.iter().map(|e| e.note.id).collect::<Vec<_>>(), vec![2, 1]);
        remove(&cfg, 2);
        assert_eq!(list(&cfg).len(), 1);
    }
}
