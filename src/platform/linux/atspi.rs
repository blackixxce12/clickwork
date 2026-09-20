//! Asking the desktop what is on screen, through AT-SPI2.
//!
//! The Linux answer to UI Automation is the accessibility bus: every GTK and Qt
//! application, and most Electron ones, describe their controls over D-Bus for
//! screen readers, and a macro can ask the same questions - a button by its name,
//! a field by its role - and press it through the application's own `Action`.
//! Games and anything drawing its own interface expose nothing, which is the
//! same limitation as on Windows and the reason this is one rung of a cascade.
//!
//! Coordinates are the awkward part. A Wayland application does not know where
//! its window is, so it reports positions relative to its own top-level frame;
//! the frame's position comes from Hyprland, and the two are added here.

use crate::uia::{Found, Query, control_id};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

/// A reference to one accessible object: the bus name of its application and
/// its object path.
pub type Elem = (String, OwnedObjectPath);

const IFACE_ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const IFACE_COMPONENT: &str = "org.a11y.atspi.Component";
const IFACE_ACTION: &str = "org.a11y.atspi.Action";
const IFACE_TEXT: &str = "org.a11y.atspi.Text";
const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";
const REGISTRY: &str = "org.a11y.atspi.Registry";

/// Coordinates relative to the top-level window, per `AtspiCoordType`.
const COORD_WINDOW: u32 = 1;

// Roles, from at-spi2-core's `AtspiRole`.
const ROLE_CHECK_BOX: u32 = 7;
const ROLE_CHECK_MENU_ITEM: u32 = 8;
const ROLE_COMBO_BOX: u32 = 11;
const ROLE_DIALOG: u32 = 16;
const ROLE_FRAME: u32 = 23;
const ROLE_LABEL: u32 = 29;
const ROLE_LIST: u32 = 31;
const ROLE_LIST_ITEM: u32 = 32;
const ROLE_MENU_ITEM: u32 = 35;
const ROLE_PAGE_TAB: u32 = 37;
const ROLE_PASSWORD_TEXT: u32 = 40;
const ROLE_PUSH_BUTTON: u32 = 43;
const ROLE_RADIO_MENU_ITEM: u32 = 45;
const ROLE_TABLE: u32 = 55;
const ROLE_TABLE_CELL: u32 = 56;
const ROLE_TEAROFF_MENU_ITEM: u32 = 59;
const ROLE_TERMINAL: u32 = 60;
const ROLE_TEXT: u32 = 61;
const ROLE_TOGGLE_BUTTON: u32 = 62;
const ROLE_TREE: u32 = 65;
const ROLE_WINDOW: u32 = 69;
const ROLE_PARAGRAPH: u32 = 73;
const ROLE_ENTRY: u32 = 79;
const ROLE_HEADING: u32 = 83;
const ROLE_LINK: u32 = 88;
const ROLE_TABLE_ROW: u32 = 90;
const ROLE_TREE_ITEM: u32 = 91;
const ROLE_DOCUMENT_TEXT: u32 = 94;
const ROLE_LIST_BOX: u32 = 98;
const ROLE_STATIC: u32 = 116;
const ROLE_PUSH_BUTTON_MENU: u32 = 129;

// States, bit numbers in the first word of `GetState`.
const STATE_ACTIVE: u32 = 1;
const STATE_SHOWING: u32 = 25;
const STATE_MANAGES_DESCENDANTS: u32 = 31;

/// The roles a picker control type stands for.
fn roles_of(control: &str) -> &'static [u32] {
    match control_id(control) {
        50000 => &[ROLE_PUSH_BUTTON, ROLE_TOGGLE_BUTTON, ROLE_PUSH_BUTTON_MENU, ROLE_LINK],
        50002 => &[ROLE_CHECK_BOX, ROLE_CHECK_MENU_ITEM],
        50003 => &[ROLE_COMBO_BOX],
        50004 => &[ROLE_TEXT, ROLE_ENTRY, ROLE_PASSWORD_TEXT, ROLE_TERMINAL, ROLE_DOCUMENT_TEXT],
        50008 => &[ROLE_LIST, ROLE_LIST_BOX, ROLE_TREE, ROLE_TABLE],
        50007 => &[ROLE_LIST_ITEM, ROLE_TREE_ITEM, ROLE_TABLE_CELL, ROLE_TABLE_ROW],
        50011 => &[ROLE_MENU_ITEM, ROLE_CHECK_MENU_ITEM, ROLE_RADIO_MENU_ITEM, ROLE_TEAROFF_MENU_ITEM],
        50018 => &[ROLE_PAGE_TAB],
        50020 => &[ROLE_LABEL, ROLE_STATIC, ROLE_HEADING, ROLE_PARAGRAPH],
        50032 => &[ROLE_FRAME, ROLE_WINDOW, ROLE_DIALOG],
        _ => &[],
    }
}

