use std::{
    collections::HashSet,
    env::VarError,
    io::{BufRead, BufReader, ErrorKind},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};

use serde::{de::IgnoredAny, Deserialize, Deserializer};
use serde_json::Value;

use super::{CrossMediaPlayer, MediaData};

const MEDIA_CONTROL_ENV_VAR: &str = "AW_WATCHER_MEDIA_CONTROL_PATH";
const MEDIA_CONTROL_DEBOUNCE_ENV_VAR: &str = "AW_WATCHER_MEDIA_CONTROL_DEBOUNCE_MS";
const FALLBACK_COMMANDS: [&str; 3] = [
    "/opt/homebrew/bin/media-control",
    "/usr/local/bin/media-control",
    "media-control",
];
const UNKNOWN_PLAYER: &str = "Unknown";
const DEFAULT_STREAM_DEBOUNCE_MS: u64 = 200;
const MISSING_BINARY_RETRY_DELAY: Duration = Duration::from_secs(15);
const STREAM_RESTART_DELAY: Duration = Duration::from_secs(2);
const STREAM_STARTUP_GRACE: Duration = Duration::from_millis(250);

pub struct MediaPlayer {
    latest: Arc<RwLock<Option<MediaData>>>,
    handler: thread::JoinHandle<()>,
}

impl CrossMediaPlayer for MediaPlayer {
    fn new() -> Self {
        let latest = Arc::new(RwLock::new(None));
        let latest_for_handler = Arc::clone(&latest);

        let handler = thread::spawn(move || {
            stream_worker(&latest_for_handler);
        });

        Self { latest, handler }
    }

    fn mediadata(&self) -> Option<MediaData> {
        assert!(
            !self.handler.is_finished(),
            "The media data cannot be retrieved anymore"
        );

        self.latest
            .read()
            .expect("Failed to read latest media state")
            .clone()
    }
}

fn stream_worker(latest: &Arc<RwLock<Option<MediaData>>>) {
    let stream_profiles = stream_command_profiles();
    let mut warned_about_missing_binary = false;
    loop {
        match spawn_stream_process(&stream_profiles) {
            Ok((mut child, command_name)) => {
                warned_about_missing_binary = false;
                debug!("Started media-control stream using {command_name}");
                run_stream_until_exit(&mut child, latest);
            }
            Err(SpawnFailure::NotFound) => {
                if !warned_about_missing_binary {
                    warn!(
                        "Unable to find media-control executable. Install it with `brew install media-control` or set {}",
                        MEDIA_CONTROL_ENV_VAR
                    );
                    warned_about_missing_binary = true;
                }

                thread::sleep(MISSING_BINARY_RETRY_DELAY);
            }
            Err(SpawnFailure::Other(error)) => {
                warn!("Failed to start media-control stream: {error}");
                thread::sleep(STREAM_RESTART_DELAY);
            }
        }
    }
}

fn run_stream_until_exit(child: &mut Child, latest: &Arc<RwLock<Option<MediaData>>>) {
    let Some(stdout) = child.stdout.take() else {
        warn!("media-control stream started without stdout");
        let _ = child.kill();
        let _ = child.wait();
        thread::sleep(STREAM_RESTART_DELAY);
        return;
    };

    for line_result in BufReader::new(stdout).lines() {
        match line_result {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }

                if let Some(parsed) = parse_stream_line(&line) {
                    apply_update(latest, parsed);
                }
            }
            Err(error) => {
                warn!("Failed to read from media-control stream: {error}");
                break;
            }
        }
    }

    match child.wait() {
        Ok(status) => {
            warn!("media-control stream exited with status {status}, restarting");
        }
        Err(error) => {
            warn!("Failed waiting for media-control stream process: {error}");
        }
    }

    thread::sleep(STREAM_RESTART_DELAY);
}

