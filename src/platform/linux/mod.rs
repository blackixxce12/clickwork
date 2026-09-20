//! The Linux platform: Wayland, Hyprland, evdev, D-Bus and Tesseract.
//!
//! Every Windows-only module in `main.rs` has a counterpart here, selected with
//! `#[cfg(not(windows))]` and a `#[path]` attribute at the declaration site, so the
//! rest of the program never learns which one it got.

// Several functions exist to mirror the Windows surface and are called from only
// one build or one self-test; dead-code warnings would say nothing useful here.
#![allow(dead_code)]

pub mod atspi;
pub mod backend;
pub mod capture;
pub mod clip;
pub mod doctor;
pub mod geom;
pub mod hooks;
pub mod hypr;
pub mod hyprbinds;
pub mod inject;
pub mod keymap;
pub mod kwin;
pub mod overlay;
pub mod platform;
pub mod recorder;
pub mod shortcuts;
pub mod single;
pub mod tess;
pub mod text;
pub mod tray;
pub mod vdesk;
pub mod wl;
pub mod wlr;
