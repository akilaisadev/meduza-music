//! Lyrics retrieval and LRC parsing.
//!
//! Uses the public [LRCLIB](https://lrclib.net) API, which needs no API key.
//! Synced (`.lrc`) lyrics are preferred; plain-text lyrics are used as a
//! fallback so the panel is still useful for tracks without timestamps.

use crate::innertube::TrackItem;
use crate::settings::{self, FetchMethod, UrlKind};
use std::io::Read;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct LyricLine {
    /// Timestamp in seconds from the start of the track.
    pub time: f32,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub plain: Option<String>,
    pub source: String,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.plain.as_ref().map_or(true, |p| p.trim().is_empty())
    }

    /// Index of the lyric line that should be active at `position` seconds.
    pub fn active_index(&self, position: f32) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }
        let mut lo = 0usize;
        let mut hi = self.lines.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.lines[mid].time <= position {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Some(lo.saturating_sub(1))
    }
}

/// Parse LRC timestamp tags: `[mm:ss.xx]`, `[mm:ss:xx]`, `[mm:ss]`.
/// A line may carry several timestamps, in which case it is emitted once per tag.
pub fn parse_lrc(input: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    for raw in input.lines() {
        let line = raw.trim_end();
        let Some(close) = line.find(']') else { continue; };
        if !line.starts_with('[') { continue; }
        let text = line[close + 1..].trim();
        let stamps = &line[1..close];
        for stamp in stamps.split(']') {
            if let Some(secs) = parse_timestamp(stamp.trim()) {
                lines.push(LyricLine { time: secs, text: text.to_string() });
            }
        }
    }
    lines.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    lines
}

fn parse_timestamp(s: &str) -> Option<f32> {
    // Accept mm:ss, mm:ss.xx and mm:ss:xx (LRC uses both separators).
    let (min, rest) = s.split_once([':', '.'])?;
    let minutes: f32 = min.trim().parse().ok()?;
    let tail = rest.trim_end_matches(']').replace(':', ".");
    let seconds: f32 = tail.parse().ok()?;
    Some(minutes * 60.0 + seconds)
}

/// Fetch lyrics for a track from LRCLIB. Returns `None` on any network or
/// parsing failure; callers treat lyrics as a best-effort enhancement.
pub fn fetch_lyrics(track: &TrackItem) -> Option<Lyrics> {
    let artist = urlencoding(track.artist.trim());
    let title = urlencoding(track.title.trim());
    if artist.is_empty() || title.is_empty() {
        return None;
    }
    let url = format!(
        "https://lrclib.net/api/get?artist_name={}&track_name={}",
        artist, title
    );
    if !settings::host_is_allowed(&url, UrlKind::Api) {
        return None;
    }
    let resp = settings::fetch_allowed(&url, UrlKind::Api, FetchMethod::Get, Duration::from_secs(8))
        .ok()?;
    let mut body = String::new();
    resp.into_reader().take(512 * 1024).read_to_string(&mut body).ok()?;
    let value: serde_json::Value = serde_json::from_str(&body).ok()?;

    let synced = value.get("syncedLyrics").and_then(|v| v.as_str()).unwrap_or("");
    let plain = value.get("plainLyrics").and_then(|v| v.as_str()).map(|s| s.to_string());
    let lines = parse_lrc(synced);
    if lines.is_empty() && plain.as_ref().map_or(true, |p| p.trim().is_empty()) {
        return None;
    }

    Some(Lyrics {
        lines,
        plain,
        source: "LRCLIB".to_string(),
    })
}

/// Minimal form encoding for query parameters (URLs are ASCII-safe here).
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lrc_timestamps() {
        let lrc = "[00:12.34]First line\n[01:02.50]Second line\n[02:03]Third line";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 3);
        assert!((lines[0].time - 12.34).abs() < 0.01);
        assert_eq!(lines[0].text, "First line");
        assert!((lines[1].time - 62.5).abs() < 0.01);
        assert!((lines[2].time - 123.0).abs() < 0.01);
    }

    #[test]
    fn active_index_tracks_position() {
        let lyrics = Lyrics {
            lines: parse_lrc("[00:01.00]A\n[00:05.00]B\n[00:10.00]C"),
            plain: None,
            source: String::new(),
        };
        assert_eq!(lyrics.active_index(0.0), Some(0));
        assert_eq!(lyrics.active_index(4.9), Some(0));
        assert_eq!(lyrics.active_index(5.0), Some(1));
        assert_eq!(lyrics.active_index(99.0), Some(2));
    }
}
