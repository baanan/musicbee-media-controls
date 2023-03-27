use tray_item::TrayItem;

pub fn create() {
    let mut tray = TrayItem::new("MusicBee Media Controls", "accessories-calculator").unwrap();

    tray.add_label("MusicBee Media Controls").unwrap();

    // tray.add_menu_item("Quit", || {
    //     println!("Hello!");
    // }).unwrap();

    tray.add_menu_item("Quit", || {
        gtk::main_quit();
    }).unwrap();
}
