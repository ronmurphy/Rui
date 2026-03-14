use gtk4::prelude::*;
use gtk4::Application;

mod layout_app;

fn main() {
    let app = Application::builder()
        .application_id("com.example.solitare")
        .build();

    app.connect_activate(|app| {
        let ui = include_str!("../layout.ui");
        // Strip Rui design-time metadata (rui-*) before passing to GTK Builder.
        let ui_clean: String = ui
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("<property name=\"rui-")
                    && !t.starts_with("<style-name>")
                    && !t.starts_with("<child><layout")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let builder = gtk4::Builder::from_string(&ui_clean);

        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("Solitaire")
            .default_width(800)
            .default_height(660)
            .build();

        // Wire HeaderBar as window titlebar (CSD — no double title bar)
        if let Some(hb) = builder.object::<gtk4::HeaderBar>("header_bar") {
            // Unparent from grid first — a widget can only have one parent
            hb.unparent();
            window.set_titlebar(Some(&hb));
            // Remove top margin left by the now-empty header row
            if let Some(grid) = builder.object::<gtk4::Grid>("main_grid") {
                grid.set_margin_top(0);
                grid.set_row_spacing(0);
            }
        }

        layout_app::connect_handlers(&builder, &window);

        if let Some(root) = builder.object::<gtk4::Widget>("main_grid") {
            window.set_child(Some(&root));
        }

        window.present();
    });

    app.run();
}
