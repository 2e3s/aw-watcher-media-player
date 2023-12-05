use std::{
    path::{Path, PathBuf},
    time::Duration,
    vec,
};

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use serde::Deserialize;

fn default_port() -> u16 {
    5600
}

fn default_host() -> String {
    String::from("localhost")
}

fn default_poll_time() -> u64 {
    5
}

#[derive(Parser, Debug)]
#[clap(author, version, about = "Watcher to report the currently playing media to ActivityWatch.", long_about = None)]
pub struct Cli {
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// ActivityWatch server host to send the data.
    /// Defaults to "localhost" if not specified.
    #[clap(long)]
    host: Option<String>,

    /// ActivityWatch server port to send the data.
    /// Defaults to 5600 if not specified.
    #[clap(long)]
    port: Option<u16>,

    /// Interval in seconds to request the currently playing media.
    /// Defaults to 5 if not specified.
    #[clap(long)]
    poll_interval: Option<u64>,

    /// Comma-separated case-insensitive list of players to report to ActivityWatch.
    /// If specified, the player name should contain the filter as a substring to be reported.
    /// Data from all players is reported if not specified.
    #[clap(long, value_name = "PLAYERS", use_value_delimiter = true)]
    include_players: Vec<String>,

    /// Comma-separated case-insensitive list of players to not report to ActivityWatch.
    /// If specified, the player name should not contain the filter as a substring to be reported.
    #[clap(long, value_name = "PLAYERS", use_value_delimiter = true)]
    exclude_players: Vec<String>,

    #[command(flatten)]
    pub verbosity: Verbosity,
}

#[derive(Deserialize, Default, Debug)]
struct Toml {
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_poll_time")]
    poll_time: u64,
    #[serde(default = "Vec::new")]
    include_players: Vec<String>,
    #[serde(default = "Vec::new")]
    exclude_players: Vec<String>,
}

impl Toml {
    pub fn new(file: Option<&Path>) -> Self {
        let file = if let Some(file) = file {
            file.to_path_buf()
        } else {
            let Some(config_dir) = dirs::config_local_dir() else {
                warn!("Impossible to find config directory, using default config");
                return Toml::default();
            };
            config_dir.join(env!("CARGO_PKG_NAME").to_string() + ".toml")
        };

        let content = std::fs::read_to_string(&file).unwrap_or_default();
        if let Ok(config) = toml::from_str(&content) {
            config
        } else {
            if file.exists() {
                warn!(
                    "Failed to parse config file {}, using defauls",
                    file.display()
                );
            } else {
                error!("Impossible to create an empty config");
            }
            Toml::default()
        }
    }
}

pub struct Config {
    pub host: String,
    pub port: u16,
    pub poll_interval: Duration,
    pub include_players: Vec<String>,
    pub exclude_players: Vec<String>,
}

impl Config {
    pub fn new(cli: Cli) -> Self {
        let _config_content = std::fs::read_to_string("config.toml").unwrap_or_default();
        let toml_data: Toml = Toml::new(cli.config.as_deref());

        trace!("TOML config: {:?}", toml_data);
        trace!("CLI config: {:?}", cli);

        let mut include_players = vec![];
        include_players.extend(cli.include_players.iter().map(|s| s.to_lowercase()));
        include_players.extend(toml_data.include_players.iter().map(|s| s.to_lowercase()));

        let mut exclude_players = vec![];
        exclude_players.extend(cli.exclude_players.iter().map(|s| s.to_lowercase()));
        exclude_players.extend(toml_data.exclude_players.iter().map(|s| s.to_lowercase()));

        Config {
            host: cli.host.unwrap_or(toml_data.host),
            port: cli.port.unwrap_or(toml_data.port),
            poll_interval: Duration::from_secs(cli.poll_interval.unwrap_or(toml_data.poll_time)),
            include_players,
            exclude_players,
        }
    }

    pub fn report_player(&self, player: &str) -> bool {
        if !self.include_players.is_empty() {
            for filter in &self.include_players {
                if player.to_lowercase().contains(filter) {
                    return true;
                }
            }
            return false;
        }
        if !self.exclude_players.is_empty() {
            for filter in &self.exclude_players {
                if player.to_lowercase().contains(filter) {
                    return false;
                }
            }
        }

        true
    }
}
