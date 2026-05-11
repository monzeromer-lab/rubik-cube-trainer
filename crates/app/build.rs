//! Build script for the `rubiks-trainer` crate.
//!
//! On Windows targets this compiles `icon.rc` into the binary so the
//! `.exe` carries its own icon (visible in Explorer, the taskbar, and
//! Alt-Tab). On every other target the script is a no-op.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // `embed-resource` knows how to invoke the right resource
        // compiler — `windres` for the `*-windows-gnu` targets (what
        // we use when cross-building from Linux with mingw-w64) and
        // `rc.exe` from the Windows SDK for `*-windows-msvc`.
        embed_resource::compile("icon.rc", embed_resource::NONE);
    }
}
