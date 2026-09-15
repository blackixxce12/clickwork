//! Hyprland's IPC socket: the compositor as a window manager.
//!
//! Wayland has no "list every window with its rectangle" and no "focus that one",
//! because it was designed so that no client can spy on or steer another. A macro
//! recorder needs exactly that, so on Hyprland it asks the compositor through its
//! own socket instead: `$XDG_RUNTIME_DIR/hypr/<instance>/.socket.sock`, one request
//! per connection, JSON answers when the request is prefixed with `j/`.
//!
//! Since 0.56 the dispatchers are Lua: `dispatch <expr>` runs `return hl.dispatch(<expr>)`,
//! so what is sent is the dispatcher expression itself, `hl.dsp.window.close({...})`,
//! and never a call to `hl.dispatch` - doubling it up makes the second call act on
//! whatever window happens to be focused. Queries stay on the plain commands, which
//! are cheaper and give JSON.
//!
//! Everything here is a plain blocking call of a fraction of a millisecond. The hot
//! paths cache their answers with a short TTL, see `super::geom`.

use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// Where the socket lives, or `None` when this is not a Hyprland session.
pub fn socket_path() -> Option<PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    let p = runtime.join("hypr").join(sig).join(".socket.sock");
    p.exists().then_some(p)
}

/// Is this a Hyprland session at all?
pub fn available() -> bool {
    static KNOWN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *KNOWN.get_or_init(|| socket_path().is_some())
}

/// One request, one answer.
pub fn request(cmd: &str) -> Option<String> {
    let path = socket_path()?;
    let mut s = UnixStream::connect(path).ok()?;
    let _ = s.set_read_timeout(Some(Duration::from_millis(1500)));
    let _ = s.set_write_timeout(Some(Duration::from_millis(500)));
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

fn json<T: for<'de> Deserialize<'de>>(cmd: &str) -> Option<T> {
    let raw = request(cmd)?;
    match serde_json::from_str::<T>(&raw) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::debug!("hyprctl {cmd}: unreadable answer ({e}): {}", crate::clip(&raw, 200));
            None
        }
    }
}

