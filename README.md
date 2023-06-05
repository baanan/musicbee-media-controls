# MusicBee Media Controls

NOTICE: THIS IS IN VERY EARLY STAGES, I'LL FIX THINGS (mostly the installation) LATER

## Installation

### Plugin

Copy over plugin/mb_LinuxMediaControls.dll over to your MusicBee plugin folder

### Handler

Currently only building from source every time is supported, run `cargo run --release`

## TODO:

- [ ] media controls
  - [x] basic functionality
  - [x] attach / detach
  - [x] config
  - [ ] all media features
    - [x] playback times
    - [ ] all events (loop, shuffle)
  - [ ] window raising
  - [ ] custom event handling (like instead of loop, etc)
- [ ] plugin
  - [ ] rebrand as generic for discord rpc
- [ ] discord rpc
  - [ ] start (currently only controls)
