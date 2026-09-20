//! Recording and hotkeys, from the input devices themselves.
//!
//! Wayland shows a client only the input aimed at its own windows, on purpose, so
//! a recorder has to go underneath the compositor: `/dev/input/event*`, the same
//! stream the compositor reads. That needs read access to the devices - the
//! `input` group, or the udev rule the package installs - and without it this
//! thread says so once and keeps checking, because the fix takes a re-login and
//! the program should notice without a restart.
//!
//! What arrives is raw: key codes, button codes, relative motion. The pointer's
//! position is the compositor's business (acceleration, scale, the edge of the
//! screen), so it is asked for it rather than reconstructed, once per sample and
//! once per click. Everything else is translated into the Windows vocabulary the
//! macro format speaks by `super::keymap`.
//!
//! Hotkeys are matched here too. They cannot be swallowed the way `RegisterHotKey`
//! swallows them - the key still reaches the application in front - which is the
//! one difference from Windows worth knowing about, and the reason the emergency
//! stop defaults to a key nothing else wants.

use super::{keymap, tray, vdesk};
use crate::{
    AppState, CAPTURE_SLOT, CAPTURED_KEY, GLOBAL_STATE, HK_FAILED, HK_ID_FASTER, HK_ID_PAUSE,
    HK_ID_PLAY, HK_ID_RECORD, HK_ID_SKIP, HK_ID_SLOWER, HK_ID_STOP, HK_IDS, HookMode, Hotkey,
    InputEventKind, NO_LAST_POS, PENDING_HOTKEYS, current_rec_time_us, emit_event,
    expander, is_hotkey_vk, nudge_speed, pack_pos, stop_everything, toggle_pause,
    toggle_playback, toggle_recording, unpack_pos,
};
use std::os::fd::AsRawFd as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use xkbcommon::xkb;

/// Whether any input device could be opened. Read by the window to explain a
/// recorder that records nothing.
pub static DEVICES_OK: AtomicBool = AtomicBool::new(false);

struct Dev {
    dev: evdev::Device,
    path: std::path::PathBuf,
    /// Hi-res wheel events seen: the coarse ones are then duplicates.
    hires_wheel: bool,
    /// Pending relative motion since the last sync.
    dx: i32,
    dy: i32,
    moved: bool,
}

fn open_devices() -> Vec<Dev> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/dev/input") else { return out };
    let mut paths: Vec<_> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event")))
        .collect();
    paths.sort();
    for path in paths {
        let Ok(dev) = evdev::Device::open(&path) else { continue };
        let _ = dev.set_nonblocking(true);
        let ev = dev.supported_events();
        if !(ev.contains(evdev::EventType::KEY) || ev.contains(evdev::EventType::RELATIVE)) {
            continue;
        }
        // Keyboards and mice only: a power button or a lid switch has KEY events
        // and nothing anyone records.
        let keys = dev.supported_keys();
        let is_kbd = keys.is_some_and(|k| k.contains(evdev::KeyCode::KEY_A));
        let is_mouse = keys.is_some_and(|k| k.contains(evdev::KeyCode::BTN_LEFT))
            || ev.contains(evdev::EventType::RELATIVE);
        if !(is_kbd || is_mouse) {
            continue;
        }
        out.push(Dev { dev, path, hires_wheel: false, dx: 0, dy: 0, moved: false });
    }
    out
}

/// The layout the user types in, for turning key presses into characters for the
/// text expander.
struct Chars {
    state: Option<xkb::State>,
    layouts: u32,
    checked_us: u64,
}

impl Chars {
    fn new() -> Self {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let (rules, model, layout, variant, options) = super::inject::layout_names();
        let km = xkb::Keymap::new_from_names(
            &ctx,
            &rules,
            &model,
            &layout,
            &variant,
            if options.is_empty() { None } else { Some(options) },
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .or_else(|| {
            xkb::Keymap::new_from_names(&ctx, "", "", "us", "", None, xkb::KEYMAP_COMPILE_NO_FLAGS)
        });
        let layouts = km.as_ref().map(|k| k.num_layouts()).unwrap_or(1);
        Self { state: km.map(|k| xkb::State::new(&k)), layouts, checked_us: 0 }
    }

    /// Follows the compositor's active layout, checked once a second.
    fn sync_group(&mut self) {
        let now = crate::now_us();
        if now.saturating_sub(self.checked_us) < 1_000_000 {
            return;
        }
        self.checked_us = now;
        let g = super::hypr::main_keyboard()
            .map(|k| k.active_layout_index.max(0) as u32)
            .unwrap_or(0)
            .min(self.layouts.saturating_sub(1));
        if let Some(st) = self.state.as_mut()
            && st.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE) != g
        {
            let (d, l, k) = (
                st.serialize_mods(xkb::STATE_MODS_DEPRESSED),
                st.serialize_mods(xkb::STATE_MODS_LATCHED),
                st.serialize_mods(xkb::STATE_MODS_LOCKED),
            );
            st.update_mask(d, l, k, 0, 0, g);
        }
    }

