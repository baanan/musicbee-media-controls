use std::sync::{Mutex, Arc};

use log::*;
use tray_item::{TrayItem, IconSource, TIError};

use crate::{logger, config::Config, media_controls::Controls, filesystem};

// TODO: fancier tray (attach toggle, metadata)

pub fn create(controls: Arc<Mutex<Controls>>, config: Arc<Config>) -> Result<(), TIError> {
    let mut tray = TrayItem::new(
        "MusicBee Media Controls",
        IconSource::Resource("musicbee-linux-mediakeys-light")
    )?;

    tray.add_label("MusicBee Media Controls")?;

    // i wish i could add separators here
    // and also mutate the label names
    // TODO: make the tray look nicer
    
    {
        let controls = controls.clone();
        tray.add_menu_item("Attach", move || {
            controls.lock().unwrap().attach()
                .unwrap_or_else(|err| error!("failed to attach: {err}"));
        })?;
    }

    {
        let controls = controls.clone();
        tray.add_menu_item("Detach", move || {
            controls.lock().unwrap().detach()
                .unwrap_or_else(|err| error!("failed to detach: {err}"));
        })?;
    }

    {
        let config = config.clone();
        tray.add_menu_item("Refresh", move || {
            filesystem::update(&mut controls.lock().unwrap(), &config)
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
