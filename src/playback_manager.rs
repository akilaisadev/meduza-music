use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::innertube::{InnerTubeClient, TrackItem};
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

    mpv_process: Arc<Mutex<Option<Child>>>,
    innertube:   Arc<InnerTubeClient>,
}

/// Returns the mpv IPC socket path. Uses XDG_RUNTIME_DIR (shared between
/// the Flatpak sandbox and the host-spawned mpv process).
fn socket_path() -> String {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let path = format!("{}/meduza-music", dir);
    let _ = std::fs::create_dir_all(&path);
    format!("{}/mpv.sock", path)
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

        // ── Background IPC poller (runs every 500 ms, never blocks the UI) ──
        let state_bg    = Arc::clone(&state);
        let progress_bg = Arc::clone(&progress);
        let duration_bg = Arc::clone(&duration);
        let ended_bg    = Arc::clone(&track_ended);
        let curr_bg     = Arc::clone(&current_track);
        let queue_bg    = Arc::clone(&queue);
        let idx_bg      = Arc::clone(&queue_index);
        let cache_bg    = Arc::clone(&stream_cache);

        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(500));

                // Poison-safe state read
                let st = lk!(state_bg).clone();
                if !matches!(st, PlaybackState::Playing | PlaybackState::Paused) {
                    continue;
                }

                // Seamless 0ms Gapless Advance: check if mpv naturally moved to appended track (playlist-pos >= 1)
                if let Some(pos_idx) = ipc_get("playlist-pos") {
                    if pos_idx >= 1.0 {
                        ipc_send(serde_json::json!({"command": ["playlist-remove", 0]}));
                        
                        let q = lk!(queue_bg);
                        if !q.is_empty() {
                            let mut idx_guard = lk!(idx_bg);
                            let next_idx = if *idx_guard + 1 < q.len() { *idx_guard + 1 } else { 0 };
                            *idx_guard = next_idx;
                            let next_track = q[next_idx].track.clone();
                            drop(q);
                            drop(idx_guard);

                            *lk!(curr_bg) = Some(next_track.clone());
                            *lk!(duration_bg) = next_track.duration_seconds as f32;
                            *lk!(progress_bg) = 0.0;
                            *lk!(cache_bg) = None;
                            *lk!(ended_bg) = false;
                            println!("[Playback] Seamless 0ms gapless transition to: {}", next_track.title);
                        }
                    }
                }

                // time-pos
                if let Some(pos) = ipc_get("time-pos") {
                    *lk!(progress_bg) = pos as f32;
                }
                // duration
                if let Some(dur) = ipc_get("duration") {
                    if dur > 0.0 {
                        *lk!(duration_bg) = dur as f32;
                    }
                }
                // eof-reached: auto-advance to next track in queue when audio reaches end
                if let Some(eof) = ipc_get_bool("eof-reached") {
                    if eof {
                        let mut end_flag = lk!(ended_bg);
                        if !*end_flag {
                            *end_flag = true;
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
            volume:        Arc::new(Mutex::new(80.0)),
            track_ended,
            is_shuffle:    Arc::new(Mutex::new(false)),
            repeat_mode:   Arc::new(Mutex::new(RepeatMode::Off)),
            liked_songs:   Arc::new(Mutex::new(Vec::new())),
            history,
            settings,
            stream_cache,
            data_saver,
            recommendation_engine,
            mpv_process:   mpv_proc,
            innertube,
        }
    }

    /// Play a track — resolves URL via yt-dlp, then streams via mpv.
    pub fn play_now(&self, track: TrackItem) {
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

        thread::spawn(move || {
            ensure_mpv_running(&mpv_proc);

            // Record user taste activity
            lk!(reco_c).record_play(track_cl);

            // Check DataSaver local disk cache (0-data instant replay!)
            if let Some(cached_path) = lk!(data_saver_c).get_cached_file(&video_id) {
                println!("[DataSaver] ZERO-DATA INSTANT PLAYBACK FROM DISK: '{}' ({})", title, cached_path);
                ipc_send(serde_json::json!({"command": ["loadfile", cached_path, "replace"]}));
                ipc_send(serde_json::json!({"command": ["set_property", "volume", volume as f64]}));
                ipc_send(serde_json::json!({"command": ["set_property", "pause", false]}));
                *lk!(state_c) = PlaybackState::Playing;
                return;
            }

            println!("[Playback] Resolving stream for '{}' ({})", title, video_id);
            let Some(url) = StreamResolver::get_audio_url(&video_id, quality) else {
                *lk!(state_c) =
                    PlaybackState::Error("yt-dlp: could not resolve stream URL".to_string());
                return;
            };

            ipc_send(serde_json::json!({"command": ["loadfile", url, "replace"]}));
            ipc_send(serde_json::json!({"command": ["set_property", "volume", volume as f64]}));
            ipc_send(serde_json::json!({"command": ["set_property", "pause", false]}));
            *lk!(state_c) = PlaybackState::Playing;
            println!("[Playback] Streaming: '{}' ({})", title, video_id);

            // Cache stream to disk in background if enabled in settings
            let (enable_cache, max_cap) = {
                let s = lk!(settings_c);
                (s.enable_cache, s.max_cache_size_mb)
            };
            if enable_cache {
                lk!(data_saver_c).cache_stream_in_bg(video_id, url, max_cap);
            }
        });

        // Immediately trigger background preloader (waits until Playing state)
        self.trigger_background_preloader(track.clone());

        // Fetch radio queue in background
        let innertube_c = Arc::clone(&self.innertube);
        let queue_c     = Arc::clone(&self.queue);
        let track_c     = track.clone();

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all().build().unwrap();
            let radio = rt.block_on(innertube_c.fetch_next_radio(&track_c.media_id));
            let mut q = lk!(queue_c);
            q.clear();
            q.push(QueueItem { track: track_c });
            for t in radio { q.push(QueueItem { track: t }); }
        });
    }

    pub fn toggle_pause(&self) {
        let st = lk!(self.state).clone();
        match st {
            PlaybackState::Playing => {
                ipc_send(serde_json::json!({"command":["set_property","pause",true]}));
                *lk!(self.state) = PlaybackState::Paused;
            }
            PlaybackState::Paused => {
                ipc_send(serde_json::json!({"command":["set_property","pause",false]}));
                *lk!(self.state) = PlaybackState::Playing;
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
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize)
                .unwrap_or(1);
            nanos % queue.len()
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

        let state_c  = Arc::clone(&self.state);
        let mpv_proc = Arc::clone(&self.mpv_process);
        let volume   = *lk!(self.volume);
        let video_id = track.media_id.clone();

        let quality  = lk!(self.settings).audio_quality;
        let cache_c  = Arc::clone(&self.stream_cache);
        let title    = track.title.clone();

        thread::spawn(move || {
            ensure_mpv_running(&mpv_proc);
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
                *lk!(state_c) = PlaybackState::Error("Could not resolve stream URL".to_string());
                return;
            };

            ipc_send(serde_json::json!({"command": ["loadfile", stream_url, "replace"]}));
            ipc_send(serde_json::json!({"command": ["set_property", "volume", volume as f64]}));
            ipc_send(serde_json::json!({"command": ["set_property", "pause", false]}));
            *lk!(state_c) = PlaybackState::Playing;
            println!("[Playback] Streaming: '{}' ({})", title, video_id);
        });

        self.trigger_background_preloader(track);
    }

    /// Trigger background preloader for the next track in the queue immediately.
    fn trigger_background_preloader(&self, current_track: TrackItem) {
        let cache_c  = Arc::clone(&self.stream_cache);
        let queue_c  = Arc::clone(&self.queue);
        let idx_c    = Arc::clone(&self.queue_index);
        let shuf_c   = Arc::clone(&self.is_shuffle);
        let curr_c   = Arc::clone(&self.current_track);
        let state_c  = Arc::clone(&self.state);
        let vid_c    = current_track.media_id.clone();
        let quality  = lk!(self.settings).audio_quality;

        thread::spawn(move || {
            // Wait until primary stream starts playing smoothly to avoid network contention
            loop {
                thread::sleep(Duration::from_millis(300));
                let st = lk!(state_c).clone();
                if matches!(st, PlaybackState::Playing) { break; }
                if matches!(st, PlaybackState::Idle | PlaybackState::Error(_)) { return; }
            }

            if let Some(ref t) = *lk!(curr_c) {
                if t.media_id != vid_c { return; }
            } else {
                return;
            }

            let q = lk!(queue_c);
            if q.is_empty() { return; }
            let idx = *lk!(idx_c);
            let next_idx = if *lk!(shuf_c) {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos() as usize)
                    .unwrap_or(1);
                nanos % q.len()
            } else if idx + 1 < q.len() {
                idx + 1
            } else {
                0
            };

            let next_vid = q[next_idx].track.media_id.clone();
            let next_title = q[next_idx].track.title.clone();
            drop(q);

            let c = lk!(cache_c);
            let needs_preload = c.is_none() || c.as_ref().unwrap().0 != next_vid;
            drop(c);

            if needs_preload {
                println!("[Preloader] Pre-resolving next track: '{}' ({})", next_title, next_vid);
                if let Some(url) = StreamResolver::get_audio_url(&next_vid, quality) {
                    *lk!(cache_c) = Some((next_vid, url));
                    println!("[Preloader] Instant Engine: Pre-resolved next track URL.");
                }
            }
        });
    }

    pub fn set_volume(&self, vol: f32) {
        *lk!(self.volume) = vol;
        ipc_send(serde_json::json!({"command":["set_property","volume", vol as f64]}));
    }

    pub fn seek_to(&self, secs: f32) {
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
    pub fn handle_auto_advance(&self) {
        // Guard: don't advance if not in a playing/paused state
        let st = lk!(self.state).clone();
        if !matches!(st, PlaybackState::Playing | PlaybackState::Paused) {
            return;
        }

        let ended = *lk!(self.track_ended);
        if !ended { return; }

        // Reset flag atomically before spawning next track
        *lk!(self.track_ended) = false;

        let rep = *lk!(self.repeat_mode);
        match rep {
            RepeatMode::One => {
                if let Some(t) = lk!(self.current_track).clone() {
                    self.play_now(t);
                }
            }
            _ => {
                self.skip_next();
            }
        }
    }
}