/// Runs a piece of Lua on the compositor. `true` when it did not complain.
pub fn dispatch_lua(code: &str) -> bool {
    match request(&format!("dispatch {code}")) {
        Some(r) => {
            let ok = r.trim() == "ok";
            if !ok {
                tracing::warn!("hyprland dispatch `{code}` answered: {}", r.trim());
            }
            ok
        }
        None => false,
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct WorkspaceRef {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Client {
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub mapped: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub at: [i32; 2],
    #[serde(default)]
    pub size: [i32; 2],
    #[serde(default)]
    pub workspace: WorkspaceRef,
    #[serde(default)]
    pub floating: bool,
    #[serde(default)]
    pub monitor: i64,
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, rename = "initialClass")]
    pub initial_class: String,
    #[serde(default)]
    pub pid: i64,
    #[serde(default)]
    pub xwayland: bool,
    #[serde(default)]
    pub fullscreen: i64,
    #[serde(default, rename = "focusHistoryID")]
    pub focus_history_id: i64,
}

impl Client {
    /// A window somebody could see and click, as opposed to a helper surface.
    pub fn is_real(&self) -> bool {
        self.mapped && !self.hidden && self.size[0] > 0 && self.size[1] > 0
    }
    /// Logical rectangle.
    pub fn rect(&self) -> (i32, i32, i32, i32) {
        (self.at[0], self.at[1], self.size[0], self.size[1])
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Monitor {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub scale: f64,
    #[serde(default)]
    pub transform: i32,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, rename = "activeWorkspace")]
    pub active_workspace: WorkspaceRef,
    #[serde(default, rename = "specialWorkspace")]
    pub special_workspace: WorkspaceRef,
}

impl Monitor {
    /// Logical size: what the compositor lays windows out in.
    pub fn logical_size(&self) -> (i32, i32) {
        let s = if self.scale > 0.0 { self.scale } else { 1.0 };
        // Transforms 1, 3, 5, 7 are the rotated ones, where the mode's width is
        // the screen's height.
        let (w, h) = if self.transform % 2 == 1 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };
        (((w as f64) / s).round().max(1.0) as i32, ((h as f64) / s).round().max(1.0) as i32)
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Keyboard {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub variant: String,
    #[serde(default)]
    pub options: String,
    #[serde(default)]
    pub rules: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub active_layout_index: i64,
    #[serde(default)]
    pub active_keymap: String,
    #[serde(default)]
    pub main: bool,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Devices {
    #[serde(default)]
    pub keyboards: Vec<Keyboard>,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct CursorPos {
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
}

pub fn clients() -> Vec<Client> {
    json::<Vec<Client>>("j/clients").unwrap_or_default()
}

pub fn active_window() -> Option<Client> {
    let c = json::<Client>("j/activewindow")?;
    (!c.address.is_empty()).then_some(c)
}

/// A layer-shell surface: a bar, a launcher, a notification, a locker - drawn
/// beside the windows rather than as one, which is why none of them ever shows up
/// in `clients`.
#[derive(Clone, Debug, Default)]
pub struct Layer {
    pub monitor: String,
    /// 0 background, 1 bottom, 2 top, 3 overlay; -1 when the key was not a number.
    pub level: i64,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub namespace: String,
    pub pid: i64,
    /// Opacity as the compositor last set it. A surface is not always unmapped when
    /// it goes away: a bar or a launcher that fades out stays mapped at zero, and
    /// counting one of those as being in the way would hold playback for good.
    pub alpha: f32,
}

/// The first level drawn over the windows rather than under them; `j/layers` numbers
/// the four layer-shell levels 0 to 3.
pub const LEVEL_TOP: i64 = 2;

/// Opaque, for a compositor that does not report opacity at all: an unreported
/// surface is a visible one, never a free pass.
fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize, Default)]
struct LayerJson {
    #[serde(default = "one")]
    alpha: f32,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default)]
    w: i32,
    #[serde(default)]
    h: i32,
    #[serde(default)]
    namespace: String,
    #[serde(default)]
    pid: i64,
}

#[derive(Clone, Debug, Deserialize, Default)]
struct LayerLevels {
    #[serde(default)]
    levels: HashMap<String, Vec<LayerJson>>,
}

/// Every layer-shell surface the compositor holds, flattened out of the tree
/// `j/layers` answers with: one entry per monitor, and inside it the four levels as
/// the *keys* of an object, which is why the level arrives as a string and is turned
/// into a number here rather than in the deserialiser.
pub fn layers() -> Vec<Layer> {
    let tree = json::<HashMap<String, LayerLevels>>("j/layers").unwrap_or_default();
    let mut out = Vec::new();
    for (monitor, m) in tree {
        for (level, list) in m.levels {
            let level: i64 = level.parse().unwrap_or(-1);
            for l in list {
                out.push(Layer {
                    monitor: monitor.clone(),
                    level,
                    x: l.x,
                    y: l.y,
                    w: l.w,
                    h: l.h,
                    namespace: l.namespace,
                    pid: l.pid,
                    alpha: l.alpha,
                });
            }
        }
    }
    // Two hash maps went into this, and every other line `--doctor` prints is the
    // same from one run to the next; the surface worth naming first is the front one.
    out.sort_by(|a, b| a.monitor.cmp(&b.monitor).then(b.level.cmp(&a.level)));
    out
}

pub fn monitors() -> Vec<Monitor> {
    json::<Vec<Monitor>>("j/monitors").unwrap_or_default()
}

/// Logical cursor position in the global layout.
pub fn cursor_pos() -> Option<(i32, i32)> {
    json::<CursorPos>("j/cursorpos").map(|c| (c.x, c.y))
}

pub fn devices() -> Devices {
    json::<Devices>("j/devices").unwrap_or_default()
}

/// The keyboard whose layout the user is typing in.
pub fn main_keyboard() -> Option<Keyboard> {
    let d = devices();
    d.keyboards.iter().find(|k| k.main).cloned().or_else(|| d.keyboards.first().cloned())
}

