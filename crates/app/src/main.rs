// On Windows release builds, run as a GUI app — no console window
// pops up alongside the game. Debug builds keep the console so
// terminal logging stays visible during development.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

//! Desktop binary entry point. All app logic lives in the library
//! crate so the Android `cdylib` target can share it.

fn main() {
    rubiks_trainer::run_app();
}
