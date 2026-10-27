//! Shows the dialog an app shows when no renderer can draw its window, so it
//! can be checked on each platform without a broken graphics driver.

fn main() {
    fastframe_graphics::Graphics::new("Example")
        .help("https://fastframe.dev")
        .explain(&"egui_glow requires opengl 2.0+.");
}
