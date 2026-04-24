use std::{
    path::{Path, PathBuf},
    time::Duration,
    vec,
};

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use serde::{Deserialize, Serialize};

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

#[derive(Serialize, Deserialize, Debug)]
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

impl Default for Toml {
    fn default() -> Self {
        Self {
            port: default_port(),
            host: default_host(),
            poll_time: default_poll_time(),
            include_players: Vec::new(),
            exclude_players: Vec::new(),
        }
    }
}

impl Toml {
    /// Creates a new `Toml` instance by reading the configuration from the specified file or from the default location.
    pub fn new(file: Option<&Path>) -> Self {
        if let Some(file) = file {
            return Self::read_custom_config(file);
        }

        let Some(config_dir) = dirs::config_local_dir() else {
            warn!("Impossible to find config directory, using defaults");
            return Toml::default();
        };

        let app_name = env!("CARGO_PKG_NAME").to_string();
        let app_dir = config_dir.join(&app_name);
        let file = app_dir.join(format!("{app_name}.toml"));

        // Ensure the app directory exists before attempting migration
        if let Err(e) = std::fs::create_dir_all(&app_dir) {
            warn!(
                "Failed to create config directory {}: {}",
                app_dir.display(),
                e
            );
            return Toml::default();
        }

        // If the old config exists, migrate it to the new location
        let old_file = config_dir.join(format!("{app_name}.toml"));
        if !file.exists() && old_file.exists() {
            std::fs::rename(&old_file, &file).ok();
        }

        if file.exists() {
            return Self::read_default_config(&file);
        }

        // Neither config exists: create default config in the new location
        let default_config = Toml::default();
        match toml::to_string_pretty(&default_config) {
            Ok(toml_str) => match std::fs::write(&file, toml_str) {
                Ok(()) => info!("Created default config at {}", file.display()),
                Err(e) => warn!(
                    "Failed to write default config to {}: {}",
                    file.display(),
                    e
                ),
            },
            Err(e) => warn!("Failed to serialize default config: {}", e),
        }

        default_config
    }

    fn read_custom_config(file: &Path) -> Self {
        let content = std::fs::read_to_string(file);
        if let Ok(content) = content {
            if let Ok(config) = toml::from_str(&content) {
                return config;
            }
            warn!(
                "Failed to parse config file {}, using defaults",
                file.display()
            );
            return Toml::default();
        }
        warn!(
            "Failed to read config file {}, using defaults",
            file.display()
        );
        Toml::default()
    }

    fn read_default_config(file: &Path) -> Self {
        let content = std::fs::read_to_string(file).unwrap_or_default();
        if let Ok(config) = toml::from_str(&content) {
            return config;
        }
        warn!(
            "Failed to parse config file {}, using defaults",
            file.display()
        );
        Toml::default()
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
        if !include_players.is_empty() {
            log::warn!("Include filters specified, exclude filters will be ignored");
            exclude_players.clear();
        }

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
    use super::*;

    use std::sync::Mutex;

    use tempfile::tempdir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
        _env_lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &std::ffi::OsStr) -> Self {
            let env_lock = ENV_LOCK.lock().unwrap();
            let previous = std::env::var_os(key);
            // Safe because tests serialize environment mutation with ENV_LOCK.
            std::env::set_var(key, value);
            Self {
                key,
                previous,
                _env_lock: env_lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // Safe because tests serialize environment mutation with ENV_LOCK.
            match &self.previous {
                Some(val) => std::env::set_var(self.key, val),
                None => std::env::remove_var(self.key),
            }
        }
    }

    const SAMPLE_CONFIG: &str = r#"
port = 1234
host = "example.com"
poll_time = 42
include_players = ["VLC", "Spotify"]
exclude_players = ["Firefox"]
"#;

    fn sample_toml() -> Toml {
        Toml {
            port: 1234,
            host: "example.com".to_string(),
            poll_time: 42,
            include_players: vec!["VLC".to_string(), "Spotify".to_string()],
            exclude_players: vec!["Firefox".to_string()],
        }
    }

    fn default_config_file(config_root: &Path) -> PathBuf {
        let app_name = env!("CARGO_PKG_NAME");
        config_root.join(app_name).join(format!("{app_name}.toml"))
    }

    fn legacy_config_file(config_root: &Path) -> PathBuf {
        let app_name = env!("CARGO_PKG_NAME");
        config_root.join(format!("{app_name}.toml"))
    }

    fn assert_toml_eq(actual: &Toml, expected: &Toml) {
        assert_eq!(actual.port, expected.port);
        assert_eq!(actual.host, expected.host);
        assert_eq!(actual.poll_time, expected.poll_time);
        assert_eq!(actual.include_players, expected.include_players);
        assert_eq!(actual.exclude_players, expected.exclude_players);
    }

    #[test]
    fn reads_custom_config_file() {
        let temp_dir = tempdir().unwrap();
        let config_file = temp_dir.path().join("custom-config.toml");
        std::fs::write(&config_file, SAMPLE_CONFIG).unwrap();

        let config = Toml::new(Some(&config_file));

        assert_toml_eq(&config, &sample_toml());
    }

