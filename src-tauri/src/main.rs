// Without this the release binary stays a console application, and Windows
// opens a terminal window behind the app for it. Debug builds keep the console
// so `cargo run` still shows the log.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dji_live_bridge_lib::run();
}
