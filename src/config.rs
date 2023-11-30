use std::time::Duration;

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
    #[clap(
        short,
        long,
        help = "ActivityWatch host to send the data. Defaults to \"localhost\" if not specified."
    )]
    host: Option<String>,

    #[clap(
        short,
        long,
        help = "ActivityWatch port to send the data. Defaults to 5600 if not specified."
    )]
    port: Option<u16>,

    #[clap(long)]
    poll_time: Option<u64>,

    #[clap(long)]
    include: Option<String>,

    #[clap(long)]
    exclude: Option<String>,

    #[command(flatten)]
    pub verbosity: Verbosity,
}

#[derive(Deserialize, Default)]
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
    pub fn new() -> Self {
        let Some(config_dir) = dirs::config_local_dir() else {
            warn!("Impossible to find config directory, using default config");
            return Toml::default();
        };
        let file = config_dir.join(env!("CARGO_PKG_NAME").to_string() + ".toml");

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
    pub poll_time: Duration,
    include: Option<String>,
    exclude: Option<String>,
}

impl Config {
    pub fn new(cli: Cli) -> Self {
        let _config_content = std::fs::read_to_string("config.toml").unwrap_or_default();
        let toml_data: Toml = Toml::new();

        Config {
            host: cli.host.unwrap_or(toml_data.host),
            port: cli.port.unwrap_or(toml_data.port),
            poll_time: Duration::from_secs(cli.poll_time.unwrap_or(toml_data.poll_time)),
            include: cli.include.or(toml_data.include),
            exclude: cli.exclude.or(toml_data.exclude),
        }
    }
}
