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

/// What a backend can actually answer.
///
/// `false` here means "nothing in this session can say", which is not the same
/// news as an empty list or a zero, and the difference is the whole reason this
/// type exists. A boolean could not carry it: the wlroots backend below lists
/// windows, focuses them and closes them, and cannot measure one, name the
/// process behind it, find the pointer or say which workspace it is on. A
/// backend that answered "yes, supported" and then returned zeroes would be the
/// silent lie this seam was introduced to end - and `Window` derives `Default`,
/// so half a window is free and looks exactly like a real one.
///
/// Grouped by the decision a caller makes rather than by trait method. Nobody
/// has ever wanted `move_to` without `resize_to`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Answers {
    /// There are windows to name: `windows`, `active_window`, `own_window`, and
    /// the `id`, `title` and `class` on each.
    pub windows: bool,
    /// `Window::rect` and `Window::monitor` are measured rather than left at
    /// zero. Without it an anchor built on a rectangle drags a whole recording
    /// into the top-left corner at a fifth of its scale, and reports success.
    pub geometry: bool,
    /// `Window::pid` is a real process. Without it `By::Process`, `By::Path`,
    /// `{process.name}` and AT-SPI's frame matching have nothing to look up -
    /// and a pid of zero matches nothing, which is the good half; the bad half
    /// is that it also fails to match *us*, so our own window starts passing as
    /// the foreign one in front.
    pub process: bool,
    /// `cursor_pos`.
    pub pointer: bool,
    /// The whole workspace picture: `focused_workspace`, `visible_workspaces`
    /// **and** `Window::workspace_id` / `workspace_name`. One flag on purpose.
    /// The only question anybody asks is whether our own window is on a
    /// workspace in view, and half an answer to that is a pause nobody can lift:
    /// a real workspace id compared against a defaulted zero is `false` forever,
    /// which stops recording and playback with no error anywhere.
    pub workspaces: bool,
    /// `focus`, `close`, `set_fullscreen`: acting on a window where it already
    /// is.
    pub steering: bool,
    /// `move_to`, `resize_to`, `center`, `move_to_workspace`,
    /// `move_to_workspace_id`: putting a window somewhere it is not.
    pub placing: bool,
}

impl Answers {
    /// A backend that really can do all of it.
    pub const EVERYTHING: Self = Self {
        windows: true,
        geometry: true,
        process: true,
        pointer: true,
        workspaces: true,
        steering: true,
        placing: true,
    };

    /// Each field with the words `--doctor` and the log put beside it.
    const NAMED: [(fn(&Self) -> bool, &'static str); 7] = [
        (|a| a.windows, "list the windows"),
        (|a| a.geometry, "measure them"),
        (|a| a.process, "name the process behind one"),
        (|a| a.pointer, "find the pointer"),
        (|a| a.workspaces, "say which workspace one is on"),
        (|a| a.steering, "focus, close and fullscreen one"),
        (|a| a.placing, "move, resize, centre and park one"),
    ];

    /// Whether this backend answers anything at all - the one tick `--doctor`
    /// puts beside the backend's name, and the whole of what `supported()` ever
    /// meant. Not a licence to skip the other fields.
    pub fn any(&self) -> bool {
        *self != Self::default()
    }

    fn list(&self, want: bool) -> String {
        Self::NAMED
            .iter()
            .filter(|(get, _)| get(self) == want)
            .map(|(_, what)| *what)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// What it can do.
    pub fn can(&self) -> String {
        self.list(true)
    }

    /// What it cannot, which is the half that explains a feature going quiet.
    pub fn cannot(&self) -> String {
        self.list(false)
    }
}

/// What the window and cursor half of `super::platform` needs from a session.
///
/// Every question may go unanswered, and an unanswered question is not an error:
/// a backend that cannot do something says so, rather than returning the most
/// plausible-looking value it can think of.
pub trait WindowBackend: Send + Sync {
    /// For the log and `--doctor`.
    fn name(&self) -> &'static str;

    /// What this backend can answer. Callers with a sensible fallback - workspace
    /// isolation, hide-to-tray, anything that reads a rectangle or a pid - pick it
    /// by asking this, instead of guessing from an empty answer what an empty
    /// answer meant.
    fn answers(&self) -> Answers;

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
    static WLROOTS: super::wlr::Wlroots = super::wlr::Wlroots;
    static NONE: Unsupported = Unsupported;
    // Hyprland first, and not only because it came first: its IPC answers every
    // question here, where the portable protocol answers four of them. A session
    // that has both should be asked the one that knows more.
    if hypr::available() {
        tracing::info!("window backend: {}", HYPRLAND.name());
        return &HYPRLAND;
    }
    if super::wlr::available() {
        tracing::info!(
            "window backend: {} - can {}; cannot {}",
            WLROOTS.name(),
            WLROOTS.answers().can(),
            WLROOTS.answers().cannot()
        );
        return &WLROOTS;
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

    fn answers(&self) -> Answers {
        Answers::EVERYTHING
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

    fn answers(&self) -> Answers {
        Answers::default()
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
