use std::{
    path::{Path, PathBuf},
    time::Duration,
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
#[clap(author, version, about, long_about = None)]
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

    /// Comma-separated list of players to report to ActivityWatch.
    /// Data from all players is reported if not specified.
    #[clap(long, value_name = "PLAYERS", use_value_delimiter = true, hide = true)]
    include_players: Option<String>,

    /// Comma-separated list of players to not report to ActivityWatch.
    #[clap(long, value_name = "PLAYERS", use_value_delimiter = true, hide = true)]
    exclude_players: Option<String>,

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
    include: Option<String>,
    exclude: Option<String>,
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
    _include_players: Option<String>,
    _exclude_players: Option<String>,
}

impl Config {
    pub fn new(cli: Cli) -> Self {
        let _config_content = std::fs::read_to_string("config.toml").unwrap_or_default();
        let toml_data: Toml = Toml::new(cli.config.as_deref());

        trace!("Config: {:?}", toml_data);

        Config {
            host: cli.host.unwrap_or(toml_data.host),
            port: cli.port.unwrap_or(toml_data.port),
            poll_interval: Duration::from_secs(cli.poll_interval.unwrap_or(toml_data.poll_time)),
            _include_players: cli.include_players.or(toml_data.include),
            _exclude_players: cli.exclude_players.or(toml_data.exclude),
        }
    }
}
