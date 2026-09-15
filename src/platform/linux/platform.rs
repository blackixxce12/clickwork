//! The platform layer for Linux: what `main.rs` asks of the operating system,
//! answered by Hyprland's socket, the Wayland protocols and `/proc`.
//!
//! Coordinates in and out of here are physical pixels; see `super::geom`.

use super::{capture, clip, geom, hypr, inject, keymap, single};
use crate::{EndAction, InputEventKind, WindowAnchor};
use std::sync::atomic::Ordering;

/// The Windows build hands its window handle around; here there is nothing to
/// hand, and the functions that took one take a unit.
pub fn app_hwnd() {}

pub fn apply_system_backdrop(_: (), _: i32) {}

pub unsafe fn send_absolute_mouse_move(x: i32, y: i32) {
    if !crate::selftest::send_blocked() {
        inject::mouse_abs(x, y);
    }
}

/// Linux timers are already fine-grained; `timeBeginPeriod` has no counterpart
/// and needs none.
pub fn begin_high_res_timer() {}
pub fn end_high_res_timer() {}

/// `w - 1` as the denominator so the right/bottom-most pixel stays reachable.
/// Kept identical to the Windows one: the self-tests exercise it.
pub fn normalize_abs(x: i32, y: i32, vx: i32, vy: i32, vw: i32, vh: i32) -> (i32, i32) {
    let dx = (vw - 1).max(1) as f64;
    let dy = (vh - 1).max(1) as f64;
    let nx = (((x - vx) as f64 / dx) * 65535.0).round().clamp(0.0, 65535.0) as i32;
    let ny = (((y - vy) as f64 / dy) * 65535.0).round().clamp(0.0, 65535.0) as i32;
    (nx, ny)
}

/// Colour of one screen pixel.
pub fn screen_pixel(x: i32, y: i32) -> Option<(u8, u8, u8)> {
    let f = capture::capture(x, y, 1, 1)?;
    let p = f.px.get(0..4)?;
    // Frames come back blue-first.
    Some((p[2], p[1], p[0]))
}

pub fn cursor_pos() -> (i32, i32) {
    match hypr::cursor_pos() {
        Some((x, y)) => geom::layout().to_phys(x as f64, y as f64),
        None => (0, 0),
    }
}

/// Local wall-clock time: year, month, day, weekday (0 = Monday), hour, minute.
pub fn local_time() -> (u16, u16, u16, u8, u16, u16) {
    let (y, mo, d, wday, h, mi, _) = local_now();
    (y, mo, d, wday, h, mi)
}

/// The same, with seconds and Monday-based weekday.
pub fn local_now() -> (u16, u16, u16, u8, u16, u16, u16) {
    unsafe {
        let mut t: libc::time_t = 0;
        libc::time(&mut t);
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return (1970, 1, 1, 3, 0, 0, 0);
        }
        (
            (tm.tm_year + 1900) as u16,
            (tm.tm_mon + 1) as u16,
            tm.tm_mday as u16,
            ((tm.tm_wday + 6) % 7) as u8,
            tm.tm_hour as u16,
            tm.tm_min as u16,
            tm.tm_sec as u16,
        )
    }
}

fn own_pid() -> i64 {
    std::process::id() as i64
}

/// The window in front, unless it is one of ours.
fn foreign_active() -> Option<hypr::Client> {
    hypr::active_window().filter(|c| c.pid != own_pid())
}

pub fn foreground_title() -> Option<String> {
    hypr::active_window().map(|c| c.title).filter(|t| !t.is_empty())
}

pub fn capture(x: i32, y: i32, w: i32, h: i32) -> Option<crate::vision::Frame> {
    capture::capture(x, y, w, h)
}

/// There is one capture path here, so the "slow" one is the same one.
pub fn capture_gdi(x: i32, y: i32, w: i32, h: i32) -> Option<crate::vision::Frame> {
    capture::capture(x, y, w, h)
}

