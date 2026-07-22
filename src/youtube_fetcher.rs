use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use reqwest::Client;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::Rng;

pub struct PoTokenGenerator;

impl PoTokenGenerator {
    pub fn generate_cold_start_token(identifier: &str, client_state: &str) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let identifier_bytes = identifier.as_bytes();
        let state_bytes = client_state.as_bytes();

        let mut rng = rand::thread_rng();
        let mut key_bytes = [0u8; 16];
        rng.fill(&mut key_bytes);

        let encrypted_id = Self::xor_encrypt(identifier_bytes, &key_bytes);
        let timestamp_bytes = Self::encode_long(timestamp);

        let mut inner_payload = Vec::new();
        inner_payload.push(0x38); // INNER_TAG
        Self::append_var_int(&mut inner_payload, state_bytes.len());
        inner_payload.extend_from_slice(state_bytes);
        inner_payload.push(0x02); // TIMESTAMP_TAG
        Self::append_var_int(&mut inner_payload, timestamp_bytes.len());
        inner_payload.extend_from_slice(&timestamp_bytes);

        let mut token_payload = Vec::new();
        token_payload.push(0x0A); // MAGIC_HEADER
        Self::append_var_int(&mut token_payload, key_bytes.len());
        token_payload.extend_from_slice(&key_bytes);
        token_payload.push(0x22); // TOKEN_VERSION
        Self::append_var_int(&mut token_payload, encrypted_id.len());
        token_payload.extend_from_slice(&encrypted_id);
        token_payload.extend_from_slice(&inner_payload);

