//! Library: playlists grid, liked songs, and the playlist detail view.

use std::path::PathBuf;

use eframe::egui::{self, Frame, Margin, Rect, Rounding, RichText, Sense};
use eframe::egui::vec2;

use meduza_core::playlist::Playlist;

use super::icons::Icon;
use super::widgets::{self, art, icon_button, pill, tile, BtnStyle};
use super::{Action, Cx, LibraryTab, ToastKind, UiState};
use crate::theme::*;

pub fn show(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState) {
    if let Some(id) = st.open_playlist.clone() {
        let snap = cx.snap;
        match snap.playlists.iter().find(|p| p.id == id) {
            Some(playlist) => {
                detail(ui, cx, st, playlist);
                return;
            }
            None => st.open_playlist = None,
        }
    }

    ui.add_space(SPACE_SM);
    ui.add_space(SPACE_MD);

    let selected = match st.library_tab {
        LibraryTab::Playlists => 0,
        LibraryTab::Liked => 1,
    };
    if let Some(i) = widgets::segmented(ui, &["Playlists", "Liked songs"], selected) {
        st.library_tab = if i == 0 { LibraryTab::Playlists } else { LibraryTab::Liked };
    }
    ui.add_space(SPACE_LG);

    egui::ScrollArea::vertical()
        .id_source("library_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| match st.library_tab {
            LibraryTab::Playlists => playlists_tab(ui, cx, st),
            LibraryTab::Liked => liked_tab(ui, cx),
        });
}

fn playlists_tab(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut st.new_playlist_name)
                .hint_text("New playlist name")
                .desired_width(260.0),
        );
        if pill(ui, "Create", BtnStyle::Primary, Some(Icon::Plus)).clicked() {
            let name = std::mem::take(&mut st.new_playlist_name);
            cx.push(Action::CreatePlaylist(name));
        }
    });
    ui.add_space(SPACE_LG);

    let snap = cx.snap;
    if snap.playlists.is_empty() {
        widgets::empty_state(
            ui,
            Icon::Library,
            "No playlists yet",
            "Create one above, or right-click any track and choose Add to playlist.",
        );
        return;
    }

    let gap = SPACE_MD;
    let avail = ui.available_width();
    let cols = ((avail + gap) / 210.0).floor().clamp(1.0, 6.0) as usize;
    let w = (avail - gap * (cols as f32 - 1.0)) / cols as f32;
    let size = vec2(w, w * 0.9);

    for row in snap.playlists.chunks(cols) {
        ui.horizontal(|ui| {
            for playlist in row {
                let count = format!("{} songs", playlist.tracks.len());
                let resp = tile(ui, size, &playlist.id, &playlist.name, &count);
                if resp.clicked() {
                    cx.push(Action::OpenPlaylist(Some(playlist.id.clone())));
                }
                resp.context_menu(|ui| playlist_menu(ui, cx, st, playlist));
                ui.add_space(gap);
            }
        });
        ui.add_space(gap);
    }
}

fn playlist_menu(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState, playlist: &Playlist) {
    ui.set_min_width(180.0);
    if ui.button("Open").clicked() {
        cx.push(Action::OpenPlaylist(Some(playlist.id.clone())));
        ui.close_menu();
    }
    if ui.button("Play").clicked() {
        cx.push(Action::PlayPlaylist { id: playlist.id.clone(), shuffle: false });
        ui.close_menu();
    }
    if ui.button("Rename").clicked() {
        st.rename = Some((playlist.id.clone(), playlist.name.clone()));
        ui.close_menu();
    }
    if ui.button("Delete").clicked() {
        cx.push(Action::DeletePlaylist(playlist.id.clone()));
        ui.close_menu();
    }
}

fn liked_tab(ui: &mut egui::Ui, cx: &mut Cx) {
    let snap = cx.snap;
    header_card(ui, cx, "liked", "Liked Songs", &format!("{} songs", snap.liked.len()), None);

    if snap.liked.is_empty() {
        widgets::empty_state(ui, Icon::Heart, "No liked songs yet", "Tap the heart on anything you love.");
        return;
    }
    widgets::track_list(ui, cx, &snap.liked, None);
}

/// Shared header for liked songs and playlists: artwork, title, buttons.
fn header_card(
    ui: &mut egui::Ui,
    cx: &mut Cx,
    id: &str,
    title: &str,
    subtitle: &str,
    first_track: Option<(&str, &str)>,
) {
    let snap = cx.snap;
    let liked_list = id == "liked";
    let size = 176.0;
    Frame::none()
        .fill(SURFACE)
        .rounding(Rounding::same(RADIUS_LG))
        .inner_margin(Margin::same(20.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                match first_track {
                    Some((media_id, url)) => art(ui, cx, media_id, url, rect, RADIUS_MD),
                    None => {
                        let seed = widgets::seed_color(id);
                        widgets::vertical_gradient(ui.painter(), rect, mix(seed, BG, 0.1), mix(seed, BG, 0.6));
                        super::icons::paint(
                            ui.painter(),
                            Rect::from_center_size(rect.center(), vec2(72.0, 72.0)),
                            if liked_list { Icon::Heart } else { Icon::Lyrics },
                            mix(T_PRI, seed, 0.3),
                        );
                    }
                }
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.add_space(SPACE_MD);
                    ui.label(RichText::new("PLAYLIST").font(font(TEXT_XS)).color(T_DIM));
                    ui.label(RichText::new(title).font(font(TEXT_HERO)).color(T_PRI).strong());
                    ui.label(RichText::new(subtitle).font(font(TEXT_SM)).color(T_SEC));
                    ui.add_space(SPACE_LG);
                    ui.horizontal(|ui| {
                        if pill(ui, "Play", BtnStyle::Primary, Some(Icon::Play)).clicked() {
                            cx.push(Action::PlayFrom { tracks: snap.liked.clone(), start: 0, shuffle: false });
                        }
                        if pill(ui, "Shuffle", BtnStyle::Secondary, Some(Icon::Shuffle)).clicked() {
                            cx.push(Action::PlayFrom { tracks: snap.liked.clone(), start: 0, shuffle: true });
                        }
                    });
                });
            });
        });
    ui.add_space(SPACE_LG);
}

