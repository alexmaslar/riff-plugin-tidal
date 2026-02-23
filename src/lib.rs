use extism_pdk::*;
use serde::Deserialize;

mod client;
mod models;
mod types;

use client::TidalClient;
use types::*;

#[derive(Deserialize)]
struct SearchInput {
    query: String,
    limit: u32,
}

#[derive(Deserialize)]
struct IdInput {
    id: String,
}

#[derive(Deserialize)]
struct StreamUrlInput {
    id: String,
    quality: StreamingQuality,
}

fn get_or_init_client() -> Result<TidalClient, Error> {
    let quality = config::get("quality")?.unwrap_or_else(|| "LOSSLESS".to_string());
    let timeout: u64 = config::get("instance_timeout")?
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    TidalClient::new(&quality, timeout)
}

#[plugin_fn]
pub fn riff_search(Json(input): Json<SearchInput>) -> FnResult<Json<StreamingSearchResults>> {
    let client = get_or_init_client()?;
    let tracks = client.search_tracks(&input.query, input.limit).unwrap_or_default();
    let albums = client.search_albums(&input.query, input.limit).unwrap_or_default();
    let artists = client.search_artists(&input.query, input.limit).unwrap_or_default();
    Ok(Json(StreamingSearchResults {
        tracks: tracks.items.iter().map(convert_track).collect(),
        albums: albums.items.iter().map(convert_album).collect(),
        artists: artists.items.iter().map(convert_artist).collect(),
    }))
}

#[plugin_fn]
pub fn riff_get_album(Json(input): Json<IdInput>) -> FnResult<Json<StreamingAlbumDetail>> {
    let client = get_or_init_client()?;
    let id: u64 = input
        .id
        .parse()
        .map_err(|e| Error::msg(format!("invalid id: {e}")))?;
    let resp = client.get_album(id)?;

    let year = resp
        .release_date
        .as_ref()
        .and_then(|d| d.split('-').next().and_then(|y| y.parse::<i32>().ok()));

    let album = StreamingAlbum {
        provider_id: resp.id.to_string(),
        title: resp.title.clone(),
        artist: convert_artist(&resp.artist),
        year,
        cover_url: resp
            .cover
            .as_ref()
            .map(|c| models::tidal_cover_url(c, 640)),
        track_count: resp
            .number_of_tracks
            .unwrap_or(resp.items.len() as u32),
        available_qualities: all_qualities(),
        album_type: None,
    };

    let tracks = resp
        .items
        .iter()
        .map(|item| convert_track(&item.item))
        .collect();
    Ok(Json(StreamingAlbumDetail { album, tracks }))
}

#[plugin_fn]
pub fn riff_get_artist_albums(Json(input): Json<IdInput>) -> FnResult<Json<Vec<StreamingAlbum>>> {
    let client = get_or_init_client()?;
    let id: u64 = input
        .id
        .parse()
        .map_err(|e| Error::msg(format!("invalid id: {e}")))?;
    let artist = client.get_artist(id)?;
    let albums = client.search_albums(&artist.artist.name, 50)?;
    let filtered: Vec<StreamingAlbum> = albums
        .items
        .iter()
        .filter(|a| a.primary_artist().id == id || a.artists.iter().any(|ar| ar.id == id))
        .map(convert_album)
        .collect();
    Ok(Json(filtered))
}

#[plugin_fn]
pub fn riff_get_stream_url(Json(input): Json<StreamUrlInput>) -> FnResult<Json<StreamUrl>> {
    let client = get_or_init_client()?;
    let id: u64 = input
        .id
        .parse()
        .map_err(|e| Error::msg(format!("invalid id: {e}")))?;
    let quality_str = quality_to_tidal(input.quality);
    let (url, mime_type) = client.get_stream_url(id, quality_str)?;
    Ok(Json(StreamUrl {
        url,
        mime_type,
        quality: input.quality,
    }))
}

#[plugin_fn]
pub fn riff_health_check(_input: String) -> FnResult<String> {
    let client = get_or_init_client()?;
    client.search_tracks("test", 1)?;
    Ok("healthy".to_string())
}

// --- Converter functions (ported from tidal/mod.rs) ---

fn quality_to_tidal(q: StreamingQuality) -> &'static str {
    match q {
        StreamingQuality::HiRes => "HI_RES_LOSSLESS",
        StreamingQuality::Lossless => "LOSSLESS",
        StreamingQuality::High => "HIGH",
        StreamingQuality::Low => "LOW",
    }
}

fn all_qualities() -> Vec<StreamingQuality> {
    vec![
        StreamingQuality::HiRes,
        StreamingQuality::Lossless,
        StreamingQuality::High,
        StreamingQuality::Low,
    ]
}

fn convert_artist(a: &models::TidalArtist) -> StreamingArtist {
    StreamingArtist {
        provider_id: a.id.to_string(),
        name: a.name.clone(),
        image_url: a
            .picture
            .as_ref()
            .map(|p| models::tidal_cover_url(p, 480)),
    }
}

fn convert_album(a: &models::TidalAlbum) -> StreamingAlbum {
    let year = a
        .release_date
        .as_ref()
        .and_then(|d| d.split('-').next().and_then(|y| y.parse::<i32>().ok()));
    StreamingAlbum {
        provider_id: a.id.to_string(),
        title: a.title.clone(),
        artist: convert_artist(&a.primary_artist()),
        year,
        cover_url: a
            .cover
            .as_ref()
            .map(|c| models::tidal_cover_url(c, 640)),
        track_count: a.number_of_tracks.unwrap_or(0),
        available_qualities: all_qualities(),
        album_type: a.album_type.clone(),
    }
}

fn convert_track(t: &models::TidalTrack) -> StreamingTrack {
    StreamingTrack {
        provider_id: t.id.to_string(),
        title: t.title.clone(),
        artist_name: t.artist.name.clone(),
        album_title: t
            .album
            .as_ref()
            .map(|a| a.title.clone())
            .unwrap_or_default(),
        track_number: t.track_number,
        disc_number: t.volume_number.unwrap_or(1),
        duration_secs: t.duration,
        available_qualities: all_qualities(),
    }
}
