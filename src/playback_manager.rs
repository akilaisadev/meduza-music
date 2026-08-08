use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::innertube::{InnerTubeClient, TrackItem};
use crate::settings;
use crate::stream_resolver::StreamResolver;

#[derive(Clone, PartialEq, Debug)]
pub enum PlaybackState {
    Idle,
    Loading,
    Playing,
    Paused,
    Error(String),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub track: TrackItem,
}

pub struct PlaybackManager {
    pub state:         Arc<Mutex<PlaybackState>>,
    pub current_track: Arc<Mutex<Option<TrackItem>>>,
    pub queue:         Arc<Mutex<Vec<QueueItem>>>,
    pub queue_index:   Arc<Mutex<usize>>,
    pub progress_secs: Arc<Mutex<f32>>,
    pub duration_secs: Arc<Mutex<f32>>,
    pub volume:        Arc<Mutex<f32>>,
    pub track_ended:   Arc<Mutex<bool>>,
    pub is_shuffle:    Arc<Mutex<bool>>,
    pub repeat_mode:   Arc<Mutex<RepeatMode>>,
    pub liked_songs:           Arc<Mutex<Vec<TrackItem>>>,
    pub history:               Arc<Mutex<Vec<TrackItem>>>,
    pub settings:              Arc<Mutex<crate::settings::AppSettings>>,
    pub stream_cache:          Arc<Mutex<Option<(String, String)>>>,
    pub data_saver:            Arc<Mutex<crate::data_saver::DataSaver>>,
    pub recommendation_engine: Arc<Mutex<crate::recommendation_engine::RecommendationEngine>>,
    pub playback_seq:          Arc<AtomicU64>,
    // BUG-01: shared paused flag for IPC reconnect re-send
    is_paused_flag: Arc<std::sync::atomic::AtomicBool>,

    mpv_process: Arc<Mutex<Option<Child>>>,
    innertube:   Arc<InnerTubeClient>,
}

/// Returns the mpv IPC socket path.
///
/// Security: the socket lives in a user-private runtime directory (0700) and is
/// chmod'd 0600 (see ensure_mpv_running) so that no other local user can
/// connect and issue commands to mpv (which supports arbitrary `run`/`loadfile`
/// commands). The directory is chosen via settings::runtime_dir(), which only
/// trusts XDG_RUNTIME_DIR when it is owned by the user and not
/// group/world-writable — it NEVER falls back to world-writable /tmp.
fn socket_path() -> PathBuf {
    settings::runtime_dir().join("mpv.sock")
}

/// Path of the PID file tracking the mpv process we spawned.
fn mpv_pid_path() -> PathBuf {
    settings::runtime_dir().join("mpv.pid")
}

/// Connect to the mpv IPC socket. Refuses symlinked sockets and sockets not
/// owned by the current user, closing the race where a planted file could
/// redirect our commands to an attacker-controlled listener.
fn connect_mpv() -> Option<UnixStream> {
    let path = socket_path();
    let meta = std::fs::symlink_metadata(&path).ok()?;
    if !meta.file_type().is_socket() {
        return None;
    }
    if meta.uid() != settings::current_uid().unwrap_or(u32::MAX) {
        return None;
    }
    UnixStream::connect(&path).ok()
}

/// Poison-safe mutex lock helper — recovers the guard even if a previous
/// thread panicked while holding the lock.
macro_rules! lk {
    ($m:expr) => {
        $m.lock().unwrap_or_else(|e| e.into_inner())
    };
}