fn detail(ui: &mut egui::Ui, cx: &mut Cx, st: &mut UiState, playlist: &Playlist) {
    ui.add_space(SPACE_SM);
    ui.horizontal(|ui| {
        if icon_button(ui, Icon::ChevronLeft, 32.0, T_SEC, "All playlists").clicked() {
            st.open_playlist = None;
            st.delete_armed = false;
        }
        ui.label(RichText::new("Playlists").font(font(TEXT_SM)).color(T_SEC));
    });
    ui.add_space(SPACE_SM);

    let total_secs: u64 = playlist.tracks.iter().map(|t| t.duration_seconds as u64).sum();
    let subtitle = format!(
        "{} songs · {}",
        playlist.tracks.len(),
        duration_text(total_secs)
    );
    let first = playlist.tracks.first().map(|t| (t.media_id.as_str(), t.thumbnail_url.as_str()));

    Frame::none()
        .fill(SURFACE)
        .rounding(Rounding::same(RADIUS_LG))
        .inner_margin(Margin::same(20.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let size = 176.0;
                let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                match first {
                    Some((media_id, url)) => art(ui, cx, media_id, url, rect, RADIUS_MD),
                    None => {
                        let seed = widgets::seed_color(&playlist.id);
                        widgets::vertical_gradient(ui.painter(), rect, mix(seed, BG, 0.1), mix(seed, BG, 0.6));
                    }
                }
                ui.add_space(24.0);
                ui.vertical(|ui| {
                    ui.add_space(SPACE_MD);
                    ui.label(RichText::new("PLAYLIST").font(font(TEXT_XS)).color(T_DIM));
                    ui.label(RichText::new(&playlist.name).font(font(TEXT_HERO)).color(T_PRI).strong());
                    ui.label(RichText::new(&subtitle).font(font(TEXT_SM)).color(T_SEC));
                    ui.add_space(SPACE_LG);
                    ui.horizontal(|ui| {
                        if pill(ui, "Play", BtnStyle::Primary, Some(Icon::Play)).clicked() {
                            cx.push(Action::PlayPlaylist { id: playlist.id.clone(), shuffle: false });
                        }
                        if pill(ui, "Shuffle", BtnStyle::Secondary, Some(Icon::Shuffle)).clicked() {
                            cx.push(Action::PlayPlaylist { id: playlist.id.clone(), shuffle: true });
                        }
                        if pill(ui, "Rename", BtnStyle::Secondary, Some(Icon::Pencil)).clicked() {
                            st.rename = Some((playlist.id.clone(), playlist.name.clone()));
                        }
                        let (label, style) = if st.delete_armed {
                            ("Confirm delete", BtnStyle::Danger)
                        } else {
                            ("Delete", BtnStyle::Secondary)
                        };
                        if pill(ui, label, style, Some(Icon::Trash)).clicked() {
                            if st.delete_armed {
                                cx.push(Action::DeletePlaylist(playlist.id.clone()));
                                st.delete_armed = false;
                            } else {
                                st.delete_armed = true;
                            }
                        }
                    });
                });
            });
        });
    ui.add_space(SPACE_MD);

    // Import / export through a file path typed by the user.
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut st.io_path)
                .hint_text("File path, e.g. /home/you/mix.json")
                .desired_width(380.0),
        );
        if pill(ui, "Export", BtnStyle::Secondary, Some(Icon::Export)).clicked() {
            if st.io_path.trim().is_empty() {
                cx.push(Action::Toast(ToastKind::Error, "Enter a file path first".into()));
            } else {
                cx.push(Action::Export { playlist: playlist.id.clone(), path: PathBuf::from(st.io_path.trim()) });
            }
        }
        if pill(ui, "Import", BtnStyle::Secondary, Some(Icon::Import)).clicked() {
            if st.io_path.trim().is_empty() {
                cx.push(Action::Toast(ToastKind::Error, "Enter a file path first".into()));
            } else {
                cx.push(Action::Import(PathBuf::from(st.io_path.trim())));
            }
        }
    });
    ui.add_space(SPACE_LG);

    if playlist.tracks.is_empty() {
        widgets::empty_state(
            ui,
            Icon::Library,
            "This playlist is empty",
            "Right-click any track and choose Add to playlist.",
        );
        return;
    }
    widgets::track_list(ui, cx, &playlist.tracks, Some(&playlist.id));
}

fn duration_text(total: u64) -> String {
    let h = total / 3600;
    let m = (total / 60) % 60;
    if h > 0 {
        format!("{} hr {} min", h, m)
    } else {
        format!("{} min", m.max(1))
    }
}
