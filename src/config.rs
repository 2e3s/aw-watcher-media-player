use std::{
    fs,
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

struct DefaultConfigPaths {
    activitywatch_file: PathBuf,
    legacy_file: PathBuf,
}

impl DefaultConfigPaths {
    fn new(config_dir: &Path) -> Self {
        let config_file_name = format!("{}.toml", env!("CARGO_PKG_NAME"));
        let activitywatch_file = config_dir
            .join("activitywatch")
            .join(env!("CARGO_PKG_NAME"))
            .join(&config_file_name);
        let legacy_file = config_dir.join(config_file_name);

        Self {
            activitywatch_file,
            legacy_file,
        }
    }
}

impl Toml {
    pub fn new(file: Option<&Path>) -> Self {
        if let Some(file) = file {
            return Self::read_from_file(file);
        }

        let Some(config_dir) = dirs::config_local_dir() else {
            warn!("Impossible to find config directory, using default config");
            return Toml::default();
        };

        let paths = DefaultConfigPaths::new(&config_dir);
        if paths.activitywatch_file.exists() {
            if paths.legacy_file.exists() {
                warn!(
                    "Found deprecated legacy config path {}. It is ignored because canonical config path {} exists.",
                    paths.legacy_file.display(),
                    paths.activitywatch_file.display(),
                );
            }
            return Self::read_from_file(&paths.activitywatch_file);
        }

        if paths.legacy_file.exists() {
            warn!(
                "Using deprecated legacy config path {}. Please move it to {}. Automatic migration is not performed.",
                paths.legacy_file.display(),
                paths.activitywatch_file.display(),
            );
            return Self::read_from_file(&paths.legacy_file);
        }

        Self::read_from_file(&paths.activitywatch_file)
    }

    fn read_from_file(file: &Path) -> Self {
        if !file.exists() {
            trace!(
                "Config file {} does not exist, using defaults",
                file.display()
            );
            return Toml::default();
        }

        let content = match fs::read_to_string(file) {
            Ok(content) => content,
            Err(error) => {
                warn!(
                    "Failed to read config file {} ({}), using defaults",
                    file.display(),
                    error
                );
                return Toml::default();
            }
        };

        match toml::from_str(&content) {
            Ok(config) => config,
            Err(error) => {
                warn!(
                    "Failed to parse config file {} ({}), using defaults",
                    file.display(),
                    error
                );
                Toml::default()
            }
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

#[cfg(test)]
mod tests {
    use super::DefaultConfigPaths;
    use std::path::Path;

    #[test]
    fn computes_activitywatch_and_legacy_paths() {
        let base = Path::new("/tmp/config-base");
        let paths = DefaultConfigPaths::new(base);
        assert_eq!(
            paths.activitywatch_file,
            base.join("activitywatch")
                .join("aw-watcher-media-player")
                .join("aw-watcher-media-player.toml")
        );
        assert_eq!(paths.legacy_file, base.join("aw-watcher-media-player.toml"));
    }
}
