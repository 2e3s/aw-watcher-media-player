use mpris::{PlaybackStatus, PlayerFinder};

use super::CrossMediaPlayer;
use super::MediaData;

pub struct MediaPlayer {
    player_finder: PlayerFinder,
}

impl CrossMediaPlayer for MediaPlayer {
    fn new() -> Self {
        Self {
            player_finder: PlayerFinder::new().expect("MPRIS is unavailable"),
        }
    }

    fn mediadata(&self) -> Option<MediaData> {
        let player = self.player_finder.find_active().ok()?;

        if player.get_playback_status().ok()? != PlaybackStatus::Playing {
            return None;
        }

        let metadata = if let Ok(metadata) = player.get_metadata() {
            Some(metadata)
        } else {
            warn!(
                "No correct metadata found for the player {}",
                player.bus_name()
            );
            None
        }?;

        Some(MediaData {
            player: player.identity().to_string(),
            album: metadata.album_name().map(std::string::ToString::to_string),
            title: metadata.album_name().map(std::string::ToString::to_string),
            uri: metadata.album_name().map(std::string::ToString::to_string),
            artists: if let Some(artists) = metadata.artists() {
                Some(
                    artists
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect(),
                )
            } else {
                metadata.album_artists().map(|artists| {
                    artists
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect()
                })
            },
        })
    }
}
