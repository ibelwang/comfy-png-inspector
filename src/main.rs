#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use comfy_png_inspector::app::App;
use comfy_png_inspector::settings::Settings;
use eframe::egui;

const DEFAULT_SIZE: [f32; 2] = [1200.0, 790.0];

fn main() -> eframe::Result<()> {
    let initial: Option<PathBuf> = std::env::args().skip(1).next().map(PathBuf::from);
    let settings = Settings::load();

    let size = settings.window_size.unwrap_or(DEFAULT_SIZE);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ComfyPNG 查看器")
        .with_decorations(false)
        .with_inner_size(size)
        .with_min_inner_size([1020.0, 660.0]);
    if let Some(pos) = settings.window_pos {
        viewport = viewport.with_position(pos);
    }
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "ComfyPNG 查看器",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, initial.clone(), settings.clone())))),
    )
}

/// Window icon (also shows up in the taskbar), decoded from the embedded PNG.
fn load_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../assets/icon-256.png");
    let image = image::load_from_memory(bytes).ok()?;
    let rgba = image.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    Some(egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}
