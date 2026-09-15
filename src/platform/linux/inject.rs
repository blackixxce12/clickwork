//! Synthetic input, through the compositor rather than around it.
//!
//! `SendInput` has two Wayland counterparts and this uses both:
//! `zwlr_virtual_pointer_v1` for the mouse and `zwp_virtual_keyboard_v1` for the
//! keyboard. Neither needs root, neither needs the `input` group, and both arrive
//! at the focused application exactly as a real device's events would - which is
//! the property that matters, because it is what makes a game that reads scan
//! codes see the same scan codes it saw when the macro was recorded.
//!
//! A virtual keyboard carries its own keymap, and the compositor interprets our
//! key codes with *that* keymap, not the physical keyboard's. So the first thing
//! done is to compile the same layout the user is typing in - Hyprland says which -
//! and hand it over, group and all. A recorded `;` then replays as `;` on an
//! English layout and `ж` on a Russian one, which is what the physical key does.
//!
//! Text that is not a keystroke - a script's `Type` step, an expansion - goes
//! through a second, generated keymap in which every needed character has a key of
//! its own. That is how `wtype` does it, and it types any Unicode at all without
//! knowing where anything is on the keyboard.

use super::keymap;
use super::wl::memfd_with;
use crate::MouseButton;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::os::fd::AsFd as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_pointer, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy as _, QueueHandle, delegate_noop};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1, zwp_virtual_keyboard_v1,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};
use xkbcommon::xkb;

struct St;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for St {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
delegate_noop!(St: ignore wl_seat::WlSeat);
delegate_noop!(St: zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1);
delegate_noop!(St: zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1);
delegate_noop!(St: zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1);
delegate_noop!(St: zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1);

#[derive(PartialEq)]
enum Map {
    /// The user's own layout.
    Native,
    /// A generated one: these characters, one key each, in this order.
    Unicode(Vec<char>),
}

struct Injector {
    conn: Connection,
    queue: EventQueue<St>,
    st: St,
    ptr: zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
    kbd: Option<zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1>,
    xkb: xkb::Context,
    native: Option<xkb::Keymap>,
    native_text: String,
    map: Map,
    state: Option<xkb::State>,
    group: u32,
    group_checked_us: u64,
    t0: Instant,
    down: HashSet<u16>,
    buttons: HashSet<u32>,
}

// The compositor objects are plain handles; nothing here is tied to a thread.
unsafe impl Send for Injector {}

static INJ: Mutex<Option<Injector>> = Mutex::new(None);
static LAST_FAIL_US: AtomicU64 = AtomicU64::new(0);
static EVER_OK: AtomicBool = AtomicBool::new(false);

/// Keyboard layout names, from the compositor's configuration or the environment.
pub fn layout_names() -> (String, String, String, String, String) {
    let mut rules = String::new();
    let mut model = String::new();
    let mut layout = String::new();
    let mut variant = String::new();
    let mut options = String::new();
    if super::hypr::available() {
        if let Some(k) = super::hypr::main_keyboard() {
            rules = k.rules;
            model = k.model;
            layout = k.layout;
            variant = k.variant;
            options = k.options;
        }
        if layout.is_empty() {
            layout = super::hypr::option_str("input:kb_layout");
            variant = super::hypr::option_str("input:kb_variant");
            options = super::hypr::option_str("input:kb_options");
            rules = super::hypr::option_str("input:kb_rules");
            model = super::hypr::option_str("input:kb_model");
        }
    }
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    if layout.is_empty() {
        layout = env("XKB_DEFAULT_LAYOUT");
        variant = env("XKB_DEFAULT_VARIANT");
        options = env("XKB_DEFAULT_OPTIONS");
        rules = env("XKB_DEFAULT_RULES");
        model = env("XKB_DEFAULT_MODEL");
    }
    (rules, model, layout, variant, options)
}

fn active_group() -> u32 {
    if super::hypr::available()
        && let Some(k) = super::hypr::main_keyboard()
    {
        return k.active_layout_index.max(0) as u32;
    }
    0
}

impl Injector {
    fn connect() -> Option<Self> {
        let conn = Connection::connect_to_env().ok()?;
        let (globals, queue) = registry_queue_init::<St>(&conn).ok()?;
        let qh = queue.handle();
        let seat = globals.bind::<wl_seat::WlSeat, St, ()>(&qh, 1..=9, ()).ok()?;
        let pm = globals
            .bind::<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1, St, ()>(
                &qh,
                1..=2,
                (),
            )
            .ok()?;
        let ptr = pm.create_virtual_pointer(Some(&seat), &qh, ());
        let kbd = globals
            .bind::<zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1, St, ()>(
                &qh,
                1..=1,
                (),
            )
            .ok()
            .map(|km| km.create_virtual_keyboard(&seat, &qh, ()));
        if kbd.is_none() {
            tracing::warn!("no zwp_virtual_keyboard_v1 - keyboard playback is unavailable");
        }
        let xkb = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let (rules, model, layout, variant, options) = layout_names();
        let native = xkb::Keymap::new_from_names(
            &xkb,
            &rules,
            &model,
            &layout,
            &variant,
            if options.is_empty() { None } else { Some(options.clone()) },
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        );
        if native.is_none() {
            tracing::warn!(
                "could not compile keymap rules='{rules}' model='{model}' layout='{layout}' \
                 variant='{variant}' options='{options}' - falling back to plain us"
            );
        }
        let native = native.or_else(|| {
            xkb::Keymap::new_from_names(
                &xkb,
                "",
                "",
                "us",
                "",
                None,
                xkb::KEYMAP_COMPILE_NO_FLAGS,
            )
        });
        let native_text = native
            .as_ref()
            .map(|k| k.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1))
            .unwrap_or_default();
        let mut me = Self {
            conn,
            queue,
            st: St,
            ptr,
            kbd,
            xkb,
            native,
            native_text,
            map: Map::Unicode(Vec::new()),
            state: None,
            group: 0,
            group_checked_us: 0,
            t0: Instant::now(),
            down: HashSet::new(),
            buttons: HashSet::new(),
        };
        me.ensure_native();
        me.flush();
        tracing::info!(
            "virtual input ready: layout '{layout}' (variant '{variant}', options '{options}'), \
             group {}",
            me.group
        );
        Some(me)
    }

