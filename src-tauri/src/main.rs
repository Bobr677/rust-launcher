#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    mc16launcher_lib::linux_startup_init();

    mc16launcher_lib::run();
}
