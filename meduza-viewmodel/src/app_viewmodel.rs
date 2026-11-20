use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use meduza_core::innertube::{BrowseSection, InnerTubeClient, TrackItem};
use meduza_core::lyrics::Lyrics;
use meduza_playback::playback_manager::{PlaybackManager, QueueItem};
use std::collections::HashSet;

pub struct AppViewModel {
    pub innertube: Arc<InnerTubeClient>,
    pub playback: Arc<PlaybackManager>,
    pub runtime: tokio::runtime::Handle,

    // Cached UI state (updated asynchronously or periodically to avoid UI freezes)
    pub queue_snapshot: RwLock<Vec<QueueItem>>,
    pub queue_index_snapshot: RwLock<usize>,
    pub history_snapshot: RwLock<Vec<TrackItem>>,
    pub heavy_rotation_snapshot: RwLock<Vec<TrackItem>>,
    pub liked_songs_snapshot: RwLock<Vec<TrackItem>>,

    // Home feed
    pub home_feed: RwLock<Vec<BrowseSection>>,
    pub is_loading_home: AtomicBool,

    // Search
    pub search_results: RwLock<Vec<TrackItem>>,
    pub is_searching: AtomicBool,

    /// Monotonically increasing counter bumped whenever any snapshot data changes.
    /// The UI thread compares this to its local copy to know when to re-clone.
    pub data_version: AtomicU64,
    /// Guards against an older search completing after a newer one.
    search_generation: AtomicU64,

    // Lyrics for the current track (issue #7)
    pub lyrics: RwLock<Lyrics>,
    pub lyrics_track_id: RwLock<Option<String>>,
    lyrics_loading: AtomicBool,
    lyrics_generation: AtomicU64,
}

impl AppViewModel {
    pub fn new(runtime: tokio::runtime::Handle) -> Arc<Self> {
        let innertube = Arc::new(InnerTubeClient::new());
        let playback = Arc::new(PlaybackManager::new(Arc::clone(&innertube)));
        #[cfg(target_os = "linux")]
        let playback_for_mpris = Arc::clone(&playback);

        let vm = Arc::new(Self {
            innertube,
            playback,
            runtime,
            queue_snapshot: RwLock::new(Vec::new()),
            queue_index_snapshot: RwLock::new(0),
            history_snapshot: RwLock::new(Vec::new()),
            heavy_rotation_snapshot: RwLock::new(Vec::new()),
            liked_songs_snapshot: RwLock::new(Vec::new()),
            home_feed: RwLock::new(Vec::new()),
            is_loading_home: AtomicBool::new(true),
            search_results: RwLock::new(Vec::new()),
            is_searching: AtomicBool::new(false),
            data_version: AtomicU64::new(0),
            search_generation: AtomicU64::new(0),
            lyrics: RwLock::new(Lyrics::default()),
            lyrics_track_id: RwLock::new(None),
            lyrics_loading: AtomicBool::new(false),
            lyrics_generation: AtomicU64::new(0),
        });

        // Start background updater thread
        vm.start_background_updater();

        // Expose playback to Linux desktop media keys / MPRIS2 clients.
        #[cfg(target_os = "linux")]
        {
            if crate::mpris::start(playback_for_mpris).is_none() {
                eprintln!("[MPRIS] D-Bus unavailable; global media keys disabled");
            }
        }

        vm
    }

