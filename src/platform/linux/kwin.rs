//! Windows on KWin, through `org_kde_plasma_window_management`.
//!
//! KWin implements none of the wlroots window protocols - no foreign-toplevel of
//! either flavour, no screencopy - so `super::wlr` can never serve a KDE session.
//! What it has instead is richer than either: this protocol carries the title, the
//! app id, the **pid**, the **geometry** and the **virtual desktop**, which is
//! three answers more than the portable one and enough to fill a `Window`
//! completely. A KDE session therefore gets window anchoring and process matching,
//! which a sway session does not.
//!
//! **It is not advertised to ordinary clients, and that is the first thing to know
//! about it.** KWin keeps a blacklist of five interfaces and hands them only to a
//! client whose executable path resolves to a desktop file naming them in
//! `X-KDE-Wayland-Interfaces`. Two consequences follow. The installed program gets
//! the protocol because the packages ship such a file; a binary run out of
//! `target/` does not, because its path matches no desktop file, so this backend
//! simply is not there during development. And the key is read through `KService`,
//! which splits on **commas** rather than on the semicolons a desktop file
//! otherwise uses - written with semicolons the whole line arrives as one
//! unrecognised string and the gate stays shut with no error anywhere.
//! `super::doctor` can tell a refusal from an absence, because
//! `org_kde_plasma_virtual_desktop_management` is *not* blacklisted: seeing that
//! without this is a KDE session that said no.
//!
//! The shape differs from `super::wlr` in three ways that each change the code.
//! A window is announced by uuid rather than by handing over an object, so it is
//! fetched in a second step and its events land on the following roundtrip - hence
//! three roundtrips to open rather than two, and no `event_created_child!`
//! anywhere, because no event here carries a new object. And `initial_state`
//! is a batch boundary that arrives **once**: before it, events are accumulated;
//! after it there is no further boundary of any kind, so they have to be applied
//! as they come. Buffering them the way the wlroots backend does would leave every
//! title frozen at the value it had when the window opened.

use super::backend::{Answers, Window, WindowBackend};
use parking_lot::Mutex;
use std::collections::HashMap;
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_protocols_plasma::plasma_virtual_desktop::client::{
    org_kde_plasma_virtual_desktop as vd, org_kde_plasma_virtual_desktop_management as vdm,
};
use wayland_protocols_plasma::plasma_window_management::client::{
    org_kde_plasma_window as win, org_kde_plasma_window_management as wm,
};

/// The state mask, from the protocol's own enum. Only the bits this program has
/// a use for; the other fourteen say things like "is resizable".
const ACTIVE: u32 = 0x1;
const MINIMIZED: u32 = 0x2;
const FULLSCREEN: u32 = 0x8;

/// One window as KWin describes it.
#[derive(Clone, Default)]
struct Plasma {
    /// KWin's own uuid, and a better identity than anything the wlroots protocol
    /// offers: it is a string the compositor minted, not an object id that gets
    /// recycled once a window closes.
    uuid: String,
    handle: Option<win::OrgKdePlasmaWindow>,
    title: String,
    app_id: String,
    pid: u32,
    /// The frame rectangle, logical pixels, in the global coordinate space.
    frame: (i32, i32, i32, i32),
    /// The whole mask from `state_changed`, which replaces the previous one.
    state: u32,
    /// Desktop uuids. **An empty list means every desktop**, never none - a
    /// window pinned to all of them leaves the list empty.
    desktops: Vec<String>,
    /// Whether `initial_state` has arrived. Before it events are accumulated in
    /// `draft`; after it they are applied directly, because no second boundary
    /// is ever sent.
    settled: bool,
    draft: Option<Box<Plasma>>,
}

impl Plasma {
    /// Where an incoming event should be written.
    fn slot(&mut self) -> &mut Plasma {
        if self.settled {
            return self;
        }
        if self.draft.is_none() {
            let mut d = self.clone();
            d.draft = None;
            self.draft = Some(Box::new(d));
        }
        self.draft.as_mut().expect("just filled")
    }

    fn commit(&mut self) {
        if let Some(d) = self.draft.take() {
            let uuid = std::mem::take(&mut self.uuid);
            let handle = self.handle.clone();
            *self = *d;
            self.uuid = uuid;
            self.handle = handle;
        }
        self.settled = true;
    }
}

#[derive(Clone, Default)]
struct Desk {
    name: String,
    position: u32,
    active: bool,
}