    #[test]
    fn reads_default_config_from_xdg_config_home() {
        let temp_dir = tempdir().unwrap();
        let _guard = EnvGuard::set("XDG_CONFIG_HOME", temp_dir.path().as_os_str());

        let config_file = default_config_file(temp_dir.path());
        std::fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        std::fs::write(&config_file, SAMPLE_CONFIG).unwrap();

        let config = Toml::new(None);

        assert_toml_eq(&config, &sample_toml());
    }

    #[test]
    fn creates_default_config_when_absent() {
        let temp_dir = tempdir().unwrap();
        let _guard = EnvGuard::set("XDG_CONFIG_HOME", temp_dir.path().as_os_str());

        let config_file = default_config_file(temp_dir.path());
        log::warn!("Default config file path: {}", config_file.display());

        let config = Toml::new(None);
        let expected = Toml::default();
        assert_toml_eq(&config, &expected);
        assert!(config_file.exists());

        let content = std::fs::read_to_string(&config_file).unwrap();
        let persisted: Toml = toml::from_str(&content).unwrap();
        assert_toml_eq(&persisted, &expected);
    }

    #[test]
    fn migrates_legacy_config_to_new_location() {
        let temp_dir = tempdir().unwrap();
        let _guard = EnvGuard::set("XDG_CONFIG_HOME", temp_dir.path().as_os_str());

        let legacy_file = legacy_config_file(temp_dir.path());
        std::fs::write(&legacy_file, SAMPLE_CONFIG).unwrap();
        assert!(legacy_file.exists());

        let config = Toml::new(None);

        assert_toml_eq(&config, &sample_toml());

        let new_file = default_config_file(temp_dir.path());
        assert!(!legacy_file.exists(), "Legacy file should have been moved");
        assert!(
            new_file.exists(),
            "New config file should exist after migration"
        );
        assert_eq!(SAMPLE_CONFIG, std::fs::read_to_string(&new_file).unwrap());
    }

    #[test]
    fn config_new_applies_cli_over_toml_and_defaults() {
        let temp_dir = tempdir().unwrap();
        let _guard = EnvGuard::set("XDG_CONFIG_HOME", temp_dir.path().as_os_str());

        let config_file = temp_dir.path().join("config.toml");
        std::fs::write(&config_file, SAMPLE_CONFIG).unwrap();

        let cli = Cli {
            config: Some(config_file),
            host: Some("cli-host".to_string()),
            port: Some(9999),
            poll_interval: Some(10),
            include_players: vec!["CliPlayer".to_string()],
            exclude_players: vec!["CliExclude".to_string()],
            verbosity: Verbosity::new(0, 1),
        };

        let config = Config::new(cli);

        assert_eq!(config.host, "cli-host");
        assert_eq!(config.port, 9999);
        assert_eq!(config.poll_interval, Duration::from_secs(10));
        assert_eq!(
            config.include_players,
            vec![
                "cliplayer".to_string(),
                "vlc".to_string(),
                "spotify".to_string()
            ]
        );
        assert!(
            config.exclude_players.is_empty(),
            "Expected no excluded players"
        );
    }

    #[test]
    fn report_player_filters_by_include() {
        let temp_dir = tempdir().unwrap();
        let _guard = EnvGuard::set("XDG_CONFIG_HOME", temp_dir.path().as_os_str());

        let cli = Cli {
            config: None,
            host: None,
            port: None,
            poll_interval: None,
            include_players: vec!["Spotify".to_string(), "Firefox".to_string()],
            exclude_players: vec!["firefox".to_string()],
            verbosity: Verbosity::new(0, 1),
        };

        let config = Config::new(cli);

        assert!(config.report_player("Spotify"));
        assert!(config.report_player("SPOTIFY-CONNECT"));
        assert!(config.report_player("Firefox"));
        assert!(config.report_player("firefox-tab"));
        assert!(!config.report_player("VLC"));

        let cli_all = Cli {
            config: None,
            host: None,
            port: None,
            poll_interval: None,
            include_players: vec![],
            exclude_players: vec![],
            verbosity: Verbosity::new(0, 1),
        };
        let config_all = Config::new(cli_all);
        assert!(config_all.report_player("Anything"));
    }

    #[test]
    fn report_player_filters_by_exclude() {
        let temp_dir = tempdir().unwrap();
        let _guard = EnvGuard::set("XDG_CONFIG_HOME", temp_dir.path().as_os_str());

        let cli = Cli {
            config: None,
            host: None,
            port: None,
            poll_interval: None,
            include_players: vec![],
            exclude_players: vec!["Firefox".to_string(), "VLC".to_string()],
            verbosity: Verbosity::new(0, 1),
        };

        let config = Config::new(cli);

        // Excluded players
        assert!(!config.report_player("Firefox"));
        assert!(!config.report_player("firefox-tab"));
        assert!(!config.report_player("VLC"));
        assert!(!config.report_player("vlc-media-player"));

        // Non-excluded players
        assert!(config.report_player("Spotify"));
        assert!(config.report_player("Spotify-Connect"));
    }
}
