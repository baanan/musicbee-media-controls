# MusicBee Media Controls

Media controls for Musicbee with Linux

## Installation

### Plugin

Copy over `plugin/mb_LinuxMediaControls.dll` over to your MusicBee plugin folder

### Handler

Move to `handler`, and run `cargo install --path .`

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