/// Screencopy carries no "nothing changed" signal, so nothing derived from a frame
/// may be reused. `None` is the honest answer and the callers already handle it.
pub fn frame_serial() -> Option<u64> {
    None
}

pub fn last_dirty() -> Option<Vec<(i32, i32, i32, i32)>> {
    None
}

pub fn release_capture_cache() {
    capture::release();
}

/// Nothing to switch. The one screencopy path already holds its session and its
/// buffer open across grabs, so the only other behaviour available would be to
/// throw them away on purpose. The setting still travels in the config for
/// Windows' sake; the checkbox that drives it is not drawn here.
pub fn set_fast_capture(_: bool) {}

pub fn capture_counters() -> (u64, u64, u64) {
    capture::counters()
}

pub fn reset_capture_counters() {
    capture::reset_counters();
}

// The benchmark probes price GDI steps that do not exist here.
pub fn probe_screen_dc() -> bool {
    false
}
pub fn probe_blt(x: i32, y: i32, w: i32, h: i32) -> bool {
    capture::capture(x, y, w, h).is_some()
}
pub fn probe_blt_ddb(_: i32, _: i32, _: i32, _: i32) -> bool {
    false
}
pub fn probe_blt_mem(_: i32, _: i32) -> bool {
    false
}

pub fn virtual_screen_rect() -> (i32, i32, i32, i32) {
    geom::layout().virtual_phys()
}

pub fn clipboard_image() -> Option<(u32, u32, Vec<u8>)> {
    clip::image()
}

pub fn clipboard_text() -> String {
    clip::text()
}

pub fn set_clipboard_text(text: &str) -> bool {
    clip::set_text(text)
}

fn phys_rect(c: &hypr::Client) -> (i32, i32, i32, i32) {
    geom::layout().rect_to_phys(c.rect())
}

pub fn foreground_anchor() -> Option<WindowAnchor> {
    let c = foreign_active()?;
    if c.title.is_empty() {
        return None;
    }
    let (x, y, w, h) = phys_rect(&c);
    Some(WindowAnchor { title: c.title, x, y, w, h })
}

/// Executable name of a process, from `/proc`.
fn comm_of(pid: i64) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn exe_of(pid: i64) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok().map(|p| p.to_string_lossy().into_owned())
}

/// The window a title fragment refers to: exact first, then a case-insensitive
/// substring of the first 24 characters, as on Windows.
fn find_by_title(title: &str) -> Option<hypr::Client> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let all: Vec<hypr::Client> = hypr::clients().into_iter().filter(|c| c.is_real()).collect();
    if let Some(c) = all.iter().find(|c| c.title == title) {
        return Some(c.clone());
    }
    let needle: String = title.to_lowercase().chars().take(24).collect::<String>().trim().to_string();
    if needle.is_empty() {
        return None;
    }
    all.into_iter().find(|c| c.title.to_lowercase().contains(&needle))
}

/// The window a `WindowRef` names, by whichever of the four ways it asks for:
/// 0 title fragment, 1 exact title, 2 process name, 3 full path.
fn resolve_window(by: u8, value: &str) -> Option<hypr::Client> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    match by {
        1 => hypr::clients().into_iter().find(|c| c.is_real() && c.title == value),
        2 => {
            let want = value.to_lowercase();
            let want = want.strip_suffix(".exe").unwrap_or(&want).to_string();
            hypr::clients().into_iter().find(|c| {
                c.is_real() && !c.title.is_empty() && {
                    let comm = comm_of(c.pid).to_lowercase();
                    let cls = c.class.to_lowercase();
                    comm == want
                        || comm.strip_suffix(".exe") == Some(&want)
                        || cls == want
                        || exe_of(c.pid)
                            .map(|e| {
                                std::path::Path::new(&e)
                                    .file_name()
                                    .map(|f| f.to_string_lossy().to_lowercase() == want)
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false)
                }
            })
        }
        3 => hypr::clients().into_iter().find(|c| {
            c.is_real() && exe_of(c.pid).is_some_and(|e| e.eq_ignore_ascii_case(value))
        }),
        _ => find_by_title(value),
    }
}

