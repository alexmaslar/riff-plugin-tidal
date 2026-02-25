# Riff Plugin: Tidal

Streaming plugin for [Riff](https://github.com/alexmaslar/riff) that provides lossless audio from Tidal via community proxy instances.

## Features

- Search tracks, albums, and artists on Tidal
- Stream up to Hi-Res Lossless (MQA/FLAC) quality
- Automatic round-robin failover across proxy instances
- DASH manifest fallback (Hi-Res -> Lossless when needed)

## Settings

| Key | Default | Description |
|-----|---------|-------------|
| `quality` | `LOSSLESS` | `HI_RES_LOSSLESS`, `LOSSLESS`, `HIGH`, or `LOW` |
| `instance_timeout` | `5` | Seconds before failing over to next proxy instance (3-15) |

## Building

```bash
rustup target add wasm32-unknown-unknown  # first time only
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/riff_plugin_tidal.wasm plugin.wasm
```

## Development

Add a dev plugin entry to your Riff `config.yaml`:

```yaml
dev_plugins:
  - path: /path/to/riff-plugin-tidal
```

Rebuild and reload without restarting the server:

```bash
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/riff_plugin_tidal.wasm plugin.wasm

curl -X POST -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/plugins/tidal/reload
```

## Exported Functions

| Function | Input | Output |
|----------|-------|--------|
| `riff_search` | `{ query, limit }` | `StreamingSearchResults` (tracks, albums, artists) |
| `riff_get_album` | `{ id }` | `StreamingAlbumDetail` (album + tracks) |
| `riff_get_artist_albums` | `{ id }` | `Vec<StreamingAlbum>` |
| `riff_get_stream_url` | `{ id, quality }` | `StreamUrl` (url, mime_type, quality) |
| `riff_health_check` | `""` | `"healthy"` |

## How It Works

The plugin discovers available Tidal API proxy instances from [monochrome.tf](https://monochrome.tf) and routes requests through them with round-robin failover. If an instance returns 401, 403, 429, or 5xx, the plugin automatically advances to the next instance. A hardcoded fallback is used if instance discovery fails.

Stream URLs are resolved by decoding base64-encoded manifests from the Tidal API. If a DASH/MPD manifest is returned (common for Hi-Res Lossless), the plugin automatically retries at Lossless quality.

## License

MIT