    fn now_ms(&self) -> u32 {
        self.t0.elapsed().as_millis() as u32
    }

    fn flush(&mut self) {
        let _ = self.conn.flush();
        // Nothing is expected back, but the queue is drained so an error from the
        // compositor does not sit unread forever.
        let _ = self.queue.dispatch_pending(&mut self.st);
    }

    fn upload(&mut self, text: &str) -> bool {
        let Some(kbd) = self.kbd.as_ref() else { return false };
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        let Some(fd) = memfd_with(&bytes) else { return false };
        kbd.keymap(1, fd.as_fd(), bytes.len() as u32);
        true
    }

    fn send_mods(&mut self) {
        let Some(kbd) = self.kbd.as_ref() else { return };
        let Some(st) = self.state.as_ref() else { return };
        kbd.modifiers(
            st.serialize_mods(xkb::STATE_MODS_DEPRESSED),
            st.serialize_mods(xkb::STATE_MODS_LATCHED),
            st.serialize_mods(xkb::STATE_MODS_LOCKED),
            st.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE),
        );
    }

    /// Puts the user's own layout on the virtual keyboard, if it is not already.
    fn ensure_native(&mut self) {
        let now = crate::now_us();
        let refresh_group = now.saturating_sub(self.group_checked_us) > 1_000_000;
        if self.map == Map::Native && !refresh_group {
            return;
        }
        let Some(km) = self.native.clone() else { return };
        if self.map != Map::Native {
            let text = self.native_text.clone();
            if !self.upload(&text) {
                return;
            }
            self.state = Some(xkb::State::new(&km));
            self.map = Map::Native;
            self.group_checked_us = 0;
        }
        if now.saturating_sub(self.group_checked_us) > 1_000_000 {
            self.group_checked_us = now;
            let g = active_group().min(km.num_layouts().saturating_sub(1));
            if g != self.group || self.state.as_ref().is_some_and(|s| s.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE) != g) {
                self.group = g;
                if let Some(st) = self.state.as_mut() {
                    st.update_mask(0, 0, 0, 0, 0, g);
                }
            }
        }
        self.send_mods();
    }

    fn key(&mut self, code: u16, down: bool) {
        self.ensure_native();
        let Some(kbd) = self.kbd.as_ref() else { return };
        kbd.key(self.now_ms(), code as u32, if down { 1 } else { 0 });
        if let Some(st) = self.state.as_mut() {
            st.update_key(
                xkb::Keycode::new(code as u32 + 8),
                if down { xkb::KeyDirection::Down } else { xkb::KeyDirection::Up },
            );
        }
        self.send_mods();
        if down {
            self.down.insert(code);
        } else {
            self.down.remove(&code);
        }
        self.flush();
    }

    /// Types `text` through a generated keymap: each distinct character gets a key.
    fn type_text(&mut self, text: &str) {
        if self.kbd.is_none() || text.is_empty() {
            return;
        }
        // Anything the macro still holds down would be interpreted under the new
        // keymap, and a held Shift there is a stuck Shift.
        let held: Vec<u16> = self.down.iter().copied().collect();
        for c in held {
            self.key(c, false);
        }
        let chars: Vec<char> = text.chars().filter(|c| *c != '\r').collect();
        let mut i = 0;
        while i < chars.len() {
            // One keymap per batch of distinct characters; 247 keys fit.
            let mut batch: Vec<char> = Vec::new();
            let mut end = i;
            while end < chars.len() {
                let c = chars[end];
                if !batch.contains(&c) {
                    if batch.len() == 247 {
                        break;
                    }
                    batch.push(c);
                }
                end += 1;
            }
            let keymap = unicode_keymap(&batch);
            if !self.upload(&keymap) {
                return;
            }
            self.state = xkb::Keymap::new_from_string(
                &self.xkb,
                keymap,
                xkb::KEYMAP_FORMAT_TEXT_V1,
                xkb::KEYMAP_COMPILE_NO_FLAGS,
            )
            .map(|k| xkb::State::new(&k));
            self.map = Map::Unicode(batch.clone());
            if let Some(kbd) = self.kbd.as_ref() {
                kbd.modifiers(0, 0, 0, 0);
            }
            self.flush();
            let Some(kbd) = self.kbd.clone() else { return };
            for &c in &chars[i..end] {
                let Some(pos) = batch.iter().position(|b| *b == c) else { continue };
                let code = (pos + 1) as u32;
                kbd.key(self.now_ms(), code, 1);
                self.flush();
                std::thread::sleep(std::time::Duration::from_micros(1500));
                kbd.key(self.now_ms(), code, 0);
                self.flush();
                std::thread::sleep(std::time::Duration::from_micros(1500));
            }
            i = end;
        }
        // Back to the real layout, so the next recorded scan code means what the
        // key it came from means.
        self.ensure_native();
        self.flush();
    }

    fn button(&mut self, code: u32, down: bool) {
        let state = if down { wl_pointer::ButtonState::Pressed } else { wl_pointer::ButtonState::Released };
        self.ptr.button(self.now_ms(), code, state);
        self.ptr.frame();
        if down {
            self.buttons.insert(code);
        } else {
            self.buttons.remove(&code);
        }
        self.flush();
    }

    fn motion_abs(&mut self, lx: f64, ly: f64) {
        let l = super::geom::layout();
        let (vx, vy, vw, vh) = l.virtual_logical();
        let x = (lx - vx as f64).round().clamp(0.0, (vw - 1).max(0) as f64) as u32;
        let y = (ly - vy as f64).round().clamp(0.0, (vh - 1).max(0) as f64) as u32;
        self.ptr.motion_absolute(self.now_ms(), x, y, vw.max(1) as u32, vh.max(1) as u32);
        self.ptr.frame();
        self.flush();
    }

    fn motion_rel(&mut self, dx: f64, dy: f64) {
        self.ptr.motion(self.now_ms(), dx, dy);
        self.ptr.frame();
        self.flush();
    }

    /// `notches` in wheel clicks, positive away from the user or to the right.
    fn wheel(&mut self, notches: f64, horizontal: bool) {
        let axis = if horizontal {
            wl_pointer::Axis::HorizontalScroll
        } else {
            wl_pointer::Axis::VerticalScroll
        };
        // Wayland's vertical axis grows towards the user, Windows' away.
        let dir = if horizontal { 1.0 } else { -1.0 };
        let value = notches * 15.0 * dir;
        let t = self.now_ms();
        self.ptr.axis_source(wl_pointer::AxisSource::Wheel);
        if self.ptr.version() >= 2 {
            self.ptr.axis_discrete(t, axis, value, (notches * dir).round() as i32);
        } else {
            self.ptr.axis(t, axis, value);
        }
        self.ptr.frame();
        self.flush();
    }

    fn release_all(&mut self) {
        let keys: Vec<u16> = self.down.iter().copied().collect();
        for k in keys {
            self.key(k, false);
        }
        let btns: Vec<u32> = self.buttons.iter().copied().collect();
        for b in btns {
            self.button(b, false);
        }
    }
}

