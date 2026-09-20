//! Windows on a wlroots compositor that is not Hyprland.
//!
//! `zwlr_foreign_toplevel_management_v1` is the one window protocol the wlroots
//! family agrees on. It names every toplevel with a title, an app id and a state,
//! and it can activate, close and fullscreen one. That is enough to list the
//! windows, say which is in front, find our own, and steer it - which is most of
//! what a macro recorder asks about windows and is four questions more than a
//! session with no backend at all can answer.
//!
//! What it cannot do is the reason `super::backend::Answers` exists. The protocol
//! carries **no geometry, no pid and no workspace**, and it has no move or resize
//! request; `set_rectangle` is a hint about where a taskbar would draw a minimise
//! animation, takes our own surface, and reads back nothing. So a window from here
//! has a real title and a zero rectangle, and the capability set is what keeps the
//! callers above from mistaking the second for a measurement.
//!
//! **Only the wlr protocol is bound, deliberately.** `ext_foreign_toplevel_list_v1`
//! is read-only - it has no requests at all - and there is no field anywhere that
//! joins one of its handles to a wlr handle, so the two lists cannot be merged:
//! matching on title and app id breaks the moment two terminals are open. The ext
//! list also carries less than the wlr one, having no state, so it cannot even name
//! the active window. It is worth binding later for per-window capture, whose
//! source manager takes an ext handle and nothing else, and those handles should
//! stay inside `super::capture` as capture tokens rather than becoming windows.
//! The wlr-only choice has a second dividend: it is the half that CI's older sway
//! also carries.
//!
//! The connection is opened once and kept for the life of the process. A toplevel
//! handle is a Wayland object on one connection, so a backend that reconnected per
//! question would be handed a `Window` naming handles that no longer exist - and
//! worse, a request on a proxy whose connection has gone is not an error, it is a
//! silent return. One connection, one queue, one map, behind one lock.

use super::backend::{Answers, Window, WindowBackend};
use parking_lot::Mutex;
use wayland_client::protocol::{wl_output, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1 as handle, zwlr_foreign_toplevel_manager_v1 as manager,
};

/// What is known about one toplevel, and the handle to act on it with.
#[derive(Clone)]
struct Toplevel {
    /// Ours, not Wayland's. Wayland recycles object ids once an object is
    /// destroyed, so a `Window` captured before a close and compared after one
    /// could name a different window entirely. A counter never reused cannot.
    id: u64,
    handle: handle::ZwlrForeignToplevelHandleV1,
    title: String,
    app_id: String,
    activated: bool,
    fullscreen: bool,
    /// Filled from the events as they arrive and swapped into the fields above
    /// only when `done` says the batch is complete.
    pending: Option<Box<Toplevel>>,
}

impl Toplevel {
    fn new(id: u64, h: handle::ZwlrForeignToplevelHandleV1) -> Self {
        Self {
            id,
            handle: h,
            title: String::new(),
            app_id: String::new(),
            activated: false,
            fullscreen: false,
            pending: None,
        }
    }

    /// The scratch copy events are written into before `done`.
    fn draft(&mut self) -> &mut Toplevel {
        if self.pending.is_none() {
            let mut d = self.clone();
            d.pending = None;
            self.pending = Some(Box::new(d));
        }
        self.pending.as_mut().expect("just filled")
    }

    fn commit(&mut self) {
        if let Some(d) = self.pending.take() {
            self.title = d.title;
            self.app_id = d.app_id;
            self.activated = d.activated;
            self.fullscreen = d.fullscreen;
        }
    }

    fn as_window(&self) -> Window {
        Window {
            id: self.id.to_string(),
            title: self.title.clone(),
            class: self.app_id.clone(),
            fullscreen: self.fullscreen,
            // pid, rect, monitor, workspace_id and workspace_name stay at their
            // defaults because nothing here can measure them. `Answers::geometry`
            // and `Answers::process` are false for exactly this reason, and that
            // is what makes the zeroes honest rather than a lie told in numbers.
            ..Default::default()
        }
    }
}

/// Everything the event queue writes into.
struct State {
    seat: Option<wl_seat::WlSeat>,
    /// Version the manager was bound at. `set_fullscreen` and `unset_fullscreen`
    /// arrived in version 2, and sending a request an object is too old for does
    /// not fail politely: the compositor hangs up and every later request on the
    /// connection becomes a silent no-op. Nothing in `wayland-backend` checks this
    /// for us - it validates child-object versions only - so it is checked here.
    version: u32,
    tops: Vec<Toplevel>,
    next_id: u64,
}

impl State {
    fn find_mut(&mut self, h: &handle::ZwlrForeignToplevelHandleV1) -> Option<&mut Toplevel> {
        self.tops.iter_mut().find(|t| &t.handle == h)
    }
}

