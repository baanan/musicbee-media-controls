use std::sync::{Mutex, Arc};

use tray_item::TrayItem;

use crate::{logger, config::Config, media_controls::Controls, filesystem};

pub fn create(controls: Arc<Mutex<Controls>>, config: Config) {
    let mut tray = TrayItem::new("MusicBee Media Controls", "accessories-calculator").unwrap();

    tray.add_label("MusicBee Media Controls").unwrap();

    // i wish i could add separators here
    // and also mutate the label names
    // TODO: make the tray look nicer
    
    {
        let controls = controls.clone();
        let config = config.clone();
        tray.add_menu_item("Attach", move || {
            controls.lock().unwrap().attach();
            filesystem::update(controls.clone(), &config)
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
            filesystem::update(controls.clone(), &config);
        }).unwrap();
    }

    tray.add_menu_item("Show Logs", move || {
        logger::open(&config);
    }).unwrap();
    
    tray.add_menu_item("Quit", || {
        gtk::main_quit();
    }).unwrap();
}
