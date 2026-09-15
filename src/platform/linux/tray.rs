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

/// Redraws the menu's show/hide label. Safe to call on every toggle.
///
/// The redraw is asked for from another thread, and that is the whole point rather
/// than an optimisation. `ksni` runs `activate` and the menu callbacks while holding
/// the lock that `update` takes, and every one of those callbacks reaches this
/// function - hiding the window is what they are for. Asking for the redraw on the
/// calling thread therefore asks that thread to wait for a lock it is itself holding,
/// which parks the tray for good: the icon stays on the bar, answers nothing, and the
/// window it just hid can no longer be brought back. Off the thread the request simply
/// lands once the callback has returned.
///
/// A click on the menu also makes `ksni` redraw on its own afterwards, so what this
/// really covers is the window being shown or hidden by something else - the close
/// button, a hotkey, `--hide` over the socket.
pub fn refresh() {
    if HANDLE.get().is_none() {
        return;
    }
    std::thread::spawn(|| {
        if let Some(h) = HANDLE.get() {
            let _ = h.update(|_| {});
        }
    });
}

/// Hands the item back to the bar and closes the connection under it.
///
/// Waits for that to happen rather than only asking for it: the request crosses to the
/// thread that owns the connection, and the only caller is a few statements from the
/// end of the process, which would win that race often enough to make the call mean
/// nothing. The wait is bounded because that same thread runs the menu callbacks, so a
/// tidy goodbye is worth a fraction of a second and never a process that will not quit.
pub fn shutdown() {
    if ACTIVE.swap(false, Ordering::Relaxed)
        && let Some(h) = HANDLE.get()
    {
        let awaiter = h.shutdown();
        let (tx, rx) = std::sync::mpsc::channel();
        // Left to run out on its own: the process is on its way out either way.
        std::thread::spawn(move || {
            awaiter.wait();
            let _ = tx.send(());
        });
        let _ = rx.recv_timeout(std::time::Duration::from_millis(300));
    }
}
