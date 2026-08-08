mod innertube;
mod stream_resolver;
mod playback_manager;
mod ui;
pub mod settings;
pub mod data_saver;
pub mod recommendation_engine;
mod workers;

use eframe::{egui, NativeOptions};
use ui::MeduzaApp;

fn main() -> eframe::Result<()> {
    // The shared Tokio runtime lives for the whole process inside workers.rs
    // (a `&'static Runtime` keeps its driver threads alive forever), so we only
    // grab a handle for the UI; no keep-alive thread or per-call runtimes.
    let handle = workers::runtime().handle().clone();

    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Meduza Music")
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([900.0, 600.0])
            .with_app_id("org.meduzamusic.MeduzaMusic")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                    .unwrap_or_default()
            ),
        follow_system_theme: false,
        ..Default::default()
    };

    eframe::run_native(
        "Meduza Music",
        native_options,
        Box::new(move |cc| {
            Box::new(MeduzaApp::new(cc, handle))
        }),
    )
}