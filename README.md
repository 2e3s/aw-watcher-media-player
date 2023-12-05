# Media Player watcher

This watcher sends information the media which is playing now to [ActivityWatch](https://activitywatch.net/).
It supports any player which can report its status to the system 
and be controllable by tray or standard multimedia keys,
such as Spotify, Foobar, browser-based players, and others. Most media players are supported.

| Environment   | Support                        |
| ------------- | -------------------------------|
| Linux         | :heavy_check_mark: Yes ([MPRIS](https://specifications.freedesktop.org/mpris-spec/latest/)) |
| Windows       | :heavy_check_mark: Yes         |
| MacOS         | :hourglass: Not yet supported  |

<details>
<summary>Examples of reported data</summary>

Spotify in Linux:
```json
{
  "album": "How to Measure a Planet? (Deluxe Edition)",
  "artist": "The Gathering",
  "player": "Spotify",
  "title": "My Electricity",
  "uri": "https://open.spotify.com/track/1cSWc2kX4z39L5uFdGcjFP"
}
```
Firefox in Linux (no plugins):
```json
{
    "artist": "Eileen",
    "player": "Mozilla Firefox",
    "title": "🇺🇦 🇵🇱 Гей, соколи! / Hej, sokoły! – Ukrainian/Polish folk song"
}
```
MS Edge in Windows:
```json
{
  "artist": "Bel Canto Choir Vilnius",
  "player": "MSEdge",
  "title": "Shchedryk (Carol of the Bells) – Bel Canto Choir Vilnius"
}
```
Default Windows player
```json
{
  "album": "Zemlya",
  "artist": "Okean Elzy",
  "player": "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic",
  "title": "Obijmy"
}
```

</details>

## Configuration

Configuration file `aw-watcher-media-player.toml` is located in [user's local configuration directory](https://docs.rs/dirs/latest/dirs/fn.config_local_dir.html).
CLI arguments override the file configuration.
Example:
```toml
port = 5600
host = "localhost"
poll_time = 5
include_players = ["Spotify", "firefox", "chrom"]
exclude_players = ["chromium"]
```
Filter options for including and excluding players for reporting look for a case-insensitive substring.
Use `-vv` to see what's reported.

Note that normally browsers report the currently playing track to the system even in a private mode.

## Build

`cargo build --release` on any platform. See [_release.yml_](https://github.com/2e3s/aw-watcher-media-player/blob/main/.github/workflows/release.yml) for details.
