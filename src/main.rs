//! Rui — A GTK4 UI Designer for Rust developers.
//!
//! Forked from rdm-editor. MIT licensed.

use gtk4::prelude::*;
use gtk4::Application;

mod app;
mod config;
mod diff_tool;
mod find;
mod goto;
mod help;
mod menubar;
mod notebook;
mod output;
mod runner;
mod schemes;
mod session;
mod sidebar;
mod statusbar;
mod tab;
mod canvas;
mod codegen;
mod outline;
mod palette;
mod inspector;
mod toolbox;
mod templates;
mod history;
mod mru;

#[cfg(feature = "preview")]
mod preview;

#[cfg(feature = "preview")]
mod ai_panel;

fn main() {
    env_logger::init();

    // The rui-theme GtkSourceView scheme is available via
    // schemes::generate_rui_scheme() if the user sets
    // color_scheme = "rui-theme" in rui.toml.

    let app = Application::builder()
        .application_id("org.rui.designer")
        .build();

    app.connect_activate(|application| {
        load_css();

        let args: Vec<String> = std::env::args().skip(1).collect();
        let paths: Vec<std::path::PathBuf> = args
            .iter()
            .map(std::path::PathBuf::from)
            .filter(|p| p.exists())
            .collect();

        app::build_ui(application, paths);
    });

    app.run();
}

fn load_css() {
    let css = gtk4::CssProvider::new();
    css.load_from_data(&crate::config::default_css());
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("No display"),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
