use std::{time::Duration, fmt::Display, io};

use log::*;

use crate::{config::Config, filesystem::ACTION_FILE};

#[allow(dead_code)]
pub enum RepeatMode {
    None,
    All,
    One,
}

impl Display for RepeatMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match self {
            Self::None => "none",
            Self::All => "all",
            Self::One => "one",
        };

        write!(f, "{string}")
    }
}

#[allow(dead_code)]
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
            Self::Shuffle(val) => write!(f, "shuffle {val}"),
            Self::Repeat(val) => write!(f, "repeat {val}"),
            Self::Seek { milis } => write!(f, "seek {milis}"),
            Self::Position(val) => write!(f, "position {}", val.as_millis()),
            #[allow(clippy::cast_possible_truncation)]
            Self::Volume(val) => write!(f, "volume {}", (val * 100.0) as i32),
        }
    }
}

impl Action {
    pub fn run(&self, config: &Config) -> io::Result<()> {
        let action = self.to_string();
        debug!("running action: {action}");

        config.write_comm_file(ACTION_FILE, &action)?;

        trace!("notifying musicbee (volume down)");

        // HACK: to notify the plugin that an action is ready,
        // the handler runs /VolumeDown
        config.run_command("/VolumeDown", None)?;
        Ok(())
    }
}