/// The keymap `wtype` would build: keycode 9 + n carries `chars[n]`.
fn unicode_keymap(chars: &[char]) -> String {
    let mut codes = String::new();
    let mut syms = String::new();
    for (i, ch) in chars.iter().enumerate() {
        let kc = 9 + i;
        codes.push_str(&format!("\t<K{i}> = {kc};\n"));
        let name = match ch {
            '\n' => "Return".to_string(),
            '\t' => "Tab".to_string(),
            '\u{8}' => "BackSpace".to_string(),
            '\u{1b}' => "Escape".to_string(),
            c => format!("U{:04X}", *c as u32),
        };
        syms.push_str(&format!("\tkey <K{i}> {{ [ {name} ] }};\n"));
    }
    format!(
        "xkb_keymap {{\nxkb_keycodes \"(unnamed)\" {{\n\tminimum = 8;\n\tmaximum = 255;\n{codes}}};\n\
         xkb_types \"(unnamed)\" {{ include \"complete\" }};\n\
         xkb_compatibility \"(unnamed)\" {{ include \"complete\" }};\n\
         xkb_symbols \"(unnamed)\" {{\n{syms}}};\n}};\n"
    )
}

/// Runs `f` with the injector, connecting on first use. A failed connection is
/// retried no more than once every few seconds, so a session without a Wayland
/// display does not pay a socket attempt per event.
fn with<R>(f: impl FnOnce(&mut Injector) -> R) -> Option<R> {
    let mut slot = INJ.lock();
    if slot.is_none() {
        let now = crate::now_us();
        let last = LAST_FAIL_US.load(Ordering::Relaxed);
        if last != 0 && now.saturating_sub(last) < 5_000_000 {
            return None;
        }
        match Injector::connect() {
            Some(i) => {
                *slot = Some(i);
                EVER_OK.store(true, Ordering::Relaxed);
            }
            None => {
                LAST_FAIL_US.store(now, Ordering::Relaxed);
                if !EVER_OK.load(Ordering::Relaxed) {
                    tracing::warn!(
                        "virtual input could not be set up: no Wayland display, or the \
                         compositor offers neither zwlr_virtual_pointer_v1 nor \
                         zwp_virtual_keyboard_v1"
                    );
                }
                return None;
            }
        }
    }
    slot.as_mut().map(f)
}

