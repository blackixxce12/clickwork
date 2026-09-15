//! Embeds the application icon and version metadata into the Windows executable.
//!
//! This is what Explorer, the taskbar and the Alt-Tab switcher read. The window's
//! own icon is set separately in `main.rs` from `assets/icon.rgba`.
//!
//! Note: `#[cfg(target_os = "windows")]` is wrong here - a build script runs on the
//! *host*, so the target OS has to come from Cargo's environment instead. The same
//! trap caught the icon itself for a while: gating this on `#[cfg(windows)]` meant a
//! Windows executable cross-built from Linux came out with no icon and no version
//! metadata, because the gate was asking where the compiler ran rather than what it
//! was building.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("FileDescription", "Clickwork");
    res.set("ProductName", "Clickwork");
    res.set("OriginalFilename", "Clickwork.exe");
    res.set("LegalCopyright", "MIT License");

    // Nothing to configure for a cross build: winresource reads the target triple and
    // looks for the GNU toolchain's resource compiler under its own prefix, which is
    // what mingw-w64 installs it as.
    //
    // A missing resource compiler (rc.exe / windres.exe) should cost you the Explorer
    // icon, not the whole build - the app still works fine without it.
    if let Err(e) = res.compile() {
        println!("cargo:warning=could not embed the executable icon: {e}");
    }
}


