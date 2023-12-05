use std::time::Duration;

use crate::platform::MediaData;
use anyhow::Context;
use aw_client_rust::{AwClient, Event as AwEvent};
use chrono::Utc;

use super::config::Config;

const BUCKET_NAME: &str = env!("CARGO_PKG_NAME");
const TCP_ERROR: &str = "tcp connect error: Connection refused";

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
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
        let mut attempts = 0;
        loop {
            let f = self
                .client
                .create_bucket_simple(&self.bucket_name, "currently-playing");
            match f.await {
                Ok(val) => return Ok(val),
                Err(e) if attempts < 3 && e.to_string().contains(TCP_ERROR) => {
                    warn!("Failed to connect, retrying: {}", e);

                    attempts += 1;
                    interval.tick().await;
                }
                Err(e) => {
                    return Err(e).context(format!("Failed to create bucket {}", self.bucket_name))
                }
            }
        }
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