    /// Feeds one key and returns what it typed, if it typed anything.
    fn key(&mut self, code: u16, down: bool) -> Option<String> {
        let st = self.state.as_mut()?;
        let kc = xkb::Keycode::new(code as u32 + 8);
        let text = if down { st.key_get_utf8(kc) } else { String::new() };
        st.update_key(kc, if down { xkb::KeyDirection::Down } else { xkb::KeyDirection::Up });
        (down && !text.is_empty()).then_some(text)
    }

    fn mod_active(&self, name: &str) -> bool {
        self.state
            .as_ref()
            .is_some_and(|s| s.mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE))
    }
}

/// Reads the input devices and drives recording and the hotkeys. Runs for the
/// life of the process, like the Windows message loop it replaces.
pub fn input_hook_thread(state: Arc<AppState>, mode: HookMode, with_tray: bool) {
    vdesk::init_thread();
    let _ = GLOBAL_STATE.set(state.clone());
    if with_tray {
        tray::init();
    }
    // The portal shortcuts belong to the window's instance, not to a headless
    // player that is gone in a minute.
    if mode == HookMode::Full {
        // The compositor takes the hotkeys it can; what it takes, evdev leaves
        // alone. It goes first because the portal reads the result off it, and a
        // mask written by a thread that has already started is a mask read too late.
        super::hyprbinds::rebind();
        super::shortcuts::start();
    }

    let mut devs = open_devices();
    let mut warned = false;
    let mut last_scan = std::time::Instant::now();
    let mut chars = Chars::new();
    // Keys held right now, by evdev code: what the hotkey test and the binding
    // capture look at.
    let mut held: std::collections::HashSet<u16> = std::collections::HashSet::new();
    let mut hk_down: [bool; 7] = [false; 7];

    loop {
        if devs.is_empty() {
            if !warned {
                warned = true;
                HK_FAILED.store(0x7F, Ordering::Relaxed);
                DEVICES_OK.store(false, Ordering::Relaxed);
                tracing::warn!(
                    "no input device could be opened: recording and hotkeys need read access \
                     to /dev/input/event* (add yourself to the `input` group and log in again, \
                     or install the udev rule from the package)"
                );
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
            devs = open_devices();
            continue;
        }
        if !DEVICES_OK.swap(true, Ordering::Relaxed) {
            tracing::info!(
                "reading {} input device(s): {}",
                devs.len(),
                devs.iter()
                    .map(|d| d.dev.name().unwrap_or("?").to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            refresh_hotkey_status();
        }

        // Wait for any device, a fifth of a second at most, so plugged-in devices
        // and a cleared "stop" flag are noticed.
        let mut fds: Vec<libc::pollfd> = devs
            .iter()
            .map(|d| libc::pollfd { fd: d.dev.as_raw_fd(), events: libc::POLLIN, revents: 0 })
            .collect();
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 200) };
        if n < 0 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let mut gone: Vec<usize> = Vec::new();
        for (i, pfd) in fds.iter().enumerate() {
            if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                gone.push(i);
                continue;
            }
            if pfd.revents & libc::POLLIN == 0 {
                continue;
            }
            let events: Vec<evdev::InputEvent> = match devs[i].dev.fetch_events() {
                Ok(it) => it.collect(),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => {
                    gone.push(i);
                    continue;
                }
            };
            for ev in events {
                handle(&state, mode, &mut devs[i], ev, &mut chars, &mut held, &mut hk_down);
            }
        }
        for i in gone.into_iter().rev() {
            tracing::info!("input device {} went away", devs[i].path.display());
            devs.remove(i);
        }
        if last_scan.elapsed() > std::time::Duration::from_secs(3) {
            last_scan = std::time::Instant::now();
            let known: std::collections::HashSet<_> = devs.iter().map(|d| d.path.clone()).collect();
            for d in open_devices() {
                if !known.contains(&d.path) {
                    tracing::info!("input device {} appeared", d.path.display());
                    devs.push(d);
                }
            }
        }
    }
}

