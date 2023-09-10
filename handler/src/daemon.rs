use std::{path::{PathBuf, Path}, sync::Arc, fs, thread, time::Duration};

use daemonize::Daemonize;
use anyhow::{Result, Context, bail, Error};
use log::{error, debug, trace};
use tokio::task;

use crate::{config::Config, listener::{media_controls::Controls, self, rpc::Rpc, Logger}, filesystem::{self, Filesystem}, tray, messages::Messages, cli::RunConfig, logger};

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

pub fn run(config: Config, run_config: &RunConfig, outstanding_error: Option<Error>) -> Result<()> {
    let RunConfig { force, detach, tray, replace } = run_config;

    logger::init(&config)
        .context("failed to start logging")?;

    // if the config originally failed to parse, notify the user
    if let Some(outstanding_error) = outstanding_error {
        error!("failed to parse config, got: {outstanding_error}. Returned to defaults");
    }

    if *replace { 
        end(&config, false).context("failed to replace previous daemon")?; 
    }

    if *force {
        if let Some(pid) = get_pid(&config)? {
            bail!("deamon is already running with pid {pid}, use --force to ignore, or omit --no-replace to replace")
        }
    } 

    if *detach {
        init_pid_dir(&config)?;
        Daemonize::new()
            .pid_file(pid_file(&config))
            .start().context("failed to start daemon")?;
    }

    crate::run_async(create(config, *tray)).context("failed to start daemon")?;

    Ok(())
}

async fn create(config: Config, tray: bool) -> Result<()> {
    // -- setup -- //

    // share config
    let config = Arc::new(config);

    // setup messages
    let messages = Messages::new(config.clone());

    {
        let tx = messages.sender();
        ctrlc::set_handler(move || tx.exit())
            .context("failed to set termination interupt")?;
    }

    // setup listeners
    let mut listeners = listener::List::new();
    listeners.add(Logger);
    listeners.add(Filesystem::new(messages.sender()));

    // media controls
    if config.media_controls.enabled {
        let controls = Controls::new(messages.sender())
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

    let sender = messages.sender();

    // start listening to messages
    let handle = task::spawn(messages.listen_until_exit(listeners, config.clone()));

    // get initial values by queueing up an update
    if filesystem::plugin_available(&config).await?.unwrap_or(false) { 
        sender.attach(); 
    }

    // finish listening to messages
    handle.await?;

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

    remove_pid(&config)
        .context("failed to remove the pid")?;

    trace!("removed pid, fully exiting");

    Ok(())
}

