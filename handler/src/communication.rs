use std::{time::Duration, fmt::Display};

use log::{debug, trace};

use crate::{config::Config, filesystem::ACTION_FILE};

pub enum RepeatMode {
    None,
    All,
    One,
}

impl Display for RepeatMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match self {
            RepeatMode::None => "none",
            RepeatMode::All => "all",
            RepeatMode::One => "one",
        };

        write!(f, "{string}")
    }
}

pub enum Action {
    Shuffle(bool),
    Repeat(RepeatMode),
    Seek { milis: i32 },
    Position(Duration),
    Volume(f32),
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::Shuffle(val) => write!(f, "shuffle {val}"),
            Action::Repeat(val) => write!(f, "repeat {val}"),
            Action::Seek { milis } => write!(f, "seek {}", milis),
            Action::Position(val) => write!(f, "position {}", val.as_millis()),
            Action::Volume(val) => write!(f, "volume {}", (val * 100.0) as i32),
        }
    }
}

impl Action {
    pub fn run(&self, config: &Config) {
        let action = self.to_string();
        debug!("running action: {action}");

        config.write_comm_file(ACTION_FILE, &action).unwrap();

        trace!("notifying musicbee (volume down)");

        // HACK: to notify the plugin that an action is ready,
        // the handler runs /VolumeDown
        config.run_command("/VolumeDown", None);
    }
}