fn apply_update(latest: &Arc<RwLock<Option<MediaData>>>, update: ParsedLine) {
    let mut latest = latest
        .write()
        .expect("Failed to update latest media-control state");
    match update {
        ParsedLine::Playing(data) => {
            if latest.as_ref() != Some(&data) {
                *latest = Some(data);
            }
        }
        ParsedLine::NotPlaying => {
            if latest.is_some() {
                *latest = None;
            }
        }
        ParsedLine::NoChange => {}
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ParsedLine {
    Playing(MediaData),
    NotPlaying,
    NoChange,
}

fn parse_stream_line(line: &str) -> Option<ParsedLine> {
    let parsed: StreamLine = match serde_json::from_str(line) {
        Ok(parsed) => parsed,
        Err(error) => {
            trace!("Failed to parse media-control line as JSON: {error}");
            return None;
        }
    };

    let mut raw_value = None;
    if parsed.message_type.is_none() {
        if let Some(raw) = ensure_raw_value(line, &mut raw_value) {
            if let Some(message_type) = find_string_value(raw, &["type"]) {
                if !is_data_message_type(&message_type) {
                    trace!("Skipping media-control line with unsupported type {message_type:?}");
                    return None;
                }
            }
        }
    }

    if !parsed.is_data_message() {
        trace!(
            "Skipping media-control line with unsupported type {:?}",
            parsed.message_type
        );
        return None;
    }

    let payload = parsed.effective_payload();
    let playback_state = payload.playback_state();

    if matches!(playback_state, Some(false)) {
        return Some(ParsedLine::NotPlaying);
    }

    if let Some(data) = payload.media_data() {
        if data.player == UNKNOWN_PLAYER {
            if let Some(ParsedLine::Playing(fallback_data)) =
                parse_stream_line_fallback(line, &mut raw_value)
            {
                return Some(ParsedLine::Playing(fallback_data));
            }
        }

        return Some(ParsedLine::Playing(data));
    }

    if let Some(fallback) = parse_stream_line_fallback(line, &mut raw_value) {
        return Some(fallback);
    }

    if matches!(playback_state, Some(true)) {
        return Some(ParsedLine::NoChange);
    }

    None
}

fn parse_stream_line_fallback(line: &str, raw_value: &mut Option<Value>) -> Option<ParsedLine> {
    let raw = ensure_raw_value(line, raw_value)?;
    parse_stream_line_fallback_from_value(raw)
}

fn parse_stream_line_fallback_from_value(raw: &Value) -> Option<ParsedLine> {
    if let Some(message_type) = find_string_value(raw, &["type"]) {
        if !is_data_message_type(&message_type) {
            return None;
        }
    }

    let payload = raw.get("payload").unwrap_or(raw);
    let playback_state =
        parse_playback_state_value(payload).or_else(|| parse_playback_state_value(raw));

    if matches!(playback_state, Some(false)) {
        return Some(ParsedLine::NotPlaying);
    }

    if let Some(data) = parse_media_data_value(payload).or_else(|| parse_media_data_value(raw)) {
        return Some(ParsedLine::Playing(data));
    }

    if matches!(playback_state, Some(true)) {
        return Some(ParsedLine::NoChange);
    }

    None
}

fn ensure_raw_value<'a>(line: &str, raw_value: &'a mut Option<Value>) -> Option<&'a Value> {
    if raw_value.is_none() {
        *raw_value = match serde_json::from_str::<Value>(line) {
            Ok(value) => Some(value),
            Err(error) => {
                trace!("Failed to parse media-control line for fallback parsing: {error}");
                None
            }
        };
    }
    raw_value.as_ref()
}

#[derive(Debug, Deserialize)]
struct StreamLine {
    #[serde(
        rename = "type",
        default,
        deserialize_with = "deserialize_optional_string"
    )]
    message_type: Option<String>,
    #[serde(default)]
    payload: Option<StreamPayload>,
    #[serde(flatten, default)]
    root: StreamPayload,
}

impl StreamLine {
    fn is_data_message(&self) -> bool {
        self.message_type.as_deref().is_none_or(|message_type| {
            message_type.eq_ignore_ascii_case("data")
                || message_type.eq_ignore_ascii_case("playback")
        })
    }

