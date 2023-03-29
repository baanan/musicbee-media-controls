use tray_item::TrayItem;

use crate::{logger, config::Config};

pub fn create(config: Config) {
    let mut tray = TrayItem::new("MusicBee Media Controls", "accessories-calculator").unwrap();

    tray.add_label("MusicBee Media Controls").unwrap();

    // i wish i could add separators here
    
    tray.add_menu_item("Refresh", || {
        todo!();
    }).unwrap();

    tray.add_menu_item("Show Logs", move || {
        logger::open(&config);
    }).unwrap();
    
    tray.add_menu_item("Quit", || {
        gtk::main_quit();
    }).unwrap();
}
