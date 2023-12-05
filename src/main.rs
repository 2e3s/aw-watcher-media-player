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

    let mut interval = time::interval(config.poll_interval);
    let run = async move {
        loop {
            interval.tick().await;
            let data = media_player.mediadata();
            if let Some(data) = data {
                if config.report_player(&data.player) {
                    watcher.send_active_window(&data).await.unwrap();
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