impl PlaybackManager {
    pub fn new(innertube: Arc<InnerTubeClient>) -> Self {
        // ALWAYS kill any orphaned background mpv process on startup
        kill_stale_mpv();

        // IMP-6: Restore CDN URLs from disk — makes frequently played songs instant
        StreamResolver::load_cache_from_disk();

        let state         = Arc::new(Mutex::new(PlaybackState::Idle));
        let progress      = Arc::new(Mutex::new(0.0f32));
        let duration      = Arc::new(Mutex::new(0.0f32));
        let track_ended   = Arc::new(Mutex::new(false));
        let mpv_proc      = Arc::new(Mutex::new(None::<Child>));
        let settings      = Arc::new(Mutex::new(crate::settings::AppSettings::load()));
        let current_track = Arc::new(Mutex::new(None));
        let queue         = Arc::new(Mutex::new(Vec::<QueueItem>::new()));
        let queue_index   = Arc::new(Mutex::new(0));
        let stream_cache          = Arc::new(Mutex::new(None));
        let history               = Arc::new(Mutex::new(Vec::<TrackItem>::new()));
        let data_saver            = Arc::new(Mutex::new(crate::data_saver::DataSaver::new()));
        let recommendation_engine = Arc::new(Mutex::new(crate::recommendation_engine::RecommendationEngine::new()));
        let volume        = Arc::new(Mutex::new(80.0f32));
        let is_shuffle    = Arc::new(Mutex::new(false));
        let repeat_mode   = Arc::new(Mutex::new(RepeatMode::Off));
        let playback_seq    = Arc::new(AtomicU64::new(0));
        let is_paused_flag  = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // ── Background IPC poller (runs every 100 ms, handles gapless transitions & track-ended signalling) ──
        let state_bg    = Arc::clone(&state);
        let progress_bg = Arc::clone(&progress);
        let duration_bg = Arc::clone(&duration);
        let ended_bg    = Arc::clone(&track_ended);
        let curr_bg     = Arc::clone(&current_track);
        let queue_bg    = Arc::clone(&queue);
        let idx_bg      = Arc::clone(&queue_index);
        let cache_bg    = Arc::clone(&stream_cache);
        let shuf_bg     = Arc::clone(&is_shuffle);
        let ds_bg       = Arc::clone(&data_saver);
        let settings_bg = Arc::clone(&settings);
        let inner_bg    = Arc::clone(&innertube);
        let paused_bg   = Arc::clone(&is_paused_flag);
        let seq_bg      = Arc::clone(&playback_seq);

        thread::spawn(move || {
            let mut stream_opt: Option<BufReader<UnixStream>> = None;
            let mut was_connected = false;
            let mut expecting_playlist_reset = false;
            loop {
                thread::sleep(Duration::from_millis(100));

                let st = lk!(state_bg).clone();
                if !matches!(st, PlaybackState::Playing | PlaybackState::Paused) {
                    continue;
                }

                // BUG-01: Re-send pause state after IPC socket reconnect
                let currently_connected = stream_opt.is_some();
                if !was_connected && currently_connected {
                    // Just reconnected — re-apply paused state if needed
                } // (handled below after get_status opens the stream)

                let status = ipc_get_status(&mut stream_opt);

                // BUG-01: Detect reconnect by tracking connection state
                let now_connected = stream_opt.is_some();
                if !was_connected && now_connected {
                    // Just (re)connected — re-send pause state to sync mpv
                    if paused_bg.load(std::sync::atomic::Ordering::SeqCst) {
                        ipc_send(serde_json::json!({"command":["set_property","pause",true]}));
                    }
                }
                was_connected = now_connected;

                // BUG-02: Guard flag — if gapless transition fired this cycle, skip EOF handling
                let mut gapless_transitioned = false;

                // 1. Seamless Gapless Advance: mpv naturally moved to appended track (playlist-pos >= 1)
                if let Some(pos_idx) = status.playlist_pos {
                    if pos_idx == 0.0 {
                        expecting_playlist_reset = false;
                    } else if pos_idx >= 1.0 && !expecting_playlist_reset {
                        expecting_playlist_reset = true;
                        // Remove the old track so mpv resets playlist_pos to 0
                        ipc_send(serde_json::json!({"command": ["playlist-remove", 0]}));

                        // BUG-03: hold BOTH locks together to prevent TOCTOU on queue length
                        let q = lk!(queue_bg);
                        if !q.is_empty() {
                            let mut idx_guard = lk!(idx_bg);
                            let cur = *idx_guard;
                            let next_idx = if cur + 1 < q.len() { cur + 1 } else { 0 };
                            // Bounds-safe access under combined lock
                            let next_track = q[next_idx].track.clone();
                            let remaining = q.len().saturating_sub(next_idx + 1);
                            *idx_guard = next_idx;
                            drop(idx_guard);
                            drop(q);

                            *lk!(curr_bg) = Some(next_track.clone());
                            *lk!(duration_bg) = next_track.duration_seconds as f32;
                            *lk!(progress_bg) = 0.0;
                            *lk!(cache_bg) = None;
                            *lk!(ended_bg) = false;
                            gapless_transitioned = true; // BUG-02
                            println!("[Playback] Seamless gapless transition to: {}", next_track.title);

                            // Trigger preloader for track after next
                            Self::preload_next_in_bg(
                                Arc::clone(&cache_bg),
                                Arc::clone(&queue_bg),
                                Arc::clone(&idx_bg),
                                Arc::clone(&shuf_bg),
                                Arc::clone(&curr_bg),
                                Arc::clone(&state_bg),
                                Arc::clone(&settings_bg),
                                Arc::clone(&ds_bg),
                                Arc::clone(&progress_bg),
                                Arc::clone(&duration_bg),
                                next_track.clone(),
                            );

                            if remaining < 3 {
                                let seq_val = seq_bg.load(std::sync::atomic::Ordering::SeqCst);
                                let quality = lk!(settings_bg).audio_quality;
                                Self::refill_radio_queue_in_bg(
                                    Arc::clone(&inner_bg),
                                    Arc::clone(&queue_bg),
                                    next_track.media_id.clone(),
                                    seq_val,
                                    Arc::clone(&seq_bg),
                                    quality,
                                );
                            }
                        }
                    }
                }

                // 2. Poll progress & duration
                if let Some(pos) = status.time_pos {
                    *lk!(progress_bg) = pos as f32;
                }
                if let Some(dur) = status.duration {
                    if dur > 0.0 {
                        *lk!(duration_bg) = dur as f32;
                    }
                }

                // 3. Track EOF → set flag; handle_auto_advance() in the UI thread
                // is the SINGLE place that acts on this flag. Calling advance from
                // here as well caused double/triple-advance and state corruption.
                if !gapless_transitioned {
                    if let Some(eof) = status.eof_reached {
                        if eof {
                            let mut end_flag = lk!(ended_bg);
                            if !*end_flag {
                                *end_flag = true;
                                println!("[Playback] Track ended (IPC eof-reached), signalling handle_auto_advance.");
                            }
                        }
                    }
                }
            }
        });

        Self {
            state,
            current_track,
            queue,
            queue_index,
            progress_secs: progress,
            duration_secs: duration,
            volume,
            track_ended,
            is_shuffle,
            repeat_mode,
            liked_songs:   Arc::new(Mutex::new(Vec::new())),
            history,
            settings,
            stream_cache,
            data_saver,
            recommendation_engine,
            playback_seq,
            is_paused_flag,
            mpv_process:   mpv_proc,
            innertube,
        }
    }

