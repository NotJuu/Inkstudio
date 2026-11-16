//! App settings: language, palette layout, recent files. Stored as JSON in the config folder.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    /// "en" / "de"; None = follow the system.
    pub lang: Option<String>,
    pub recent: Vec<PathBuf>,
    pub simulate_pressure: bool,
    /// egui_dock layout as RON (it contains infinite rects, which JSON cannot store).
    #[serde(default)]
    pub dock_ron: Option<String>,
    pub last_dir: Option<PathBuf>,
    /// Autosave interval in seconds (0 = off).
    #[serde(default = "default_autosave")]
    pub autosave_secs: u32,
    /// Autosave also writes the project file (otherwise only a recovery copy).
    #[serde(default)]
    pub autosave_project: bool,
    /// Workspace layout in use (built-in id or "custom:<name>").
    #[serde(default = "default_layout")]
    pub active_layout: String,
    /// Saved dock state (RON) per layout id. Built-ins without an entry use their default.
    #[serde(default)]
    pub layouts: std::collections::BTreeMap<String, String>,
    /// User layouts, in creation order.
    #[serde(default)]
    pub custom_layouts: Vec<String>,
    /// Switch layout when a sheet of another kind is opened.
    #[serde(default = "yes")]
    pub auto_layout: bool,
    /// Layout chosen last for each sheet kind (kind key → layout id).
    #[serde(default)]
    pub kind_layouts: std::collections::BTreeMap<String, String>,
}

fn default_layout() -> String {
    crate::layouts::STANDARD.into()
}

fn yes() -> bool {
    true
}

fn default_autosave() -> u32 {
    180
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: None,
            recent: Vec::new(),
            simulate_pressure: false,
            dock_ron: None,
            last_dir: None,
            autosave_secs: default_autosave(),
            autosave_project: false,
            active_layout: default_layout(),
            layouts: Default::default(),
            custom_layouts: Vec::new(),
            auto_layout: true,
            kind_layouts: Default::default(),
        }
    }
}

/// Platform config folder (override with INKSTUDIO_CONFIG_DIR).
pub fn config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("INKSTUDIO_CONFIG_DIR") {
        return d.into();
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| ".".into());
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(home).join("Inkstudio")
    } else if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Inkstudio")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")).join("inkstudio")
    }
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| ".".into())
}

impl Settings {
    fn path(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    pub fn load_from(dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save_to(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("settings.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, Self::path(dir))
    }

    /// Put `p` first in the recent list (max 10, no duplicates, existing files only).
    pub fn add_recent(&mut self, p: &Path) {
        self.recent.retain(|r| r != p && r.exists());
        self.recent.insert(0, p.to_path_buf());
        self.recent.truncate(10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_recent() {
        let dir = std::env::temp_dir().join(format!("inkstudio-settings-{}", std::process::id()));
        let f1 = dir.join("a.inks");
        let f2 = dir.join("b.inks");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&f1, b"x").unwrap();
        std::fs::write(&f2, b"x").unwrap();
        let mut s = Settings { lang: Some("de".into()), ..Default::default() };
        s.add_recent(&f1);
        s.add_recent(&f2);
        s.add_recent(&f1);
        assert_eq!(s.recent, vec![f1.clone(), f2.clone()]);
        s.save_to(&dir).unwrap();
        let back = Settings::load_from(&dir);
        assert_eq!(back.lang.as_deref(), Some("de"));
        assert_eq!(back.recent.len(), 2);
    }
}
