#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::MediaPlayer;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::MediaPlayer;

use serde_json::{Map, Value};

pub trait CrossMediaPlayer {
    fn new() -> Self;

    fn mediadata(&self) -> Option<MediaData>;
}

pub struct MediaData {
    artists: Option<Vec<String>>,
    album: Option<String>,
    title: Option<String>,
    uri: Option<String>,
    pub player: String,
}

impl MediaData {
    pub fn serialize(&self) -> Map<String, Value> {
        let mut data = Map::new();

        data.insert("player".to_string(), Value::String(self.player.to_string()));
        if let Some(artists) = &self.artists {
            let artists = artists.join(", ");
            if !artists.is_empty() {
                data.insert("artist".to_string(), Value::String(artists));
            }
        }
        if let Some(album) = &self.album {
            if !album.is_empty() {
                data.insert("album".to_string(), Value::String(album.to_string()));
            }
        }
        if let Some(title) = &self.title {
            data.insert("title".to_string(), Value::String(title.to_string()));
        }
        if let Some(uri) = &self.uri {
            if !uri.is_empty() {
                data.insert("uri".to_string(), Value::String(uri.to_string()));
            }
        }

        data
    }
}
