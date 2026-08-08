use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use reqwest::Client;

// ── Client definitions ───────────────────────────────────────────────────────

const CLIENT_NAME: &str = "WEB_REMIX";
const CLIENT_VERSION: &str = "1.20240501.01.00";
const CLIENT_ID: &str = "67";
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const ORIGIN: &str = "https://music.youtube.com";
const API_KEY: &str = "AIzaSyC9XL3ZjWddXya6X74dJoCTL-WEYFDNX30";

// ── Data models ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TrackItem {
    pub title: String,
    pub artist: String,
    pub media_id: String,
    pub thumbnail_url: String,
    pub duration_seconds: u32,
}

impl TrackItem {
    pub fn duration_str(&self) -> String {
        let m = self.duration_seconds / 60;
        let s = self.duration_seconds % 60;
        format!("{:02}:{:02}", m, s)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BrowseSection {
    pub title: String,
    pub items: Vec<TrackItem>,
}

// ── Client ───────────────────────────────────────────────────────────────────

pub struct InnerTubeClient {
    client: Client,
}

impl InnerTubeClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .user_agent(USER_AGENT)
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert("Origin",  ORIGIN.parse().unwrap());
                h.insert("Referer", format!("{}/", ORIGIN).parse().unwrap());
                h.insert("X-Origin", ORIGIN.parse().unwrap());
                h.insert("X-Goog-Api-Format-Version", "1".parse().unwrap());
                h.insert("X-YouTube-Client-Name",    CLIENT_ID.parse().unwrap());
                h.insert("X-YouTube-Client-Version", CLIENT_VERSION.parse().unwrap());
                h
            })
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    fn ctx(&self) -> serde_json::Value {
        json!({
            "client": {
                "clientName": CLIENT_NAME,
                "clientVersion": CLIENT_VERSION,
                "hl": "en",
                "gl": "US",
                "platform": "DESKTOP"
            }
        })
    }

    fn api_url(endpoint: &str) -> String {
        format!("https://music.youtube.com/youtubei/v1/{}?key={}&prettyPrint=false", endpoint, API_KEY)
    }

    // ── Parsing helpers ──────────────────────────────────────────────────────

    fn video_id(v: &serde_json::Value) -> Option<String> {
        let paths: &[&[&str]] = &[
            &["navigationEndpoint","watchEndpoint","videoId"],
            &["overlay","musicItemThumbnailOverlayRenderer","content","musicPlayButtonRenderer","playNavigationEndpoint","watchEndpoint","videoId"],
            &["thumbnailOverlay","musicItemThumbnailOverlayRenderer","content","musicPlayButtonRenderer","playNavigationEndpoint","watchEndpoint","videoId"],
        ];
        for path in paths {
            let mut cur = v;
            for key in *path {
                cur = &cur[key];
            }
            if let Some(id) = cur.as_str() {
                if id.len() == 11 { return Some(id.to_string()); }
            }
        }
        None
    }

    fn thumbnail(v: &serde_json::Value, video_id: &str) -> String {
        let paths: &[&[&str]] = &[
            &["thumbnailRenderer","musicThumbnailRenderer","thumbnail","thumbnails"],
            &["thumbnail","musicThumbnailRenderer","thumbnail","thumbnails"],
            &["thumbnail","thumbnails"],
        ];
        for path in paths {
            let mut cur = v;
            for key in *path { cur = &cur[key]; }
            if let Some(thumbs) = cur.as_array() {
                if let Some(last) = thumbs.last() {
                    if let Some(url) = last["url"].as_str() {
                        // Strip size params to get high-quality square thumbnail
                        let clean = url.split('=').next().unwrap_or(url);
                        return format!("{}=w400-h400-l90-rj", clean);
                    }
                }
            }
        }
        format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", video_id)
    }

    fn text_run(v: &serde_json::Value) -> String {
        v.as_array()
            .and_then(|runs| runs.first())
            .and_then(|r| r["text"].as_str())
            .unwrap_or("")
            .to_string()
    }

    fn parse_duration(s: &str) -> u32 {
        let parts: Vec<u32> = s.split(':').filter_map(|p| p.parse().ok()).collect();
        match parts.len() {
            2 => parts[0] * 60 + parts[1],
            3 => parts[0] * 3600 + parts[1] * 60 + parts[2],
            _ => 0,
        }
    }

    /// Parse a musicResponsiveListItemRenderer (used in search + shelf results)
    fn parse_responsive(r: &serde_json::Value) -> Option<TrackItem> {
        // Video ID — try overlay first, then menu
        let vid = Self::video_id(r)
            .or_else(|| {
                r["flexColumns"][0]["musicResponsiveListItemFlexColumnRenderer"]
                ["text"]["runs"][0]["navigationEndpoint"]["watchEndpoint"]["videoId"]
                    .as_str().map(|s| s.to_string())
            })?;
        if vid.len() != 11 { return None; }

        let title = r["flexColumns"][0]["musicResponsiveListItemFlexColumnRenderer"]
            ["text"]["runs"][0]["text"].as_str().unwrap_or("").to_string();
        if title.is_empty() { return None; }

        // Extract artist by joining text runs from the second flex column, skipping metadata labels
        let mut artist = String::new();
        if let Some(runs) = r["flexColumns"][1]["musicResponsiveListItemFlexColumnRenderer"]["text"]["runs"].as_array() {
            let mut parts = Vec::new();
            for run in runs {
                if let Some(txt) = run["text"].as_str() {
                    let t = txt.trim();
                    if !t.is_empty() && t != "•" && t != "&" && t != "Song" && t != "Video" && t != "EP" && t != "Single" && t != "Album" {
                        parts.push(t);
                    }
                }
            }
            artist = parts.join(", ");
        }
        
        // Fallback to first flex column (if it's a playlist track etc.)
        if artist.is_empty() {
            artist = r["flexColumns"][0]["musicResponsiveListItemFlexColumnRenderer"]
                ["text"]["runs"][2]["text"].as_str()
                .unwrap_or("Unknown Artist").to_string();
        }

        // Duration from fixed column
        let dur_str = r["fixedColumns"][0]["musicResponsiveListItemFixedColumnRenderer"]
            ["text"]["runs"][0]["text"].as_str().unwrap_or("");
        let duration_seconds = Self::parse_duration(dur_str);

        let tl = title.to_lowercase();
        let al = artist.to_lowercase();
        if tl.contains("podcast") || tl.contains("episode") || al.contains("podcast") || al.contains("episode") {
            return None;
        }

        let thumbnail_url = Self::thumbnail(r, &vid);
        Some(TrackItem { title, artist, media_id: vid, thumbnail_url, duration_seconds })
    }

    /// Parse a musicTwoRowItemRenderer (used in carousel/grid)
    fn parse_two_row(r: &serde_json::Value) -> Option<TrackItem> {
        let vid = Self::video_id(r)?;
        if vid.len() != 11 { return None; }
        let title = Self::text_run(&r["title"]["runs"]);
        if title.is_empty() { return None; }
        let artist = Self::text_run(&r["subtitle"]["runs"]);

        let tl = title.to_lowercase();
        let al = artist.to_lowercase();
        if tl.contains("podcast") || tl.contains("episode") || al.contains("podcast") || al.contains("episode") {
            return None;
        }

        let thumbnail_url = Self::thumbnail(r, &vid);
        Some(TrackItem { title, artist, media_id: vid, thumbnail_url, duration_seconds: 0 })
    }

    fn parse_shelf_contents(contents: &serde_json::Value) -> Vec<TrackItem> {
        let mut items = Vec::new();
        if let Some(arr) = contents.as_array() {
            for item in arr {
                let track = if let Some(r) = item.get("musicTwoRowItemRenderer") {
                    Self::parse_two_row(r)
                } else if let Some(r) = item.get("musicResponsiveListItemRenderer") {
                    Self::parse_responsive(r)
                } else {
                    None
                };
                if let Some(t) = track { items.push(t); }
            }
        }
        items
    }

    fn sections_from_value(data: &serde_json::Value) -> Vec<BrowseSection> {
        let mut sections = Vec::new();

        // Try singleColumnBrowseResultsRenderer → tabs[0] → content → sectionListRenderer
        let section_list = data["contents"]["singleColumnBrowseResultsRenderer"]
            ["tabs"][0]["tabRenderer"]["content"]["sectionListRenderer"]["contents"]
            .as_array();

        let Some(list) = section_list else { return sections; };

        for sec in list {
            // unwrap itemSectionRenderer wrapper
            let inner = sec.get("itemSectionRenderer")
                .map(|isr| &isr["contents"][0])
                .unwrap_or(sec);

            if let Some(carousel) = inner.get("musicCarouselShelfRenderer") {
                let title = carousel["header"]["musicCarouselShelfBasicHeaderRenderer"]
                    ["title"]["runs"][0]["text"].as_str()
                    .unwrap_or("Featured").to_string();
                let items = Self::parse_shelf_contents(&carousel["contents"]);
                if !items.is_empty() {
                    sections.push(BrowseSection { title, items });
                }
            } else if let Some(shelf) = inner.get("musicShelfRenderer") {
                let title = Self::text_run(&shelf["title"]["runs"]);
                let title = if title.is_empty() { "Tracks".to_string() } else { title };
                let items = Self::parse_shelf_contents(&shelf["contents"]);
                if !items.is_empty() {
                    sections.push(BrowseSection { title, items });
                }
            } else if let Some(grid) = inner.get("gridRenderer") {
                let items = Self::parse_shelf_contents(&grid["items"]);
                if !items.is_empty() {
                    sections.push(BrowseSection { title: "New Releases".to_string(), items });
                }
            }
        }
        sections
    }

    // ── Public API ───────────────────────────────────────────────────────────

    pub async fn fetch_browse(&self, browse_id: &str) -> Vec<BrowseSection> {
        let body = json!({
            "browseId": browse_id,
            "context": self.ctx()
        });
        let Ok(resp) = self.client.post(Self::api_url("browse")).json(&body).send().await else {
            return vec![];
        };
        if !resp.status().is_success() { return vec![]; }
        let Ok(data) = resp.json::<serde_json::Value>().await else { return vec![]; };
        let sections = Self::sections_from_value(&data);
        println!("[InnerTube] browse({}) → {} sections", browse_id, sections.len());
        sections
    }

    /// Fetch home + explore + charts + moods + trending in parallel (or fully
    /// sequential when `parallel` is 1 — the low-end profile builds the same
    /// feed with a single connection at a time).
    /// Enriches with rich Spotify-style playlist category shelves!
    pub async fn fetch_home_feed(&self, parallel: usize) -> Vec<BrowseSection> {
        let do_parallel = parallel > 1;
        let (explore, home, trending, charts, moods) = if do_parallel {
            tokio::join!(
                self.fetch_browse("FEmusic_explore"),
                self.fetch_browse("FEmusic_home"),
                self.fetch_browse("FEmusic_trending"),
                self.fetch_browse("FEmusic_charts"),
                self.fetch_browse("FEmusic_moods_and_genres"),
            )
        } else {
            let home = self.fetch_browse("FEmusic_home").await;
            let explore = self.fetch_browse("FEmusic_explore").await;
            let charts = self.fetch_browse("FEmusic_charts").await;
            let trending = self.fetch_browse("FEmusic_trending").await;
            let moods = self.fetch_browse("FEmusic_moods_and_genres").await;
            (explore, home, trending, charts, moods)
        };
        let mut all = Vec::new();
        all.extend(home);
        all.extend(explore);
        all.extend(charts);
        all.extend(trending);
        all.extend(moods);

        // Deduplicate sections by title
        let mut seen = std::collections::HashSet::new();
        all.retain(|s| seen.insert(s.title.clone()));
        // Filter out empty-title sections
        all.retain(|s| !s.title.is_empty() && !s.items.is_empty());

        // Spotify-style category shelves enrichment
        let (top, lofi, workout, viral, acoustic) = if do_parallel {
            tokio::join!(
                self.search_tracks("Top Global Hits"),
                self.search_tracks("Chill Lofi Study Beats"),
                self.search_tracks("Workout Dance Hype"),
                self.search_tracks("Viral Hits TikTok Trending"),
                self.search_tracks("Acoustic Indie Favorites"),
            )
        } else {
            let top = self.search_tracks("Top Global Hits").await;
            let lofi = self.search_tracks("Chill Lofi Study Beats").await;
            let workout = self.search_tracks("Workout Dance Hype").await;
            let viral = self.search_tracks("Viral Hits TikTok Trending").await;
            let acoustic = self.search_tracks("Acoustic Indie Favorites").await;
            (top, lofi, workout, viral, acoustic)
        };

        if !top.is_empty() {
            all.push(BrowseSection { title: "🔥 Top Hits & Charting".to_string(), items: top });
        }
        if !lofi.is_empty() {
            all.push(BrowseSection { title: "🎧 Chill & Lofi Beats".to_string(), items: lofi });
        }
        if !workout.is_empty() {
            all.push(BrowseSection { title: "⚡ High Energy & Workout".to_string(), items: workout });
        }
        if !viral.is_empty() {
            all.push(BrowseSection { title: "🌟 Viral & Trending Today".to_string(), items: viral });
        }
        if !acoustic.is_empty() {
            all.push(BrowseSection { title: "🎸 Acoustic & Indie Jams".to_string(), items: acoustic });
        }

        all
    }

    /// InnerTube search — instant results, no yt-dlp needed.
    pub async fn search_tracks(&self, query: &str) -> Vec<TrackItem> {
        if query.trim().is_empty() { return vec![]; }
        let body = json!({
            "query": query,
            "context": self.ctx()
        });
        let Ok(resp) = self.client.post(Self::api_url("search")).json(&body).send().await else {
            return vec![];
        };
        if !resp.status().is_success() { return vec![]; }
        let Ok(data) = resp.json::<serde_json::Value>().await else { return vec![]; };

        let mut results = Vec::new();

        // Walk tabbedSearchResultsRenderer → tabs → sectionListRenderer → contents
        let tabs = &data["contents"]["tabbedSearchResultsRenderer"]["tabs"];
        let sections = &tabs[0]["tabRenderer"]["content"]["sectionListRenderer"]["contents"];

        if let Some(arr) = sections.as_array() {
            for sec in arr {
                // Search results are typically wrapped in itemSectionRenderer -> contents
                let inner_contents = sec.get("itemSectionRenderer")
                    .and_then(|isr| isr["contents"].as_array())
                    .or_else(|| sec.get("musicShelfRenderer").and_then(|s| s["contents"].as_array()));

                if let Some(contents) = inner_contents {
                    for item in contents {
                        if let Some(r) = item.get("musicResponsiveListItemRenderer") {
                            if let Some(t) = Self::parse_responsive(r) {
                                results.push(t);
                            }
                        }
                    }
                } else if let Some(card) = sec.get("musicCardShelfRenderer") {
                    // Top result card
                    let vid = Self::video_id(card);
                    if let Some(vid) = vid {
                        let title = card["header"]["musicCardShelfHeaderBasicRenderer"]
                            ["title"]["runs"][0]["text"].as_str().unwrap_or("").to_string();
                        let artist = Self::text_run(&card["subtitle"]["runs"]);
                        let thumbnail_url = Self::thumbnail(card, &vid);
                        if !title.is_empty() {
                            results.insert(0, TrackItem { title, artist, media_id: vid, thumbnail_url, duration_seconds: 0 });
                        }
                    }
                }
            }
        }

        println!("[InnerTube] search('{}') → {} results", query, results.len());
        results
    }

    /// Fetch radio/autoplay queue for a given video (uses YouTube Music RDAMVM radio playlist).
    pub async fn fetch_next_radio(&self, video_id: &str) -> Vec<TrackItem> {
        let radio_id = format!("RDAMVM{}", video_id);
        let body = json!({
            "playlistId": radio_id,
            "isAudioOnly": true,
            "context": self.ctx()
        });
        let Ok(resp) = self.client.post(Self::api_url("next")).json(&body).send().await else {
            return vec![];
        };
        if !resp.status().is_success() { return vec![] ; }
        let Ok(data) = resp.json::<serde_json::Value>().await else { return vec![]; };

        let mut tracks = Vec::new();
        let panel = &data["contents"]["singleColumnMusicWatchNextResultsRenderer"]
            ["tabbedRenderer"]["watchNextTabbedResultsRenderer"]
            ["tabs"][0]["tabRenderer"]["content"]
            ["musicQueueRenderer"]["content"]
            ["playlistPanelRenderer"]["contents"];

        if let Some(arr) = panel.as_array() {
            for item in arr {
                if let Some(r) = item.get("playlistPanelVideoRenderer") {
                    let vid = r["videoId"].as_str().unwrap_or("");
                    if vid.len() != 11 || vid == video_id { continue; }
                    let title = Self::text_run(&r["title"]["runs"]);
                    let artist = Self::text_run(&r["longBylineText"]["runs"]);
                    let dur_str = r["lengthText"]["simpleText"].as_str().unwrap_or("");
                    let thumbnail_url = Self::thumbnail(r, vid);
                    tracks.push(TrackItem {
                        title, artist,
                        media_id: vid.to_string(),
                        thumbnail_url,
                        duration_seconds: Self::parse_duration(dur_str),
                    });
                }
            }
        }

        if tracks.is_empty() {
            println!("[InnerTube] Radio fallback search for {}", video_id);
            tracks = self.search_tracks("Top Music Hits Mix").await;
        }

        println!("[InnerTube] radio({}) → {} tracks", video_id, tracks.len());
        tracks
    }

    pub async fn fetch_search_suggestions(&self, query: &str) -> Vec<String> {
        if query.trim().is_empty() { return vec![]; }
        let body = json!({ "input": query, "context": self.ctx() });
        let Ok(resp) = self.client
            .post(Self::api_url("music/get_search_suggestions"))
            .json(&body).send().await else { return vec![]; };
        if !resp.status().is_success() { return vec![]; }
        let Ok(data) = resp.json::<serde_json::Value>().await else { return vec![]; };

        let mut suggestions = Vec::new();
        if let Some(contents) = data["contents"][0]["searchSuggestionsSectionRenderer"]["contents"].as_array() {
            for item in contents {
                if let Some(s) = item.get("searchSuggestionRenderer") {
                    let text = s["suggestion"]["runs"].as_array()
                        .map(|runs| runs.iter().filter_map(|r| r["text"].as_str()).collect::<String>())
                        .unwrap_or_default();
                    if !text.is_empty() { suggestions.push(text); }
                }
            }
        }
        suggestions
    }
}
