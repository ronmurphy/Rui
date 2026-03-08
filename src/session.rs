//! Session restore — persist open tab paths between launches.

use std::path::PathBuf;

fn session_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("rui")
        .join("session.json")
}

pub fn save(paths: &[PathBuf]) {
    let p = session_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let strings: Vec<&str> = paths.iter().filter_map(|pb| pb.to_str()).collect();
    if let Ok(json) = serde_json::to_string(&strings) {
        let _ = std::fs::write(&p, json);
    }
}

pub fn load() -> Vec<PathBuf> {
    let text = match std::fs::read_to_string(session_path()) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let strings: Vec<String> = serde_json::from_str(&text).unwrap_or_default();
    strings
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect()
}
