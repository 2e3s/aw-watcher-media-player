#![warn(clippy::pedantic)]

use std::{sync::Arc, thread, time::Duration};

use anyhow::Context;
use aw_client_rust::{blocking::AwClient, Event as AwEvent};
use chrono::Utc;
use mpris::{PlaybackStatus, PlayerFinder};
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};

const POLL_TIME: Duration = Duration::from_secs(5);
const BUCKET_NAME: &str = env!("CARGO_PKG_NAME");

struct Watcher {
    player_finder: PlayerFinder,
    client: AwClient,
    bucket_name: String,
}

impl Watcher {
    fn new() -> Self {
        let hostname = gethostname::gethostname().into_string().unwrap();
        Self {
            player_finder: PlayerFinder::new().expect("MPRIS is unavailable"),
            client: AwClient::new("localhost", "5600", "aw-watcher-media-player"),
            bucket_name: format!("{BUCKET_NAME}_{hostname}"),
        }
    }

    fn init(&self) -> anyhow::Result<()> {
        self.client
            .create_bucket_simple(&self.bucket_name, "currently-playing")
            .with_context(|| format!("Failed to create bucket {}", self.bucket_name))
    }

    fn send_active_window(&self, data: Map<String, Value>) -> anyhow::Result<()> {
        println!("Reporting {data:?}");

        let event = AwEvent {
            id: None,
            timestamp: Utc::now(),
            duration: chrono::Duration::zero(),
            data,
        };

        self.client
            .heartbeat(&self.bucket_name, &event, POLL_TIME.as_secs_f64() + 1.0)
            .with_context(|| "Failed to send heartbeat for active window")
    }

    fn get_data(&self) -> Option<Map<String, Value>> {
        let player = self.player_finder.find_active().ok()?;

        if player.get_playback_status().ok()? != PlaybackStatus::Playing {
            return None;
        }

        let metadata = if let Ok(metadata) = player.get_metadata() {
            Some(metadata)
        } else {
            println!(
                "No correct metadata found for the player {}",
                player.bus_name()
            );
            None
        }?;

        let mut data = Map::new();

        data.insert(
            "player".to_string(),
            Value::String(player.identity().to_string()),
        );
        if let Some(artists) = metadata.artists() {
            let artists = artists.join(", ");
            if !artists.is_empty() {
                data.insert("artist".to_string(), Value::String(artists));
            }
        } else if let Some(artists) = metadata.album_artists() {
            let artists = artists.join(", ");
            if !artists.is_empty() {
                data.insert("artist".to_string(), Value::String(artists));
            }
        }
        if let Some(album) = metadata.album_name() {
            if !album.is_empty() {
                data.insert("album".to_string(), Value::String(album.to_string()));
            }
        }
        if let Some(title) = metadata.title() {
            data.insert("title".to_string(), Value::String(title.to_string()));
        }
        if let Some(uri) = metadata.url() {
            if !uri.is_empty() {
                data.insert("uri".to_string(), Value::String(uri.to_string()));
            }
        }

        Some(data)
    }
}

fn main() -> anyhow::Result<()> {
    let watcher = Watcher::new();
    watcher.init()?;

    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term))?;

    let mut start_time = std::time::Instant::now();
    while !term.load(Ordering::Relaxed) {
        if start_time.elapsed() >= POLL_TIME {
            start_time = std::time::Instant::now();
            if let Some(data) = watcher.get_data() {
                watcher.send_active_window(data)?;
            }
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(())
}
