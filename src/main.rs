#![warn(clippy::pedantic)]

use std::{sync::Arc, thread};

use anyhow::Context;
use aw_client_rust::{blocking::AwClient, Event as AwEvent};
use chrono::{Duration, Utc};
use mpris::PlayerFinder;
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};

const POLL_TIME_SECONDS: u16 = 5;
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
            duration: Duration::zero(),
            data,
        };

        self.client
            .heartbeat(
                &self.bucket_name,
                &event,
                f64::from(POLL_TIME_SECONDS) + 1.0,
            )
            .with_context(|| "Failed to send heartbeat for active window")
    }

    fn get_data(&self) -> Option<Map<String, Value>> {
        let player = self.player_finder.find_active().ok()?;
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
            data.insert("artist".to_string(), Value::String(artists.join(", ")));
        } else if let Some(artists) = metadata.album_artists() {
            data.insert("artist".to_string(), Value::String(artists.join(", ")));
        }
        if let Some(album) = metadata.album_name() {
            data.insert("album".to_string(), Value::String(album.to_string()));
        }
        if let Some(title) = metadata.title() {
            data.insert("title".to_string(), Value::String(title.to_string()));
        }
        if let Some(url) = metadata.url() {
            data.insert("url".to_string(), Value::String(url.to_string()));
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

    while !term.load(Ordering::Relaxed) {
        if let Some(data) = watcher.get_data() {
            watcher.send_active_window(data)?;
        }

        thread::sleep(std::time::Duration::from_secs(POLL_TIME_SECONDS.into()));
    }

    Ok(())
}
