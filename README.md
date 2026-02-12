# Media Player watcher

This watcher sends information the media which is playing now to [ActivityWatch](https://activitywatch.net/).
It supports any player which can report its status to the system 
and be controllable by tray or standard multimedia keys,
such as Spotify, Foobar, browser-based players, and others. Most media players are supported.

| Environment   | Support                        |
| ------------- | -------------------------------|
| Linux         | :heavy_check_mark: Yes ([MPRIS](https://specifications.freedesktop.org/mpris-spec/latest/)) |
| Windows       | :heavy_check_mark: Yes         |
| MacOS         | :heavy_check_mark: Yes (requires [`media-control`](https://github.com/ungive/media-control)) |

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

## Installation

- **Linux**:
  - Install the attached _.deb_ or _.rpm_ file from the [latest release](https://github.com/2e3s/aw-watcher-media-player/releases/latest).
  - To install manually and make it available for ActivityWatch,
    run `sudo unzip -j aw-watcher-media-player-linux.zip aw-watcher-media-player-linux -d /usr/local/bin` in the console.
    - Optionally, to use visualizations, run `sudo unzip -d /usr/local/share/aw-watcher-media-player/visualization aw-watcher-media-player-linux.zip 'visualization/*'`.

  **Windows**:
  - Download and run the attached installer `aw-watcher-media-player-installer.exe` from the [latest release](https://github.com/2e3s/aw-watcher-media-player/releases/latest).
  - To install manually and make it available for ActivityWatch,
    unpack the executable from `aw-watcher-media-player-windows.zip` into any new folder,
    right-click on "Start" -> "System" -> "Advanced system settings" - "Advanced" tab -> "Environment Variables..." -> upper "Edit...", add the new folder path.
- **MacOS**:
  - Install [`media-control`](https://github.com/ungive/media-control): `brew install media-control`.
  - If `media-control` is not available in the runtime `PATH`, set `AW_WATCHER_MEDIA_CONTROL_PATH=/absolute/path/to/media-control`.
  - The watcher prefers `media-control stream --no-diff --debounce=200` and automatically falls back to older command variants when needed.
- Optionally, add `aw-watcher-media-player` to autostart at `aw-qt/aw-qt.toml` in [config directory](https://docs.activitywatch.net/en/latest/directories.html#config).

## Configuration

Configuration file `aw-watcher-media-player.toml` is located under [user's local configuration directory](https://docs.rs/dirs/latest/dirs/fn.config_local_dir.html).
By default, the watcher looks for:

- `<config_local_dir>/activitywatch/aw-watcher-media-player/aw-watcher-media-player.toml`

Legacy fallback path:

- `<config_local_dir>/aw-watcher-media-player.toml` (deprecated; a warning is logged and automatic migration is not performed)

The config file may be created manually before running the binary.
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
On MacOS, you can set `AW_WATCHER_MEDIA_CONTROL_PATH` to the `media-control` binary if it is not discoverable in `PATH`.
On MacOS, you can set `AW_WATCHER_MEDIA_CONTROL_DEBOUNCE_MS` to change stream debounce (defaults to `200`).

### Troubleshooting (MacOS stream fallback)

Run with `-vv` and check for these exact log message bodies:

- Preferred profile selected:
  - `Started media-control stream using /opt/homebrew/bin/media-control stream --no-diff --debounce=200`
- Fallback to no-debounce profile selected:
  - `Started media-control stream using /opt/homebrew/bin/media-control stream --no-diff`
- Fallback to legacy profile selected:
  - `Started media-control stream using /opt/homebrew/bin/media-control stream`
- All profiles failed for a candidate:
  - `Failed to start media-control stream: ... exited before startup with status ...`
- Binary not found:
  - `Unable to find media-control executable. Install it with \`brew install media-control\` or set AW_WATCHER_MEDIA_CONTROL_PATH`

Notes:
- The binary path may differ (`/usr/local/bin/media-control` or `media-control` from `PATH`).
- The debounce value in the first line reflects `AW_WATCHER_MEDIA_CONTROL_DEBOUNCE_MS` if set.

**Note that normally browsers report the currently playing media to the system even in a private mode/tab/window.**

## Custom Visualization

![custom_visualization](images/aw-vizualization-example.png)

This watcher has a visualization which attempts to do its best to display the sorted list of artists with the overall play time for each artist.
Note that ActiveWatch UI gives no abilities for the widget to control its sizing, so it may appear smaller than builtin visualizations.

1.
   - aw-server: Add the following section to your `aw-server/aw-server.toml` file in [config directory](https://docs.activitywatch.net/en/latest/directories.html#config):
      ```toml
      [server.custom_static]
      aw-watcher-media-player = "/path/to/aw-watcher-media-player/visualization"
      # aw-watcher-media-player = "/usr/share/aw-watcher-media-player/visualization" # .deb or .rpm installation
      # aw-watcher-media-player = "/usr/local/share/aw-watcher-media-player/visualization" # Linux installation from archive
      # aw-watcher-media-player = 'C:\Users\<USER>\AppData\Local\aw-watcher-media-player\visualization' # Windows installer
      ```
   - aw-server-rust: Add the following section to your `aw-server-rust/config.toml` file in [config directory](https://docs.activitywatch.net/en/latest/directories.html#config):
      ```toml
      [custom_static]
      aw-watcher-media-player = "/path/to/aw-watcher-media-player/visualization"
      # aw-watcher-media-player = "/usr/share/aw-watcher-media-player/visualization" # .deb or .rpm installation
      # aw-watcher-media-player = "/usr/local/share/aw-watcher-media-player/visualization" # Linux installation from archive
      # aw-watcher-media-player = 'C:\Users\<USER>\AppData\Local\aw-watcher-media-player\visualization' # Windows installer
      ```
2. Restart ActivityWatch
3. Add custom visualizations from the Activity Watch GUI: `Activity > Edit View > Add Visualization > Custom Visualization`
4. Enter `aw-watcher-media-player` for the watcher name.

The visualization is not customizable from ActivityWatch UI. In order to change, the output, open `index.html`:
- Find `getAggregation` function and change `event.data.artist` to `event.data.player` to aggregate by players.
- Change `MAX_AGGREGATIONS` to determine the maximum number of entries (default is 50).

## Build

`cargo build --release` on any platform. See [_release.yml_](https://github.com/2e3s/aw-watcher-media-player/blob/main/.github/workflows/release.yml) for details.