/// The connection, its queue and its state, kept together because none of the
/// three is any use without the others.
///
/// The connection is in here even though no field names it: `EventQueue` owns a
/// `Connection` of its own (`wayland-client`'s `event_queue.rs`), so holding the
/// queue is what keeps the socket open. That matters more than it looks. Every
/// generated request begins by upgrading a weak reference to the backend and
/// returns quietly if it has gone, so a dropped connection does not produce an
/// error anywhere - it turns `activate` and `close` into functions that do
/// nothing and say nothing.
struct Live {
    queue: EventQueue<State>,
    state: State,
}

impl Live {
    fn open() -> Option<Self> {
        let conn = Connection::connect_to_env().ok()?;
        let (globals, mut queue) = wayland_client::globals::registry_queue_init::<State>(&conn).ok()?;
        let qh = queue.handle();

        // 1..=3: version 3 adds the `parent` event, which is ignored here, but
        // binding as high as the compositor offers is what makes `set_fullscreen`
        // reachable at all. The lower bound is 1 so a compositor with only the
        // first version still gets a window list.
        let mgr = globals.bind::<manager::ZwlrForeignToplevelManagerV1, State, ()>(&qh, 1..=3, ()).ok()?;
        let version = mgr.version();
        let seat = globals.bind::<wl_seat::WlSeat, State, ()>(&qh, 1..=9, ()).ok();

        let mut state = State { seat, version, tops: Vec::new(), next_id: 1 };
        // Two trips: the first brings the `toplevel` events, the second the
        // title/app_id/state batch each of them sends straight afterwards.
        queue.roundtrip(&mut state).ok()?;
        queue.roundtrip(&mut state).ok()?;
        Some(Live { queue, state })
    }

    /// Catches up with whatever the compositor has said since the last question.
    fn refresh(&mut self) {
        let _ = self.queue.roundtrip(&mut self.state);
    }
}

static LIVE: Mutex<Option<Option<Live>>> = Mutex::new(None);

/// Is there a wlroots window protocol on this session at all?
///
/// Opens the connection on the first call and keeps it. `Some(None)` is the
/// remembered answer "asked, and there is not", so a session without the protocol
/// does not reconnect on every question.
pub fn available() -> bool {
    with(|_| ()).is_some()
}

fn with<T>(f: impl FnOnce(&mut Live) -> T) -> Option<T> {
    let mut slot = LIVE.lock();
    if slot.is_none() {
        let live = Live::open();
        if live.is_some() {
            tracing::info!("wlroots foreign-toplevel backend is available");
        }
        *slot = Some(live);
    }
    let live = slot.as_mut().expect("just filled").as_mut()?;
    live.refresh();
    Some(f(live))
}

// ---- the backend ------------------------------------------------------------

pub struct Wlroots;

impl Wlroots {
    /// Runs `f` against the live handle a `Window` names. `false` when the window
    /// has gone, which is a different answer from "this session cannot do that" -
    /// the second is `Answers`, and the callers above tell them apart.
    fn act(&self, w: &Window, f: impl FnOnce(&State, &handle::ZwlrForeignToplevelHandleV1)) -> bool {
        with(|live| {
            let Some(t) = live.state.tops.iter().find(|t| t.id.to_string() == w.id) else {
                return false;
            };
            let h = t.handle.clone();
            f(&live.state, &h);
            // The request is only queued until something flushes it, and the next
            // question might be a long time coming.
            let _ = live.queue.flush();
            true
        })
        .unwrap_or(false)
    }
}

