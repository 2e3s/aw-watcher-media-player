#![warn(clippy::pedantic)]

mod config;
mod platform;

use anyhow::Context;
use aw_client_rust::{AwClient, Event as AwEvent};
use chrono::Utc;
use clap::Parser;
use config::{Cli, Config};
use platform::CrossMediaPlayer;
use serde_json::{Map, Value};
use std::time::Duration;
use tokio::{signal, time};

#[macro_use]
extern crate log;

const POLL_TIME: Duration = Duration::from_secs(5);
const BUCKET_NAME: &str = env!("CARGO_PKG_NAME");

struct Watcher {
    client: AwClient,
    bucket_name: String,
}

impl Watcher {
    fn new(config: &Config) -> Self {
        let hostname = gethostname::gethostname().into_string().unwrap();
        Self {
            client: AwClient::new(&config.host, &config.port.to_string(), BUCKET_NAME),
            bucket_name: format!("{BUCKET_NAME}_{hostname}"),
        }
    }

    async fn init(&self) -> anyhow::Result<()> {
        self.client
            .create_bucket_simple(&self.bucket_name, "currently-playing")
            .await
            .with_context(|| format!("Failed to create bucket {}", self.bucket_name))
    }

    async fn send_active_window(&self, data: Map<String, Value>) -> anyhow::Result<()> {
        info!("Reporting {data:?}");

        let event = AwEvent {
            id: None,
            timestamp: Utc::now(),
            duration: chrono::Duration::zero(),
            data,
        };

        self.client
            .heartbeat(&self.bucket_name, &event, POLL_TIME.as_secs_f64() + 1.0)
            .await
            .with_context(|| "Failed to send heartbeat for active window")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let verbosity = cli.verbosity.log_level().unwrap_or(log::Level::Error);
    simple_logger::init_with_level(verbosity).unwrap();

    let config = Config::new(cli);

    let media_player = platform::MediaPlayer::new();

    let watcher = Watcher::new(&config);
    watcher.init().await?;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    let mut interval = time::interval(config.poll_interval);
    let run = async move {
        loop {
            interval.tick().await;
            let data = media_player.mediadata();
            if let Some(data) = data {
                watcher.send_active_window(data.serialize()).await.unwrap();
            }
        }
    };

    tokio::select! {
        () = run => { Ok(()) },
        () = ctrl_c => {
            info!("Interruption signal received");
            Ok(())
        },
        () = terminate => {
            info!("Terminate signal received");
            Ok(())
        },
    }
}