struct State {
    wins: Vec<Plasma>,
    desks: HashMap<String, Desk>,
    /// The desktop object a `vd` event arrived on, so it can be attributed.
    vd_of: HashMap<vd::OrgKdePlasmaVirtualDesktop, String>,
}

impl State {
    fn find(&mut self, h: &win::OrgKdePlasmaWindow) -> Option<&mut Plasma> {
        self.wins.iter_mut().find(|w| w.handle.as_ref() == Some(h))
    }

    /// A workspace id the rest of the program can compare. KWin's own X11
    /// numbering is the desktop's position plus one, so that is what is used
    /// rather than inventing a second scheme.
    fn ws_id(&self, uuid: &str) -> i64 {
        self.desks.get(uuid).map(|d| d.position as i64 + 1).unwrap_or(0)
    }

    fn ws_name(&self, uuid: &str) -> String {
        self.desks.get(uuid).map(|d| d.name.clone()).unwrap_or_default()
    }

    fn active_desk(&self) -> Option<(&String, &Desk)> {
        self.desks.iter().find(|(_, d)| d.active)
    }

    fn as_window(&self, p: &Plasma) -> Window {
        // An empty desktop list means the window is on all of them, never on
        // none - so the honest answer to "which workspace is it on" is then the
        // one in view, and a window pinned everywhere never counts as away.
        let uuid = match p.desktops.first() {
            Some(u) => u.clone(),
            None => self.active_desk().map(|(id, _)| id.clone()).unwrap_or_default(),
        };
        Window {
            id: p.uuid.clone(),
            title: p.title.clone(),
            class: p.app_id.clone(),
            pid: p.pid as i64,
            rect: p.frame,
            monitor: 0,
            workspace_id: self.ws_id(&uuid),
            workspace_name: self.ws_name(&uuid),
            fullscreen: p.state & FULLSCREEN != 0,
        }
    }
}

struct Live {
    queue: EventQueue<State>,
    state: State,
}

impl Live {
    fn open() -> Option<Self> {
        let conn = Connection::connect_to_env().ok()?;
        let (globals, mut queue) =
            wayland_client::globals::registry_queue_init::<State>(&conn).ok()?;
        let qh = queue.handle();

        // 13 is where `window_with_uuid` arrives, and 18 is as high as the
        // bindings go - KWin offers 20, and binding above what the crate knows
        // panics rather than negotiating down. Asking for less than 13 would
        // leave only the deprecated integer-id announcement.
        let mgr = globals
            .bind::<wm::OrgKdePlasmaWindowManagement, State, ()>(&qh, 13..=18, ())
            .ok()?;
        // Not blacklisted by KWin, so this one binds even where the manager
        // above was refused. It is bound after the manager on purpose: there is
        // no point in a desktop list with no windows to put on it.
        let _vdm = globals
            .bind::<vdm::OrgKdePlasmaVirtualDesktopManagement, State, ()>(&qh, 1..=2, ())
            .ok();
        let _ = mgr;

        let mut state = State { wins: Vec::new(), desks: HashMap::new(), vd_of: HashMap::new() };
        // Three, not two. The first brings the uuid announcements, which ask for
        // a window object each; the second brings those objects' events; the
        // third catches the virtual-desktop objects requested along the way.
        for _ in 0..3 {
            queue.roundtrip(&mut state).ok()?;
        }
        Some(Live { queue, state })
    }

    fn refresh(&mut self) {
        let _ = self.queue.roundtrip(&mut self.state);
    }
}

static LIVE: Mutex<Option<Option<Live>>> = Mutex::new(None);

/// Is there a KWin window protocol on this session, and were we allowed it?
pub fn available() -> bool {
    with(|_| ()).is_some()
}

/// True when this looks like KWin but the protocol was withheld.
///
/// `org_kde_plasma_virtual_desktop_management` is not on KWin's blacklist, so a
/// session offering that and not the window manager is one that refused us
/// rather than one that never had it. Worth telling apart: the first is a
/// missing line in a desktop file, the second is not KDE at all.
pub fn refused() -> bool {
    let names = super::wl::globals();
    names.iter().any(|n| n == "org_kde_plasma_virtual_desktop_management")
        && !names.iter().any(|n| n == "org_kde_plasma_window_management")
}

