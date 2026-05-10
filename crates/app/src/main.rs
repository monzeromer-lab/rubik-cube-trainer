//! Desktop binary entry point. All app logic lives in the library
//! crate so the Android `cdylib` target can share it.

fn main() {
    rubiks_trainer::run_app();
}
