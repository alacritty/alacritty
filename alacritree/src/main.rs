#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bindings;
mod colors;
mod config;
mod fonts;
mod git_status;
mod input;
mod projects;
mod session;
mod state;
mod terminal_view;

use app::AlacritreeApp;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0])
            .with_title("Alacritree"),
        ..Default::default()
    };

    eframe::run_native(
        "Alacritree",
        native_options,
        Box::new(|cc| Ok(Box::new(AlacritreeApp::new(cc)))),
    )
}
