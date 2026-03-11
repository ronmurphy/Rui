use gtk4::prelude::*;
use gtk4::Application;

mod layout_app;

fn main() {
    let app = Application::builder()
        .application_id("com.example.RuiDogfood")
        .build();

    app.connect_activate(|app| {
        // Load the UI xml from the parent directory
        let ui = include_str!("../layout.ui");
        
        // Strip out Rui design metadata
        let ui_clean: String = ui
            .lines()
            .filter(|l| !l.trim_start().starts_with("<property name=\"rui-"))
            .collect::<Vec<_>>()
            .join("\n");
            
        let builder = gtk4::Builder::from_string(&ui_clean);

        // Connect signals stubbed below
        layout_app::connect_handlers(&builder);

        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("Rui Layout Dogfood")
            .default_width(1200)
            .default_height(800)
            .build();

        if let Some(root) = builder.object::<gtk4::Widget>("rui_root") {
            window.set_child(Some(&root));
        }

        window.present();
    });

    app.run();
}
