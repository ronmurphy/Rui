//! Session restore — persist open tab paths and layout between launches.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
struct SessionData {
    files: Vec<String>,
    #[serde(default)]
    layout: String, // "code" or "designer"
}

fn session_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("rui")
        .join("session.json")
}

pub fn save(paths: &[PathBuf], layout: &str) {
    let p = session_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let data = SessionData {
        files: paths.iter().filter_map(|pb| pb.to_str().map(String::from)).collect(),
        layout: layout.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&data) {
        let _ = std::fs::write(&p, json);
    }
}

pub fn load() -> (Vec<PathBuf>, String) {
    let text = match std::fs::read_to_string(session_path()) {
        Ok(t) => t,
        Err(_) => return (Vec::new(), "code".into()),
    };
    let data: SessionData = serde_json::from_str(&text).unwrap_or_default();
    let paths = data.files
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    let layout = if data.layout == "designer" { "designer".into() } else { "code".into() };
    (paths, layout)
}