        URL_SAFE_NO_PAD.encode(&token_payload)
    }

    fn xor_encrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
        data.iter()
            .enumerate()
            .map(|(i, &b)| b ^ key[i % key.len()])
            .collect()
    }

    fn encode_long(value: u64) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut v = value;
        for _ in 0..8 {
            buf.push((v & 0xFF) as u8);
            v >>= 8;
        }
        let mut len = 8;
        while len > 1 && buf[len - 1] == 0 {
            len -= 1;
        }
        buf.truncate(len);
        buf
    }

    fn append_var_int(buf: &mut Vec<u8>, value: usize) {
        let mut v = value;
        while v >= 0x80 {
            buf.push(((v & 0x7F) | 0x80) as u8);
            v >>= 7;
        }
        buf.push(v as u8);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrackItem {
    pub title: String,
    pub artist: String,
    pub media_id: String,
    pub thumbnail_url: String,
    pub duration_seconds: u32,
}

pub struct YouTubeFetcher {
    client: Client,
    pub innertube: crate::innertube::InnerTubeClient,
}

impl YouTubeFetcher {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let innertube = crate::innertube::InnerTubeClient::new();
        Self { client, innertube }
    }

    pub async fn get_audio_stream_url(&self, video_id: &str) -> Option<String> {
        let visitor_data = "CgtRODZlUXRVYWpQRSj0y_jSBjIKCgJTRxIEGgAgOWLfAgrcAjIwLllUPU9vdjFWVi1MSGY4Z3d5V2ltQ1BuazdBSG91NVlXZ0dYTHA0T0RCZFN2emxCNTRsblU3NjdlZURaT1Jybkg2SWZuT1I5VkRCZW55Ylh5YXIydGRmWmpWVV96NWtBeElGNnI2WHVETGt4UUpKODZDUG50T2U0N1dsdEtiUEdhanByandGOGVDYnBydXY5bkNnUGhXYktvRWxxV2VVSm01ZzFjbTdaWGVpdW5BVk16c1U2dlNCZzIyOTJiT3NkOEg4SDE2N2RGQWhCR0Q1c3IydlViSW5ldW9BQkJCcnVkWC1rYVFKcmRRbV9QVDJRODlFNm50U3NEM1JEZ3JIYWtHRDRETjk2aEZjem85OEN0NHYtbEpnRmpkcnFYMmYyV0NvUmRMek9KN2QtdTAwVVloZnlxU0NyOVczVWN3NWpTQVlVamt6YTR0cDhCT01Oblp2cFhzYzVGQQ==";
        let po_token = PoTokenGenerator::generate_cold_start_token(visitor_data, "player");

        let body = json!({
            "videoId": video_id,
            "context": {
                "client": {
                    "clientName": "ANDROID_VR",
                    "clientVersion": "1.61.48",
                    "hl": "en",
                    "gl": "US",
                    "osName": "Android",
                    "osVersion": "12",
                    "deviceMake": "Oculus",
                    "deviceModel": "Quest 3",
                    "androidSdkVersion": "32",
                    "visitorData": visitor_data,
                },
                "request": {
                    "internalExperimentFlags": [],
                    "useSsl": true
                },
                "user": {
                    "lockedSafetyMode": false
                }
            },
            "serviceIntegrityDimensions": {
                "poToken": po_token
            }
        });

        let url = "https://www.youtube.com/youtubei/v1/player?prettyPrint=false";
        let res = self.client.post(url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "com.google.android.apps.youtube.vr.oculus/1.61.48 (Linux; U; Android 12; en_US; Quest 3; Build/SQ3A.220605.009.A1; Cronet/132.0.6808.3)")
            .header("Origin", "https://www.youtube.com")
            .header("Referer", "https://www.youtube.com/")
            .header("X-Origin", "https://www.youtube.com")
            .header("X-Goog-Api-Format-Version", "1")
            .header("X-YouTube-Client-Name", "28")
            .header("X-YouTube-Client-Version", "1.61.48")
            .json(&body)
            .send()
            .await;

        match res {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(data) = response.json::<serde_json::Value>().await {
                        if let Some(status) = data["playabilityStatus"]["status"].as_str() {
                            if status != "OK" {
                                return None;
                            }
                        }

                        if let Some(adaptive_formats) = data["streamingData"]["adaptiveFormats"].as_array() {
                            let mut best_url = None;
                            let mut max_bitrate = 0;

                            for format in adaptive_formats {
                                let mime_type = format["mimeType"].as_str().unwrap_or("");
                                if mime_type.contains("audio/") {
                                    let url = format["url"].as_str().unwrap_or("");
                                    let bitrate = format["bitrate"].as_i64().unwrap_or(0);
                                    if !url.is_empty() && bitrate > max_bitrate {
                                        best_url = Some(url.to_string());
                                        max_bitrate = bitrate;
                                    }
                                }
                            }
                            return best_url;
                        }
                    }
                }
            }
            Err(_) => {}
        }
        None
    }

    pub async fn search_tracks(&self, query: &str) -> Vec<TrackItem> {
        let body = json!({
            "query": query,
            "context": {
                "client": {
                    "clientName": "WEB",
                    "clientVersion": "2.20240501.01.00",
                    "hl": "en",
                    "gl": "US"
                }
            }
        });

        let url = "https://www.youtube.com/youtubei/v1/search";
        let res = self.client.post(url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .json(&body)
            .send()
            .await;

        let mut tracks = Vec::new();

        if let Ok(response) = res {
            if response.status().is_success() {
                if let Ok(data) = response.json::<serde_json::Value>().await {
                    if let Some(contents) = data["contents"]["twoColumnSearchResultsRenderer"]["primaryContents"]["sectionListRenderer"]["contents"].as_array() {
                        let mut items = None;
                        for section in contents {
                            if let Some(item_section) = section.get("itemSectionRenderer") {
                                if let Some(contents) = item_section.get("contents") {
                                    items = contents.as_array();
                                    break;
                                }
                            }
                        }

                        if let Some(items) = items {
                            for item in items {
                                if let Some(video) = item.get("videoRenderer") {
                                    if let Some(video_id) = video["videoId"].as_str() {
                                        let title = video["title"]["runs"][0]["text"].as_str().unwrap_or("Unknown").to_string();
                                        let artist = video["ownerText"]["runs"][0]["text"].as_str().unwrap_or("Unknown").to_string();
                                        let duration_str = video["lengthText"]["simpleText"].as_str().unwrap_or("");
                                        
                                        let mut duration_seconds = 0;
                                        if !duration_str.is_empty() {
                                            let parts: Vec<&str> = duration_str.split(':').collect();
                                            if parts.len() == 2 {
                                                let m: u32 = parts[0].parse().unwrap_or(0);
                                                let s: u32 = parts[1].parse().unwrap_or(0);
                                                duration_seconds = m * 60 + s;
                                            } else if parts.len() == 3 {
                                                let h: u32 = parts[0].parse().unwrap_or(0);
                                                let m: u32 = parts[1].parse().unwrap_or(0);
                                                let s: u32 = parts[2].parse().unwrap_or(0);
                                                duration_seconds = h * 3600 + m * 60 + s;
                                            }
                                        }

                                        let thumbnail_url = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", video_id);

                                        tracks.push(TrackItem {
                                            title,
                                            artist,
                                            media_id: video_id.to_string(),
                                            thumbnail_url,
                                            duration_seconds,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        tracks
    }

    #[allow(dead_code)]
    pub async fn get_related_tracks(&self, video_id: &str, title: &str, artist: &str) -> Vec<TrackItem> {
        let query = format!("{} {} mix", artist, title);
        let mut results = self.search_tracks(&query).await;
        results.retain(|t| t.media_id != video_id);
        results.truncate(15);
        results
    }
}
