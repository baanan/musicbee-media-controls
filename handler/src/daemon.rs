use std::{path::{PathBuf, Path}, sync::Arc, fs, thread, time::Duration};

use daemonize::Daemonize;
use anyhow::{Result, Context, bail};
use log::{error, debug, trace};

use crate::{config::Config, listener::{media_controls::Controls, self, rpc::Rpc, Listener}, filesystem, tray, messages::Messages};

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
        bail!("deamon is already running with pid {pid}, use --force to ignore, or omit --no-replace to replace")
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
    std::process::Command::new("kill")
        .arg(pid.to_string())
        .spawn()?;

    println!("waiting for old daemon to be killed...");
    // HACK: constant sleep
    std::thread::sleep(Duration::from_millis(500));

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
    // -- setup -- //

    if detach {
        init_pid_dir(&config)?;
        Daemonize::new()
            .pid_file(pid_file(&config))
            .start().context("failed to start daemon")?;
    }

    // share config
    let config = Arc::new(config);

    // setup messages
    let messages = Messages::new();

    {
        let tx = messages.sender();
        ctrlc::set_handler(move || tx.exit())
            .context("failed to set termination interupt")?;
    }

    // setup listeners
    let mut listeners = listener::List::new();

    // media controls
    if config.media_controls.enabled {
        let controls = Controls::new(config.clone())
            .context("failed to initialize the media controls")?;
        listeners.add(controls);
    }

    // rpc
    if config.rpc.enabled {
        let rpc = Rpc::new(config.clone());
        listeners.add(rpc);
    }

    // start watching the filesystem
    let watcher = filesystem::watch(messages.sender(), config.clone())
        .context("failed to start to watch the filesystem")?;

    // set up the system tray
    let gtk_handle = tray.then(|| {
        let config = config.clone();
        let tx = messages.sender();
        // initialize gtk in another thread
        // so this thread can handle messages
        thread::spawn(move || 
            tray::start(tx, config)
                .unwrap_or_else(|err| error!("failed to start system tray: {err:?}"))
        )
    });

    // -- running -- //

    // get initial values by queueing up an update
    messages.sender().update();

    // start listening to messages
    messages.listen_until_exit(&mut listeners, &config)?;

    // -- cleanup -- //

    debug!("recieved exit signal");

    // stop watching the filesystem before dropping everything else
    drop(watcher);

    // cleanup
    if let Some(gtk_handle) = gtk_handle {
        glib::idle_add_once(gtk::main_quit);
        gtk_handle.join().expect("tray panicked");
    }

    trace!("caught up with gtk");

    // Detach the listeners at the end
    // this isn't necessary for the media controls,
    // but it is for rpc
    listeners.detach()
        .context("failed to finally detach everything")?;

    trace!("listeners have detached");

    remove_pid(&config)
        .context("failed to remove the pid")?;

    trace!("removed pid, fully exiting");

    Ok(())
}

