#![warn(clippy::pedantic)]

mod config;
mod platform;
mod watcher;

use clap::Parser;
use config::{Cli, Config};
use platform::CrossMediaPlayer;
use tokio::{signal, time};
use watcher::Watcher;

#[macro_use]
extern crate log;

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

    let run = async move {
        let mut interval = time::interval(config.poll_interval);
        let mut failed_attempts = 0;
        loop {
            if !tick(failed_attempts, &mut interval).await {
                return Err(anyhow::anyhow!("Maximum failed attempts reached"));
            }
            let data = media_player.mediadata();
            if let Some(data) = data {
                if config.report_player(&data.player) {
                    if let Err(e) = watcher.send_data(&data).await {
                        error!("Failed to send data to the server: {}", e);
                        failed_attempts += 1;
                        continue;
                    }
                } else {
                    trace!("Player \"{}\" is filtered out", data.player);
                }
            }
            failed_attempts = 0;
        }
    };

    tokio::select! {
        result = run => result,
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

async fn tick(failed_attempts: u32, interval: &mut time::Interval) -> bool {
    interval.tick().await;

    if failed_attempts == 0 {
        return true;
    }
    if failed_attempts > 100 {
        return false;
    }

    let backoff = interval.period() * failed_attempts.min(10);
    warn!(
        "Backing off for {:?} ({} failed attempts)",
        backoff, failed_attempts
    );
    time::sleep(backoff).await;

    true
}
