mod innertube;
mod stream_resolver;
mod playback_manager;
mod ui;
pub mod settings;
pub mod data_saver;
pub mod recommendation_engine;

use eframe::{egui, NativeOptions};
use ui::MeduzaApp;

fn main() -> eframe::Result<()> {
    // Build a Tokio runtime for async InnerTube calls
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    let handle = runtime.handle().clone();

    // Keep runtime alive in background thread
    std::thread::spawn(move || {
        runtime.block_on(std::future::pending::<()>());
    });

    let native_options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Meduza Music")
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([900.0, 600.0])
            .with_visible(false)
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