// ── Free IPC helpers ────────────────────────────────────────────────────────

/// Fire-and-forget IPC send (no response read → always fast).
fn ipc_send(cmd: serde_json::Value) {
    std::thread::spawn(move || {
        if let Ok(mut s) = UnixStream::connect(socket_path()) {
            s.set_write_timeout(Some(Duration::from_millis(200))).ok();
            let mut msg = cmd.to_string();
            msg.push('\n');
            let _ = s.write_all(msg.as_bytes());
        }
    });
}

/// Read a numeric property from mpv IPC. Called only from the background thread.
fn ipc_get(property: &str) -> Option<f64> {
    let Ok(mut s) = UnixStream::connect(socket_path()) else { return None; };
    s.set_write_timeout(Some(Duration::from_millis(200))).ok();
    s.set_read_timeout(Some(Duration::from_millis(500))).ok();
    let cmd = serde_json::json!({"command":["get_property", property], "request_id": 42});
    let mut msg = cmd.to_string();
    msg.push('\n');
    s.write_all(msg.as_bytes()).ok()?;
    let reader = BufReader::new(s);
    for line in reader.lines().flatten() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            if v.get("request_id").and_then(|id| id.as_i64()) == Some(42) {
                return v.get("data").and_then(|d| d.as_f64());
            }
        }
    }
    None
}