    /// Play a track — resolves URL via yt-dlp, then streams via mpv.
    pub fn play_now(&self, track: TrackItem) {
        let seq = self.playback_seq.fetch_add(1, Ordering::SeqCst) + 1;

        // Record in listening history
        {
            let mut h = lk!(self.history);
            h.retain(|t| t.media_id != track.media_id);
            h.insert(0, track.clone());
            if h.len() > 30 { h.pop(); }
        }

        // Immediately halt old track audio output so old song never keeps playing
        ipc_send(serde_json::json!({"command": ["stop"]}));

        *lk!(self.current_track) = Some(track.clone());
        *lk!(self.state)         = PlaybackState::Loading;
        *lk!(self.progress_secs) = 0.0;
        *lk!(self.duration_secs) = track.duration_seconds as f32;
        *lk!(self.track_ended)   = false;
        *lk!(self.queue_index)   = 0;
        *lk!(self.stream_cache)  = None;

        let seq_c        = Arc::clone(&self.playback_seq);
        let state_c      = Arc::clone(&self.state);
        let mpv_proc     = Arc::clone(&self.mpv_process);
        let volume       = *lk!(self.volume);
        let video_id     = track.media_id.clone();
        let title        = track.title.clone();
        let quality      = lk!(self.settings).audio_quality;
        let data_saver_c = Arc::clone(&self.data_saver);
        let reco_c       = Arc::clone(&self.recommendation_engine);
        let track_cl     = track.clone();
        let settings_c   = Arc::clone(&self.settings);
        let inner_c      = Arc::clone(&self.innertube);
        let queue_c_io   = Arc::clone(&self.queue);
        let current_track_c = Arc::clone(&self.current_track);
        let duration_c   = Arc::clone(&self.duration_secs);

        crate::workers::io().submit(move || {
            if seq_c.load(Ordering::SeqCst) != seq { return; }

            let mut vid = video_id.clone();
            let mut play_title = title.clone();
            let mut track_to_reco = track_cl.clone();

            // If it's a playlist, fetch its tracks and play the first one!
            if vid.starts_with("PL:") {
                println!("[Playback] Resolving playlist: {}", play_title);
                let playlist_tracks = crate::workers::block_on(inner_c.fetch_next_radio(&vid));
                if seq_c.load(Ordering::SeqCst) != seq { return; }
                
                if playlist_tracks.is_empty() {
                    *lk!(state_c) = PlaybackState::Error("Playlist is empty or could not be loaded".to_string());
                    return;
                }
                
                let first_track = playlist_tracks[0].clone();
                vid = first_track.media_id.clone();
                play_title = first_track.title.clone();
                track_to_reco = first_track.clone();

                *lk!(current_track_c) = Some(first_track.clone());
                *lk!(duration_c) = first_track.duration_seconds as f32;
                
                let mut q = lk!(queue_c_io);
                q.clear();
                for t in playlist_tracks {
                    q.push(QueueItem { track: t });
                }
            }

            ensure_mpv_running(&mpv_proc, lk!(settings_c).mpv_small_buffer());

            // Record user taste activity
            lk!(reco_c).record_play(track_to_reco.clone());

            // Check DataSaver local disk cache (0-data instant replay!)
            if let Some(cached_path) = lk!(data_saver_c).get_cached_file(&vid) {
                if seq_c.load(Ordering::SeqCst) != seq { return; }
                println!("[DataSaver] ZERO-DATA INSTANT PLAYBACK FROM DISK: '{}' ({})", play_title, cached_path);
                ipc_send(serde_json::json!({"command": ["loadfile", cached_path, "replace"]}));
                ipc_send(serde_json::json!({"command": ["set_property", "volume", volume as f64]}));
                ipc_send(serde_json::json!({"command": ["set_property", "pause", false]}));
                *lk!(state_c) = PlaybackState::Playing;
                return;
            }

            println!("[Playback] Resolving stream for '{}' ({})", play_title, vid);
            let Some(url) = StreamResolver::get_audio_url(&vid, quality) else {
                if seq_c.load(Ordering::SeqCst) == seq {
                    *lk!(state_c) = PlaybackState::Error("yt-dlp: could not resolve stream URL".to_string());
                }
                return;
            };

            if seq_c.load(Ordering::SeqCst) != seq {
                println!("[Playback] Discarding stale resolved stream for '{}'", play_title);
                return;
            }

            ipc_send(serde_json::json!({"command": ["loadfile", url, "replace"]}));
            ipc_send(serde_json::json!({"command": ["set_property", "volume", volume as f64]}));
            ipc_send(serde_json::json!({"command": ["set_property", "pause", false]}));
            *lk!(state_c) = PlaybackState::Playing;
            println!("[Playback] Streaming: '{}' ({})", play_title, vid);

            // Cache stream to disk in background if enabled in settings
            let (enable_cache, max_cap, pace) = {
                let s = lk!(settings_c);
                (s.enable_cache, s.max_cache_size_mb, s.pace_downloads())
            };
            if enable_cache {
                lk!(data_saver_c).cache_stream_in_bg(vid, url, max_cap, pace);
            }
        });

        // Only trigger preloader for real tracks (not playlist markers)
        if !track.media_id.starts_with("PL:") {
            // Immediately trigger background preloader (waits until Playing state)
            self.trigger_background_preloader(track.clone());

            // IMP-1: Fetch radio queue in background, guarded by playback_seq,
            // then speculatively warm the URL cache for the next 2 tracks.
            let innertube_c = Arc::clone(&self.innertube);
            let queue_c     = Arc::clone(&self.queue);
            let track_c     = track.clone();
            let seq_c2      = Arc::clone(&self.playback_seq);
            let seq_val     = seq;
            let quality_w   = lk!(self.settings).audio_quality;

            crate::workers::download().submit(move || {
                if seq_c2.load(Ordering::SeqCst) != seq_val { return; }
                let radio = crate::workers::block_on(innertube_c.fetch_next_radio(&track_c.media_id));
                if seq_c2.load(Ordering::SeqCst) != seq_val { return; }
                // Warm next 2 URLs before populating queue so skips are instant
                let warm_ids: Vec<String> = radio.iter().take(2).map(|t| t.media_id.clone()).collect();
                let mut q = lk!(queue_c);
                q.clear();
                q.push(QueueItem { track: track_c });
                for t in radio { q.push(QueueItem { track: t }); }
                drop(q);
                Self::warm_url_cache_in_bg(warm_ids, quality_w);
            });
        }
        // For playlists: queue and current_track are set in the io thread after
        // fetching playlist tracks. The preloader will be triggered after the
        // io thread resolves the actual first track.
    }