pub fn window_exists(by: u8, value: &str) -> bool {
    resolve_window(by, value).is_some()
}

/// A desktop notification, through the freedesktop service every notification
/// daemon speaks.
pub fn notify(title: &str, body: &str) -> bool {
    let ok = notify_rust::Notification::new()
        .appname(crate::APP_TITLE)
        .summary(title)
        .body(body)
        .icon("clickwork")
        .show()
        .is_ok();
    if !ok {
        tracing::info!("notify: {title} - {body} (no notification service answered)");
    }
    ok
}

pub fn window_is_active(by: u8, value: &str) -> bool {
    match (resolve_window(by, value), hypr::active_window()) {
        (Some(w), Some(a)) => w.address == a.address,
        _ => false,
    }
}

pub fn window_rect_of(by: u8, value: &str) -> Option<(i32, i32, i32, i32)> {
    resolve_window(by, value).map(|c| phys_rect(&c))
}

pub fn find_window_rect(title: &str) -> Option<(i32, i32, i32, i32)> {
    find_by_title(title).map(|c| phys_rect(&c))
}

pub fn foreground_rect() -> Option<(i32, i32, i32, i32)> {
    hypr::active_window().map(|c| phys_rect(&c))
}

/// The special workspace a "minimised" window is parked on. Hyprland has no
/// minimise of its own; a silent move to a special workspace is what its users
/// do instead, and `restore` brings the window back to the workspace in view.
const PARKED: &str = "special:clickwork_min";

/// Does one thing to one window: 0 front, 1 minimise, 2 maximise, 3 restore,
/// 4 close, 5 move, 6 resize, 7 centre. `arg` is the pair for move and resize.
pub fn window_action(by: u8, value: &str, action: u8, arg: (i32, i32)) -> bool {
    let Some(c) = resolve_window(by, value) else {
        return false;
    };
    let l = geom::layout();
    match action {
        0 => {
            if c.workspace.name.starts_with("special:") {
                let ws = hypr::focused_workspace_id().unwrap_or(c.workspace.id);
                let _ = hypr::move_to_workspace_id(&c, ws, false);
            }
            hypr::focus(&c)
        }
        1 => hypr::move_to_workspace(&c, PARKED, true),
        2 => hypr::fullscreen(&c, true),
        3 => {
            if c.workspace.name.starts_with("special:") {
                let ws = hypr::focused_workspace_id().unwrap_or(1);
                hypr::move_to_workspace_id(&c, ws, false) && hypr::focus(&c)
            } else if c.fullscreen != 0 {
                hypr::fullscreen(&c, false)
            } else {
                true
            }
        }
        4 => hypr::close(&c),
        5 => {
            let (lx, ly) = l.to_logical(arg.0, arg.1);
            hypr::move_to(&c, lx.round() as i32, ly.round() as i32)
        }
        6 => {
            let s = l.scale_at_phys(c.at[0], c.at[1]).max(0.01);
            hypr::resize_to(
                &c,
                (arg.0.max(1) as f64 / s).round() as i32,
                (arg.1.max(1) as f64 / s).round() as i32,
            )
        }
        7 => hypr::center(&c),
        _ => false,
    }
}

/// Dots per inch of the display the front window is on. 96 is 100 %.
pub fn current_dpi() -> u32 {
    let l = geom::layout();
    let scale = hypr::active_window()
        .and_then(|c| l.mons.iter().find(|m| m.id == c.monitor).map(|m| m.scale))
        .or_else(|| l.focused().map(|m| m.scale))
        .unwrap_or(1.0);
    (96.0 * scale).round().max(1.0) as u32
}

