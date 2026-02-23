use base64::Engine;
use extism_pdk::*;

use crate::models::*;

const DEFAULT_FALLBACK: &str = "https://tidal-api.binimum.org";
const INSTANCES_URL: &str = "https://monochrome.tf/instances.json";
const CLIENT_HEADER: &str = "BiniLossless/v3.4";

const VAR_INSTANCES: &str = "tidal_instances";
const VAR_INSTANCE_IDX: &str = "tidal_instance_idx";

pub struct TidalClient {
    _quality: String,
    _timeout: u64,
}

impl TidalClient {
    pub fn new(quality: &str, timeout: u64) -> Result<Self, Error> {
        let client = Self {
            _quality: quality.to_string(),
            _timeout: timeout,
        };
        // Load instances on first creation if not already stored
        let existing = var::get::<String>(VAR_INSTANCES)?;
        if existing.is_none() {
            client.load_instances()?;
        }
        Ok(client)
    }

    /// Fetch available API instances from monochrome.tf, falling back to the default.
    fn load_instances(&self) -> Result<(), Error> {
        let req = HttpRequest::new(INSTANCES_URL)
            .with_method("GET");
        let result = http::request::<()>(&req, None);

        match result {
            Ok(resp) => {
                if resp.status_code() == 200 {
                    match serde_json::from_slice::<InstanceInfo>(&resp.body()) {
                        Ok(info) => {
                            let apis: Vec<String> = info
                                .api
                                .iter()
                                .map(|s| s.trim_end_matches('/').to_string())
                                .collect();
                            if !apis.is_empty() {
                                let json = serde_json::to_string(&apis)
                                    .map_err(|e| Error::msg(format!("serialize instances: {e}")))?;
                                var::set(VAR_INSTANCES, &json)?;
                                var::set(VAR_INSTANCE_IDX, "0")?;
                                return Ok(());
                            }
                        }
                        Err(_) => {}
                    }
                }
                // Fallback
                self.set_fallback_instances()
            }
            Err(_) => self.set_fallback_instances(),
        }
    }

    fn set_fallback_instances(&self) -> Result<(), Error> {
        let fallback = serde_json::to_string(&vec![DEFAULT_FALLBACK.to_string()])
            .map_err(|e| Error::msg(format!("serialize fallback: {e}")))?;
        var::set(VAR_INSTANCES, &fallback)?;
        var::set(VAR_INSTANCE_IDX, "0")?;
        Ok(())
    }

    fn get_instances(&self) -> Result<Vec<String>, Error> {
        let json = var::get::<String>(VAR_INSTANCES)?
            .unwrap_or_else(|| format!("[\"{DEFAULT_FALLBACK}\"]"));
        serde_json::from_str(&json)
            .map_err(|e| Error::msg(format!("deserialize instances: {e}")))
    }

    fn get_instance_idx(&self) -> Result<usize, Error> {
        let idx_str = var::get::<String>(VAR_INSTANCE_IDX)?
            .unwrap_or_else(|| "0".to_string());
        idx_str.parse().map_err(|e| Error::msg(format!("parse idx: {e}")))
    }

    fn set_instance_idx(&self, idx: usize) -> Result<(), Error> {
        var::set(VAR_INSTANCE_IDX, &idx.to_string())
    }

    /// Execute a GET request with round-robin failover across instances.
    fn request_with_failover(&self, path: &str) -> Result<Vec<u8>, Error> {
        let instances = self.get_instances()?;
        let count = instances.len();
        if count == 0 {
            return Err(Error::msg("no tidal instances available"));
        }
        let start = self.get_instance_idx()? % count;

        for i in 0..count {
            let idx = (start + i) % count;
            let url = format!("{}{}", instances[idx], path);

            let req = HttpRequest::new(&url)
                .with_method("GET")
                .with_header("x-client", CLIENT_HEADER);

            match http::request::<()>(&req, None) {
                Ok(resp) => {
                    let status = resp.status_code();
                    if status == 401 || status == 403 || status == 429 || status >= 500 {
                        self.set_instance_idx((idx + 1) % count)?;
                        continue;
                    }
                    // Park index here for next request
                    self.set_instance_idx(idx)?;
                    let body = resp.body();
                    if status < 200 || status >= 300 {
                        let text = String::from_utf8_lossy(&body);
                        return Err(Error::msg(format!(
                            "tidal API error {status} for {path}: {text}"
                        )));
                    }
                    return Ok(body);
                }
                Err(_) => {
                    self.set_instance_idx((idx + 1) % count)?;
                    continue;
                }
            }
        }

        Err(Error::msg(format!(
            "all tidal instances failed for path: {path}"
        )))
    }

