//! Which part of the session answers questions about windows - and what happens
//! when nothing does.
//!
//! Wayland deliberately offers no way to list windows, read their rectangles,
//! focus one or ask where the pointer is: a client may not spy on or steer its
//! neighbours. Every answer therefore comes out of a compositor's own side
//! channel, and Hyprland's is the one `super::hypr` speaks.
//!
//! The rest of the Linux layer asks this seam rather than asking Hyprland, so that
//! a session which is not Hyprland is a backend that *says* it cannot answer,
//! instead of a pile of empty lists and zeroes indistinguishable from real ones. A
//! cursor reported at the origin and a cursor nobody could find look the same to
//! every caller above, and one of them ends as a macro clicking the top-left
//! corner.
//!
//! A KWin, GNOME or X11 backend is a new `impl WindowBackend` and one more arm in
//! `pick`; no caller in the window domain changes. Two neighbours are outside that
//! domain and still speak to Hyprland directly, on purpose: `super::geom` reads the
//! monitor layout, and falls back to `wl_output` where there is no socket, and
//! `super::doctor` reports Hyprland facts as Hyprland facts. Until somebody can test
//! a backend against the compositor it names, `Unsupported` is the honest answer.

use super::hypr;
use std::sync::OnceLock;

/// A window in the terms this program needs, and no compositor's own.
///
/// Rectangles are logical pixels, the unit a compositor lays windows out in;
/// `super::geom` turns them into the physical ones the rest of the program counts
/// in.
#[derive(Clone, Debug, Default)]
pub struct Window {
    /// Whatever the backend needs to name this window again later. Compared for
    /// identity, never parsed.
    pub id: String,
    pub title: String,
    pub class: String,
    pub pid: i64,
    /// Logical x, y, width, height.
    pub rect: (i32, i32, i32, i32),
    pub monitor: i64,
    pub workspace_id: i64,
    pub workspace_name: String,
    pub fullscreen: bool,
}

/// What the window and cursor half of `super::platform` needs from a session.
///
/// Every question may go unanswered, and an unanswered question is not an error:
/// a backend that cannot do something says so, rather than returning the most
/// plausible-looking value it can think of.
pub trait WindowBackend: Send + Sync {
    /// For the log and `--doctor`.
    fn name(&self) -> &'static str;

    /// Whether this backend answers anything at all. Callers with a sensible
    /// fallback - workspace isolation, hide-to-tray - pick it by asking this,
    /// instead of guessing from an empty answer what an empty answer meant.
    fn supported(&self) -> bool;

    /// Every window somebody could see and click.
    fn windows(&self) -> Vec<Window>;
    fn active_window(&self) -> Option<Window>;
    /// This program's own main window.
    fn own_window(&self) -> Option<Window>;
    /// Logical cursor position in the global layout.
    fn cursor_pos(&self) -> Option<(i32, i32)>;
    fn focused_workspace(&self) -> Option<i64>;
    /// The workspaces on screen right now.
    fn visible_workspaces(&self) -> Vec<i64>;

    fn focus(&self, w: &Window) -> bool;
    fn close(&self, w: &Window) -> bool;
    /// Exact position, in logical pixels.
    fn move_to(&self, w: &Window, x: i32, y: i32) -> bool;
    /// Exact size, in logical pixels.
    fn resize_to(&self, w: &Window, width: i32, height: i32) -> bool;
    fn center(&self, w: &Window) -> bool;
    fn set_fullscreen(&self, w: &Window, on: bool) -> bool;
    fn move_to_workspace(&self, w: &Window, name: &str, silent: bool) -> bool;
    fn move_to_workspace_id(&self, w: &Window, id: i64, silent: bool) -> bool;
}

/// The backend this session gets. Decided once, on the first question asked.
pub fn backend() -> &'static dyn WindowBackend {
    static PICKED: OnceLock<&'static (dyn WindowBackend + 'static)> = OnceLock::new();
    *PICKED.get_or_init(pick)
}

fn pick() -> &'static (dyn WindowBackend + 'static) {
    static HYPRLAND: Hyprland = Hyprland;
    static NONE: Unsupported = Unsupported;
    // A second compositor is probed here, ahead of `NONE`.
    if hypr::available() {
        tracing::info!("window backend: {}", HYPRLAND.name());
        return &HYPRLAND;
    }
    tracing::info!(
        "no window backend for this session: window steps, the window title and the cursor \
         position have nothing to answer them here"
    );
    &NONE
}

