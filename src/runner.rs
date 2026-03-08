use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::output::OutputPanel;

enum RunEvent {
    Stdout(String),
    Stderr(String),
    Done { success: bool, exit_code: Option<i32> },
}

#[derive(Clone)]
pub struct RunManager {
    stop_flag: Arc<AtomicBool>,
}

impl RunManager {
    pub fn new() -> Self {
        Self { stop_flag: Arc::new(AtomicBool::new(false)) }
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        !self.stop_flag.load(Ordering::Relaxed)
    }

    pub fn run_file(
        &self,
        path: &Path,
        output: &OutputPanel,
        on_start: impl Fn() + 'static,
        on_done: impl Fn(bool) + 'static,
    ) {
        self.stop_flag.store(false, Ordering::Relaxed);

        let Some((program, args, cwd)) = build_command(path) else {
            output.append_run_error(&format!(
                "Don't know how to run: {}",
                path.display()
            ));
            return;
        };

        output.clear_run();
        output.show_panel();
        output.switch_to_run();
        output.append_run_line(&format!(
            "▶ {} {}",
            program,
            args.join(" ")
        ));
        on_start();

        let stop_flag = self.stop_flag.clone();
        let (tx, rx) = async_channel::unbounded::<RunEvent>();

        std::thread::spawn(move || {
            let mut child = match Command::new(&program)
                .args(&args)
                .current_dir(&cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send_blocking(RunEvent::Stderr(format!(
                        "Failed to start '{}': {}",
                        program, e
                    )));
                    let _ = tx.send_blocking(RunEvent::Done { success: false, exit_code: None });
                    return;
                }
            };

            let stdout = child.stdout.take();
            let stderr_pipe = child.stderr.take();
            let tx2 = tx.clone();
            let flag2 = stop_flag.clone();

            if let Some(err) = stderr_pipe {
                std::thread::spawn(move || {
                    for line in std::io::BufReader::new(err).lines() {
                        if flag2.load(Ordering::Relaxed) { break; }
                        if let Ok(l) = line {
                            let _ = tx2.send_blocking(RunEvent::Stderr(l));
                        }
                    }
                });
            }

            if let Some(out) = stdout {
                for line in std::io::BufReader::new(out).lines() {
                    if stop_flag.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        break;
                    }
                    if let Ok(l) = line {
                        let _ = tx.send_blocking(RunEvent::Stdout(l));
                    }
                }
            }

            let exit_code = child.wait().ok().and_then(|s| s.code());
            let success = exit_code == Some(0);
            let _ = tx.send_blocking(RunEvent::Done { success, exit_code });
        });

        let output = output.clone();
        gtk4::glib::spawn_future_local(async move {
            while let Ok(ev) = rx.recv().await {
                match ev {
                    RunEvent::Stdout(line) => output.append_run_line(&line),
                    RunEvent::Stderr(line) => output.append_run_error(&line),
                    RunEvent::Done { success, exit_code } => {
                        let msg = match exit_code {
                            Some(code) => format!(
                                "── Process exited with code {} ──",
                                code
                            ),
                            None => "── Process terminated ──".to_string(),
                        };
                        if success {
                            output.append_run_success(&msg);
                        } else {
                            output.append_run_error(&msg);
                        }
                        on_done(success);
                        break;
                    }
                }
            }
        });
    }

    /// Run `cargo run` (or `cargo build`) in a given directory.
    /// This is used when a project_dir is set.
    pub fn run_in_dir(
        &self,
        dir: &Path,
        cargo_args: &[&str],
        output: &OutputPanel,
    ) {
        self.stop_flag.store(false, Ordering::Relaxed);

        let program = "cargo".to_string();
        let args: Vec<String> = cargo_args.iter().map(|s| s.to_string()).collect();
        let cwd = dir.to_path_buf();

        output.clear_run();
        output.show_panel();
        output.switch_to_run();
        output.append_run_line(&format!("▶ cargo {}", cargo_args.join(" ")));

        let stop_flag = self.stop_flag.clone();
        let (tx, rx) = async_channel::unbounded::<RunEvent>();

        std::thread::spawn(move || {
            let mut child = match Command::new(&program)
                .args(&args)
                .current_dir(&cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send_blocking(RunEvent::Stderr(format!(
                        "Failed to start '{}': {}",
                        program, e
                    )));
                    let _ = tx.send_blocking(RunEvent::Done { success: false, exit_code: None });
                    return;
                }
            };

            let stdout = child.stdout.take();
            let stderr_pipe = child.stderr.take();
            let tx2 = tx.clone();
            let flag2 = stop_flag.clone();

            if let Some(err) = stderr_pipe {
                std::thread::spawn(move || {
                    for line in std::io::BufReader::new(err).lines() {
                        if flag2.load(Ordering::Relaxed) { break; }
                        if let Ok(l) = line {
                            let _ = tx2.send_blocking(RunEvent::Stderr(l));
                        }
                    }
                });
            }

            if let Some(out) = stdout {
                for line in std::io::BufReader::new(out).lines() {
                    if stop_flag.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        break;
                    }
                    if let Ok(l) = line {
                        let _ = tx.send_blocking(RunEvent::Stdout(l));
                    }
                }
            }

            let exit_code = child.wait().ok().and_then(|s| s.code());
            let success = exit_code == Some(0);
            let _ = tx.send_blocking(RunEvent::Done { success, exit_code });
        });

        let output = output.clone();
        gtk4::glib::spawn_future_local(async move {
            while let Ok(ev) = rx.recv().await {
                match ev {
                    RunEvent::Stdout(line) => output.append_run_line(&line),
                    RunEvent::Stderr(line) => output.append_run_error(&line),
                    RunEvent::Done { success, exit_code } => {
                        let msg = match exit_code {
                            Some(code) => format!(
                                "── Process exited with code {} ──",
                                code
                            ),
                            None => "── Process terminated ──".to_string(),
                        };
                        if success {
                            output.append_run_success(&msg);
                        } else {
                            output.append_run_error(&msg);
                        }
                        break;
                    }
                }
            }
        });
    }

    pub fn open_in_browser(path: &Path) {
        let url = format!("file://{}", path.display());
        let _ = Command::new("xdg-open").arg(&url).spawn();
    }
}

