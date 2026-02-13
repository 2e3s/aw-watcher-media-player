#![warn(clippy::pedantic)]

mod config;
mod platform;
mod watcher;

use anyhow::Context;
use clap::Parser;
use config::{Cli, Config};
use platform::{CrossMediaPlayer, MediaData};
use std::time::Duration;
use tokio::{signal, time};
use watcher::Watcher;

#[macro_use]
extern crate log;

const HEARTBEAT_RETRY_DELAY: Duration = Duration::from_millis(300);
const REINIT_RETRY_INTERVAL: u32 = 5;

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
        let mut consecutive_send_failures: u32 = 0;

        loop {
            interval.tick().await;
            let data = media_player.mediadata();
            if let Some(data) = data {
                if config.report_player(&data.player) {
                    match send_with_retry(&watcher, &data).await {
                        Ok(()) => {
                            if consecutive_send_failures > 0 {
                                info!(
                                    "Recovered media heartbeat delivery after {consecutive_send_failures} consecutive failure(s)"
                                );
                                consecutive_send_failures = 0;
                            }
                        }
                        Err(error) => {
                            consecutive_send_failures = consecutive_send_failures.saturating_add(1);
                            log_send_failure(consecutive_send_failures, &error);

                            if should_attempt_reinit(consecutive_send_failures) {
                                match watcher.init().await {
                                    Ok(()) => {
                                        info!(
                                            "Reinitialized ActivityWatch bucket after heartbeat failures"
                                        );
                                        consecutive_send_failures = 0;
                                    }
                                    Err(init_error) => {
                                        warn!(
                                            "Failed to reinitialize ActivityWatch bucket: {init_error:#}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                } else {
                    trace!("Player \"{}\" is filtered out", data.player);
                }
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

async fn send_with_retry(watcher: &Watcher, data: &MediaData) -> anyhow::Result<()> {
    match watcher.send_active_window(data).await {
        Ok(()) => Ok(()),
        Err(first_error) => {
            let first_error = format!("{first_error:#}");
            time::sleep(HEARTBEAT_RETRY_DELAY).await;
            watcher.send_active_window(data).await.with_context(|| {
                format!(
                    "Failed to send heartbeat after one retry (first attempt failed with: {first_error})"
                )
            })
        }
    }
}

fn should_attempt_reinit(consecutive_send_failures: u32) -> bool {
    consecutive_send_failures == 1
        || consecutive_send_failures.is_multiple_of(REINIT_RETRY_INTERVAL)
}

fn log_send_failure(consecutive_send_failures: u32, error: &anyhow::Error) {
    if should_warn_on_failure(consecutive_send_failures) {
        warn!(
            "Failed to send media heartbeat (consecutive failures: {consecutive_send_failures}): {error:#}"
        );
    } else {
        debug!(
            "Failed to send media heartbeat (consecutive failures: {consecutive_send_failures}): {error:#}"
        );
    }
}

fn should_warn_on_failure(consecutive_send_failures: u32) -> bool {
    consecutive_send_failures == 1 || consecutive_send_failures.is_power_of_two()
}

#[cfg(test)]
mod tests {
    use super::{should_attempt_reinit, should_warn_on_failure};

    #[test]
    fn reinit_is_attempted_on_first_and_periodic_failures() {
        assert!(should_attempt_reinit(1));
        assert!(!should_attempt_reinit(2));
        assert!(!should_attempt_reinit(4));
        assert!(should_attempt_reinit(5));
        assert!(should_attempt_reinit(10));
    }

    #[test]
    fn warnings_are_throttled_to_power_of_two_failures() {
        assert!(should_warn_on_failure(1));
        assert!(should_warn_on_failure(2));
        assert!(!should_warn_on_failure(3));
        assert!(should_warn_on_failure(4));
        assert!(!should_warn_on_failure(6));
        assert!(should_warn_on_failure(8));
    }
}
