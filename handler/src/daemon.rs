use std::{path::{PathBuf, Path}, sync::Arc, fs, process::Command};

use daemonize::Daemonize;
use anyhow::{Result, Context, bail};

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
    if let Some(pid) = get_pid(&config)? {
        bail!("deamon is already running with pid {pid}, use --force to ignore")
    }
    create(config, detach, tray)
}

pub fn end(config: &Config, force: bool) -> Result<()> {
    let Some(pid) = get_pid(config)? else {
        if force {
            bail!("no pid found, the daemon might not be running");
        }
        return Ok(());
    };

    // WARN: this might not work for everything
    Command::new("kill")
        .arg(pid.to_string())
        .spawn()?;

    remove_pid(config)
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
            .start().context("failed to start daemon")?;
    }

    // share config
    let config = Arc::new(config);

    // attach to media controls
    let controls = Controls::init(config.clone())
        .context("failed to initialize the media controls")?;
    let _watcher = filesystem::watch(controls.clone(), config.clone())
        .context("failed to start to watch the filesystem")?;

    if tray {
        // initialize gtk
        gtk::init().unwrap();

        // start system tray
        tray::create(controls, config.clone())
            .context("failed to start system tray")?;

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
        .context("failed to remove the pid")?;
    Ok(())
}