#[derive(Clone, Debug, Deserialize, Default)]
struct OptionAnswer {
    #[serde(default)]
    str: String,
    #[serde(default)]
    set: bool,
}

/// A string option from the running configuration, or empty.
pub fn option_str(name: &str) -> String {
    match json::<OptionAnswer>(&format!("j/getoption {name}")) {
        Some(o) if o.set && o.str != "[[EMPTY]]" => o.str,
        _ => String::new(),
    }
}

/// Our own top-level window, by pid and class.
pub fn own_window() -> Option<Client> {
    let me = std::process::id() as i64;
    let mut mine: Vec<Client> =
        clients().into_iter().filter(|c| c.pid == me && c.is_real()).collect();
    // The main window rather than the handbook or the variables window: the one
    // whose title is the application's own.
    mine.sort_by_key(|c| (c.title != crate::APP_TITLE, c.focus_history_id));
    mine.into_iter().next()
}

/// Quotes a string for a Lua source literal.
pub fn lua_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `address:0x...`, the selector every dispatcher takes.
pub fn selector(c: &Client) -> String {
    lua_str(&format!("address:{}", c.address))
}

// ---- actions ---------------------------------------------------------------

pub fn focus(c: &Client) -> bool {
    dispatch_lua(&format!("hl.dsp.focus({{ window = {} }})", selector(c)))
}

pub fn close(c: &Client) -> bool {
    dispatch_lua(&format!("hl.dsp.window.close({{ window = {} }})", selector(c)))
}

/// Exact position, in logical pixels.
pub fn move_to(c: &Client, x: i32, y: i32) -> bool {
    dispatch_lua(&format!(
        "hl.dsp.window.move({{ window = {}, x = {x}, y = {y}, exact = true }})",
        selector(c)
    ))
}

/// Exact size, in logical pixels.
pub fn resize_to(c: &Client, w: i32, h: i32) -> bool {
    dispatch_lua(&format!(
        "hl.dsp.window.resize({{ window = {}, x = {}, y = {}, exact = true }})",
        selector(c),
        w.max(1),
        h.max(1)
    ))
}

pub fn center(c: &Client) -> bool {
    dispatch_lua(&format!("hl.dsp.window.center({{ window = {} }})", selector(c)))
}

/// Toggles fullscreen. Hyprland has no "maximise" of its own for tiled windows; the
/// closest honest thing is fullscreen, and `restore` is the same toggle the other
/// way.
pub fn fullscreen(c: &Client, on: bool) -> bool {
    if (c.fullscreen != 0) == on {
        return true;
    }
    dispatch_lua(&format!(
        "hl.dsp.window.fullscreen({{ window = {} }})",
        selector(c)
    ))
}

pub fn move_to_workspace(c: &Client, workspace: &str, silent: bool) -> bool {
    dispatch_lua(&format!(
        "hl.dsp.window.move({{ window = {}, workspace = {}, silent = {} }})",
        selector(c),
        lua_str(workspace),
        if silent { "true" } else { "false" }
    ))
}

pub fn move_to_workspace_id(c: &Client, id: i64, silent: bool) -> bool {
    dispatch_lua(&format!(
        "hl.dsp.window.move({{ window = {}, workspace = {id}, silent = {} }})",
        selector(c),
        if silent { "true" } else { "false" }
    ))
}

/// Which workspaces are on screen right now, one per monitor.
pub fn visible_workspace_ids() -> Vec<i64> {
    monitors()
        .iter()
        .filter(|m| !m.disabled)
        .flat_map(|m| {
            let mut v = vec![m.active_workspace.id];
            if m.special_workspace.id != 0 {
                v.push(m.special_workspace.id);
            }
            v
        })
        .collect()
}

/// The workspace the cursor's monitor shows.
pub fn focused_workspace_id() -> Option<i64> {
    monitors().iter().find(|m| m.focused).map(|m| m.active_workspace.id)
}

/// Compositor version string, for the log.
pub fn version() -> String {
    request("version")
        .and_then(|v| v.lines().next().map(str::to_string))
        .unwrap_or_default()
}