/// Can input be injected at all on this session?
pub fn available() -> bool {
    with(|_| ()).is_some()
}

/// Moves the pointer to a physical position.
pub fn mouse_abs(px: i32, py: i32) {
    let (lx, ly) = super::geom::layout().to_logical(px, py);
    with(|i| i.motion_abs(lx, ly));
}

/// Moves the pointer by a physical delta.
pub fn mouse_rel(dx: i32, dy: i32) {
    if dx == 0 && dy == 0 {
        return;
    }
    let l = super::geom::layout();
    let (cx, cy) = super::hypr::cursor_pos()
        .map(|(x, y)| l.to_phys(x as f64, y as f64))
        .unwrap_or((0, 0));
    let s = l.scale_at_phys(cx, cy);
    with(|i| i.motion_rel(dx as f64 / s, dy as f64 / s));
}

pub fn button(b: MouseButton, down: bool) {
    with(|i| i.button(keymap::btn_of_button(b), down));
}

/// `delta` in Windows units: 120 per notch, positive away from the user.
pub fn wheel(delta: i32, horizontal: bool) {
    if delta == 0 {
        return;
    }
    with(|i| i.wheel(delta as f64 / 120.0, horizontal));
}

/// One evdev key, down or up, under the user's own layout.
pub fn key(code: u16, down: bool) {
    with(|i| i.key(code, down));
}

pub fn tap(code: u16) {
    with(|i| {
        i.key(code, true);
        std::thread::sleep(std::time::Duration::from_micros(2000));
        i.key(code, false);
    });
}

/// Types text, any text, through a keymap made for it.
pub fn type_text(text: &str) {
    with(|i| i.type_text(text));
}

pub fn release_all() {
    with(|i| i.release_all());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keymap_compiles() {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let text = unicode_keymap(&['П', 'р', 'и', 'в', 'е', 'т', ' ', '\n', '1', '€']);
        let km = xkb::Keymap::new_from_string(
            &ctx,
            text,
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .expect("keymap compiles");
        let st = xkb::State::new(&km);
        // Keycode 9 is 'П'.
        assert_eq!(st.key_get_utf8(xkb::Keycode::new(9)), "П");
        assert_eq!(st.key_get_utf8(xkb::Keycode::new(15)), " ");
        assert_eq!(st.key_get_one_sym(xkb::Keycode::new(16)), xkb::keysyms::KEY_Return.into());
        assert_eq!(st.key_get_utf8(xkb::Keycode::new(18)), "€");
    }
}
