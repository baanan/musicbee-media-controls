# MusicBee Media Controls

Media controls for Musicbee with Linux

## Installation

### Plugin

Copy `plugin/mb_LinuxMediaControls.dll` over to your MusicBee plugin folder

### Handler

In `handler`, run `cargo install --path .`

## Use

Most of the usage comes with the `musicbee_mediakeys` command which can start or end the daemon. Run `musicbee_mediakeys run` to start the daemon. 

## Config

Generic configuration can be found with `musicbee_mediakeys config-file --open`. The most important configuration to change is `commands.musicbee_location` to send commands to MusicBee.

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
    detach_on_stop: true,
    // should the handler be closed when musicbee is closed
    exit_with_plugin: true,
    // how long should the default seek be
    seek_amount: (
        secs: 5,
        nanos: 0,
    ),
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
  - [ ] start (currently only controls)