    fn start_background_updater(self: &Arc<Self>) {
        // Hold a Weak reference: an Arc here would keep AppViewModel (and
        // therefore PlaybackManager) alive forever, so its Drop never ran and
        // mpv was left orphaned on exit.
        let vm = Arc::downgrade(self);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500)); // Relaxed polling — data changes are rare
                let Some(vm) = vm.upgrade() else { break; };

                // Only bump data_version when a snapshot actually changed.
                // Bumping unconditionally forced the UI to deep-clone the
                // entire feed/search results twice per second.
                let mut changed = false;

                if let (Ok(q), Ok(mut snap)) = (vm.playback.queue.try_lock(), vm.queue_snapshot.write()) {
                    if *snap != *q {
                        *snap = q.clone();
                        changed = true;
                    }
                }
                if let (Ok(idx), Ok(mut snap)) = (vm.playback.queue_index.try_lock(), vm.queue_index_snapshot.write()) {
                    if *snap != *idx {
                        *snap = *idx;
                        changed = true;
                    }
                }

                // Update recommendations occasionally
                static UPDATE_TICK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                if UPDATE_TICK.fetch_add(1, Ordering::Relaxed) % 20 == 0 { // Every ~10 seconds
                    let mut seen = HashSet::new();
                    if let Ok(reco) = vm.playback.recommendation_engine.try_lock() {
                        let heavy = reco.get_heavy_rotation(8);
                        let unique_heavy = meduza_core::recommendation_engine::RecommendationEngine::filter_unique(&heavy, &mut seen);
                        if let Ok(mut snap) = vm.heavy_rotation_snapshot.write() {
                            if *snap != unique_heavy {
                                *snap = unique_heavy;
                                changed = true;
                            }
                        }
                    }

                    if let Ok(history) = vm.playback.history.try_lock() {
                        let unique_hist = meduza_core::recommendation_engine::RecommendationEngine::filter_unique(&history, &mut seen);
                        if let Ok(mut snap) = vm.history_snapshot.write() {
                            if *snap != unique_hist {
                                *snap = unique_hist;
                                changed = true;
                            }
                        }
                    }

                    if let Ok(liked) = vm.playback.liked_songs.try_lock() {
                        let mut liked_seen = HashSet::new();
                        let unique_liked = meduza_core::recommendation_engine::RecommendationEngine::filter_unique(&liked, &mut liked_seen);
                        if let Ok(mut snap) = vm.liked_songs_snapshot.write() {
                            if *snap != unique_liked {
                                *snap = unique_liked;
                                changed = true;
                            }
                        }
                    }
                }

                if changed {
                    vm.data_version.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
    }

    pub fn load_home_feed(self: &Arc<Self>) {
        self.is_loading_home.store(true, Ordering::SeqCst);
        let vm = Arc::clone(self);

        // Instant start like before: serve the last good feed from disk while
        // the fresh network fetch runs in the background.
        let cache_path = meduza_core::settings::app_cache_root().join("home_cache.json");
        if let Ok(data) = std::fs::read_to_string(&cache_path) {
            if let Ok(cached) = serde_json::from_str::<Vec<BrowseSection>>(&data) {
                if !cached.is_empty() {
                    if let Ok(mut snap) = vm.home_feed.write() {
                        *snap = cached;
                    }
                    vm.is_loading_home.store(false, Ordering::SeqCst);
                }
            }
        }

        self.runtime.spawn(async move {
            let mut feed = Vec::new();
            let mut top_artist = None;
            let mut top_tracks = Vec::new();
            
            if let Ok(reco) = vm.playback.recommendation_engine.try_lock() {
                top_artist = reco.get_top_artist();
                top_tracks = reco.get_heavy_rotation(10);
            }
            
            if !top_tracks.is_empty() {
                feed.push(BrowseSection {
                    title: "Jump back in".to_string(),
                    items: top_tracks.clone(),
                });
            }
            
            if let Some(top_track) = top_tracks.first() {
                let radio = vm.innertube.fetch_next_radio(&top_track.media_id).await;
                if !radio.is_empty() {
                    feed.push(BrowseSection {
                        title: format!("Because you listen to {}", top_track.artist),
                        items: radio,
                    });
                }
            }
            
            if let Some(artist) = top_artist {
                let artist_mix = vm.innertube.search_tracks(&format!("{} mix", artist)).await;
                if !artist_mix.is_empty() {
                    feed.push(BrowseSection {
                        title: format!("{} & Similar Artists", artist),
                        items: artist_mix,
                    });
                }
            }
            
            let feed_parallel = if let Ok(settings) = vm.playback.settings.try_lock() {
                settings.home_feed_parallel()
            } else {
                8
            };
            
            let std_feed = vm.innertube.fetch_home_feed(feed_parallel).await;
            feed.extend(std_feed);

            // Persist for instant startup next time
            if !feed.is_empty() {
                let _ = meduza_core::settings::write_private(
                    &cache_path,
                    &serde_json::to_string(&feed).unwrap_or_default(),
                );
            }

            if let Ok(mut snap) = vm.home_feed.write() {
                *snap = feed;
            }
            vm.is_loading_home.store(false, Ordering::SeqCst);
            vm.data_version.fetch_add(1, Ordering::SeqCst);
        });
    }

    pub fn search(self: &Arc<Self>, query: String) {
        let gen = self.search_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.is_searching.store(true, Ordering::SeqCst);
        if let Ok(mut snap) = self.search_results.write() {
            snap.clear();
        }
        let vm = Arc::clone(self);
        self.runtime.spawn(async move {
            let res = vm.innertube.search_tracks(&query).await;
            // An older query may finish after a newer one — ignore stale data.
            if vm.search_generation.load(Ordering::SeqCst) != gen {
                return;
            }
            if let Ok(mut snap) = vm.search_results.write() {
                *snap = res;
            }
            vm.is_searching.store(false, Ordering::SeqCst);
            vm.data_version.fetch_add(1, Ordering::SeqCst);
        });
    }

    /// Fetch lyrics for `track` in the background, unless they are already
    /// loaded or being fetched. Stale responses are discarded.
    pub fn ensure_lyrics(self: &Arc<Self>, track: TrackItem) {
        if let Ok(id) = self.lyrics_track_id.read() {
            if id.as_deref() == Some(track.media_id.as_str()) {
                return;
            }
        }
        if self.lyrics_loading.swap(true, Ordering::SeqCst) {
            return;
        }
        let gen = self.lyrics_generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut id) = self.lyrics_track_id.write() {
            *id = Some(track.media_id.clone());
        }
        if let Ok(mut l) = self.lyrics.write() {
            *l = Lyrics::default();
        }

        let vm = Arc::clone(self);
        meduza_core::workers::download().submit(move || {
            let result = meduza_core::lyrics::fetch_lyrics(&track);
            if vm.lyrics_generation.load(Ordering::SeqCst) != gen {
                return;
            }
            if let Ok(mut l) = vm.lyrics.write() {
                *l = result.unwrap_or_default();
            }
            vm.lyrics_loading.store(false, Ordering::SeqCst);
            vm.data_version.fetch_add(1, Ordering::SeqCst);
        });
    }
}
