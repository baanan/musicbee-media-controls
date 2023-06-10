use std::fs;

fn main() {
	let home_dir = dirs::home_dir().expect("failed to get home directory");
	let home = home_dir.to_str().expect("failed to parse home directory");

	let icon_dir = format!("{home}/.local/share/icons/hicolor/64x64/apps");

	fs::write(
		format!("{icon_dir}/musicbee-linux-mediakeys-light.png"),
		include_bytes!("res/light.png")
	).expect("failed to install light icon");
	fs::write(
		format!("{icon_dir}/musicbee-linux-mediakeys-dark.png"),
		include_bytes!("res/dark.png")
	).expect("failed to install dark icon");
}