// ---- Hyprland ---------------------------------------------------------------

struct Hyprland;

/// One of Hyprland's windows as one of ours.
fn win(c: &hypr::Client) -> Window {
    Window {
        id: c.address.clone(),
        title: c.title.clone(),
        class: c.class.clone(),
        pid: c.pid,
        rect: c.rect(),
        monitor: c.monitor,
        workspace_id: c.workspace.id,
        workspace_name: c.workspace.name.clone(),
        fullscreen: c.fullscreen != 0,
    }
}

/// Back again, carrying the two fields the dispatchers read: the address they
/// select by, and the fullscreen state `hypr::fullscreen` compares against before
/// deciding whether the toggle is needed at all.
fn client(w: &Window) -> hypr::Client {
    hypr::Client {
        address: w.id.clone(),
        fullscreen: if w.fullscreen { 1 } else { 0 },
        ..Default::default()
    }
}

impl WindowBackend for Hyprland {
    fn name(&self) -> &'static str {
        "Hyprland"
    }

    fn supported(&self) -> bool {
        true
    }

    fn windows(&self) -> Vec<Window> {
        hypr::clients().into_iter().filter(|c| c.is_real()).map(|c| win(&c)).collect()
    }

    fn active_window(&self) -> Option<Window> {
        hypr::active_window().map(|c| win(&c))
    }

    fn own_window(&self) -> Option<Window> {
        hypr::own_window().map(|c| win(&c))
    }

    fn cursor_pos(&self) -> Option<(i32, i32)> {
        hypr::cursor_pos()
    }

    fn focused_workspace(&self) -> Option<i64> {
        hypr::focused_workspace_id()
    }

    fn visible_workspaces(&self) -> Vec<i64> {
        hypr::visible_workspace_ids()
    }

    fn focus(&self, w: &Window) -> bool {
        hypr::focus(&client(w))
    }

    fn close(&self, w: &Window) -> bool {
        hypr::close(&client(w))
    }

    fn move_to(&self, w: &Window, x: i32, y: i32) -> bool {
        hypr::move_to(&client(w), x, y)
    }

    fn resize_to(&self, w: &Window, width: i32, height: i32) -> bool {
        hypr::resize_to(&client(w), width, height)
    }

    fn center(&self, w: &Window) -> bool {
        hypr::center(&client(w))
    }

    fn set_fullscreen(&self, w: &Window, on: bool) -> bool {
        hypr::fullscreen(&client(w), on)
    }

    fn move_to_workspace(&self, w: &Window, name: &str, silent: bool) -> bool {
        hypr::move_to_workspace(&client(w), name, silent)
    }

    fn move_to_workspace_id(&self, w: &Window, id: i64, silent: bool) -> bool {
        hypr::move_to_workspace_id(&client(w), id, silent)
    }
}

// ---- nobody -----------------------------------------------------------------

/// A session whose compositor this program has no way to ask.
///
/// The temptation is to guess - a cursor at the origin, a window list that is
/// merely empty - because a guess keeps the features looking present. It does not
/// keep them working: nothing above can tell a guess from a fact, and a step that
/// silently acts on the top-left corner is worse than a step that reports it could
/// not run.
struct Unsupported;

impl WindowBackend for Unsupported {
    fn name(&self) -> &'static str {
        "none"
    }

    fn supported(&self) -> bool {
        false
    }

    fn windows(&self) -> Vec<Window> {
        Vec::new()
    }

    fn active_window(&self) -> Option<Window> {
        None
    }

    fn own_window(&self) -> Option<Window> {
        None
    }

    fn cursor_pos(&self) -> Option<(i32, i32)> {
        None
    }

    fn focused_workspace(&self) -> Option<i64> {
        None
    }

    fn visible_workspaces(&self) -> Vec<i64> {
        Vec::new()
    }

    fn focus(&self, _: &Window) -> bool {
        false
    }

    fn close(&self, _: &Window) -> bool {
        false
    }

    fn move_to(&self, _: &Window, _: i32, _: i32) -> bool {
        false
    }

    fn resize_to(&self, _: &Window, _: i32, _: i32) -> bool {
        false
    }

    fn center(&self, _: &Window) -> bool {
        false
    }

    fn set_fullscreen(&self, _: &Window, _: bool) -> bool {
        false
    }

    fn move_to_workspace(&self, _: &Window, _: &str, _: bool) -> bool {
        false
    }

    fn move_to_workspace_id(&self, _: &Window, _: i64, _: bool) -> bool {
        false
    }
}