/// How many displays there are, and which one the window in front is on, from 1.
pub fn monitor_here() -> (u32, u32) {
    let l = geom::layout();
    let n = l.mons.len() as u32;
    let id = hypr::active_window()
        .map(|c| c.monitor)
        .or_else(|| l.focused().map(|m| m.id));
    let at = id
        .and_then(|id| l.mons.iter().position(|m| m.id == id))
        .map(|i| i as u32 + 1)
        .unwrap_or(0);
    (n, at)
}

/// The keyboard layout in use, as a BCP-47-ish name: `en-US`, `ru-RU`.
pub fn keyboard_layout() -> String {
    let Some(k) = hypr::main_keyboard() else {
        return std::env::var("XKB_DEFAULT_LAYOUT").unwrap_or_default();
    };
    let idx = k.active_layout_index.max(0) as usize;
    let code = k.layout.split(',').nth(idx).unwrap_or("").trim().to_lowercase();
    match code.as_str() {
        "us" => "en-US",
        "gb" => "en-GB",
        "ru" => "ru-RU",
        "ua" => "uk-UA",
        "de" => "de-DE",
        "fr" => "fr-FR",
        "es" => "es-ES",
        "pt" => "pt-PT",
        "br" => "pt-BR",
        "it" => "it-IT",
        "pl" => "pl-PL",
        "cn" => "zh-CN",
        "jp" => "ja-JP",
        "kr" => "ko-KR",
        "tr" => "tr-TR",
        "by" => "be-BY",
        "kz" => "kk-KZ",
        "" => return k.active_keymap,
        other => return other.to_string(),
    }
    .to_string()
}

/// Executable name of the window in front.
pub fn foreground_process() -> String {
    hypr::active_window().map(|c| comm_of(c.pid)).unwrap_or_default()
}

/// Is a process whose name contains `name` running? A substring, without case.
pub fn process_running(name: &str) -> bool {
    let needle = name.trim().to_lowercase();
    let needle = needle.strip_suffix(".exe").unwrap_or(&needle).to_string();
    if needle.is_empty() {
        return false;
    }
    let Ok(dir) = std::fs::read_dir("/proc") else { return false };
    for e in dir.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<i64>() else { continue };
        if comm_of(pid).to_lowercase().contains(&needle) {
            return true;
        }
        if let Ok(cmd) = std::fs::read(format!("/proc/{pid}/cmdline"))
            && let Some(first) = cmd.split(|b| *b == 0).next()
        {
            let arg0 = String::from_utf8_lossy(first).to_lowercase();
            let base = arg0.rsplit('/').next().unwrap_or("").to_string();
            if base.contains(&needle) {
                return true;
            }
        }
    }
    false
}

/// There is no message loop to time on Wayland; the responsiveness probe has
/// nothing honest to say.
pub fn probe_window_us(_: &str, _: u32) -> Option<u64> {
    None
}

/// Resident memory, open file descriptors, threads.
pub fn process_cost() -> (u64, u32, u32) {
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
    let rss = std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()))
        .unwrap_or(0)
        * page;
    let fds = std::fs::read_dir("/proc/self/fd").map(|d| d.count() as u32).unwrap_or(0);
    let threads = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("Threads:"))
                .and_then(|v| v.trim().parse::<u32>().ok())
        })
        .unwrap_or(0);
    (rss, fds, threads)
}

pub fn acquire_single_instance() -> bool {
    single::acquire()
}

/// Hides or restores our own top-level window.
///
/// winit cannot hide a Wayland window, so on Hyprland the window is parked on a
/// special workspace of its own and fetched back to the workspace in view.
/// Elsewhere it is minimised, which is the most a Wayland client may ask.
pub fn set_window_hidden(hidden: bool) {
    if hypr::available()
        && let Some(c) = hypr::own_window()
    {
        if hidden {
            let _ = hypr::move_to_workspace(&c, "special:clickwork", true);
        } else {
            let ws = hypr::focused_workspace_id().unwrap_or(c.workspace.id);
            let _ = hypr::move_to_workspace_id(&c, ws, false);
            let _ = hypr::focus(&c);
        }
        return;
    }
    if let Some(ctx) = crate::UI_CTX.get() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(hidden));
        if !hidden {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        ctx.request_repaint();
    }
}

