use std::{sync::{Mutex, Arc}, io::Cursor};

use gio::ResourceLookupFlags;
use tray_item::{TrayItem, IconSource};

use crate::{logger, config::Config, media_controls::Controls, filesystem};

pub fn create(controls: Arc<Mutex<Controls>>, config: Arc<Config>) {
    let mut tray = TrayItem::new(
        "MusicBee Media Controls",
        IconSource::Resource("musicbee-linux-mediakeys-light")
    ).unwrap();

    tray.add_label("MusicBee Media Controls").unwrap();

    // i wish i could add separators here
    // and also mutate the label names
    // TODO: make the tray look nicer
    
    {
        let controls = controls.clone();
        tray.add_menu_item("Attach", move || {
            controls.lock().unwrap().attach();
        }).unwrap();
    }

    {
        let controls = controls.clone();
        tray.add_menu_item("Detach", move || {
            controls.lock().unwrap().detach();
        }).unwrap();
    }

    {
        let config = config.clone();
        tray.add_menu_item("Refresh", move || {
            filesystem::update(&mut controls.lock().unwrap(), &config);
        }).unwrap();
    }

    tray.add_menu_item("Show Logs", move || {
        logger::open(&config);
    }).unwrap();
    
    tray.add_menu_item("Quit", || {
        gtk::main_quit();
    }).unwrap();
}
