# MusicBee Linux Media Controls

Media controls for Musicbee with Linux

## Installation

### Plugin

Copy `plugin/mb_LinuxMediaControls.dll` over to your MusicBee plugin folder

### Handler

In `handler`, run `cargo install --path .`

## Use

Most of the usage comes with the `musicbee_media_controls` command which can start or end the daemon. Run `musicbee_media_controls run` to start the daemon. 

## Known Issues

- The handler freaks out when changing volume while using [Aylur's Widgets](https://extensions.gnome.org/extension/5338/aylurs-widgets/), use `send_volume: false` to stop this.

## Config

Generic configuration can be found with `musicbee_media_controls config-file --open`. The most important configuration to change is `commands.musicbee_location` to send commands to MusicBee.

```ron
(
    // configuration for the plugin's use of musicbee command line commands
    commands: (
        wine_command: "wine",
        wine_prefix: "{home_dir}/Documents/executables/musicbee/.wine",
        musicbee_location: "C:/Program Files/MusicBee/MusicBee.exe",
    ),
    // communication coming from musicbee is largely done in this directory,
    // it must be the same between the handler and the plugin
    communication: (
        directory: "/tmp/musicbee-mediakeys",
    ),
    // media control handling
    media_controls: (
        enabled: true,
        // how long should the default seek be
        seek_amount: (
            secs: 5,
            nanos: 0,
        ),
        // should the media controls allow externally setting the volume
        send_volume: true,
    ),
    // discord rich presence
    rpc: (
        enabled: false,
    ),
    // a mapping between the music folder of the wine prefix and your own music folder
    music_file_mapper: (
        from: "C:/Users/{username}/Music",
        to: "{home_dir}/Music",
    ),
    // a mapping between other files within the prefix
    temporary_file_mapper: (
        from: "C:/",
        to: "{wine_prefix}/drive_c/",
    ),
    // should the handler detach the media controls when musicbee is stopped
    detach_on_stop: false,
    // should the handler be closed when musicbee is closed
    exit_with_plugin: true,
)
```

## TODO:

- [ ] media controls
  - [x] basic functionality
  - [x] attach / detach
  - [x] config
  - [ ] all media features
    - [x] playback times
    - [x] seeking
    - [ ] all events (loop, shuffle)
      - [x] plugin
      - [ ] handler (issue with souvlaki)
  - [ ] window raising
  - [ ] custom event handling (like instead of loop, etc)
- [ ] plugin
  - [ ] rebrand as generic for discord rpc
- [ ] discord rpc
  - [x] start
  - [ ] async cover uploading (big performance issue currently)
  - [ ] custom config
