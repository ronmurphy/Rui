//! history.rs — Designer undo/redo via XML snapshots on disk.
//!
//! Each canvas mutation (drag, drop, palette insert, merge, etc.) pushes a
//! snapshot of the current .ui XML to a per-session temp directory.
//! Ctrl+Z / Ctrl+Y in Design mode walk back/forward through snapshots.
//!
//! Session dir: /tmp/rui-<unix_seconds>-<pid>/
//!   pid         — contains the owning process ID (for crash detection)
//!   00001.xml, 00002.xml, … — ordered snapshots
//!
//! On clean exit: session dir is deleted.
//! On crash: the dir is left behind. On next startup, Rui finds orphaned
//! dirs (PID no longer running) and offers to recover the last snapshot.

use std::path::PathBuf;
use std::fs;

/// Maximum number of snapshots kept per session (older ones are pruned).
const MAX_SNAPSHOTS: usize = 500;

pub struct DesignerHistory {
    pub session_dir: PathBuf,
    counter: u32,
    /// Ordered list of snapshot file paths.
    snapshots: Vec<PathBuf>,
    /// Index of the currently applied snapshot (None = no history yet).
    current: Option<usize>,
    /// Set to true while undo/redo is being applied so the buffer-changed
    /// hook does not immediately re-snapshot the restored state.
    pub applying: bool,
    /// Debounce generation counter — bumped on every buffer change,
    /// snapshot only fires if the generation hasn't changed after the delay.
    pub gen: u64,
}

impl DesignerHistory {
    pub fn new() -> Self {
        let now = {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        };
        let pid = std::process::id();
        let dir_name = format!("rui-{}-{}", now, pid);
        let session_dir = std::env::temp_dir().join(dir_name);
        let _ = fs::create_dir_all(&session_dir);
        let _ = fs::write(session_dir.join("pid"), pid.to_string());

        DesignerHistory {
            session_dir,
            counter: 0,
            snapshots: Vec::new(),
            current: None,
            applying: false,
            gen: 0,
        }
    }

    /// Push a new snapshot. Truncates any redo history beyond current position.
    pub fn push(&mut self, xml: &str) {
        // Truncate redo history
        if let Some(cur) = self.current {
            let removed: Vec<_> = self.snapshots.drain(cur + 1..).collect();
            for path in removed {
                let _ = fs::remove_file(path);
            }
        }

        self.counter += 1;
        let filename = format!("{:05}.xml", self.counter);
        let path = self.session_dir.join(&filename);
        if fs::write(&path, xml).is_ok() {
            self.snapshots.push(path);
            // Prune oldest if over limit
            while self.snapshots.len() > MAX_SNAPSHOTS {
                let old = self.snapshots.remove(0);
                let _ = fs::remove_file(old);
            }
            self.current = Some(self.snapshots.len() - 1);
        }
    }

    pub fn can_undo(&self) -> bool {
        self.current.map(|c| c > 0).unwrap_or(false)
    }

    pub fn can_redo(&self) -> bool {
        match self.current {
            Some(cur) => cur + 1 < self.snapshots.len(),
            None => false,
        }
    }

    /// Step back one snapshot, returning the XML.
    pub fn undo(&mut self) -> Option<String> {
        let cur = self.current?;
        if cur == 0 { return None; }
        self.current = Some(cur - 1);
        fs::read_to_string(&self.snapshots[cur - 1]).ok()
    }

    /// Step forward one snapshot, returning the XML.
    pub fn redo(&mut self) -> Option<String> {
        let cur = self.current?;
        let next = cur + 1;
        if next >= self.snapshots.len() { return None; }
        self.current = Some(next);
        fs::read_to_string(&self.snapshots[next]).ok()
    }

    /// Reset history (called when a new .ui file is connected to the canvas).
    pub fn reset(&mut self) {
        let removed: Vec<_> = self.snapshots.drain(..).collect();
        for path in removed {
            let _ = fs::remove_file(path);
        }
        self.current = None;
        self.counter = 0;
        self.gen = 0;
    }

    /// Delete the session directory. Call on clean exit.
    pub fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.session_dir);
    }

    /// Scan /tmp for orphaned Rui session dirs from dead processes.
    /// Returns (session_dir, last_xml) pairs for crash recovery.
    pub fn find_recoveries() -> Vec<(PathBuf, String)> {
        let tmp = std::env::temp_dir();
        let current_pid = std::process::id();
        let mut recoveries = Vec::new();

        let Ok(entries) = fs::read_dir(&tmp) else { return recoveries; };

        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("rui-") { continue; }

            let dir = entry.path();
            if !dir.is_dir() { continue; }

            // Read stored PID
            let Ok(pid_str) = fs::read_to_string(dir.join("pid")) else { continue; };
            let Ok(pid) = pid_str.trim().parse::<u32>() else { continue; };
            if pid == current_pid { continue; }

            // Check if process is still alive (Linux: /proc/<pid> exists)
            if std::path::Path::new(&format!("/proc/{}", pid)).exists() { continue; }

            // Find the latest snapshot
            let mut snaps: Vec<PathBuf> = fs::read_dir(&dir)
                .into_iter().flatten().flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("xml"))
                .collect();
            snaps.sort();

            if let Some(last) = snaps.last() {
                if let Ok(xml) = fs::read_to_string(last) {
                    recoveries.push((dir, xml));
                }
            }
        }

        recoveries
    }

    /// Delete a crash-recovery session dir (user declined or accepted recovery).
    pub fn discard_recovery(dir: &PathBuf) {
        let _ = fs::remove_dir_all(dir);
    }
}