    pub fn toggle_pause(&self) {
        let st = lk!(self.state).clone();
        match st {
            PlaybackState::Playing => {
                // BUG-09: Only update Rust state if IPC send succeeds
                if ipc_send_reliable(serde_json::json!({"command":["set_property","pause",true]})) {
                    *lk!(self.state) = PlaybackState::Paused;
                    self.is_paused_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            PlaybackState::Paused => {
                if ipc_send_reliable(serde_json::json!({"command":["set_property","pause",false]})) {
                    *lk!(self.state) = PlaybackState::Playing;
                    self.is_paused_flag.store(false, std::sync::atomic::Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }

    pub fn skip_next(&self) {
        let queue = lk!(self.queue).clone();
        if queue.is_empty() { return; }

        let mut idx     = lk!(self.queue_index);
        let is_shuf     = *lk!(self.is_shuffle);

        let next_idx = if is_shuf {
            // BUG-05: Better shuffle — mix nanos with thread ID to reduce bias
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize)
                .unwrap_or(1);
            let tid = format!("{:?}", std::thread::current().id());
            let tid_hash: usize = tid.bytes().fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
            let cur = *idx;
            let r = (nanos ^ tid_hash ^ (cur.wrapping_mul(2654435761))) % queue.len();
            // Don't repeat same track if more than 1 in queue
            if queue.len() > 1 && r == cur { (r + 1) % queue.len() } else { r }
        } else if *idx + 1 < queue.len() {
            *idx + 1
        } else {
            0
        };

        *idx = next_idx;
        let next_track = queue[next_idx].track.clone();
        drop(idx);

        let cached_url = {
            let c = lk!(self.stream_cache);
            if let Some((ref vid, ref url)) = *c {
                if vid == &next_track.media_id {
                    Some(url.clone())
                } else { None }
            } else { None }
        };

        if let Some(url) = cached_url {
            let volume = *lk!(self.volume);
            ipc_send(serde_json::json!({"command": ["loadfile", url, "replace"]}));
            ipc_send(serde_json::json!({"command": ["set_property", "volume", volume as f64]}));
            ipc_send(serde_json::json!({"command": ["set_property", "pause", false]}));
            *lk!(self.current_track) = Some(next_track.clone());
            *lk!(self.state)         = PlaybackState::Playing;
            *lk!(self.progress_secs) = 0.0;
            *lk!(self.duration_secs) = next_track.duration_seconds as f32;
            *lk!(self.track_ended)   = false;
            *lk!(self.stream_cache)  = None;
            println!("[Playback] Instant Skip Next (0.01s): {}", next_track.title);

            self.trigger_background_preloader(next_track);
        } else {
            ipc_send(serde_json::json!({"command": ["stop"]}));
            self.play_now_without_queue_reset(next_track);
        }
    }

    pub fn skip_prev(&self) {
        let queue = lk!(self.queue).clone();
        if queue.is_empty() { return; }

        let mut idx  = lk!(self.queue_index);
        let prev_idx = if *idx > 0 { *idx - 1 } else { queue.len() - 1 };

        *idx = prev_idx;
        let prev_track = queue[prev_idx].track.clone();
        drop(idx);

        ipc_send(serde_json::json!({"command": ["stop"]}));
        self.play_now_without_queue_reset(prev_track);
    }

    /// Internal helper: play track without resetting active radio queue.
    fn play_now_without_queue_reset(&self, track: TrackItem) {
        let seq = self.playback_seq.fetch_add(1, Ordering::SeqCst) + 1;

        // Record in listening history
        {
            let mut h = lk!(self.history);
            h.retain(|t| t.media_id != track.media_id);
            h.insert(0, track.clone());
            if h.len() > 30 { h.pop(); }
        }

        ipc_send(serde_json::json!({"command": ["stop"]}));

        *lk!(self.current_track) = Some(track.clone());
        *lk!(self.state)         = PlaybackState::Loading;
        *lk!(self.progress_secs) = 0.0;
        *lk!(self.duration_secs) = track.duration_seconds as f32;
        *lk!(self.track_ended)   = false;

        let seq_c    = Arc::clone(&self.playback_seq);
        let state_c  = Arc::clone(&self.state);
        let mpv_proc = Arc::clone(&self.mpv_process);
        let volume   = *lk!(self.volume);
        let video_id = track.media_id.clone();

        let quality  = lk!(self.settings).audio_quality;
        let cache_c  = Arc::clone(&self.stream_cache);
        let title    = track.title.clone();
        let settings_c2 = Arc::clone(&self.settings);

        crate::workers::io().submit(move || {
            if seq_c.load(Ordering::SeqCst) != seq { return; }

            ensure_mpv_running(&mpv_proc, lk!(settings_c2).mpv_small_buffer());
            println!("[Playback] Resolving stream for '{}' ({})", title, video_id);
            
            let cached_url = {
                let mut c = lk!(cache_c);
                if let Some((vid, url)) = c.as_ref() {
                    if vid == &video_id {
                        let res = url.clone();
                        *c = None;
                        Some(res)
                    } else { None }
                } else { None }
            };

            let stream_url = if let Some(url) = cached_url {
                println!("[Preloader] Instant load pre-resolved stream URL for '{}' ({})", title, video_id);
                url
            } else if let Some(url) = StreamResolver::get_audio_url(&video_id, quality) {
                url
            } else {
                if seq_c.load(Ordering::SeqCst) == seq {
                    *lk!(state_c) = PlaybackState::Error("Could not resolve stream URL".to_string());
                }
                return;
            };

            if seq_c.load(Ordering::SeqCst) != seq { return; }

            ipc_send(serde_json::json!({"command": ["loadfile", stream_url, "replace"]}));
            ipc_send(serde_json::json!({"command": ["set_property", "volume", volume as f64]}));
            ipc_send(serde_json::json!({"command": ["set_property", "pause", false]}));
            *lk!(state_c) = PlaybackState::Playing;
            println!("[Playback] Streaming: '{}' ({})", title, video_id);
        });

        self.trigger_background_preloader(track);
    }

    /// Trigger background preloader for the next track in the queue.
    fn trigger_background_preloader(&self, current_track: TrackItem) {
        Self::preload_next_in_bg(
            Arc::clone(&self.stream_cache),
            Arc::clone(&self.queue),
            Arc::clone(&self.queue_index),
            Arc::clone(&self.is_shuffle),
            Arc::clone(&self.current_track),
            Arc::clone(&self.state),
            Arc::clone(&self.settings),
            Arc::clone(&self.data_saver),
            Arc::clone(&self.progress_secs),
            Arc::clone(&self.duration_secs),
            current_track,
        );
    }

    pub fn preload_next_in_bg(
        cache_c: Arc<Mutex<Option<(String, String)>>>,
        queue_c: Arc<Mutex<Vec<QueueItem>>>,
        idx_c: Arc<Mutex<usize>>,
        shuf_c: Arc<Mutex<bool>>,
        curr_c: Arc<Mutex<Option<TrackItem>>>,
        state_c: Arc<Mutex<PlaybackState>>,
        settings_c: Arc<Mutex<crate::settings::AppSettings>>,
        data_saver_c: Arc<Mutex<crate::data_saver::DataSaver>>,
        progress_c: Arc<Mutex<f32>>,
        duration_c: Arc<Mutex<f32>>,
        current_track: TrackItem,
    ) {
        let vid_c = current_track.media_id.clone();
        // Use download pool (long-running sleeps) — io pool is for short latency work.
        // We keep the preloader on download to avoid starving URL resolves on the io pool.
        crate::workers::download().submit(move || {
            let (quality, is_data_saver) = {
                let s = lk!(settings_c);
                (s.audio_quality, s.audio_quality == crate::settings::AudioQuality::DataSaver)
            };

            // IMP-2: Short bounded wait (max 2s) instead of infinite loop.
            // Start resolving the next URL as soon as the current track is loading.
            let mut waited = 0u32;
            loop {
                thread::sleep(Duration::from_millis(200));
                waited += 1;
                let st = lk!(state_c).clone();
                if matches!(st, PlaybackState::Playing) { break; }
                if matches!(st, PlaybackState::Idle | PlaybackState::Error(_)) { return; }
                if waited >= 10 { break; } // 2s max — proceed optimistically
            }

            if let Some(ref t) = *lk!(curr_c) {
                if t.media_id != vid_c { return; }
            } else { return; }

            let q = lk!(queue_c);
            if q.is_empty() { return; }
            let idx = *lk!(idx_c);
            let next_idx = if *lk!(shuf_c) {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos() as usize)
                    .unwrap_or(1);
                let r = (nanos ^ idx.wrapping_mul(2654435761)) % q.len();
                if q.len() > 1 && r == idx { (r + 1) % q.len() } else { r }
            } else if idx + 1 < q.len() {
                idx + 1
            } else {
                0
            };

            let next_vid   = q[next_idx].track.media_id.clone();
            let next_title = q[next_idx].track.title.clone();
            drop(q);

            // De-dup: skip if already cached
            {
                let c = lk!(cache_c);
                if c.as_ref().map_or(false, |(vid, _)| vid == &next_vid) {
                    println!("[Preloader] Already cached '{}' — skipping resolve", next_title);
                    return;
                }
            }

            // 1. Check DataSaver local disk cache (zero-data instant replay)
            let cached_file = lk!(data_saver_c).get_cached_file(&next_vid);

            let stream_url = if let Some(local_path) = cached_file {
                println!("[Preloader] 0-Data disk cache hit for next: '{}'", next_title);
                Some(local_path)
            } else {
                // IMP-4: Racing resolver — two parallel threads, first URL wins.
                // Race only when CPU headroom allows (disabled by low-end mode).
                let racing = lk!(settings_c).resolve_racing();
                if racing {
                    println!("[Preloader] Racing-resolve next: '{}' ({})", next_title, next_vid);
                    StreamResolver::get_audio_url_racing(&next_vid, quality)
                } else {
                    println!("[Preloader] Resolve next: '{}' ({})", next_title, next_vid);
                    StreamResolver::get_audio_url(&next_vid, quality)
                }
            };

            let Some(url) = stream_url else { return; };

            // Check current track hasn't changed during resolution
            if let Some(ref t) = *lk!(curr_c) {
                if t.media_id != vid_c { return; }
            } else { return; }

            // Store URL in cache so skip_next() can use it for instant gapless play
            *lk!(cache_c) = Some((next_vid.clone(), url.clone()));

            // IMP-5: DataSaver mode — wait until 70% before appending to mpv
            // Normal mode — wait until 15% (or 25s) to avoid competing with current stream
            if is_data_saver {
                loop {
                    thread::sleep(Duration::from_millis(800));
                    let st = lk!(state_c).clone();
                    if !matches!(st, PlaybackState::Playing) { return; }
                    if let Some(ref t) = *lk!(curr_c) {
                        if t.media_id != vid_c { return; }
                    } else { return; }
                    let prog = *lk!(progress_c);
                    let dur  = *lk!(duration_c);
                    if dur > 0.0 && (prog / dur >= 0.70 || dur - prog <= 30.0) { break; }
                }
            } else {
                // Wait until 15% progress — current stream has buffered enough by then
                loop {
                    thread::sleep(Duration::from_millis(500));
                    let st = lk!(state_c).clone();
                    if !matches!(st, PlaybackState::Playing | PlaybackState::Paused) { return; }
                    if let Some(ref t) = *lk!(curr_c) {
                        if t.media_id != vid_c { return; }
                    } else { return; }
                    let prog = *lk!(progress_c);
                    let dur  = *lk!(duration_c);
                    if dur > 0.0 && (prog / dur >= 0.15 || prog >= 25.0) { break; }
                }
            }

            // Final staleness check
            if let Some(ref t) = *lk!(curr_c) {
                if t.media_id != vid_c { return; }
            } else { return; }

            // Append to mpv playlist for native gapless 0ms transition
            ipc_send(serde_json::json!({"command": ["loadfile", url, "append"]}));
            println!("[Preloader] Appended '{}' to mpv playlist for gapless playback", next_title);
        });
    }

    pub fn refill_radio_queue_in_bg(
        innertube: Arc<InnerTubeClient>,
        queue_c: Arc<Mutex<Vec<QueueItem>>>,
        video_id: String,
        seq_val: u64,
        seq_arc: Arc<AtomicU64>,
        quality: crate::settings::AudioQuality,
    ) {
        crate::workers::download().submit(move || {
            if seq_arc.load(Ordering::SeqCst) != seq_val { return; }
            let radio_tracks = crate::workers::block_on(innertube.fetch_next_radio(&video_id));
            if seq_arc.load(Ordering::SeqCst) != seq_val { return; }
            if radio_tracks.is_empty() { return; }

            let warm_ids: Vec<String> = radio_tracks.iter().take(2)
                .map(|t| t.media_id.clone()).collect();

            let mut q = lk!(queue_c);
            let existing_ids: std::collections::HashSet<String> =
                q.iter().map(|item| item.track.media_id.clone()).collect();

            let mut added = 0;
            for track in radio_tracks {
                if !existing_ids.contains(&track.media_id) {
                    q.push(QueueItem { track });
                    added += 1;
                }
            }
            drop(q);
            println!("[Playback] Auto-refilled radio queue with {} new tracks.", added);

            // IMP-1: Warm URL cache for the new tracks
            Self::warm_url_cache_in_bg(warm_ids, quality);
        });
    }

    /// IMP-1: Speculatively pre-resolve CDN URLs for upcoming tracks.
    /// Results are stored in StreamResolver's in-memory + disk cache so
    /// subsequent play/skip calls are instant (0.001s) cache hits.
    pub fn warm_url_cache_in_bg(video_ids: Vec<String>, quality: crate::settings::AudioQuality) {
        if video_ids.is_empty() { return; }
        crate::workers::io().submit(move || {
            for (i, vid) in video_ids.into_iter().enumerate() {
                // Stagger to avoid simultaneous yt-dlp processes
                if i > 0 { thread::sleep(Duration::from_millis(800)); }
                StreamResolver::get_audio_url(&vid, quality);
            }
        });
    }

    pub fn execute_bg_auto_advance(
        queue_c: Arc<Mutex<Vec<QueueItem>>>,
        idx_c: Arc<Mutex<usize>>,
        shuf_c: Arc<Mutex<bool>>,
        rep_c: Arc<Mutex<RepeatMode>>,
        curr_c: Arc<Mutex<Option<TrackItem>>>,
        state_c: Arc<Mutex<PlaybackState>>,
        cache_c: Arc<Mutex<Option<(String, String)>>>,
        ds_c: Arc<Mutex<crate::data_saver::DataSaver>>,
        settings_c: Arc<Mutex<crate::settings::AppSettings>>,
        duration_c: Arc<Mutex<f32>>,
        progress_c: Arc<Mutex<f32>>,
        ended_c: Arc<Mutex<bool>>,
        mpv_proc: Arc<Mutex<Option<Child>>>,
        vol_c: Arc<Mutex<f32>>,
        innertube_c: Arc<InnerTubeClient>,
        seq_arc: Arc<AtomicU64>,
    ) {
        let queue = lk!(queue_c).clone();
        if queue.is_empty() { return; }

        let rep = *lk!(rep_c);
        let mut idx = lk!(idx_c);

        let next_idx = match rep {
            RepeatMode::One => *idx,
            _ => {
                if *lk!(shuf_c) {
                    // BUG-05: Better shuffle in auto-advance
                    let nanos = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.subsec_nanos() as usize)
                        .unwrap_or(1);
                    let cur = *idx;
                    let r = (nanos ^ cur.wrapping_mul(2654435761)) % queue.len();
                    if queue.len() > 1 && r == cur { (r + 1) % queue.len() } else { r }
                } else if *idx + 1 < queue.len() {
                    *idx + 1
                } else {
                    0
                }
            }
        };

        *idx = next_idx;
        let next_track = queue[next_idx].track.clone();
        let remaining = queue.len().saturating_sub(next_idx + 1);
        drop(idx);

        *lk!(curr_c) = Some(next_track.clone());
        *lk!(state_c) = PlaybackState::Loading;
        *lk!(progress_c) = 0.0;
        *lk!(duration_c) = next_track.duration_seconds as f32;
        *lk!(ended_c) = false;

        let video_id = next_track.media_id.clone();
        let title = next_track.title.clone();
        let quality = lk!(settings_c).audio_quality;
        let volume = *lk!(vol_c);

        let cache_c_spawn    = Arc::clone(&cache_c);
        let ds_c_spawn       = Arc::clone(&ds_c);
        let settings_c_spawn = Arc::clone(&settings_c);
        let state_c_spawn    = Arc::clone(&state_c);
        let mpv_proc_spawn   = Arc::clone(&mpv_proc);

        crate::workers::io().submit(move || {
            ensure_mpv_running(&mpv_proc_spawn, lk!(settings_c_spawn).mpv_small_buffer());

            let cached_url = {
                let mut c = lk!(cache_c_spawn);
                if let Some((ref vid, ref url)) = *c {
                    if vid == &video_id {
                        let res = url.clone();
                        *c = None;
                        Some(res)
                    } else { None }
                } else { None }
            };

            let stream_url = if let Some(url) = cached_url {
                println!("[Playback] Direct advance using preloaded stream: '{}' ({})", title, video_id);
                url
            } else if let Some(local_path) = lk!(ds_c_spawn).get_cached_file(&video_id) {
                println!("[DataSaver] 0-Data local playback for auto advance: '{}'", title);
                local_path
            } else if let Some(url) = StreamResolver::get_audio_url(&video_id, quality) {
                url
            } else {
                *lk!(state_c_spawn) = PlaybackState::Error("Failed to resolve stream URL".to_string());
                return;
            };

            ipc_send(serde_json::json!({"command": ["loadfile", stream_url, "replace"]}));
            ipc_send(serde_json::json!({"command": ["set_property", "volume", volume as f64]}));
            ipc_send(serde_json::json!({"command": ["set_property", "pause", false]}));
            *lk!(state_c_spawn) = PlaybackState::Playing;
            println!("[Playback] Auto-advanced to: '{}'", title);

            let (enable_cache, max_cap, pace) = {
                let s = lk!(settings_c_spawn);
                (s.enable_cache, s.max_cache_size_mb, s.pace_downloads())
            };
            if enable_cache {
                lk!(ds_c_spawn).cache_stream_in_bg(video_id, stream_url, max_cap, pace);
            }
        });

        // Preload track after next
        Self::preload_next_in_bg(
            cache_c,
            queue_c.clone(),
            idx_c,
            shuf_c,
            curr_c,
            state_c,
            Arc::clone(&settings_c),
            ds_c,
            progress_c,
            duration_c,
            next_track.clone(),
        );

        // Auto-refill radio queue if < 3
        if remaining < 3 {
            let seq_val = seq_arc.load(Ordering::SeqCst);
            let quality = lk!(settings_c).audio_quality;
            Self::refill_radio_queue_in_bg(innertube_c, queue_c, next_track.media_id, seq_val, seq_arc, quality);
        }
    }

    pub fn set_volume(&self, vol: f32) {
        *lk!(self.volume) = vol;
        ipc_send(serde_json::json!({"command":["set_property","volume", vol as f64]}));
    }

    pub fn seek_to(&self, secs: f32) {
        // BUG-14: Clamp seek value to valid range
        let dur = *lk!(self.duration_secs);
        let secs = secs.clamp(0.0, if dur > 0.0 { dur } else { f32::MAX });
        ipc_send(serde_json::json!({"command":["seek", secs as f64,"absolute"]}));
        *lk!(self.progress_secs) = secs;
    }

    pub fn toggle_shuffle(&self) {
        let mut shuf = lk!(self.is_shuffle);
        *shuf = !*shuf;
    }

    pub fn toggle_repeat(&self) {
        let mut rep = lk!(self.repeat_mode);
        *rep = match *rep {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        };
    }

    pub fn is_liked(&self, media_id: &str) -> bool {
        lk!(self.liked_songs).iter().any(|t| t.media_id == media_id)
    }

    pub fn toggle_like(&self, track: TrackItem) {
        let mut liked = lk!(self.liked_songs);
        if let Some(pos) = liked.iter().position(|t| t.media_id == track.media_id) {
            liked.remove(pos);
        } else {
            liked.push(track);
        }
    }

    /// Check & reset the track-ended flag (called from UI thread, no IPC).
    /// This is now the SINGLE authority for auto-advancing tracks.
    pub fn handle_auto_advance(&self) {
        // Guard: only act on ended flag, regardless of state
        let ended = *lk!(self.track_ended);
        if !ended { return; }

        // Reset flag before spawning next track to prevent double-advance
        *lk!(self.track_ended) = false;

        let rep = *lk!(self.repeat_mode);
        match rep {
            RepeatMode::One => {
                if let Some(t) = lk!(self.current_track).clone() {
                    println!("[Playback] Repeat-One: replaying '{}'", t.title);
                    self.play_now_without_queue_reset(t);
                }
            }
            _ => {
                let queue = lk!(self.queue).clone();
                if queue.is_empty() { return; }
                // Use execute_bg_auto_advance so preloaded URLs are used
                Self::execute_bg_auto_advance(
                    Arc::clone(&self.queue),
                    Arc::clone(&self.queue_index),
                    Arc::clone(&self.is_shuffle),
                    Arc::clone(&self.repeat_mode),
                    Arc::clone(&self.current_track),
                    Arc::clone(&self.state),
                    Arc::clone(&self.stream_cache),
                    Arc::clone(&self.data_saver),
                    Arc::clone(&self.settings),
                    Arc::clone(&self.duration_secs),
                    Arc::clone(&self.progress_secs),
                    Arc::clone(&self.track_ended),
                    Arc::clone(&self.mpv_process),
                    Arc::clone(&self.volume),
                    Arc::clone(&self.innertube),
                    Arc::clone(&self.playback_seq),
                );
            }
        }
    }
}

// ── Free IPC helpers ────────────────────────────────────────────────────────

pub struct MpvStatus {
    pub playlist_pos: Option<f64>,
    pub time_pos: Option<f64>,
    pub duration: Option<f64>,
    pub eof_reached: Option<bool>,
}

/// Fast batch read of mpv properties in a single socket connection
fn ipc_get_status(stream_opt: &mut Option<BufReader<UnixStream>>) -> MpvStatus {
    let mut status = MpvStatus {
        playlist_pos: None,
        time_pos: None,
        duration: None,
        eof_reached: None,
    };

    if stream_opt.is_none() {
        if let Some(s) = connect_mpv() {
            s.set_write_timeout(Some(Duration::from_millis(100))).ok();
            // BUG-04/11: Reduce read timeout from 150ms to 50ms to reduce worst-case blocking
            s.set_read_timeout(Some(Duration::from_millis(50))).ok();
            *stream_opt = Some(BufReader::new(s));
        }
    }

    let Some(reader) = stream_opt.as_mut() else { return status; };

    static REQ_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1000);
    let base_id = REQ_ID.fetch_add(4, std::sync::atomic::Ordering::Relaxed);
    let (id_pos, id_time, id_dur, id_eof) = (base_id, base_id + 1, base_id + 2, base_id + 3);

    let batch = format!(
        "{}\n{}\n{}\n{}\n",
        serde_json::json!({"command":["get_property", "playlist-pos"], "request_id": id_pos}),
        serde_json::json!({"command":["get_property", "time-pos"], "request_id": id_time}),
        serde_json::json!({"command":["get_property", "duration"], "request_id": id_dur}),
        serde_json::json!({"command":["get_property", "eof-reached"], "request_id": id_eof}),
    );

    if reader.get_mut().write_all(batch.as_bytes()).is_ok() {
        let mut line = String::new();
        let mut received_responses = 0;
        let mut reads = 0;
        // BUG-11: Cap reads at 10 (was 50) — worst case now 500ms not 7.5s
        while received_responses < 4 && reads < 10 {
            reads += 1;
            line.clear();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                *stream_opt = None; // Connection closed or timeout
                break;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(req_id) = v.get("request_id").and_then(|id| id.as_u64().map(|u| u as usize)) {
                    if req_id == id_pos { status.playlist_pos = v.get("data").and_then(|d| d.as_f64()); received_responses += 1; }
                    else if req_id == id_time { status.time_pos = v.get("data").and_then(|d| d.as_f64()); received_responses += 1; }
                    else if req_id == id_dur { status.duration = v.get("data").and_then(|d| d.as_f64()); received_responses += 1; }
                    else if req_id == id_eof { status.eof_reached = v.get("data").and_then(|d| d.as_bool()); received_responses += 1; }
                }
            }
        }
    } else {
        *stream_opt = None;
    }

    status

}

/// Direct synchronous IPC write to socket (instant, zero OS thread overhead).
fn ipc_send(cmd: serde_json::Value) {
    if let Some(mut s) = connect_mpv() {
        s.set_write_timeout(Some(Duration::from_millis(100))).ok();
        let mut msg = cmd.to_string();
        msg.push('\n');
        let _ = s.write_all(msg.as_bytes());
    }
}

/// BUG-09: Reliable IPC send — returns true only if write succeeded.
fn ipc_send_reliable(cmd: serde_json::Value) -> bool {
    if let Some(mut s) = connect_mpv() {
        s.set_write_timeout(Some(Duration::from_millis(100))).ok();
        let mut msg = cmd.to_string();
        msg.push('\n');
        return s.write_all(msg.as_bytes()).is_ok();
    }
    false
}

/// Kill a previous meduza-music mpv instance. Uses a PID file we write when
/// spawning mpv, and only sends SIGTERM after verifying the process cmdline
/// actually matches our own socket — never a broad `pkill -f` pattern.
fn kill_stale_mpv() {
    let Ok(pid_str) = std::fs::read_to_string(mpv_pid_path()) else { return; };
    let Ok(pid) = pid_str.trim().parse::<u32>() else { return; };
    let cmdline = std::fs::read_to_string(format!("/proc/{}/cmdline", pid)).unwrap_or_default();
    if cmdline.contains("input-ipc-server") && cmdline.contains("meduza-music") {
        let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).output();
    }
}

