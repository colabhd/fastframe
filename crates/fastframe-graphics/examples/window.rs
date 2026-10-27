//! Opens a small window through `Graphics::run_native` and says which
//! renderer drew it. On a computer without OpenGL 2, run it with
//! `--features wgpu` to see the fallback, or without it to see the dialog.
//!
//! `-- --report <file>` writes the renderer's name to the file and closes the
//! window after its first frames, for checking a machine over SSH.

use eframe::egui;

struct Window {
    report: Option<std::path::PathBuf>,
    frames: u32,
}

impl eframe::App for Window {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let renderer = fastframe_graphics::renderer().map_or_else(
            || "no renderer".to_owned(),
            |renderer| format!("{renderer:?}"),
        );
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("fastframe-graphics");
            ui.label(format!("Drawn with {renderer}"));
        });
        if let Some(path) = &self.report {
            self.frames += 1;
            if self.frames == 3 {
                std::fs::write(path, format!("{renderer}\n")).expect("the report file is writable");
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ui.ctx().request_repaint();
        }
    }
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let mut arguments = std::env::args_os().skip(1);
    let report = match arguments.next() {
        Some(flag) if flag == "--report" => arguments.next().map(Into::into),
        _ => None,
    };
    fastframe_graphics::Graphics::new("fastframe-graphics example")
        .help("https://fastframe.dev")
        .run_native(
            || eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default().with_inner_size([360.0, 160.0]),
                ..Default::default()
            },
            || {
                let report = report.clone();
                Box::new(move |_| Ok(Box::new(Window { report, frames: 0 })))
            },
        )
}
