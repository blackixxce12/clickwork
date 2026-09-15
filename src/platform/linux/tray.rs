//! The tray icon, as a StatusNotifierItem over D-Bus.
//!
//! That is the protocol every Wayland bar speaks - waybar, Noctalia, KDE's panel,
//! the GNOME extension - and `ksni` implements it in Rust with no toolkit behind
//! it. The menu mirrors the Windows one item for item.

use crate::{
    APP_TITLE, GLOBAL_STATE, ICON_RGBA, ICON_SIZE, WINDOW_VISIBLE, quit_application,
    stop_everything, toggle_main_window, toggle_playback, toggle_recording,
};
use ksni::blocking::TrayMethods as _;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static HANDLE: OnceLock<ksni::blocking::Handle<Tray>> = OnceLock::new();

struct Tray;

fn icon() -> ksni::Icon {
    // ARGB32, network byte order, from the RGBA the window icon already is.
    let mut data = Vec::with_capacity(ICON_RGBA.len());
    for p in ICON_RGBA.chunks_exact(4) {
        data.extend_from_slice(&[p[3], p[0], p[1], p[2]]);
    }
    ksni::Icon { width: ICON_SIZE as i32, height: ICON_SIZE as i32, data }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "clickwork".into()
    }
    fn title(&self) -> String {
        APP_TITLE.into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![icon()]
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        toggle_main_window();
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let show = if WINDOW_VISIBLE.load(Ordering::Relaxed) { "Hide window" } else { "Show window" };
        vec![
            StandardItem {
                label: show.into(),
                activate: Box::new(|_: &mut Self| toggle_main_window()),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Record / stop".into(),
                activate: Box::new(|_: &mut Self| {
                    if let Some(s) = GLOBAL_STATE.get() {
                        toggle_recording(s);
                    }
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Play / stop".into(),
                activate: Box::new(|_: &mut Self| {
                    if let Some(s) = GLOBAL_STATE.get() {
                        toggle_playback(s);
                    }
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Emergency stop".into(),
                activate: Box::new(|_: &mut Self| {
                    if let Some(s) = GLOBAL_STATE.get() {
                        stop_everything(s);
                    }
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Exit".into(),
                activate: Box::new(|_: &mut Self| quit_application()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// The Windows build needs the icon's window for balloons; nothing here does.
pub fn hwnd() {}

pub fn init() {
    if HANDLE.get().is_some() {
        return;
    }
    match Tray.spawn() {
        Ok(h) => {
            let _ = HANDLE.set(h);
            ACTIVE.store(true, Ordering::Relaxed);
            tracing::info!("tray icon registered");
        }
        Err(e) => tracing::warn!("no tray icon: {e}"),
    }
}

/// True once the icon is up.
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Redraws the menu's show/hide label. Cheap; safe to call on every toggle.
pub fn refresh() {
    if let Some(h) = HANDLE.get() {
        let _ = h.update(|_| {});
    }
}

pub fn shutdown() {
    if ACTIVE.swap(false, Ordering::Relaxed)
        && let Some(h) = HANDLE.get()
    {
        h.shutdown();
    }
}
