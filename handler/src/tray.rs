use std::{sync::{Mutex, Arc}, io::Cursor};

use gio::ResourceLookupFlags;
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
            controls.lock().unwrap().attach();
        })?;
    }

    {
        let controls = controls.clone();
        tray.add_menu_item("Detach", move || {
            controls.lock().unwrap().detach();
        })?;
    }

    {
        let config = config.clone();
        tray.add_menu_item("Refresh", move || {
            filesystem::update(&mut controls.lock().unwrap(), &config);
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
