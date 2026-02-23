use super::{CrossMediaPlayer, MediaData};
use media_remote::NowPlayingPerl;

pub struct MediaPlayer {
    now_playing: NowPlayingPerl,
}

impl CrossMediaPlayer for MediaPlayer {
    fn new() -> Self {
        Self {
            now_playing: NowPlayingPerl::new(),
        }
    }

    fn mediadata(&self) -> Option<MediaData> {
        let guard = self.now_playing.get_info();
        let info = guard.as_ref()?;

        if !info.is_playing.unwrap_or(false) {
            return None;
        }

        Some(MediaData {
            title: info.title.clone(),
            artists: info.artist.clone().map(|artist| vec![artist]),
            album: info.album.clone(),
            player: info.bundle_name.clone().unwrap_or_default(),
            uri: None,
        })
    }
}