    pub fn search_tracks(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<TidalPage<TidalTrack>, Error> {
        let encoded = urlencoding::encode(query);
        let path = format!("/search?s={encoded}&limit={limit}");
        let body = self.request_with_failover(&path)?;
        let page: TidalResponse<TidalPage<TidalTrack>> = serde_json::from_slice(&body)
            .map_err(|e| Error::msg(format!("parsing tidal track search: {e}")))?;
        Ok(page.data)
    }

    pub fn search_albums(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<TidalPage<TidalAlbum>, Error> {
        let encoded = urlencoding::encode(query);
        let path = format!("/search?al={encoded}&limit={limit}");
        let body = self.request_with_failover(&path)?;
        let nested: TidalResponse<TidalNestedSearch> = serde_json::from_slice(&body)
            .map_err(|e| Error::msg(format!("parsing tidal album search: {e}")))?;
        Ok(nested.data.albums)
    }

    pub fn search_artists(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<TidalPage<TidalArtist>, Error> {
        let encoded = urlencoding::encode(query);
        let path = format!("/search?a={encoded}&limit={limit}");
        let body = self.request_with_failover(&path)?;
        let nested: TidalResponse<TidalNestedSearch> = serde_json::from_slice(&body)
            .map_err(|e| Error::msg(format!("parsing tidal artist search: {e}")))?;
        Ok(nested.data.artists)
    }

    pub fn get_album(&self, id: u64) -> Result<TidalAlbumResponse, Error> {
        let path = format!("/album/?id={id}");
        let body = self.request_with_failover(&path)?;
        let wrapped: TidalResponse<TidalAlbumResponse> = serde_json::from_slice(&body)
            .map_err(|e| Error::msg(format!("parsing tidal album: {e}")))?;
        Ok(wrapped.data)
    }

    pub fn get_artist(&self, id: u64) -> Result<TidalArtistResponse, Error> {
        let path = format!("/artist/?id={id}");
        let body = self.request_with_failover(&path)?;
        let resp: TidalArtistResponse = serde_json::from_slice(&body)
            .map_err(|e| Error::msg(format!("parsing tidal artist: {e}")))?;
        Ok(resp)
    }

    /// Get a stream URL for a track. Decodes the base64 manifest.
    /// On DASH manifest (HI_RES_LOSSLESS), falls back to LOSSLESS quality.
    pub fn get_stream_url(
        &self,
        track_id: u64,
        quality: &str,
    ) -> Result<(String, String), Error> {
        let path = format!("/track/?id={track_id}&quality={quality}");
        let body = self.request_with_failover(&path)?;
        let wrapped: TidalResponse<TidalTrackDownload> = serde_json::from_slice(&body)
            .map_err(|e| Error::msg(format!("parsing tidal track download: {e}")))?;
        let download = wrapped.data;

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&download.manifest)
            .map_err(|e| Error::msg(format!("base64 decode manifest: {e}")))?;
        let manifest_str = String::from_utf8(decoded)
            .map_err(|e| Error::msg(format!("manifest utf8: {e}")))?;

        // Try to parse as JSON manifest
        if let Ok(manifest) = serde_json::from_str::<TidalManifest>(&manifest_str) {
            if let Some(url) = manifest.urls.first() {
                let mime = manifest
                    .mime_type
                    .unwrap_or_else(|| "audio/flac".to_string());
                return Ok((url.clone(), mime));
            }
        }

        // If the manifest is a DASH XML (HI_RES_LOSSLESS), fallback to LOSSLESS
        if manifest_str.contains("</MPD>") || manifest_str.contains("dash") {
            if quality != "LOSSLESS" {
                return self.get_stream_url(track_id, "LOSSLESS");
            }
            return Err(Error::msg(
                "tidal returned DASH manifest even at LOSSLESS quality",
            ));
        }

        Err(Error::msg("could not extract URL from tidal manifest"))
    }
}

/// Minimal percent-encoding for URL query parameters.
mod urlencoding {
    pub fn encode(input: &str) -> String {
        let mut result = String::with_capacity(input.len() * 3);
        for byte in input.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(byte as char);
                }
                _ => {
                    result.push('%');
                    result.push_str(&format!("{:02X}", byte));
                }
            }
        }
        result
    }
}
