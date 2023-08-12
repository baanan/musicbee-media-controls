use std::{path::{PathBuf, Path}, sync::{Arc, mpsc}, fs, process::Command, thread, time::Duration};

use daemonize::Daemonize;
use anyhow::{Result, Context, bail};
use log::error;

use crate::{config::Config, listener::{media_controls::Controls, self, rpc::Rpc, Listener}, filesystem, tray};

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
    Command::new("kill")
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

pub enum Message {
    Exit,
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

    // setup messages
    let (tx, rx) = mpsc::sync_channel(0);

    {
        let tx = tx.clone();
        ctrlc::set_handler(move || { 
            tx.send(Message::Exit)
                .expect("reciever doesn't hang up except if the whole program exits"); }
        ).context("failed to set termination interupt")?;
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

    // finish setting up the listeners
    let listeners = listeners
        .attach_if_available(&config)?
        .wrap_shared();

    // start watching the filesystem
    let _watcher = filesystem::watch(listeners.clone(), config.clone(), tx.clone())
        .context("failed to start to watch the filesystem")?;

    if tray {
        let listeners = listeners.clone();
        let config = config.clone();
        // initialize gtk in another thread
        // so this thread can handle messages
        thread::spawn(move || {
            // initialize gtk
            gtk::init().unwrap();

            // start system tray
            tray::create(listeners, config, tx)
                .unwrap_or_else(|err| error!("failed to start system tray: {err:?}"));

            // start gtk event loop
            gtk::main();
        });
    }

    // wait until something has been recieved
    // (either an exit signal or a notification that all senders have hung up)
    let _ = rx.recv();

    // cleanup
    if tray {
        glib::idle_add_once(gtk::main_quit);
    }

    // Detach the listeners at the end
    // this isn't necessary for the media controls,
    // but it is for rpc
    listeners.lock().unwrap().detach()
        .context("failed to finally detach everything")?;

    remove_pid(&config)
        .context("failed to remove the pid")?;

    Ok(())
}