/// The picker name for a role, when a script would press or read it.
fn control_of_role(role: u32) -> &'static str {
    match role {
        ROLE_PUSH_BUTTON | ROLE_TOGGLE_BUTTON | ROLE_PUSH_BUTTON_MENU | ROLE_LINK => "Button",
        ROLE_CHECK_BOX => "CheckBox",
        ROLE_COMBO_BOX => "ComboBox",
        ROLE_TEXT | ROLE_ENTRY | ROLE_PASSWORD_TEXT | ROLE_TERMINAL | ROLE_DOCUMENT_TEXT => "Edit",
        ROLE_LIST | ROLE_LIST_BOX | ROLE_TREE | ROLE_TABLE => "List",
        ROLE_LIST_ITEM | ROLE_TREE_ITEM | ROLE_TABLE_CELL | ROLE_TABLE_ROW => "ListItem",
        ROLE_MENU_ITEM | ROLE_CHECK_MENU_ITEM | ROLE_RADIO_MENU_ITEM | ROLE_TEAROFF_MENU_ITEM => {
            "MenuItem"
        }
        ROLE_PAGE_TAB => "Tab",
        ROLE_LABEL | ROLE_STATIC | ROLE_HEADING => "Text",
        ROLE_FRAME | ROLE_WINDOW | ROLE_DIALOG => "Window",
        _ => "",
    }
}

static CONN: OnceLock<Option<Connection>> = OnceLock::new();

/// The accessibility bus, found through the session bus. Toolkits only start
/// describing themselves once something says accessibility is wanted, so that
/// flag is set on the way in.
fn connect() -> Option<Connection> {
    let session = Connection::session().ok()?;
    let bus = Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus").ok()?;
    let addr: String = bus.call("GetAddress", &()).ok()?;
    if let Ok(status) = Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Status") {
        let _ = status.set_property("IsEnabled", true);
    }
    let conn = zbus::blocking::connection::Builder::address(addr.as_str()).ok()?.build().ok()?;
    tracing::info!("accessibility bus at {addr}");
    Some(conn)
}

fn conn() -> Option<&'static Connection> {
    CONN.get_or_init(connect).as_ref()
}

fn proxy<'a>(c: &'a Connection, e: &Elem, iface: &'static str) -> Option<Proxy<'a>> {
    Proxy::new(c, e.0.clone(), e.1.clone(), iface).ok()
}

fn children(c: &Connection, e: &Elem) -> Vec<Elem> {
    proxy(c, e, IFACE_ACCESSIBLE)
        .and_then(|p| p.call::<_, _, Vec<(String, OwnedObjectPath)>>("GetChildren", &()).ok())
        .unwrap_or_default()
}

fn parent(c: &Connection, e: &Elem) -> Option<Elem> {
    let p = proxy(c, e, IFACE_ACCESSIBLE)?;
    let (name, path): (String, OwnedObjectPath) = p.get_property("Parent").ok()?;
    if path.as_str() == "/org/a11y/atspi/null" || name.is_empty() {
        return None;
    }
    Some((name, path))
}

fn name(c: &Connection, e: &Elem) -> String {
    proxy(c, e, IFACE_ACCESSIBLE)
        .and_then(|p| p.get_property::<String>("Name").ok())
        .unwrap_or_default()
}

fn accessible_id(c: &Connection, e: &Elem) -> String {
    let Some(p) = proxy(c, e, IFACE_ACCESSIBLE) else { return String::new() };
    if let Ok(id) = p.get_property::<String>("AccessibleId")
        && !id.is_empty()
    {
        return id;
    }
    // Web content and some toolkits put the identifier in the attributes.
    p.call::<_, _, HashMap<String, String>>("GetAttributes", &())
        .ok()
        .and_then(|m| m.get("id").cloned())
        .unwrap_or_default()
}

