//! Settings: appearance, playback, storage and about, grouped into cards.

use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use meduza_core::settings::AudioQuality;

use super::icons::Icon;
use super::widgets::{card, pill, segmented, switch_row, BtnStyle};
use super::{Action, Cx, UiState};
use crate::theme::*;

const PRELOAD_CHOICES: [usize; 4] = [1, 2, 3, 5];
const REPO_URL: &str = "https://github.com/akilaisadev/meduza-music";
const PROFILE_URL: &str = "https://github.com/akilaisadev";

pub fn show(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState) {
    let vm = cx.vm;
    let mut settings = match vm.playback.settings.try_lock() {
        Ok(guard) => guard.clone(),
        Err(_) => {
            ui.label(RichText::new("Loading…").color(T_DIM));
            return;
        }
    };
    let mut changed = false;

    ui.add_space(SPACE_SM);
    ui.add_space(SPACE_LG);

    egui::ScrollArea::vertical()
        .id_source("settings_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            group(ui, "Appearance");
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Interface scale").color(T_PRI).font(font(TEXT_MD)));
                    ui.label(
                        RichText::new(format!("{:.0}%", cx.zoom * 100.0))
                            .color(T_DIM)
                            .font(font(TEXT_SM)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        if pill(ui, "Reset", BtnStyle::Secondary, None).clicked() {
                            ui.ctx().set_zoom_factor(DEFAULT_ZOOM);
                        }
                    });
                });
                ui.add_space(4.0);
                switch_row(
                    ui,
                    "Expanded sidebar",
                    "Show playlist names next to the navigation.",
                    &mut st.sidebar_expanded,
                );
            });

            ui.add_space(SPACE_XL);
            group(ui, "Playback");
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.label(RichText::new("Audio quality").color(T_PRI).font(font(TEXT_MD)));
                });
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    let selected = match settings.audio_quality {
                        AudioQuality::DataSaver => 0,
                        AudioQuality::Normal => 1,
                        AudioQuality::High => 2,
                    };
                    if let Some(i) = segmented(ui, &["Data saver", "Normal", "High"], selected) {
                        settings.audio_quality = match i {
                            0 => AudioQuality::DataSaver,
                            1 => AudioQuality::Normal,
                            _ => AudioQuality::High,
                        };
                        changed = true;
                    }
                });
                ui.add_space(6.0);

                changed |= switch_row(
                    ui,
                    "Gapless playback",
                    "Pre-buffer the next track for seamless transitions.",
                    &mut settings.gapless_playback,
                );
                changed |= switch_row(
                    ui,
                    "Autoplay radio",
                    "Keep related music going when the queue runs out.",
                    &mut settings.autoplay_radio,
                );
                changed |= switch_row(
                    ui,
                    "Preload next track",
                    "Resolve upcoming stream links in the background.",
                    &mut settings.preload_next_track,
                );

                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.label(RichText::new("Preload buffer").color(T_PRI).font(font(TEXT_MD)));
                    ui.label(RichText::new("tracks ahead").color(T_DIM).font(font(TEXT_SM)));
                });
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    let selected = PRELOAD_CHOICES
                        .iter()
                        .position(|&n| n == settings.preload_buffer_tracks)
                        .unwrap_or(0);
                    let labels = ["1", "2", "3", "5"];
                    if let Some(i) = segmented(ui, &labels, selected) {
                        settings.preload_buffer_tracks = PRELOAD_CHOICES[i];
                        changed = true;
                    }
                });
                ui.add_space(6.0);

                changed |= switch_row(
                    ui,
                    "Low-end performance mode",
                    "Smaller buffers and fewer background jobs for older machines.",
                    &mut settings.low_end_mode,
                );
            });

            ui.add_space(SPACE_XL);
            group(ui, "Storage");
            if st.cache_checked_at.elapsed() >= Duration::from_secs(2) {
                st.cache_checked_at = Instant::now();
                if let Ok(ds) = vm.playback.data_saver.try_lock() {
                    st.cache_size_mb = ds.get_cache_size_mb();
                }
            }
            card(ui, |ui| {
                changed |= switch_row(
                    ui,
                    "Offline audio cache",
                    "Keep played tracks on disk so replays use no data.",
                    &mut settings.enable_cache,
                );
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.label(
                        RichText::new(format!("{:.1} MB used", st.cache_size_mb))
                            .color(T_SEC)
                            .font(font(TEXT_SM)),
                    );
                    ui.add_space(12.0);
                    if pill(ui, "Clear cache", BtnStyle::Danger, Some(Icon::Trash)).clicked() {
                        cx.push(Action::ClearAudioCache);
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    let mut cap = settings.max_cache_size_mb as f32;
                    let slider = ui.add(
                        egui::Slider::new(&mut cap, 100.0..=5000.0)
                            .step_by(100.0)
                            .suffix(" MB")
                            .text("Cache limit"),
                    );
                    if slider.changed() {
                        settings.max_cache_size_mb = cap as u64;
                        changed = true;
                    }
                });
                ui.add_space(12.0);
            });

            ui.add_space(SPACE_XL);
            group(ui, "About");
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.vertical(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Meduza Music").color(T_PRI).font(font(TEXT_LG)).strong());
                        ui.label(
                            RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                                .color(T_DIM)
                                .font(font(TEXT_SM)),
                        );
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if pill(ui, "GitHub profile", BtnStyle::Secondary, None).clicked() {
                                cx.push(Action::OpenUrl(PROFILE_URL.to_string()));
                            }
                            if pill(ui, "Star the project", BtnStyle::Primary, Some(Icon::Heart)).clicked() {
                                cx.push(Action::OpenUrl(REPO_URL.to_string()));
                            }
                        });
                        ui.add_space(6.0);
                    });
                });
            });
            ui.add_space(SPACE_XL);
        });

    if changed {
        settings.save();
        if let Ok(mut guard) = vm.playback.settings.try_lock() {
            *guard = settings;
        }
    }
}

fn group(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).font(font(TEXT_LG)).color(T_PRI).strong());
    ui.add_space(SPACE_SM);
}
