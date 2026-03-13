use gtk4::{glib, Builder, Window};

fn main() {
    // Initialize GTK
    gtk4::init().expect("Failed to initialize GTK");

    // Load UI file
    let builder = Builder::new_from_file("/home/brad/Documents/Rui/GamesDemos/Solitare/layout.ui")
        .expect("Failed to load layout file");

    // Get the window and show it
    let window = builder.get_object::<Window>("MainWindow").unwrap();
    window.present().expect("Failed to present window");
}