fn role(c: &Connection, e: &Elem) -> u32 {
    proxy(c, e, IFACE_ACCESSIBLE)
        .and_then(|p| p.call::<_, _, u32>("GetRole", &()).ok())
        .unwrap_or(0)
}

fn states(c: &Connection, e: &Elem) -> (u32, u32) {
    proxy(c, e, IFACE_ACCESSIBLE)
        .and_then(|p| p.call::<_, _, Vec<u32>>("GetState", &()).ok())
        .map(|v| (v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0)))
        .unwrap_or((0, 0))
}

fn has_state(st: (u32, u32), bit: u32) -> bool {
    if bit < 32 { st.0 & (1 << bit) != 0 } else { st.1 & (1 << (bit - 32)) != 0 }
}

/// Window-relative rectangle, logical pixels.
fn extents(c: &Connection, e: &Elem) -> Option<(i32, i32, i32, i32)> {
    let p = proxy(c, e, IFACE_COMPONENT)?;
    p.call::<_, _, (i32, i32, i32, i32)>("GetExtents", &(COORD_WINDOW,)).ok()
}

fn text_of(c: &Connection, e: &Elem) -> String {
    proxy(c, e, IFACE_TEXT)
        .and_then(|p| p.call::<_, _, String>("GetText", &(0i32, -1i32)).ok())
        .unwrap_or_default()
}

