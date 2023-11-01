#![warn(clippy::pedantic)]

mod platform;

use anyhow::Context;
use aw_client_rust::{blocking::AwClient, Event as AwEvent};
use chrono::Utc;
use platform::CrossMediaPlayer;
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{sync::Arc, thread, time::Duration};

#[macro_use]
extern crate log;

const POLL_TIME: Duration = Duration::from_secs(5);
const BUCKET_NAME: &str = env!("CARGO_PKG_NAME");

struct Watcher {
    client: AwClient,
    bucket_name: String,
}

impl Watcher {
    fn new() -> Self {
        let hostname = gethostname::gethostname().into_string().unwrap();
        Self {
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
        info!("Reporting {data:?}");

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
}

fn main() -> anyhow::Result<()> {
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let media_player = platform::MediaPlayer::new();

    let watcher = Watcher::new();
    watcher.init()?;

    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term))?;

    let mut start_time = std::time::Instant::now();
    while !term.load(Ordering::Relaxed) {
        if start_time.elapsed() >= POLL_TIME {
            start_time = std::time::Instant::now();
            if let Some(data) = media_player.mediadata() {
                watcher.send_active_window(data.serialize())?;
            }
        }
        thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(())
}
