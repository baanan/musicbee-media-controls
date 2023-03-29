use std::{path::Path, fs, io, env};

#[derive(Clone)]
pub struct Mapping {
    pub from: String,
    pub to: String,
}

impl Mapping {
    pub fn map(&self, string: &str) -> String {
        string.replace(&self.from, &self.to)
    }
}

#[derive(Clone)]
pub struct Config {
    pub wine_prefix: String,
    pub musicbee_location: String,
    pub communication_directory: String,
    pub music_file_mapper: Mapping,
    pub temporary_file_mapper: Mapping,
}

impl Config {
    pub fn get_comm_path(&self, name: &str) -> String {
        format!("{}/{name}", self.communication_directory)
    }

    pub fn read_comm_file(&self, name: &str) -> io::Result<String> {
        fs::read_to_string(Path::new(&self.get_comm_path(name)))
    }

    pub fn map_filename(&self, name: &str) -> String {
        // we don't like \ here
        let name = name.replace('\\', "/");

        // TODO: better check
        if name.contains("Temp") {
            self.temporary_file_mapper.map(&name)
        } else {
            self.music_file_mapper.map(&name)
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        // this is dirty, but it works
        let home_dir = env::var("HOME").unwrap();
        let username = home_dir.replace("/home/", "");

        // TODO: this is an unreasonable default
        let wine_prefix = format!("{home_dir}/Documents/executables/musicbee/.wine");

        Self {
            musicbee_location: r#"C:/Program Files/MusicBee/MusicBee.exe"#.to_string(),
            communication_directory: "/tmp/musicbee-mediakeys".to_string(),
            // NOTE: this might be unreasonable, but i'm not sure
            music_file_mapper: Mapping {
                from: format!("C:/Users/{username}/Music"),
                to: format!("{home_dir}/Music"),
            },
            temporary_file_mapper: Mapping {
                from: "C:/".to_string(),
                to: format!("{wine_prefix}/drive_c/")
            },
            wine_prefix,
        }   
    }
}

pub fn get_config() -> Config {
    Config::default()
}