/// All slots count as registered once devices can be read; there is nothing to
/// register with, so nothing can refuse.
fn refresh_hotkey_status() {
    HK_FAILED.store(0, Ordering::Relaxed);
}

fn handle(
    state: &Arc<AppState>,
    mode: HookMode,
    d: &mut Dev,
    ev: evdev::InputEvent,
    chars: &mut Chars,
    held: &mut std::collections::HashSet<u16>,
    hk_down: &mut [bool; 7],
) {
    match ev.destructure() {
        evdev::EventSummary::Key(_, key, value) => {
            let code = key.code();
            let down = value != 0;
            let repeat = value == 2;
            if (0x110..0x120).contains(&code) {
                if let Some(button) = keymap::button_of_btn(code) {
                    if mode == HookMode::Full {
                        expander::reset();
                    }
                    // The same guard the move path below carries, and for the
                    // same reason: a click recorded at the top-left corner is
                    // not a lost click, it is a step that presses the corner on
                    // every playback - and the click is then turned into a
                    // picture anchor by cropping around that corner, so the
                    // damage outlives the coordinate. This became reachable the
                    // moment sessions other than Hyprland got a window backend;
                    // before that nothing answered on them at all.
                    if let Some(st) = should_record(mode)
                        && let Some((x, y)) = super::platform::cursor_pos_checked()
                    {
                        emit_event(st, InputEventKind::MouseButton { button, down, x, y });
                    }
                }
                return;
            }
            if repeat {
                return;
            }
            if down {
                held.insert(code);
            } else {
                held.remove(&code);
            }
            chars.sync_group();
            let typed = chars.key(code, down);
            let Some((vk, scan, extended)) = keymap::vk_of_key(code) else { return };
            let ctrl = held.contains(&29) || held.contains(&97);
            let alt = held.contains(&56) || held.contains(&100);
            let shift = held.contains(&42) || held.contains(&54);
            let win = held.contains(&125) || held.contains(&126);

            if handle_key_capture(vk as u32, down, ctrl, alt, shift) {
                return;
            }
            if hotkey(state, vk as u32, down, ctrl, alt, shift, hk_down) {
                return;
            }
            if is_hotkey_vk(vk as u32) {
                return;
            }
            if mode == HookMode::Full {
                expander::on_key_linux(vk, down, typed.as_deref(), ctrl, alt, win, chars.mod_active("Lock"));
                if let Some(st) = should_record(mode) {
                    emit_event(st, InputEventKind::Key { vk, scan, down, extended });
                }
            }
        }
        evdev::EventSummary::RelativeAxis(_, axis, value) => match axis {
            evdev::RelativeAxisCode::REL_X => {
                d.dx += value;
                d.moved = true;
            }
            evdev::RelativeAxisCode::REL_Y => {
                d.dy += value;
                d.moved = true;
            }
            evdev::RelativeAxisCode::REL_WHEEL_HI_RES => {
                d.hires_wheel = true;
                wheel(state, mode, value, false);
            }
            evdev::RelativeAxisCode::REL_HWHEEL_HI_RES => {
                d.hires_wheel = true;
                wheel(state, mode, value, true);
            }
            evdev::RelativeAxisCode::REL_WHEEL => {
                if !d.hires_wheel {
                    wheel(state, mode, value * 120, false);
                }
            }
            evdev::RelativeAxisCode::REL_HWHEEL => {
                if !d.hires_wheel {
                    wheel(state, mode, value * 120, true);
                }
            }
            _ => {}
        },
        evdev::EventSummary::Synchronization(..) => {
            if d.moved {
                d.moved = false;
                d.dx = 0;
                d.dy = 0;
                if let Some(st) = should_record(mode)
                    && st.capture_mouse_moves.load(Ordering::Relaxed)
                {
                    let now = current_rec_time_us(st);
                    let last = st.last_move_us.load(Ordering::Relaxed);
                    let step = st.mouse_sample_us.load(Ordering::Relaxed);
                    // A move is worth writing down only when somebody can say where
                    // the pointer went. Recorded as the top-left corner it is not a
                    // lost move but a step that drags the pointer to the corner on
                    // every playback.
                    if (last == 0 || now.saturating_sub(last) >= step)
                        && let Some((x, y)) = super::platform::cursor_pos_checked()
                    {
                        st.last_move_us.store(now, Ordering::Relaxed);
                        let prev = st.last_pos.swap(pack_pos(x, y), Ordering::Relaxed);
                        let (dx, dy) = if prev == NO_LAST_POS {
                            (0, 0)
                        } else {
                            let (lx, ly) = unpack_pos(prev);
                            (x.saturating_sub(lx), y.saturating_sub(ly))
                        };
                        emit_event(st, InputEventKind::MouseMove { x, y, dx, dy });
                    }
                }
            }
        }
        _ => {}
    }
}

