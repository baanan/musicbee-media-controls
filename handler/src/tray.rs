use std::sync::Arc;

use tray_item::{TrayItem, IconSource};

use crate::{config::Config, messages::MessageSender, logger};

use anyhow::Result;

// TODO: fancier tray (attach toggle, metadata)

pub fn start(message_sender: MessageSender, config: Arc<Config>) -> Result<()> {
    // initialize gtk
    gtk::init().unwrap();

    // create tray
    self::create(message_sender, config)?;

    // start gtk event loop
    gtk::main();

    Ok(())
}

pub fn create(message_sender: MessageSender, config: Arc<Config>) -> Result<()> {
    let mut tray = TrayItem::new(
        "MusicBee Media Controls",
        IconSource::Resource("musicbee-linux-mediakeys-light")
    )?;

    tray.add_label("MusicBee Media Controls")?;

    // i wish i could add separators here
    // and also mutate the label names
    // TODO: make the tray look nicer
    
    {
        let message_sender = message_sender.clone();
        tray.add_menu_item("Attach", move || message_sender.blocking_attach())?;
    }

    {
        let message_sender = message_sender.clone();
        tray.add_menu_item("Detach", move || message_sender.blocking_detach())?;
    }

    {
        let message_sender = message_sender.clone();
        tray.add_menu_item("Refresh", move || message_sender.blocking_update())?;
    }

    tray.add_menu_item("Show Logs", move || logger::open(&config))?;
    tray.add_menu_item("Quit", move || message_sender.blocking_exit())?;

    Ok(())
}
