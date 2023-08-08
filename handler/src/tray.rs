use std::sync::{Mutex, Arc};

use log::*;
use tray_item::{TrayItem, IconSource, TIError};

use crate::{logger, config::Config, listener::{Listener, self}, filesystem};

// TODO: fancier tray (attach toggle, metadata)

pub fn create(listeners: Arc<Mutex<listener::List>>, config: Arc<Config>) -> Result<(), TIError> {
    let mut tray = TrayItem::new(
        "MusicBee Media Controls",
        IconSource::Resource("musicbee-linux-mediakeys-light")
    )?;

    tray.add_label("MusicBee Media Controls")?;

    // i wish i could add separators here
    // and also mutate the label names
    // TODO: make the tray look nicer
    
    {
        let listeners = listeners.clone();
        tray.add_menu_item("Attach", move || {
            listeners.lock().unwrap().attach()
                .unwrap_or_else(|err| error!("failed to attach: {err}"));
        })?;
    }

    {
        let listeners = listeners.clone();
        tray.add_menu_item("Detach", move || {
            listeners.lock().unwrap().detach()
                .unwrap_or_else(|err| error!("failed to detach: {err}"));
        })?;
    }

    {
        let config = config.clone();
        tray.add_menu_item("Refresh", move || {
            filesystem::update(&mut *listeners.lock().unwrap(), &config)
                .unwrap_or_else(|err| error!("failed to refresh controls: {err}"));
        })?;
    }

    tray.add_menu_item("Show Logs", move || {
        logger::open(&config);
    })?;
    
    tray.add_menu_item("Quit", || {
        crate::exit();
    })?;

    Ok(())
}