impl WindowBackend for Wlroots {
    fn name(&self) -> &'static str {
        "wlroots"
    }

    fn answers(&self) -> Answers {
        Answers {
            windows: true,
            steering: true,
            // Not geometry and not process: the protocol carries neither a
            // rectangle nor a pid, and there is no second protocol to ask.
            //
            // Not workspaces, which is the surprising one. `ext_workspace_manager_v1`
            // lists workspaces and says which are on screen, and this session may
            // well have it - but nothing in any wlroots protocol says which
            // workspace a toplevel is on, and that is the only workspace question
            // this program asks. Half of that answer is worse than none: it would
            // compare a real workspace id against a defaulted zero, decide our
            // window is never in view, and stop recording and playback for good.
            //
            // Not placing: no move, no resize, and `set_rectangle` is a taskbar
            // hint rather than a request to put a window anywhere.
            ..Answers::default()
        }
    }

    fn windows(&self) -> Vec<Window> {
        with(|live| {
            live.state
                .tops
                .iter()
                // A toplevel with neither a title nor an app id has not finished
                // announcing itself; it is not something somebody could click yet.
                .filter(|t| !t.title.is_empty() || !t.app_id.is_empty())
                .map(|t| t.as_window())
                .collect()
        })
        .unwrap_or_default()
    }

    fn active_window(&self) -> Option<Window> {
        with(|live| live.state.tops.iter().find(|t| t.activated).map(|t| t.as_window())).flatten()
    }

    fn own_window(&self) -> Option<Window> {
        with(|live| {
            // By app id, and then the same ordering `hypr::own_window` uses: the
            // window whose title is the application's own, so an open handbook
            // cannot answer for the main window. egui gives child viewports an
            // empty app id today, which would be enough on its own - but that is
            // an accident of how eframe builds them, not a promise.
            let mut mine: Vec<&Toplevel> =
                live.state.tops.iter().filter(|t| t.app_id == crate::APP_ID).collect();
            mine.sort_by_key(|t| (t.title != crate::APP_TITLE, t.id));
            mine.first().map(|t| t.as_window())
        })
        .flatten()
    }

    fn cursor_pos(&self) -> Option<(i32, i32)> {
        // Wayland tells a client where the pointer is over its own surfaces and
        // nowhere else. There is no global answer to ask for.
        None
    }

    fn focused_workspace(&self) -> Option<i64> {
        None
    }

    fn visible_workspaces(&self) -> Vec<i64> {
        Vec::new()
    }

    fn focus(&self, w: &Window) -> bool {
        self.act(w, |st, h| {
            if let Some(seat) = st.seat.as_ref() {
                // Measured on Hyprland and on sway: this works from a process that
                // has never held input focus, takes no serial, and pulls a window
                // off a hidden workspace or out of sway's scratchpad into view.
                h.activate(seat);
            }
        })
    }

    fn close(&self, w: &Window) -> bool {
        self.act(w, |_, h| h.close())
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

    fn set_fullscreen(&self, w: &Window, on: bool) -> bool {
        self.act(w, |st, h| {
            // Version 2 or the connection dies. See `State::version`.
            if st.version < 2 {
                return;
            }
            if on {
                h.set_fullscreen(None);
            } else {
                h.unset_fullscreen();
            }
        }) && with(|live| live.state.version >= 2).unwrap_or(false)
    }

    fn move_to_workspace(&self, _: &Window, _: &str, _: bool) -> bool {
        false
    }

    fn move_to_workspace_id(&self, _: &Window, _: i64, _: bool) -> bool {
        false
    }
}

// ---- the event queue --------------------------------------------------------

impl Dispatch<manager::ZwlrForeignToplevelManagerV1, ()> for State {
    fn event(
        st: &mut Self,
        _: &manager::ZwlrForeignToplevelManagerV1,
        ev: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let manager::Event::Toplevel { toplevel } = ev {
            let id = st.next_id;
            st.next_id += 1;
            st.tops.push(Toplevel::new(id, toplevel));
        }
    }

    // Mandatory, and its absence is a panic rather than a compile error: the
    // `toplevel` event carries a new object, and without this the queue reaches
    // the default body, which panics the first time a window exists. A session
    // with no windows at all would look perfectly healthy.
    wayland_client::event_created_child!(State, manager::ZwlrForeignToplevelManagerV1, [
        manager::EVT_TOPLEVEL_OPCODE => (handle::ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<handle::ZwlrForeignToplevelHandleV1, ()> for State {
    fn event(
        st: &mut Self,
        h: &handle::ZwlrForeignToplevelHandleV1,
        ev: handle::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match ev {
            handle::Event::Title { title } => {
                if let Some(t) = st.find_mut(h) {
                    t.draft().title = title;
                }
            }
            handle::Event::AppId { app_id } => {
                if let Some(t) = st.find_mut(h) {
                    t.draft().app_id = app_id;
                }
            }
            handle::Event::State { state } => {
                // A wl_array of u32 in the machine's own byte order, and the whole
                // state rather than what changed: a set that no longer contains
                // `activated` is a window that has just lost the focus, not a
                // window whose focus went unmentioned.
                let mut activated = false;
                let mut fullscreen = false;
                for c in state.chunks_exact(4) {
                    let v = u32::from_ne_bytes([c[0], c[1], c[2], c[3]]);
                    match handle::State::try_from(v) {
                        Ok(handle::State::Activated) => activated = true,
                        Ok(handle::State::Fullscreen) => fullscreen = true,
                        // Maximized, Minimized, and anything a later version adds.
                        _ => {}
                    }
                }
                if let Some(t) = st.find_mut(h) {
                    let d = t.draft();
                    d.activated = activated;
                    d.fullscreen = fullscreen;
                }
            }
            // The only event that means anything has settled. sway sends two of
            // these while first listing the windows where Hyprland sends one, so
            // the count says nothing; each one means "apply what has arrived".
            handle::Event::Done => {
                if let Some(t) = st.find_mut(h) {
                    t.commit();
                }
            }
            handle::Event::Closed => {
                if let Some(i) = st.tops.iter().position(|t| &t.handle == h) {
                    // The client finishes the destruction the compositor began.
                    st.tops[i].handle.destroy();
                    st.tops.remove(i);
                }
            }
            _ => {}
        }
    }
}

delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore wl_output::WlOutput);

impl Dispatch<wl_registry::WlRegistry, wayland_client::globals::GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &wayland_client::globals::GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
