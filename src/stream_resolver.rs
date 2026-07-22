use std::process::Command;
use crate::innertube::TrackItem;

pub struct StreamResolver;

impl StreamResolver {
    fn yt_dlp_cmd() -> Command {
        let is_flatpak = std::path::Path::new("/.flatpak-info").exists();
        if is_flatpak {
            let mut cmd = Command::new("flatpak-spawn");
            cmd.args(["--host", "python3", "-m", "yt_dlp"]);
            cmd
        } else {
            let mut cmd = Command::new("python3");
            cmd.args(["-m", "yt_dlp"]);
            cmd
        }
    }

    /// Resolves a direct audio CDN URL for a given YouTube video ID using yt-dlp.
    /// Passes cookies from the system browser so YouTube doesn't block with bot-detection.
    pub fn get_audio_url(video_id: &str, quality: crate::settings::AudioQuality) -> Option<String> {
        let url = format!("https://www.youtube.com/watch?v={}", video_id);
        
        let format_arg = match quality {
            crate::settings::AudioQuality::DataSaver => "bestaudio[abr<=64]/bestaudio[abr<=96]/bestaudio[ext=webm]",
            crate::settings::AudioQuality::Normal => "bestaudio[abr<=128]/bestaudio[ext=webm]",
            crate::settings::AudioQuality::High => "bestaudio[ext=webm]/bestaudio/best",
        };

        for browser in &["firefox", "chrome"] {
            let mut cmd = Self::yt_dlp_cmd();
            let output = cmd
                .args([
                    "-f", format_arg,
                    "--no-warnings",
                    "--quiet",
                    "--cookies-from-browser", browser,
                    "--get-url",
                    &url,
                ])
                .output();

            if let Ok(out) = output {
                if out.status.success() {
                    let raw = String::from_utf8_lossy(&out.stdout);
                    let stream_url = raw.lines().next().unwrap_or("").trim().to_string();
                    if !stream_url.is_empty() {
                        println!("[StreamResolver] Resolved via {} cookies: {} ({})", browser, &stream_url[..stream_url.len().min(60)], video_id);
                        return Some(stream_url);
                    }
                }
            }
        }
        // Last resort: try without cookies
        let mut cmd = Self::yt_dlp_cmd();
        let output = cmd
            .args([
                "-f", format_arg,
                "--no-warnings",
                "--quiet",
                "--get-url",
                &url,
            ])
            .output()
            .ok()?;


        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            let stream_url = raw.lines().next().unwrap_or("").trim().to_string();
            if !stream_url.is_empty() {
                println!("[StreamResolver] Resolved (no cookies): {} ({})", &stream_url[..stream_url.len().min(60)], video_id);
                return Some(stream_url);
            }
        }
        let err = String::from_utf8_lossy(&output.stderr);
        println!("[StreamResolver] yt-dlp failed for {}: {}", video_id, err.lines().next().unwrap_or("unknown error"));
        None
    }

    /// Search YouTube Music for a query and return the first video ID found.
    #[allow(dead_code)]
    pub fn search_first_video_id(query: &str) -> Option<String> {
        let search_query = format!("ytmsearch1:{}", query);
        let mut cmd = Self::yt_dlp_cmd();
        let output = cmd
            .args([
                "--no-warnings",
                "--quiet",
                "--print", "id",
                "--no-playlist",
                &search_query,
            ])
            .output()
            .ok()?;

        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            let video_id = raw.trim().to_string();
            if !video_id.is_empty() && video_id.len() == 11 {
                println!("[StreamResolver] Search '{}' -> {}", query, video_id);
                return Some(video_id);
            }
        }
        None
    }

    /// Search YouTube Music and return full TrackItems.
    #[allow(dead_code)]
    pub fn search_tracks(query: &str, limit: usize) -> Vec<TrackItem> {
        let search_query = format!("ytmsearch{}:{}", limit, query);
        let mut cmd = Self::yt_dlp_cmd();
        let output = cmd
            .args([
                "--no-warnings",
                "--quiet",
                "--cookies-from-browser", "firefox",
                "--print", "%(id)s\t%(title)s\t%(uploader)s\t%(duration)s",
                "--no-playlist",
                &search_query,
            ])
            .output();

        let mut results = Vec::new();
        if let Ok(out) = output {
            if out.status.success() {
                let raw = String::from_utf8_lossy(&out.stdout);
                for line in raw.lines().take(limit) {
                    let parts: Vec<&str> = line.splitn(4, '\t').collect();
                    if parts.len() >= 2 {
                        let video_id = parts[0].trim().to_string();
                        let title = parts[1].trim().to_string();
                        let artist = if parts.len() > 2 { parts[2].trim().to_string() } else { "YouTube Music".to_string() };
                        let duration_seconds: u32 = if parts.len() > 3 { parts[3].trim().parse().unwrap_or(0) } else { 0 };

                        if video_id.len() == 11 && !title.is_empty() {
                            let thumbnail_url = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", video_id);
                            results.push(TrackItem {
                                title,
                                artist,
                                media_id: video_id,
                                thumbnail_url,
                                duration_seconds,
                            });
                        }
                    }
                }
            }
        }
        println!("[StreamResolver] Search '{}' -> {} results", query, results.len());
        results
    }
}