fn wheel(state: &Arc<AppState>, mode: HookMode, delta: i32, horizontal: bool) {
    let _ = state;
    if delta == 0 {
        return;
    }
    if let Some(st) = should_record(mode) {
        let (x, y) = super::platform::cursor_pos();
        // Windows counts the wheel away from the user as positive; evdev the same
        // for REL_WHEEL, and REL_HWHEEL positive to the right.
        emit_event(st, InputEventKind::MouseWheel { delta, x, y, horizontal });
    }
}

/// Cheap by construction: one atomic load plus two cached answers - the workspace the
/// window is on, and whether anything is drawn over it.
fn should_record(mode: HookMode) -> Option<&'static Arc<AppState>> {
    if mode != HookMode::Full {
        return None;
    }
    let state = GLOBAL_STATE.get()?;
    if !state.recording.load(Ordering::Relaxed) {
        return None;
    }
    if !vdesk::is_app_on_active_desktop_cached(()) || vdesk::shell_switcher_in_front() {
        return None;
    }
    Some(state)
}

/// Handles "press any key to bind". Returns true if the key was consumed.
fn handle_key_capture(vk: u32, down: bool, ctrl: bool, alt: bool, shift: bool) -> bool {
    if CAPTURE_SLOT.load(Ordering::Relaxed) == 0 {
        return false;
    }
    if !down {
        return true;
    }
    if keymap::modifier_of_vk(vk as u16).is_some() {
        return true;
    }
    if vk == 0x1B {
        CAPTURE_SLOT.store(0, Ordering::Relaxed);
        return true;
    }
    *CAPTURED_KEY.lock() = Some(Hotkey { vk, ctrl, alt, shift });
    true
}

/// Matches a press against the seven slots and fires the one that matches.
fn hotkey(
    state: &Arc<AppState>,
    vk: u32,
    down: bool,
    ctrl: bool,
    alt: bool,
    shift: bool,
    hk_down: &mut [bool; 7],
) -> bool {
    let hk = *PENDING_HOTKEYS.lock();
    let handled = super::hyprbinds::HANDLED.load(Ordering::Relaxed);
    let mut matched = false;
    for (i, k) in hk.iter().enumerate() {
        if k.vk == 0 || k.vk != vk {
            continue;
        }
        if handled & (1 << i) != 0 {
            // Hyprland consumed this key and ran the command itself.
            matched = true;
            continue;
        }
        if !down {
            hk_down[i] = false;
            matched = true;
            continue;
        }
        if k.ctrl != ctrl || k.alt != alt || k.shift != shift {
            continue;
        }
        if hk_down[i] {
            continue; // held, not pressed again
        }
        hk_down[i] = true;
        matched = true;
        let id = HK_IDS[i];
        if !super::shortcuts::claim(i) {
            // The portal got there first. Reading the devices happens underneath
            // the compositor rather than instead of it, so a key the desktop has
            // already turned into an activation still arrives here as well.
            tracing::debug!("hotkey {id} was already delivered by the portal");
            continue;
        }
        tracing::info!("hotkey {id} delivered");
        match id {
            HK_ID_RECORD => toggle_recording(state),
            HK_ID_PLAY => toggle_playback(state),
            HK_ID_STOP => stop_everything(state),
            HK_ID_PAUSE => toggle_pause(state),
            HK_ID_FASTER => nudge_speed(state, 1.25),
            HK_ID_SLOWER => nudge_speed(state, 0.8),
            HK_ID_SKIP => state.skip_step.store(true, Ordering::Relaxed),
            _ => {}
        }
    }
    matched
}
