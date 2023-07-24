use std::{path::{PathBuf, Path}, sync::Arc, fs, process::Command};

use daemonize::Daemonize;
use anyhow::{Result, Context};

use crate::{config::Config, media_controls::Controls, filesystem, tray};

pub fn pid_file(config: &Config) -> PathBuf {
    crate::project_dirs().and_then(|directories| directories.runtime_dir().map(Path::to_owned))
        .unwrap_or_else(|| Path::new(&config.communication.directory).to_owned())
        .join("daemon.pid")
}

pub fn get_pid(config: &Config) -> Result<Option<u32>> {
    let file = pid_file(config);
    if !file.exists() { return Ok(None); }
    let text = fs::read_to_string(pid_file(config))
        .context("failed to read pid file")?;
    Ok(Some(text.trim().parse().context("failed to parse pid from file")?))
}

pub fn remove_pid(config: &Config) -> Result<()> {
    let file = pid_file(config);
    if file.exists() {
        fs::remove_file(file)?;
    }
    Ok(())
}

pub fn run(config: Config, detach: bool, tray: bool) -> Result<()> {
    let pid = get_pid(&config)?;
    if pid.is_none() {
        create(config, detach, tray)?;
    }
    Ok(())
}

pub fn end(config: &Config) -> Result<()> {
    let pid = get_pid(config)?;
    if let Some(pid) = pid {
        // WARN: this might not work for everything
        Command::new("kill")
            .arg(pid.to_string())
            .spawn()?;
        remove_pid(config)?;
    }
    Ok(())
}

pub fn init_pid_dir(config: &Config) -> Result<()> {
    fs::create_dir_all(
        pid_file(config).parent()
            .expect("pid file must be in a directory")
    )?;
    Ok(())
}

pub fn create(config: Config, detach: bool, tray: bool) -> Result<()> {
    if detach {
        init_pid_dir(&config)?;
        Daemonize::new()
            .pid_file(pid_file(&config))
            .start().expect("failed to start daemon");
    }

    // share config
    let config = Arc::new(config);

    // attach to media controls
    let controls = Controls::init(config.clone())
        .expect("failed to initialize the media controls");
    let _watcher = filesystem::watch(controls.clone(), config.clone())
        .expect("failed to start to watch the filesystem");

    if tray {
        // initialize gtk
        gtk::init().unwrap();

        // start system tray
        tray::create(controls, config.clone())
            .expect("failed to start system tray");

        // start gtk event loop
        gtk::main();
    } else {
        // pause the thread
        // (presumably until a kill command is sent)
        std::thread::park();
    }

    // cleanup
    // NOTE: this isn't run if the process was killed
    // removing the pid is also done by self::end
    remove_pid(&config)
        .expect("failed to remove the pid");
    Ok(())
}

