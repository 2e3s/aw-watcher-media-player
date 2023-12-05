use std::sync::mpsc;
use std::thread;

use mpris::{PlaybackStatus, PlayerFinder};

use super::CrossMediaPlayer;
use super::MediaData;

pub struct MediaPlayer {
    sender: mpsc::Sender<()>,
    receiver: mpsc::Receiver<Option<MediaData>>,
    handler: thread::JoinHandle<()>,
}

impl CrossMediaPlayer for MediaPlayer {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let (resp_tx, resp_rx) = mpsc::channel();

        let handler = thread::spawn(move || {
            let player_finder = PlayerFinder::new().expect("MPRIS is unavailable");

            while let Ok(()) = rx.recv() {
                resp_tx
                    .send(mediadata(&player_finder))
                    .expect("Failed to send media data");
            }
        });

        Self {
            sender: tx,
            receiver: resp_rx,
            handler,
        }
    }

    fn mediadata(&self) -> Option<MediaData> {
        assert!(
            !self.handler.is_finished(),
            "The media data cannot be retrieved anymore"
        );

        self.sender.send(()).expect("Failed to request media data");
        self.receiver.recv().expect("Failed to receive media data")
    }
}

fn mediadata(player_finder: &PlayerFinder) -> Option<MediaData> {
    let player = player_finder.find_active().ok()?;

    if player.get_playback_status().ok()? != PlaybackStatus::Playing {
        trace!(
            "Player {} is not playing with status {}",
            player.bus_name(),
            player
                .get_playback_status()
                .map(|status| format!("{status:?}"))
                .unwrap_or("not found".to_string())
        );

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
        title: metadata.title().map(std::string::ToString::to_string),
        uri: metadata.url().map(std::string::ToString::to_string),
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