/// Read a boolean property from mpv IPC.
fn ipc_get_bool(property: &str) -> Option<bool> {
    let Ok(mut s) = UnixStream::connect(socket_path()) else { return None; };
    s.set_write_timeout(Some(Duration::from_millis(200))).ok();
    s.set_read_timeout(Some(Duration::from_millis(500))).ok();
    let cmd = serde_json::json!({"command":["get_property", property], "request_id": 43});
    let mut msg = cmd.to_string();
    msg.push('\n');
    s.write_all(msg.as_bytes()).ok()?;
    let reader = BufReader::new(s);
    for line in reader.lines().flatten() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            if v.get("request_id").and_then(|id| id.as_i64()) == Some(43) {
                return v.get("data").and_then(|d| d.as_bool());
            }
        }
    }
    None
}

fn kill_stale_mpv() {
    let is_flatpak = std::path::Path::new("/.flatpak-info").exists();
    let mut cmd = if is_flatpak {
        let mut c = Command::new("flatpak-spawn");
        c.args(["--host", "pkill", "-f", "input-ipc-server=.*meduza-music/mpv.sock"]);
        c
    } else {
        let mut c = Command::new("pkill");
        c.args(["-f", "input-ipc-server=.*meduza-music/mpv.sock"]);
        c
    };
    let _ = cmd.output();
}

/// Launch mpv idle process if not already running.
fn ensure_mpv_running(mpv_process: &Arc<Mutex<Option<Child>>>) {
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

    let is_flatpak = std::path::Path::new("/.flatpak-info").exists();
    let mut cmd = if is_flatpak {
        let mut c = Command::new("flatpak-spawn");
        c.args(["--host", "mpv"]);
        c.env_remove("PULSE_SERVER");
        c.env_remove("PULSE_CLIENTCONFIG");
        c.env_remove("ALSA_CONFIG_DIR");
        c.env_remove("ALSA_CONFIG_PATH");
        c.env_remove("LD_LIBRARY_PATH");
        c
    } else {
        Command::new("mpv")
    };

    let child = match cmd
        .args([
            "--no-video",
            "--idle=yes",
            "--keep-open=yes",
            "--terminal=no",
            "--no-input-default-bindings",
            &format!("--input-ipc-server={}", socket),
            "--gapless-audio=yes",
            "--cache=yes",
            "--demuxer-max-bytes=100MiB",
            "--demuxer-readahead-secs=60",
            "--log-file=/tmp/meduza-mpv.log",
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

    *proc = Some(child);

    // Wait for socket file to appear (max 3 s)
    let mut success = false;
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        if std::path::Path::new(&socket).exists() {
            success = true;
            break;
        }
    }
    if success {
        println!("[Playback] mpv IPC ready at {}", socket);
    } else {
        eprintln!("[Playback] WARNING: mpv socket not found at {} after 3s", socket);
    }
}

impl Drop for PlaybackManager {
    fn drop(&mut self) {
        ipc_send(serde_json::json!({"command":["quit"]}));
        if let Some(ref mut c) = *lk!(self.mpv_process) {
            let _ = c.wait();
        }
        let _ = std::fs::remove_file(socket_path());
    }
}