    fn effective_payload(&self) -> StreamPayload {
        self.payload.as_ref().map_or_else(
            || self.root.clone(),
            |payload| payload.with_fallback(&self.root),
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
struct StreamPayload {
    #[serde(default, deserialize_with = "deserialize_optional_bool")]
    playing: Option<bool>,
    #[serde(
        rename = "isPlaying",
        default,
        deserialize_with = "deserialize_optional_bool"
    )]
    is_playing: Option<bool>,
    #[serde(
        rename = "playbackRate",
        default,
        deserialize_with = "deserialize_optional_f64"
    )]
    playback_rate: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_optional_f64")]
    rate: Option<f64>,
    #[serde(
        rename = "playbackState",
        default,
        deserialize_with = "deserialize_optional_string"
    )]
    playback_state_text: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    state: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    status: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_artists")]
    artist: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_artists")]
    artists: Option<Vec<String>>,
    #[serde(
        rename = "albumArtist",
        default,
        deserialize_with = "deserialize_optional_artists"
    )]
    album_artist: Option<Vec<String>>,
    #[serde(
        rename = "albumArtists",
        default,
        deserialize_with = "deserialize_optional_artists"
    )]
    album_artists: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    album: Option<String>,
    #[serde(
        rename = "albumTitle",
        default,
        deserialize_with = "deserialize_optional_string"
    )]
    album_title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    uri: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    url: Option<String>,
    #[serde(
        rename = "bundleIdentifier",
        default,
        deserialize_with = "deserialize_optional_string"
    )]
    bundle_identifier: Option<String>,
    #[serde(
        rename = "bundle_identifier",
        default,
        deserialize_with = "deserialize_optional_string"
    )]
    bundle_identifier_snake: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    player: Option<String>,
    #[serde(
        rename = "sourceAppUserModelId",
        default,
        deserialize_with = "deserialize_optional_string"
    )]
    source_app_user_model_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    application: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    app: Option<String>,
}

impl StreamPayload {
    fn with_fallback(&self, fallback: &Self) -> Self {
        Self {
            playing: self.playing.or(fallback.playing),
            is_playing: self.is_playing.or(fallback.is_playing),
            playback_rate: self.playback_rate.or(fallback.playback_rate),
            rate: self.rate.or(fallback.rate),
            playback_state_text: self
                .playback_state_text
                .clone()
                .or_else(|| fallback.playback_state_text.clone()),
            state: self.state.clone().or_else(|| fallback.state.clone()),
            status: self.status.clone().or_else(|| fallback.status.clone()),
            title: self.title.clone().or_else(|| fallback.title.clone()),
            name: self.name.clone().or_else(|| fallback.name.clone()),
            artist: self.artist.clone().or_else(|| fallback.artist.clone()),
            artists: self.artists.clone().or_else(|| fallback.artists.clone()),
            album_artist: self
                .album_artist
                .clone()
                .or_else(|| fallback.album_artist.clone()),
            album_artists: self
                .album_artists
                .clone()
                .or_else(|| fallback.album_artists.clone()),
            album: self.album.clone().or_else(|| fallback.album.clone()),
            album_title: self
                .album_title
                .clone()
                .or_else(|| fallback.album_title.clone()),
            uri: self.uri.clone().or_else(|| fallback.uri.clone()),
            url: self.url.clone().or_else(|| fallback.url.clone()),
            bundle_identifier: self
                .bundle_identifier
                .clone()
                .or_else(|| fallback.bundle_identifier.clone()),
            bundle_identifier_snake: self
                .bundle_identifier_snake
                .clone()
                .or_else(|| fallback.bundle_identifier_snake.clone()),
            player: self.player.clone().or_else(|| fallback.player.clone()),
            source_app_user_model_id: self
                .source_app_user_model_id
                .clone()
                .or_else(|| fallback.source_app_user_model_id.clone()),
            application: self
                .application
                .clone()
                .or_else(|| fallback.application.clone()),
            app: self.app.clone().or_else(|| fallback.app.clone()),
        }
    }