/// The process behind a bus name, from the bus itself.
fn pid_of_name(c: &Connection, bus_name: &str) -> Option<i64> {
    let dbus = Proxy::new(c, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus").ok()?;
    dbus.call::<_, _, u32>("GetConnectionUnixProcessID", &(bus_name,)).ok().map(|p| p as i64)
}

/// Every application on the bus.
fn applications(c: &Connection) -> Vec<Elem> {
    let root: Elem = (REGISTRY.to_string(), OwnedObjectPath::try_from(ROOT_PATH).expect("path"));
    children(c, &root)
}

/// A top-level frame of an application, and where Hyprland says it is (logical).
struct Frame {
    elem: Elem,
    x: i32,
    y: i32,
}

/// The frame to search from: the window in front, or every frame of every
/// application when `in_front` is off.
fn frames(c: &Connection, in_front: bool) -> Vec<Frame> {
    let win = super::backend::backend();
    let active = win.active_window();
    let clients = win.windows();
    // Matching an AT-SPI application to a window by pid only works where the
    // backend knows the pid. Where it does not, every window carries a defaulted
    // zero, which equals no real application's pid - so the filter below would
    // skip every application in turn and the search would come back empty, which
    // is worse than the untargeted search it is meant to narrow. The title test
    // further down still works, so fall through to that instead.
    let by_pid = win.answers().process;
    let mut out = Vec::new();
    for app in applications(c) {
        let pid = pid_of_name(c, &app.0);
        if in_front && by_pid && active.as_ref().is_some_and(|a| Some(a.pid) != pid) {
            continue;
        }
        for frame in children(c, &app) {
            let st = states(c, &frame);
            if in_front && !has_state(st, STATE_ACTIVE) {
                // The application may own several windows; the active one is the
                // one in front. Fall back to the title when no state says so.
                let title = name(c, &frame);
                if active.as_ref().is_none_or(|a| a.title != title) {
                    continue;
                }
            }
            let title = name(c, &frame);
            // `windows()` has already dropped everything that is not a real window.
            let found = clients
                .iter()
                .filter(|w| !by_pid || Some(w.pid) == pid)
                .find(|w| w.title == title)
                .or_else(|| clients.iter().find(|w| by_pid && Some(w.pid) == pid));
            let (x, y) = found.map(|w| (w.rect.0, w.rect.1)).unwrap_or((0, 0));
            out.push(Frame { elem: frame, x, y });
        }
        if in_front && !out.is_empty() {
            break;
        }
    }
    out
}

fn found_of(c: &Connection, e: &Elem, frame: &Frame, r: u32) -> Option<Found> {
    let (ex, ey, ew, eh) = extents(c, e)?;
    if ew <= 0 || eh <= 0 {
        return None;
    }
    let layout = super::geom::layout();
    let (px, py, pw, ph) = layout.rect_to_phys((frame.x + ex, frame.y + ey, ew, eh));
    let value = if matches!(r, ROLE_TEXT | ROLE_ENTRY | ROLE_PASSWORD_TEXT | ROLE_DOCUMENT_TEXT) {
        text_of(c, e)
    } else {
        String::new()
    };
    Some(Found { name: name(c, e), value, x: px + pw / 2, y: py + ph / 2, w: pw, h: ph })
}

/// Depth-first over one frame, within a budget of nodes and time.
fn search(
    c: &Connection,
    frame: &Frame,
    q: &Query,
    exact: bool,
    deadline: Instant,
    budget: &mut usize,
) -> Option<(Found, Elem)> {
    let want = q.name.trim().to_lowercase();
    let want_id = q.automation_id.trim();
    let roles = roles_of(&q.control);
    let mut stack: Vec<Elem> = vec![frame.elem.clone()];
    while let Some(e) = stack.pop() {
        if *budget == 0 || Instant::now() > deadline {
            return None;
        }
        *budget -= 1;
        let r = role(c, &e);
        let role_ok = roles.is_empty() || roles.contains(&r);
        let mut hit = role_ok;
        if hit && !want_id.is_empty() {
            hit = accessible_id(c, &e) == want_id;
        }
        if hit && !want.is_empty() {
            let n = name(c, &e).to_lowercase();
            hit = if exact { n == want } else { n.contains(&want) };
        }
        if hit && e != frame.elem
            && let Some(f) = found_of(c, &e, frame, r)
        {
            return Some((f, e));
        }
        let st = states(c, &e);
        if has_state(st, STATE_MANAGES_DESCENDANTS) {
            continue;
        }
        let mut kids = children(c, &e);
        kids.reverse();
        stack.extend(kids);
    }
    None
}

/// One look. The waiting is added by `uia::find`.
pub fn look(q: &Query) -> Option<(Found, Elem)> {
    if q.is_empty() {
        return None;
    }
    let c = conn()?;
    let frames = frames(c, q.in_front);
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut budget = 6000usize;
    // Exact name first, then the substring sweep, as on Windows.
    for exact in [true, false] {
        if exact && q.name.trim().is_empty() {
            continue;
        }
        for f in &frames {
            if let Some(hit) = search(c, f, q, exact, deadline, &mut budget) {
                return Some(hit);
            }
        }
        if q.name.trim().is_empty() {
            break;
        }
    }
    None
}

/// Presses the element the way the application itself would.
pub fn invoke(e: &Elem) -> bool {
    let Some(c) = conn() else { return false };
    let Some(p) = proxy(c, e, IFACE_ACTION) else { return false };
    let n: i32 = p.get_property("NActions").unwrap_or(0);
    if n <= 0 {
        return false;
    }
    p.call::<_, _, bool>("DoAction", &(0i32,)).unwrap_or(false)
}

/// What is under this physical point, described as a query that would find it.
pub fn at(x: i32, y: i32) -> Option<Query> {
    let c = conn()?;
    let layout = super::geom::layout();
    let (lx, ly) = layout.to_logical(x, y);
    let frames = frames(c, true);
    let frame = frames.first()?;
    let (rx, ry) = ((lx - frame.x as f64).round() as i32, (ly - frame.y as f64).round() as i32);
    // Down from the frame to the deepest child at the point.
    let mut e = frame.elem.clone();
    for _ in 0..32 {
        let Some(p) = proxy(c, &e, IFACE_COMPONENT) else { break };
        let Ok((n, path)) = p.call::<_, _, (String, OwnedObjectPath)>(
            "GetAccessibleAtPoint",
            &(rx, ry, COORD_WINDOW),
        ) else {
            break;
        };
        if path.as_str() == "/org/a11y/atspi/null" || n.is_empty() || (n.clone(), path.clone()) == e {
            break;
        }
        e = (n, path);
    }
    // Then up until something worth naming: an identifier, or a name plus a
    // control type a script would press. Stops at the window.
    for _ in 0..6 {
        let r = role(c, &e);
        let control = control_of_role(r);
        if control == "Window" {
            return None;
        }
        let id = accessible_id(c, &e);
        let nm = name(c, &e);
        let worth_it = !id.trim().is_empty() || (!nm.trim().is_empty() && !control.is_empty());
        if worth_it {
            return Some(Query {
                name: if id.trim().is_empty() { nm } else { String::new() },
                automation_id: id,
                control: control.to_string(),
                in_front: true,
            });
        }
        e = parent(c, &e)?;
    }
    None
}

#[allow(dead_code)]
fn showing(c: &Connection, e: &Elem) -> bool {
    has_state(states(c, e), STATE_SHOWING)
}
