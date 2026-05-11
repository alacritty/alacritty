#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bindings;
mod builtin_font;
mod clipboard;
mod colors;
mod config;
mod fonts;
mod git_status;
mod input;
mod links;
mod projects;
mod session;
mod state;
mod terminal_view;
mod worktree;

use app::AlacritreeApp;

/// Pre-resized from the 2048x2048 source so we don't embed a 4 MB blob for
/// what egui only needs at ~256x256.
const WINDOW_ICON: &[u8] = include_bytes!("../assets/icon-256.png");

fn main() -> eframe::Result<()> {
    // egui_winit warns on every cold X11 clipboard probe even when it recovers.
    let default_filter = "info,egui_winit::clipboard=error";
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_filter))
        .init();

    let config = config::load();
    let translucent = config.window.opacity < 1.0;

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([640.0, 400.0])
        .with_title("Alacritree")
        .with_transparent(translucent);
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions { viewport, ..Default::default() };

    eframe::run_native(
        "Alacritree",
        native_options,
        Box::new(move |cc| Ok(Box::new(AlacritreeApp::new(cc, config)))),
    )
}

/// A bad icon is cosmetic — log and fall back to the OS default rather than
/// refusing to start.
fn load_window_icon() -> Option<egui::IconData> {
    let decoder = png::Decoder::new(std::io::Cursor::new(WINDOW_ICON));
    let mut reader = match decoder.read_info() {
        Ok(reader) => reader,
        Err(err) => {
            log::warn!("failed to read window icon header: {err}");
            return None;
        },
    };
    let mut rgba = vec![0; reader.output_buffer_size()];
    let info = match reader.next_frame(&mut rgba) {
        Ok(info) => info,
        Err(err) => {
            log::warn!("failed to decode window icon: {err}");
            return None;
        },
    };
    rgba.truncate(info.buffer_size());
    Some(egui::IconData { rgba, width: info.width, height: info.height })
}