    fn playback_state(&self) -> Option<bool> {
        if let Some(playing) = self.playing.or(self.is_playing) {
            return Some(playing);
        }

        if let Some(playback_rate) = self.playback_rate.or(self.rate) {
            return Some(playback_rate > 0.0);
        }

        if let Some(state) = first_some_str(&[&self.playback_state_text, &self.state, &self.status])
        {
            let state = state.to_ascii_lowercase();
            if state.contains("pause")
                || state.contains("stop")
                || state.contains("interrupt")
                || state == "none"
            {
                return Some(false);
            }

            if state.contains("play") {
                return Some(true);
            }
        }

        None
    }

    fn media_data(&self) -> Option<MediaData> {
        let title = first_some_string(&[&self.title, &self.name]);
        let artists = first_some_artists(&[
            &self.artist,
            &self.artists,
            &self.album_artist,
            &self.album_artists,
        ]);
        let album = first_some_string(&[&self.album, &self.album_title]);
        let uri = first_some_string(&[&self.uri, &self.url]);
        let player = first_some_string(&[
            &self.bundle_identifier,
            &self.bundle_identifier_snake,
            &self.player,
            &self.source_app_user_model_id,
            &self.application,
            &self.app,
        ])
        .unwrap_or_else(|| UNKNOWN_PLAYER.to_string());

        if title.is_none() && artists.is_none() && album.is_none() && uri.is_none() {
            return None;
        }

        Some(MediaData {
            artists,
            album,
            title,
            uri,
            player,
        })
    }
}

fn first_some_str<'a>(values: &[&'a Option<String>]) -> Option<&'a str> {
    values.iter().find_map(|value| value.as_deref())
}

fn first_some_string(values: &[&Option<String>]) -> Option<String> {
    values.iter().find_map(|value| (*value).clone())
}

fn first_some_artists(values: &[&Option<Vec<String>>]) -> Option<Vec<String>> {
    values.iter().find_map(|value| (*value).clone())
}

fn is_data_message_type(message_type: &str) -> bool {
    message_type.eq_ignore_ascii_case("data") || message_type.eq_ignore_ascii_case("playback")
}

fn parse_media_data_value(value: &Value) -> Option<MediaData> {
    let title = find_string_value(value, &["title", "name"]);
    let artists = find_artists_value(value, &["artist", "artists", "albumArtist", "albumArtists"]);
    let album = find_string_value(value, &["album", "albumTitle"]);
    let uri = find_string_value(value, &["uri", "url"]);
    let player = find_string_value(
        value,
        &[
            "bundleIdentifier",
            "bundle_identifier",
            "player",
            "sourceAppUserModelId",
            "application",
            "app",
        ],
    )
    .unwrap_or_else(|| UNKNOWN_PLAYER.to_string());

    if title.is_none() && artists.is_none() && album.is_none() && uri.is_none() {
        return None;
    }

    Some(MediaData {
        artists,
        album,
        title,
        uri,
        player,
    })
}

fn parse_playback_state_value(value: &Value) -> Option<bool> {
    if let Some(playing) = find_bool_value(value, &["playing", "isPlaying"]) {
        return Some(playing);
    }

    if let Some(playback_rate) = find_f64_value(value, &["playbackRate", "rate"]) {
        return Some(playback_rate > 0.0);
    }

    if let Some(state) = find_string_value(value, &["playbackState", "state", "status"]) {
        let state = state.to_ascii_lowercase();
        if state.contains("pause")
            || state.contains("stop")
            || state.contains("interrupt")
            || state == "none"
        {
            return Some(false);
        }

        if state.contains("play") {
            return Some(true);
        }
    }

    None
}

fn find_string_value(value: &Value, keys: &[&str]) -> Option<String> {
    find_key_value(value, keys).and_then(|found| match found {
        Value::String(value) => {
            let value = value.trim();
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        }
        _ => None,
    })
}

fn find_artists_value(value: &Value, keys: &[&str]) -> Option<Vec<String>> {
    find_key_value(value, keys).and_then(|found| match found {
        Value::String(artist) => {
            let artist = artist.trim();
            if artist.is_empty() {
                None
            } else {
                Some(vec![artist.to_string()])
            }
        }
        Value::Array(artists) => {
            let parsed: Vec<String> = artists
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect();
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        }
        _ => None,
    })
}

