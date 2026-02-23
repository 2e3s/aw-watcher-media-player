use super::{CrossMediaPlayer, MediaData};

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

pub struct MediaPlayer {}

impl CrossMediaPlayer for MediaPlayer {
    fn new() -> Self {
        Self {}
    }

    fn mediadata(&self) -> Option<MediaData> {
        let session_manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .expect("Failed to request media session manager")
            .get()
            .expect("Failed to get media session manager");

        let session = session_manager.GetCurrentSession().ok()?;

        let status = session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()?;
        if status != GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
            return None;
        }

        let properties = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;

        let title = properties.Title().ok().map(|s| s.to_string());
        let artists = properties.Artist().ok().map(|s| vec![s.to_string()]);
        let album = properties.AlbumTitle().ok().map(|s| s.to_string());
        let player = session.SourceAppUserModelId().ok()?.to_string();

        Some(MediaData {
            artists,
            album,
            title,
            uri: None,
            duration_s: None,
            player,
        })
    }
}