/// Asks the main window to close, the way the close button does.
pub fn request_app_close() {
    if let Some(ctx) = crate::UI_CTX.get() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        ctx.request_repaint();
    }
}

pub fn focus_existing_instance() {
    let _ = single::send("show");
}

/// A terminal is inherited on Linux; nothing to attach.
pub fn attach_parent_console() {}

pub fn set_dpi_awareness() {}

fn spawn_detached(cmd: &str) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt as _;
    let mut c = std::process::Command::new("sh");
    c.arg("-c").arg(cmd);
    c.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Its own session, so it outlives this process and a `--no-gui` run can ask
    // for a shutdown and exit before it happens.
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    c.spawn().map(|_| ()).map_err(|e| anyhow::anyhow!("could not start `{cmd}`: {e}"))
}

pub fn run_end_action(action: EndAction, delay_s: u32, reason: &str) -> anyhow::Result<()> {
    let verb = match action {
        EndAction::Stop => return Ok(()),
        EndAction::Shutdown => "poweroff",
        EndAction::Reboot => "reboot",
        EndAction::Sleep => "suspend",
        EndAction::Hibernate => "hibernate",
        EndAction::LogOff => {
            let session = std::env::var("XDG_SESSION_ID").unwrap_or_default();
            let cmd = if !session.is_empty() {
                format!("sleep {delay_s}; loginctl terminate-session {session}")
            } else if hypr::available() {
                format!("sleep {delay_s}; hyprctl dispatch 'hl.dsp.exit()'")
            } else {
                anyhow::bail!("no session to log out of");
            };
            tracing::info!("log off in {delay_s} s: {reason}");
            return spawn_detached(&cmd);
        }
    };
    tracing::info!("{verb} in {delay_s} s: {reason}");
    spawn_detached(&format!("sleep {delay_s}; exec systemctl {verb}"))
}

/// The Linux `SendInput`: one recorded event, through the compositor.
pub unsafe fn send_input_event(
    kind: &InputEventKind,
    state: &crate::AppState,
    pressed: &mut crate::PressedInputs,
    map: crate::CoordMap,
    mv: &mut crate::MoveEngine,
) {
    crate::selftest::note_input(kind);
    let blocked = crate::selftest::send_blocked();
    match kind {
        InputEventKind::Key { vk, scan, down, extended } => {
            if !blocked {
                match keymap::key_of_event(*vk, *scan, *extended) {
                    Some(code) => inject::key(code, *down),
                    None => tracing::debug!("no Linux key for vk {vk:#x} scan {scan:#x}"),
                }
            }
            pressed.note_key(*vk, *scan, *extended, *down);
        }
        InputEventKind::MouseMove { x, y, dx, dy } => {
            if state.absolute_mouse.load(Ordering::Relaxed) {
                let (nx, ny) = map.map(*x, *y);
                mv.goto(nx, ny);
            } else {
                let (sdx, sdy) = map.map_delta(*dx, *dy);
                if !blocked {
                    inject::mouse_rel(sdx, sdy);
                }
            }
        }
        InputEventKind::MouseButton { button, down, x, y } => {
            if state.absolute_mouse.load(Ordering::Relaxed) && (*x != 0 || *y != 0) {
                let (nx, ny) = map.map(*x, *y);
                mv.goto(nx, ny);
            }
            if !blocked {
                inject::button(*button, *down);
            }
            pressed.note_button(*button, *down);
        }
        InputEventKind::MouseWheel { delta, horizontal, .. } => {
            if !blocked {
                inject::wheel(*delta, *horizontal);
            }
        }
    }
}