fn find_bool_value(value: &Value, keys: &[&str]) -> Option<bool> {
    find_key_value(value, keys).and_then(Value::as_bool)
}

fn find_f64_value(value: &Value, keys: &[&str]) -> Option<f64> {
    find_key_value(value, keys).and_then(Value::as_f64)
}

fn find_key_value<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                if keys
                    .iter()
                    .any(|expected| key.eq_ignore_ascii_case(expected))
                {
                    return Some(item);
                }
            }

            for item in map.values() {
                if let Some(found) = find_key_value(item, keys) {
                    return Some(found);
                }
            }

            None
        }
        Value::Array(items) => items.iter().find_map(|item| find_key_value(item, keys)),
        _ => None,
    }
}

fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<RawString>::deserialize(deserializer)?;
    Ok(value.and_then(RawString::into_string))
}

fn deserialize_optional_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<RawBool>::deserialize(deserializer)?;
    Ok(value.and_then(RawBool::into_bool))
}

fn deserialize_optional_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<RawF64>::deserialize(deserializer)?;
    Ok(value.and_then(RawF64::into_f64))
}

fn deserialize_optional_artists<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<RawArtists>::deserialize(deserializer)?;
    Ok(value.and_then(RawArtists::into_artists))
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawString {
    String(String),
    Other(IgnoredAny),
}