fn with<T>(f: impl FnOnce(&mut Live) -> T) -> Option<T> {
    let mut slot = LIVE.lock();
    if slot.is_none() {
        let live = Live::open();
        if live.is_some() {
            tracing::info!("KWin plasma-window-management backend is available");
        } else if refused() {
            tracing::warn!(
                "this is a KWin session and it withheld org_kde_plasma_window_management: \
                 the running binary's path matches no desktop file carrying \
                 X-KDE-Wayland-Interfaces, which is expected when running from a build \
                 directory rather than from an installed package"
            );
        }
        *slot = Some(live);
    }
    let live = slot.as_mut().expect("just filled").as_mut()?;
    live.refresh();
    Some(f(live))
}

// ---- the backend ------------------------------------------------------------

pub struct Kwin;

impl Kwin {
    fn act(&self, w: &Window, f: impl FnOnce(&win::OrgKdePlasmaWindow)) -> bool {
        with(|live| {
            let Some(h) = live
                .state
                .wins
                .iter()
                .find(|p| p.uuid == w.id)
                .and_then(|p| p.handle.clone())
            else {
                return false;
            };
            f(&h);
            let _ = live.queue.flush();
            true
        })
        .unwrap_or(false)
    }
}

impl WindowBackend for Kwin {
    fn name(&self) -> &'static str {
        "KWin"
    }

    fn answers(&self) -> Answers {
        Answers {
            windows: true,
            // Both of these are why a KDE session is worth a backend of its own:
            // the portable protocol carries neither, so window anchoring and
            // matching a window to a process work here and not on sway.
            geometry: true,
            process: true,
            // Virtual desktops, and - unlike every wlroots protocol - the
            // protocol says which one a window is on, so the question
            // `super::vdesk` asks has an answer.
            workspaces: true,
            steering: true,
            // No pointer position: Wayland has no such question for a client and
            // KWin adds none.
            pointer: false,
            // No move, no resize, no centre. The protocol can put a window on
            // another virtual desktop but nowhere on the screen, and `placing`
            // is one flag covering both - so it is false, and the two requests
            // that would answer half of it are left unimplemented rather than
            // half-declared.
            placing: false,
        }
    }

    fn windows(&self) -> Vec<Window> {
        with(|live| {
            live.state
                .wins
                .iter()
                .filter(|p| p.settled && (!p.title.is_empty() || !p.app_id.is_empty()))
                .map(|p| live.state.as_window(p))
                .collect()
        })
        .unwrap_or_default()
    }

    fn active_window(&self) -> Option<Window> {
        with(|live| {
            live.state
                .wins
                .iter()
                .find(|p| p.settled && p.state & ACTIVE != 0)
                .map(|p| live.state.as_window(p))
        })
        .flatten()
    }

    fn own_window(&self) -> Option<Window> {
        with(|live| {
            let mut mine: Vec<&Plasma> = live
                .state
                .wins
                .iter()
                .filter(|p| p.settled && p.app_id == crate::APP_ID)
                .collect();
            // The main window before the handbook, the same ordering
            // `hypr::own_window` uses.
            mine.sort_by_key(|p| (p.title != crate::APP_TITLE, p.uuid.clone()));
            mine.first().map(|p| live.state.as_window(p))
        })
        .flatten()
    }

    fn cursor_pos(&self) -> Option<(i32, i32)> {
        None
    }

    fn focused_workspace(&self) -> Option<i64> {
        with(|live| live.state.active_desk().map(|(id, _)| live.state.ws_id(id))).flatten()
    }

    fn visible_workspaces(&self) -> Vec<i64> {
        // One at a time on KDE: a virtual desktop is not a per-output thing the
        // way a wlroots workspace is, so there is exactly one in view.
        with(|live| {
            live.state
                .active_desk()
                .map(|(id, _)| vec![live.state.ws_id(id)])
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn focus(&self, w: &Window) -> bool {
        // `set_state` speaks in two masks: which bits are being talked about,
        // and what they should become. Nothing comes back - the request has no
        // reply - so success here means the request was sent to a window that
        // still exists, and the state event that follows is the real answer.
        self.act(w, |h| h.set_state(ACTIVE, ACTIVE))
    }

    fn close(&self, w: &Window) -> bool {
        self.act(w, |h| h.close())
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
        self.act(w, |h| h.set_state(FULLSCREEN, if on { FULLSCREEN } else { 0 }))
    }

    fn move_to_workspace(&self, _: &Window, _: &str, _: bool) -> bool {
        false
    }

    fn move_to_workspace_id(&self, _: &Window, _: i64, _: bool) -> bool {
        false
    }
}

// ---- the event queue --------------------------------------------------------

impl Dispatch<wm::OrgKdePlasmaWindowManagement, ()> for State {
    fn event(
        st: &mut Self,
        mgr: &wm::OrgKdePlasmaWindowManagement,
        ev: wm::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        // No `event_created_child!` in this file, and none is missing: not one
        // event here carries a new object. A window arrives as a uuid and is
        // fetched with a request, which is the opposite arrangement from the
        // wlroots protocol.
        if let wm::Event::WindowWithUuid { uuid, .. } = ev {
            if st.wins.iter().any(|w| w.uuid == uuid) {
                return;
            }
            let h = mgr.get_window_by_uuid(uuid.clone(), qh, ());
            st.wins.push(Plasma { uuid, handle: Some(h), ..Default::default() });
        }
    }
}

impl Dispatch<win::OrgKdePlasmaWindow, ()> for State {
    fn event(
        st: &mut Self,
        h: &win::OrgKdePlasmaWindow,
        ev: win::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match ev {
            win::Event::TitleChanged { title } => {
                if let Some(w) = st.find(h) {
                    w.slot().title = title;
                }
            }
            win::Event::AppIdChanged { app_id } => {
                if let Some(w) = st.find(h) {
                    w.slot().app_id = app_id;
                }
            }
            win::Event::PidChanged { pid } => {
                if let Some(w) = st.find(h) {
                    w.slot().pid = pid;
                }
            }
            win::Event::StateChanged { flags } => {
                if let Some(w) = st.find(h) {
                    w.slot().state = flags;
                }
            }
            win::Event::Geometry { x, y, width, height } => {
                if let Some(w) = st.find(h) {
                    w.slot().frame = (x, y, width as i32, height as i32);
                }
            }
            win::Event::VirtualDesktopEntered { id } => {
                if let Some(w) = st.find(h) {
                    let d = w.slot();
                    if !d.desktops.contains(&id) {
                        d.desktops.push(id);
                    }
                }
            }
            // The argument really is named `is`: a typo in the protocol's own
            // XML, reproduced faithfully by the scanner.
            win::Event::VirtualDesktopLeft { is } => {
                if let Some(w) = st.find(h) {
                    w.slot().desktops.retain(|d| *d != is);
                }
            }
            win::Event::InitialState => {
                if let Some(w) = st.find(h) {
                    w.commit();
                }
            }
            win::Event::Unmapped => {
                if let Some(i) = st.wins.iter().position(|w| w.handle.as_ref() == Some(h)) {
                    if let Some(hh) = st.wins[i].handle.take() {
                        hh.destroy();
                    }
                    st.wins.remove(i);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<vdm::OrgKdePlasmaVirtualDesktopManagement, ()> for State {
    fn event(
        st: &mut Self,
        m: &vdm::OrgKdePlasmaVirtualDesktopManagement,
        ev: vdm::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match ev {
            vdm::Event::DesktopCreated { desktop_id, position } => {
                st.desks.entry(desktop_id.clone()).or_default().position = position;
                let obj = m.get_virtual_desktop(desktop_id.clone(), qh, ());
                st.vd_of.insert(obj, desktop_id);
            }
            vdm::Event::DesktopRemoved { desktop_id } => {
                st.desks.remove(&desktop_id);
                st.vd_of.retain(|_, v| *v != desktop_id);
            }
            _ => {}
        }
    }
}

impl Dispatch<vd::OrgKdePlasmaVirtualDesktop, ()> for State {
    fn event(
        st: &mut Self,
        o: &vd::OrgKdePlasmaVirtualDesktop,
        ev: vd::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(id) = st.vd_of.get(o).cloned() else { return };
        match ev {
            vd::Event::Name { name } => {
                st.desks.entry(id).or_default().name = name;
            }
            vd::Event::Activated => {
                // Exactly one at a time, so the others are cleared rather than
                // left to drift: KWin sends `deactivated` too, but relying on
                // both arriving is how two desktops come to look active at once.
                for (k, d) in st.desks.iter_mut() {
                    d.active = *k == id;
                }
            }
            vd::Event::Deactivated => {
                st.desks.entry(id).or_default().active = false;
            }
            _ => {}
        }
    }
}

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