/// Launch mpv idle process if not already running. `small_buffer` (low-end
/// profile) trades a little read-ahead/ahead-caching for a smaller resident
/// memory footprint and gentler disk/network usage.
fn ensure_mpv_running(mpv_process: &Arc<Mutex<Option<Child>>>, small_buffer: bool) {
    let mut proc = lk!(mpv_process);
    if let Some(ref mut child) = *proc {
        if child.try_wait().ok().flatten().is_none() {
            return; // already alive
        }
    }
    // Child died or never started — clean up and restart
    kill_stale_mpv();
    let socket = socket_path();
    let _ = std::fs::remove_file(&socket);
    let _ = std::fs::remove_file(mpv_pid_path());

    let mut cmd = Command::new("mpv");
    cmd.env_remove("LD_LIBRARY_PATH");
    cmd.env_remove("APPDIR");

    // Place the log in the private cache dir, restricted to the owner. The log
    // can contain signed CDN stream URLs, so it is also truncated each start
    // and mpv is told to log at warn level only.
    let log_dir = settings::app_cache_root();
    settings::ensure_private_dir(&log_dir);
    let log_file_path = log_dir.join("mpv.log");
    let _ = std::fs::remove_file(&log_file_path);
    let log_file_arg = format!("--log-file={}", log_file_path.display());

    let child = match cmd
        .args([
            "--no-video",
            "--idle=yes",
            "--keep-open=yes",
            "--terminal=no",
            "--no-input-default-bindings",
            &format!("--input-ipc-server={}", socket.display()),
            "--gapless-audio=yes",
            "--cache=yes", {
                // Low-end profile: pause on rebuffer rather than glitch hard,
                // smaller stream/readahead buffers to trim mpv RAM/IO.
                if small_buffer { "--cache-pause=yes" } else { "--cache-pause=no" }
            },
            if small_buffer { "--stream-buffer-size=64KiB" } else { "--stream-buffer-size=128KiB" },
            "--demuxer-lavf-o=probesize=32768,analyzeduration=0",
            // IMP-3: Tuned for low-latency CDN streaming
            if small_buffer { "--demuxer-readahead-secs=6" } else { "--demuxer-readahead-secs=10" },
            if small_buffer { "--audio-buffer=0.75" } else { "--audio-buffer=0.25" }, // buffering vs crackle
            "--network-timeout=4",
            "--ytdl=no",                     // We handle URL resolution; disable mpv's own yt-dlp
            "--hr-seek=no",
            "--msg-level=all=warn",
            &log_file_arg,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[Playback] ERROR: Failed to launch mpv: {}", e);
            eprintln!("[Playback] Install mpv: sudo apt install mpv");
            return;
        }
    };

    let pid = child.id();
    *proc = Some(child);
    let _ = settings::write_private(&mpv_pid_path(), &pid.to_string());

    // Wait for socket file to appear (max 3 s)
    let mut success = false;
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        if socket.exists() {
            success = true;
            break;
        }
    }
    if success {
        // SECURITY: restrict the socket to the owner (0600) so other local
        // users cannot connect and drive mpv (RCE via mpv `run` command).
        let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));
        // mpv.log contains full CDN stream URLs (signed tokens) — owner-only.
        let _ = std::fs::set_permissions(&log_file_path, std::fs::Permissions::from_mode(0o600));
        println!("[Playback] mpv IPC ready at {}", socket.display());
    } else {
        eprintln!("[Playback] WARNING: mpv socket not found at {} after 3s", socket.display());
    }
}

impl Drop for PlaybackManager {
    fn drop(&mut self) {
        ipc_send(serde_json::json!({"command":["quit"]}));
        if let Some(ref mut c) = *lk!(self.mpv_process) {
            let _ = c.wait();
        }
        let _ = std::fs::remove_file(socket_path());
        let _ = std::fs::remove_file(mpv_pid_path());
        // IMP-6: Persist resolved CDN URLs to disk so next session is instant
        StreamResolver::save_cache_to_disk();
    }
}