impl RawString {
    fn into_string(self) -> Option<String> {
        match self {
            Self::String(value) => {
                let value = value.trim();
                if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                }
            }
            Self::Other(_) => None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawBool {
    Bool(bool),
    String(String),
    Number(f64),
    Other(IgnoredAny),
}

impl RawBool {
    fn into_bool(self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(value),
            Self::String(value) => {
                let value = value.trim();
                if value.eq_ignore_ascii_case("true")
                    || value.eq_ignore_ascii_case("yes")
                    || value.eq_ignore_ascii_case("on")
                    || value == "1"
                {
                    Some(true)
                } else if value.eq_ignore_ascii_case("false")
                    || value.eq_ignore_ascii_case("no")
                    || value.eq_ignore_ascii_case("off")
                    || value == "0"
                {
                    Some(false)
                } else {
                    None
                }
            }
            Self::Number(value) => Some(value > 0.0),
            Self::Other(_) => None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawF64 {
    Number(f64),
    String(String),
    Other(IgnoredAny),
}

impl RawF64 {
    fn into_f64(self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(value),
            Self::String(value) => value.trim().parse::<f64>().ok(),
            Self::Other(_) => None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawArtists {
    String(String),
    Strings(Vec<String>),
    Other(IgnoredAny),
}

impl RawArtists {
    fn into_artists(self) -> Option<Vec<String>> {
        let artists = match self {
            Self::String(artist) => vec![artist],
            Self::Strings(artists) => artists,
            Self::Other(_) => return None,
        };

        let parsed: Vec<String> = artists
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    }
}

#[derive(Clone, Debug)]
struct StreamCommandProfile {
    args: Vec<String>,
}

impl StreamCommandProfile {
    fn new(args: Vec<String>) -> Self {
        Self { args }
    }

    fn display(&self) -> String {
        self.args.join(" ")
    }
}

fn stream_command_profiles() -> Vec<StreamCommandProfile> {
    let debounce_ms = stream_debounce_ms();
    vec![
        StreamCommandProfile::new(vec![
            "stream".to_string(),
            "--no-diff".to_string(),
            format!("--debounce={debounce_ms}"),
        ]),
        StreamCommandProfile::new(vec!["stream".to_string(), "--no-diff".to_string()]),
        StreamCommandProfile::new(vec!["stream".to_string()]),
    ]
}

fn stream_debounce_ms() -> u64 {
    match std::env::var(MEDIA_CONTROL_DEBOUNCE_ENV_VAR) {
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(parsed) => parsed,
            Err(error) => {
                warn!(
                    "Invalid {} value {:?}: {}. Using default {}ms",
                    MEDIA_CONTROL_DEBOUNCE_ENV_VAR, raw, error, DEFAULT_STREAM_DEBOUNCE_MS
                );
                DEFAULT_STREAM_DEBOUNCE_MS
            }
        },
        Err(VarError::NotPresent) => DEFAULT_STREAM_DEBOUNCE_MS,
        Err(VarError::NotUnicode(_)) => {
            warn!(
                "Invalid {} value (not valid UTF-8). Using default {}ms",
                MEDIA_CONTROL_DEBOUNCE_ENV_VAR, DEFAULT_STREAM_DEBOUNCE_MS
            );
            DEFAULT_STREAM_DEBOUNCE_MS
        }
    }
}

#[derive(Debug)]
enum SpawnFailure {
    NotFound,
    Other(String),
}

fn spawn_stream_process(
    stream_profiles: &[StreamCommandProfile],
) -> Result<(Child, String), SpawnFailure> {
    let mut saw_not_found = false;
    let mut other_errors = Vec::new();

    for candidate in media_control_candidates() {
        for stream_profile in stream_profiles {
            let profile_display = stream_profile.display();
            let mut command = Command::new(&candidate);
            command
                .args(&stream_profile.args)
                .stdout(Stdio::piped())
                .stderr(Stdio::null());

            match command.spawn() {
                Ok(mut child) => {
                    thread::sleep(STREAM_STARTUP_GRACE);
                    match child.try_wait() {
                        Ok(None) => {
                            return Ok((
                                child,
                                format!("{} {}", candidate.display(), profile_display),
                            ));
                        }
                        Ok(Some(status)) => {
                            other_errors.push(format!(
                                "{} {} exited before startup with status {}",
                                candidate.display(),
                                profile_display,
                                status
                            ));
                        }
                        Err(error) => {
                            cleanup_spawn_probe_child(&mut child);
                            other_errors.push(format!(
                                "{} {} failed during startup check ({error})",
                                candidate.display(),
                                profile_display,
                            ));
                        }
                    }
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    saw_not_found = true;
                }
                Err(error) => {
                    other_errors.push(format!(
                        "{} {} ({error})",
                        candidate.display(),
                        profile_display,
                    ));
                }
            }
        }
    }

    if !other_errors.is_empty() {
        return Err(SpawnFailure::Other(other_errors.join(", ")));
    }

    if saw_not_found {
        return Err(SpawnFailure::NotFound);
    }

    Err(SpawnFailure::Other(
        "No media-control command candidates available".to_string(),
    ))
}

fn cleanup_spawn_probe_child(child: &mut Child) {
    if let Err(error) = child.kill() {
        if error.kind() != ErrorKind::InvalidInput {
            trace!("Failed to terminate media-control probe process: {error}");
        }
    }

    if let Err(error) = child.wait() {
        trace!("Failed to wait media-control probe process: {error}");
    }
}

fn media_control_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(explicit_path) = std::env::var_os(MEDIA_CONTROL_ENV_VAR) {
        if !explicit_path.is_empty() {
            candidates.push(PathBuf::from(explicit_path));
        }
    }

    for command in FALLBACK_COMMANDS {
        candidates.push(PathBuf::from(command));
    }

    let mut seen = HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.clone()));
    candidates
}

#[cfg(test)]
mod tests {
    use super::{parse_stream_line, ParsedLine};
    use crate::platform::MediaData;

    #[test]
    fn parses_simple_playing_payload() {
        let line = r#"{"type":"playback","payload":{"bundleIdentifier":"com.spotify.client","title":"400 Lux","artist":"Lorde","album":"Pure Heroine"}}"#;

        let parsed = parse_stream_line(line);
        assert_eq!(
            parsed,
            Some(ParsedLine::Playing(MediaData {
                artists: Some(vec!["Lorde".to_string()]),
                album: Some("Pure Heroine".to_string()),
                title: Some("400 Lux".to_string()),
                uri: None,
                player: "com.spotify.client".to_string(),
            }))
        );
    }

    #[test]
    fn parses_not_playing_payload() {
        let line = r#"{"type":"playback","payload":{"bundleIdentifier":"com.spotify.client","playbackRate":0}}"#;
        assert_eq!(parse_stream_line(line), Some(ParsedLine::NotPlaying));
    }

    #[test]
    fn parses_artists_array() {
        let line = r#"{"payload":{"player":"Music","artists":["A","B"],"title":"Song","album":"Compilation","uri":"https://example.com","playing":true}}"#;

        let parsed = parse_stream_line(line);
        assert_eq!(
            parsed,
            Some(ParsedLine::Playing(MediaData {
                artists: Some(vec!["A".to_string(), "B".to_string()]),
                album: Some("Compilation".to_string()),
                title: Some("Song".to_string()),
                uri: Some("https://example.com".to_string()),
                player: "Music".to_string(),
            }))
        );
    }

    #[test]
    fn parses_playing_state_without_metadata_as_no_change() {
        let line = r#"{"payload":{"state":"playing"}}"#;
        assert_eq!(parse_stream_line(line), Some(ParsedLine::NoChange));
    }

    #[test]
    fn playing_flag_overrides_stale_zero_playback_rate() {
        let line = r#"{"type":"data","payload":{"playing":true,"playbackRate":0,"bundleIdentifier":"com.vivaldi.Vivaldi","title":"Song","artist":"Artist"}}"#;

        let parsed = parse_stream_line(line);
        assert_eq!(
            parsed,
            Some(ParsedLine::Playing(MediaData {
                artists: Some(vec!["Artist".to_string()]),
                album: None,
                title: Some("Song".to_string()),
                uri: None,
                player: "com.vivaldi.Vivaldi".to_string(),
            }))
        );
    }

    #[test]
    fn paused_flag_overrides_stale_nonzero_playback_rate() {
        let line = r#"{"type":"data","payload":{"playing":false,"playbackRate":1,"bundleIdentifier":"com.vivaldi.Vivaldi","title":"Song","artist":"Artist"}}"#;
        assert_eq!(parse_stream_line(line), Some(ParsedLine::NotPlaying));
    }

    #[test]
    fn parses_root_level_payload_without_nested_payload() {
        let line =
            r#"{"type":"data","playing":true,"player":"Music","artists":["A"],"title":"Song"}"#;

        let parsed = parse_stream_line(line);
        assert_eq!(
            parsed,
            Some(ParsedLine::Playing(MediaData {
                artists: Some(vec!["A".to_string()]),
                album: None,
                title: Some("Song".to_string()),
                uri: None,
                player: "Music".to_string(),
            }))
        );
    }

    #[test]
    fn parses_case_variant_nested_payload_via_fallback() {
        let line = r#"{"Type":"Data","Payload":{"Meta":{"BundleIdentifier":"com.vivaldi.Vivaldi","Title":"Song","Artist":"Artist","Playing":true}}}"#;

        let parsed = parse_stream_line(line);
        assert_eq!(
            parsed,
            Some(ParsedLine::Playing(MediaData {
                artists: Some(vec!["Artist".to_string()]),
                album: None,
                title: Some("Song".to_string()),
                uri: None,
                player: "com.vivaldi.Vivaldi".to_string(),
            }))
        );
    }

    #[test]
    fn ignores_non_data_type_with_case_variant_type_key() {
        let line =
            r#"{"Type":"Status","payload":{"playing":true,"player":"Music","title":"Song"}}"#;
        assert_eq!(parse_stream_line(line), None);
    }

    #[test]
    fn ignores_empty_payload_and_non_data_types() {
        assert_eq!(
            parse_stream_line(r#"{"type":"data","diff":false,"payload":{}}"#),
            None
        );
        assert_eq!(
            parse_stream_line(
                r#"{"type":"status","payload":{"playing":true,"player":"Music","title":"Song"}}"#
            ),
            None
        );
    }

    #[test]
    fn ignores_invalid_lines() {
        assert_eq!(parse_stream_line("not-json"), None);
        assert_eq!(parse_stream_line(r#"{"payload":{"duration":123}}"#), None);
    }
}