fn build_command(path: &Path) -> Option<(String, Vec<String>, PathBuf)> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let cwd = path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let file_str = path.to_string_lossy().to_string();

    match ext {
        "py" => Some(("python3".into(), vec![file_str], cwd)),

        "js" | "mjs" => Some(("node".into(), vec![file_str], cwd)),

        "ts" => {
            if node_supports_strip_types() {
                Some(("node".into(), vec!["--experimental-strip-types".into(), file_str], cwd))
            } else {
                Some(("npx".into(), vec!["ts-node".into(), file_str], cwd))
            }
        }

        "html" | "htm" | "css" => {
            Some(("echo".into(), vec!["Opening in browser…".into()], cwd))
        }

        "rs" => {
            if let Some(cargo_dir) = find_cargo_toml(path) {
                // Don't `cargo run` the Rui IDE itself — only the user's project
                if is_rui_workspace(&cargo_dir) {
                    // Compile and run this single file with rustc instead
                    let out = format!(
                        "/tmp/rui_{}",
                        path.file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "out".into())
                    );
                    Some((
                        "sh".into(),
                        vec![
                            "-c".into(),
                            format!("rustc {} -o {} && {}", file_str, out, out),
                        ],
                        cwd,
                    ))
                } else {
                    Some(("cargo".into(), vec!["run".into()], cargo_dir))
                }
            } else {
                let out = format!(
                    "/tmp/rui_{}",
                    path.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "out".into())
                );
                Some((
                    "sh".into(),
                    vec![
                        "-c".into(),
                        format!("rustc {} -o {} && {}", file_str, out, out),
                    ],
                    cwd,
                ))
            }
        }

        "sh" | "bash" => Some(("bash".into(), vec![file_str], cwd)),

        // .ui file inside a Cargo project → cargo run that project
        "ui" => {
            if let Some(cargo_dir) = find_cargo_toml(path) {
                if is_rui_workspace(&cargo_dir) {
                    None
                } else {
                    Some(("cargo".into(), vec!["run".into()], cargo_dir))
                }
            } else {
                None
            }
        }

        _ => None,
    }
}

fn find_cargo_toml(path: &Path) -> Option<PathBuf> {
    let mut dir = path.parent()?.to_path_buf();
    loop {
        if dir.join("Cargo.toml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Check if a Cargo workspace is the Rui IDE itself.
/// We never want to `cargo run` ourselves.
fn is_rui_workspace(cargo_dir: &Path) -> bool {
    let toml_path = cargo_dir.join("Cargo.toml");
    if let Ok(contents) = std::fs::read_to_string(&toml_path) {
        // Quick check: look for our application-id or package name
        if contents.contains("name = \"rui\"") {
            return true;
        }
    }
    false
}

fn node_supports_strip_types() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|v| {
            v.trim()
                .trim_start_matches('v')
                .split('.')
                .next()
                .and_then(|maj| maj.parse::<u32>().ok())
        })
        .map(|maj| maj >= 22)
        .unwrap_or(false)
}
