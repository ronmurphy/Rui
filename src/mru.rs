//! Most-Recently-Used list for files and projects.
//!
//! Persisted to `~/.config/rui/mru.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_RECENT: usize = 10;

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Mru {
    #[serde(default)]
    pub files: Vec<PathBuf>,
    #[serde(default)]
    pub projects: Vec<PathBuf>,
}

fn mru_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("rui")
        .join("mru.json")
}

impl Mru {
    pub fn load() -> Self {
        let path = mru_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Mru::default(),
        }
    }

    pub fn save(&self) {
        let path = mru_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, text);
        }
    }

    /// Prepend `path` to the recent-files list (deduplicated, max 10).
    pub fn add_file(&mut self, path: PathBuf) {
        self.files.retain(|p| p != &path);
        self.files.insert(0, path);
        self.files.truncate(MAX_RECENT);
        self.save();
    }

    /// Prepend `path` to the recent-projects list (deduplicated, max 10).
    pub fn add_project(&mut self, path: PathBuf) {
        self.projects.retain(|p| p != &path);
        self.projects.insert(0, path);
        self.projects.truncate(MAX_RECENT);
        self.save();
    }
}
