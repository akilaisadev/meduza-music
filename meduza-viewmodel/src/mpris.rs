//! MPRIS2 / global media-key integration for Linux (issue #6).
//!
//! Owns a `souvlaki::MediaControls` handle on a dedicated thread, publishes
//! track metadata and playback state, and forwards desktop media-key events
//! (Play/Pause, Next, Previous, Seek, Volume) to the `PlaybackManager`.

use std::sync::{Arc, Weak};
use std::time::Duration;

use meduza_core::innertube::TrackItem;
use meduza_playback::playback_manager::{PlaybackManager, PlaybackState};
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};

/// Start the MPRIS service. Returns `None` if no session bus / D-Bus name is
/// available (e.g. headless CI); the app continues normally either way.
pub fn start(playback: Arc<PlaybackManager>) -> Option<()> {
    let config = PlatformConfig {
        dbus_name: "org.meduzamusic.MeduzaMusic",
        display_name: "Meduza Music",
        hwnd: None,
    };

    let mut controls = MediaControls::new(config).ok()?;

    // Weak references everywhere: an Arc here would keep PlaybackManager alive
    // after the window closes, so mpv would never be shut down cleanly.
    let weak = Arc::downgrade(&playback);
    let weak_events = Weak::clone(&weak);
    controls.attach(move |event| {
        let Some(pb) = weak_events.upgrade() else { return; };
        match event {
            MediaControlEvent::Play => pb.play_current_or_next(),
            MediaControlEvent::Pause => {
                if matches!(
                    pb.state.lock().map(|g| g.clone()).unwrap_or(PlaybackState::Idle),
                    PlaybackState::Playing
                ) {
                    pb.toggle_pause();
                }
            }
            MediaControlEvent::Toggle => pb.toggle_pause(),
            MediaControlEvent::Next => pb.skip_next(),
            MediaControlEvent::Previous => pb.skip_prev(),
            MediaControlEvent::Stop => {
                if matches!(
                    pb.state.lock().map(|g| g.clone()).unwrap_or(PlaybackState::Idle),
                    PlaybackState::Playing
                ) {
                    pb.toggle_pause();
                }
            }
            MediaControlEvent::SeekBy(dir, delta) => {
                let cur = pb.progress_secs.lock().map(|g| *g).unwrap_or(0.0);
                let dur = pb.duration_secs.lock().map(|g| *g).unwrap_or(0.0);
                let seconds = delta.as_secs_f32();
                let next = match dir {
                    SeekDirection::Forward => (cur + seconds).min(dur),
                    SeekDirection::Backward => (cur - seconds).max(0.0),
                };
                pb.seek_to(next);
            }
            MediaControlEvent::SetPosition(MediaPosition(d)) => {
                pb.seek_to(d.as_secs_f32().max(0.0));
            }
            MediaControlEvent::SetVolume(v) => {
                pb.set_volume((v.clamp(0.0, 1.0) * 100.0) as f32);
            }
            MediaControlEvent::Quit => {}
            _ => {}
        }
    }).ok()?;

    std::thread::Builder::new()
        .name("mpris-sync".to_string())
        .spawn(move || sync_loop(controls, weak))
        .ok()?;
    Some(())
}

fn sync_loop(mut controls: MediaControls, weak: Weak<PlaybackManager>) {
    let mut last_track_id: Option<String> = None;
    let mut last_state = PlaybackState::Idle;
    let mut last_volume = -1.0f32;
    let mut last_playback_publish = std::time::Instant::now();

    loop {
        std::thread::sleep(Duration::from_millis(250));
        let Some(playback) = weak.upgrade() else { break; };

        let track: Option<TrackItem> = playback.current_track.lock().ok().and_then(|g| g.clone());
        let state = playback.state.lock().map(|g| g.clone()).unwrap_or(PlaybackState::Idle);
        let volume = playback.volume.lock().map(|g| *g).unwrap_or(80.0);
        let position = playback.progress_secs.lock().map(|g| *g).unwrap_or(0.0);
        let progress = Some(MediaPosition(Duration::from_secs_f32(position.max(0.0))));

        if track.as_ref().map(|t| t.media_id.as_str()) != last_track_id.as_deref() {
            let meta = MediaMetadata {
                title: track.as_ref().map(|t| t.title.as_str()),
                artist: track.as_ref().map(|t| t.artist.as_str()),
                album: None,
                // Desktop shells can display remote art URLs directly.
                cover_url: track.as_ref().map(|t| t.thumbnail_url.as_str()),
                duration: track.as_ref().and_then(|t| {
                    if t.duration_seconds > 0 {
                        Some(Duration::from_secs(t.duration_seconds as u64))
                    } else {
                        None
                    }
                }),
            };
            let _ = controls.set_metadata(meta);
            last_track_id = track.map(|t| t.media_id);
        }

        // MPRIS clients read position from the last published playback state,
        // so refresh it roughly once a second while playing.
        let position_refresh = state == PlaybackState::Playing
            && last_playback_publish.elapsed() >= Duration::from_secs(1);
        if state != last_state || position_refresh {
            let playback_status = match state {
                PlaybackState::Playing | PlaybackState::Loading => {
                    MediaPlayback::Playing { progress }
                }
                PlaybackState::Paused => MediaPlayback::Paused { progress },
                _ => MediaPlayback::Stopped,
            };
            let _ = controls.set_playback(playback_status);
            last_state = state;
            last_playback_publish = std::time::Instant::now();
        }

        if (volume - last_volume).abs() > 0.5 {
            let _ = controls.set_volume((volume / 100.0) as f64);
            last_volume = volume;
        }
    }
}
