use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::output::OutputPanel;

fn is_error_location(text: &str) -> bool {
    text.trim().starts_with("--> ")
}

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

    /// Run `cargo check` as a pre-flight, then — only if it passes — run the
    /// given `build_args` (e.g. `&["build", "--release"]`). Both phases stream
    /// to the output panel; any error stops the sequence.
    pub fn check_then_build_in_dir(
        &self,
        dir: &Path,
        build_args: &[&str],
        output: &OutputPanel,
    ) {
        self.stop_flag.store(false, Ordering::Relaxed);

        let cwd = dir.to_path_buf();
        let build_args_owned: Vec<String> = build_args.iter().map(|s| s.to_string()).collect();

        output.clear_run();
        output.show_panel();
        output.switch_to_run();
        output.append_run_line("▶ cargo check  (pre-flight)");

        let stop_flag = self.stop_flag.clone();
        let (tx, rx) = async_channel::unbounded::<RunEvent>();

        std::thread::spawn(move || {
            let check_ok = run_cargo_sync(&cwd, &["check"], &tx, &stop_flag);

            if !check_ok || stop_flag.load(Ordering::Relaxed) {
                let exit_code = if stop_flag.load(Ordering::Relaxed) { None } else { Some(1) };
                let _ = tx.send_blocking(RunEvent::Done { success: false, exit_code });
                return;
            }

            let args_refs: Vec<&str> = build_args_owned.iter().map(|s| s.as_str()).collect();
            let _ = tx.send_blocking(RunEvent::Stdout(format!(
                "\n── Pre-flight passed ✓ ──  running: cargo {} ──",
                build_args_owned.join(" ")
            )));

            let build_ok = run_cargo_sync(&cwd, &args_refs, &tx, &stop_flag);
            let exit_code = if build_ok { Some(0) } else { Some(1) };
            let _ = tx.send_blocking(RunEvent::Done { success: build_ok, exit_code });
        });

        let output = output.clone();
        gtk4::glib::spawn_future_local(async move {
            while let Ok(ev) = rx.recv().await {
                match ev {
                    RunEvent::Stdout(line) => output.append_run_line(&line),
                    RunEvent::Stderr(line) => output.append_run_error(&line),
                    RunEvent::Done { success, exit_code } => {
                        let msg = match exit_code {
                            Some(code) => format!("── Process exited with code {} ──", code),
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

    /// Run `cargo <args> --message-format=json` in `dir`.
    /// Human-readable compiler output (from the `rendered` field) streams to the
    /// output panel.  All raw JSON lines are collected and passed to `on_done`
    /// so the caller can parse diagnostics.
    pub fn run_in_dir_json(
        &self,
        dir: &Path,
        cargo_args: &[&str],
        output: &OutputPanel,
        on_done: impl Fn(Vec<String>) + 'static,
    ) {
        self.stop_flag.store(false, Ordering::Relaxed);

        let mut args: Vec<String> = cargo_args.iter().map(|s| s.to_string()).collect();
        args.push("--message-format=json".to_string());
        let cwd = dir.to_path_buf();

        output.clear_run();
        output.show_panel();
        output.switch_to_run();
        output.append_run_line(&format!("▶ cargo {}", cargo_args.join(" ")));

        let stop_flag = self.stop_flag.clone();
        let (tx, rx) = async_channel::unbounded::<RunEvent>();
        // Extra channel for raw JSON lines
        let (json_tx, json_rx) = async_channel::unbounded::<String>();

        std::thread::spawn(move || {
            let mut child = match Command::new("cargo")
                .args(&args)
                .current_dir(&cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send_blocking(RunEvent::Stderr(format!("Failed to start cargo: {}", e)));
                    let _ = tx.send_blocking(RunEvent::Done { success: false, exit_code: None });
                    return;
                }
            };

            let stderr_pipe = child.stderr.take();
            let tx2 = tx.clone();
            let flag2 = stop_flag.clone();
            if let Some(err) = stderr_pipe {
                std::thread::spawn(move || {
                    for line in std::io::BufReader::new(err).lines() {
                        if flag2.load(Ordering::Relaxed) { break; }
                        if let Ok(l) = line { let _ = tx2.send_blocking(RunEvent::Stderr(l)); }
                    }
                });
            }

            if let Some(out) = child.stdout.take() {
                for line in std::io::BufReader::new(out).lines() {
                    if stop_flag.load(Ordering::Relaxed) { let _ = child.kill(); break; }
                    if let Ok(l) = line {
                        // Extract rendered text for the output panel
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&l) {
                            if val["reason"].as_str() == Some("compiler-message") {
                                if let Some(rendered) = val["message"]["rendered"].as_str() {
                                    for rline in rendered.lines() {
                                        let _ = tx.send_blocking(RunEvent::Stderr(rline.to_string()));
                                    }
                                }
                            }
                        }
                        let _ = json_tx.send_blocking(l);
                    }
                }
            }

            let exit_code = child.wait().ok().and_then(|s| s.code());
            let _ = tx.send_blocking(RunEvent::Done { success: exit_code == Some(0), exit_code });
        });

        let output = output.clone();
        gtk4::glib::spawn_future_local(async move {
            let mut json_lines: Vec<String> = Vec::new();
            // Drain json channel
            while let Ok(line) = json_rx.try_recv() {
                json_lines.push(line);
            }
            while let Ok(ev) = rx.recv().await {
                // Also drain any buffered json lines
                while let Ok(line) = json_rx.try_recv() {
                    json_lines.push(line);
                }
                match ev {
                    RunEvent::Stdout(line) => output.append_run_line(&line),
                    RunEvent::Stderr(line) => {
                        if is_error_location(&line) || line.trim_start().starts_with("error") || line.trim_start().starts_with("warning") {
                            output.append_run_error(&line);
                        } else {
                            output.append_run_line(&line);
                        }
                    }
                    RunEvent::Done { success, exit_code } => {
                        // Final drain
                        while let Ok(line) = json_rx.try_recv() {
                            json_lines.push(line);
                        }
                        let msg = match exit_code {
                            Some(c) => format!("── Process exited with code {} ──", c),
                            None    => "── Process terminated ──".to_string(),
                        };
                        if success {
                            output.append_run_success(&msg);
                        } else {
                            output.append_run_error(&msg);
                        }
                        on_done(json_lines);
                        break;
                    }
                }
            }
        });
    }

    /// Run `rustfmt --edition=2021 <path>` in a background thread.
    /// Calls `on_done(Ok(()))` on success or `on_done(Err(msg))` on failure,
    /// from the GTK main loop.
    pub fn rustfmt_file(path: PathBuf, on_done: impl Fn(Result<(), String>) + 'static) {
        let (tx, rx) = async_channel::bounded::<Result<(), String>>(1);
        std::thread::spawn(move || {
            let result = Command::new("rustfmt")
                .arg("--edition=2021")
                .arg(&path)
                .output();
            match result {
                Ok(out) if out.status.success() => {
                    let _ = tx.send_blocking(Ok(()));
                }
                Ok(out) => {
                    let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    let _ = tx.send_blocking(Err(msg));
                }
                Err(e) => {
                    // rustfmt not installed — silently ignore
                    log::debug!("rustfmt not available: {}", e);
                }
            }
        });
        gtk4::glib::spawn_future_local(async move {
            if let Ok(result) = rx.recv().await {
                on_done(result);
            }
        });
    }

    /// Run `rg --json <query> <dir>`, parse results, call `on_done(matches)`.
    pub fn run_ripgrep(
        query: String,
        dir: PathBuf,
        on_done: impl Fn(Vec<crate::output::SearchMatch>) + 'static,
    ) {
        let (tx, rx) = async_channel::unbounded::<String>();
        std::thread::spawn(move || {
            let child = Command::new("rg")
                .args(["--json", "--", &query])
                .current_dir(&dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else {
                log::warn!("rg not found — install ripgrep for project-wide search");
                return;
            };
            if let Some(out) = child.stdout.take() {
                for line in std::io::BufReader::new(out).lines().flatten() {
                    let _ = tx.send_blocking(line);
                }
            }
            let _ = child.wait();
        });

        gtk4::glib::spawn_future_local(async move {
            let mut matches: Vec<crate::output::SearchMatch> = Vec::new();
            while let Ok(line) = rx.recv().await {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                if val["type"].as_str() != Some("match") { continue }
                let data = &val["data"];
                let file = PathBuf::from(data["path"]["text"].as_str().unwrap_or(""));
                let line_num = data["line_number"].as_u64().unwrap_or(1);
                let col = data["submatches"][0]["start"].as_u64().unwrap_or(0) + 1;
                let text = data["lines"]["text"].as_str().unwrap_or("").to_string();
                matches.push(crate::output::SearchMatch { file, line: line_num, col, text });
            }
            on_done(matches);
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

        // .ui file → cargo run its own project directory.
        // Only check the immediate parent so that projects nested inside other
        // workspaces (e.g. the Rui source tree) use their own Cargo.toml.
        "ui" => {
            let cargo_dir = path
                .parent()
                .filter(|d| d.join("Cargo.toml").exists() && !is_rui_workspace(d))
                .map(|d| d.to_path_buf());
            cargo_dir.map(|d| ("cargo".into(), vec!["run".into()], d))
        }

        _ => None,
    }
}

/// Run a single `cargo` command synchronously (blocking the calling thread),
/// streaming stdout/stderr via `tx`. Returns `true` if the process exited with
/// code 0 and was not stopped.
fn run_cargo_sync(
    dir: &Path,
    args: &[&str],
    tx: &async_channel::Sender<RunEvent>,
    stop_flag: &Arc<AtomicBool>,
) -> bool {
    let mut child = match Command::new("cargo")
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send_blocking(RunEvent::Stderr(format!("Failed to start cargo: {}", e)));
            return false;
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
                if let Ok(l) = line { let _ = tx2.send_blocking(RunEvent::Stderr(l)); }
            }
        });
    }

    if let Some(out) = stdout {
        for line in std::io::BufReader::new(out).lines() {
            if stop_flag.load(Ordering::Relaxed) {
                let _ = child.kill();
                return false;
            }
            if let Ok(l) = line { let _ = tx.send_blocking(RunEvent::Stdout(l)); }
        }
    }

    child.wait().ok().and_then(|s| s.code()) == Some(0)
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
