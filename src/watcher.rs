use std::time::Duration;

use crate::platform::MediaData;
use anyhow::Context;
use aw_client_rust::{AwClient, Event as AwEvent};
use chrono::Utc;

use super::config::Config;

const BUCKET_NAME: &str = env!("CARGO_PKG_NAME");

pub struct Watcher {
    client: AwClient,
    bucket_name: String,
    poll_interval: Duration,
}

impl Watcher {
    pub fn new(config: &Config) -> Self {
        let hostname = gethostname::gethostname().into_string().unwrap();

        Self {
            client: AwClient::new(&config.host, &config.port.to_string(), BUCKET_NAME),
            bucket_name: format!("{BUCKET_NAME}_{hostname}"),
            poll_interval: config.poll_interval,
        }
    }

    pub async fn init(&self) -> anyhow::Result<()> {
        self.client
            .create_bucket_simple(&self.bucket_name, "currently-playing")
            .await
            .with_context(|| format!("Failed to create bucket {}", self.bucket_name))
    }

    pub async fn send_active_window(&self, data: &MediaData) -> anyhow::Result<()> {
        let data = data.serialize();
        info!("Reporting {data:?}");

        let event = AwEvent {
            id: None,
            timestamp: Utc::now(),
            duration: chrono::Duration::zero(),
            data,
        };

        self.client
            .heartbeat(
                &self.bucket_name,
                &event,
                self.poll_interval.as_secs_f64() + 1.0,
            )
            .await
            .with_context(|| "Failed to send heartbeat for active window")
    }
}
