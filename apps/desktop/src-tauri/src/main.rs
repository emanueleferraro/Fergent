//! The Fergents desktop application binary.
// Hide the console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    fergents_desktop_lib::run();
}
