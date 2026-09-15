//! Hotkeys the compositor takes for us.
//!
//! An evdev hotkey sees the key but cannot keep it from the application in front:
//! F6 still reaches the browser, which opens whatever it has on F6. A compositor
//! keybind is the opposite - Hyprland consumes the key before any window sees it.
//! So on Hyprland every hotkey slot is also registered as a bind at run time:
//!
//! ```text
//! hl.bind("CTRL + F6", hl.dsp.exec_cmd("'/usr/bin/clickwork' --cmd record"),
//!         { description = "clickwork:record" })
//! ```
//!
//! The bound command is this program's own command line, which talks to the
//! running instance over its socket. Binds carry a description starting with
//! `clickwork:`, which is how ours are told apart from the user's, and a combo the
//! user already has a bind on is left alone - the evdev hotkey keeps it, with a
//! line in the log. Everything is undone on exit and while a new key is being
//! captured, so the capture sees the key and not the action.

use super::hypr;
use crate::{HK_IDS, Hotkey, PENDING_HOTKEYS};
use parking_lot::Mutex;
use serde::Deserialize;
use std::sync::atomic::{AtomicU32, Ordering};

/// Bit `i` set: slot `i` is served by a Hyprland bind, so the evdev hook must not
/// fire it a second time.
pub static HANDLED: AtomicU32 = AtomicU32::new(0);

/// The combos this process bound, so it removes only its own.
static BOUND: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The word `--cmd` takes for each slot, in `HK_IDS` order.
const WORDS: [&str; 7] = ["record", "play", "stop", "pause", "faster", "slower", "skip"];

/// The name Hyprland's bind parser knows a virtual key by. An X11 keysym, which is
/// also how the XDG shortcuts syntax spells a key, so the portal tier borrows it
/// rather than keeping a second copy of the same table.
pub(super) fn keysym_of_vk(vk: u32) -> Option<String> {
    Some(match vk {
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        0x41..=0x5A => ((vk as u8) as char).to_ascii_lowercase().to_string(),
        0x30..=0x39 => ((vk as u8) as char).to_string(),
        0x60..=0x69 => format!("KP_{}", vk - 0x60),
        0x6A => "KP_Multiply".into(),
        0x6B => "KP_Add".into(),
        0x6D => "KP_Subtract".into(),
        0x6E => "KP_Decimal".into(),
        0x6F => "KP_Divide".into(),
        0x13 => "Pause".into(),
        0x91 => "Scroll_Lock".into(),
        0x90 => "Num_Lock".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "Prior".into(),
        0x22 => "Next".into(),
        0x26 => "Up".into(),
        0x28 => "Down".into(),
        0x25 => "Left".into(),
        0x27 => "Right".into(),
        0x20 => "space".into(),
        0x0D => "Return".into(),
        0x09 => "Tab".into(),
        0x08 => "BackSpace".into(),
        0x1B => "Escape".into(),
        0x2C => "Print".into(),
        0xC0 => "grave".into(),
        0xBD => "minus".into(),
        0xBB => "equal".into(),
        0xDB => "bracketleft".into(),
        0xDD => "bracketright".into(),
        0xBA => "semicolon".into(),
        0xDE => "apostrophe".into(),
        0xDC => "backslash".into(),
        0xBC => "comma".into(),
        0xBE => "period".into(),
        0xBF => "slash".into(),
        _ => return None,
    })
}

/// `CTRL + ALT + F9`, or `None` for a key Hyprland cannot be asked for.
fn combo_of(hk: &Hotkey) -> Option<String> {
    let key = keysym_of_vk(hk.vk)?;
    let mut parts: Vec<&str> = Vec::new();
    if hk.ctrl {
        parts.push("CTRL");
    }
    if hk.alt {
        parts.push("ALT");
    }
    if hk.shift {
        parts.push("SHIFT");
    }
    let mut combo = parts.join(" + ");
    if !combo.is_empty() {
        combo.push_str(" + ");
    }
    combo.push_str(&key);
    Some(combo)
}

/// Hyprland's modifier mask for a hotkey: SHIFT 1, CTRL 4, ALT 8.
fn modmask_of(hk: &Hotkey) -> u32 {
    (hk.shift as u32) | ((hk.ctrl as u32) << 2) | ((hk.alt as u32) << 3)
}

#[derive(Deserialize, Default)]
struct Bind {
    #[serde(default)]
    modmask: u32,
    #[serde(default)]
    key: String,
    #[serde(default)]
    description: String,
}

fn current_binds() -> Vec<Bind> {
    hypr::request("j/binds")
        .and_then(|raw| serde_json::from_str::<Vec<Bind>>(&raw).ok())
        .unwrap_or_default()
}

fn eval(code: &str) -> bool {
    match hypr::request(&format!("eval {code}")) {
        Some(r) if r.trim() == "ok" => true,
        Some(r) => {
            tracing::warn!("hyprland eval `{code}` answered: {}", r.trim());
            false
        }
        None => false,
    }
}

/// Single-quoted for `sh -c`, which is how Hyprland runs an `exec`.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Removes every bind this process made, and any `clickwork:` bind an earlier
/// process left behind.
pub fn unbind_all() {
    if !hypr::available() {
        return;
    }
    let mut mine = BOUND.lock();
    let mut combos: Vec<String> = mine.drain(..).collect();
    // Leftovers from a crash carry our description but are not in the list.
    for b in current_binds() {
        if b.description.starts_with("clickwork:") {
            let mut parts: Vec<&str> = Vec::new();
            if b.modmask & 4 != 0 {
                parts.push("CTRL");
            }
            if b.modmask & 8 != 0 {
                parts.push("ALT");
            }
            if b.modmask & 1 != 0 {
                parts.push("SHIFT");
            }
            let mut combo = parts.join(" + ");
            if !combo.is_empty() {
                combo.push_str(" + ");
            }
            combo.push_str(&b.key);
            if !combos.contains(&combo) {
                combos.push(combo);
            }
        }
    }
    for c in &combos {
        eval(&format!("hl.unbind({})", hypr::lua_str(c)));
    }
    HANDLED.store(0, Ordering::Relaxed);
}

/// Binds every configured slot the compositor will let us have. Idempotent: the
/// old binds go first, so calling it after every change is the whole protocol.
pub fn rebind() {
    if !hypr::available() {
        return;
    }
    unbind_all();
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(_) => "clickwork".into(),
    };
    let existing = current_binds();
    let hk = *PENDING_HOTKEYS.lock();
    let mut handled = 0u32;
    let mut mine = BOUND.lock();
    for (i, k) in hk.iter().enumerate() {
        if k.vk == 0 {
            continue;
        }
        let Some(combo) = combo_of(k) else {
            tracing::info!("hotkey {} has no compositor name; the evdev hook keeps it", k.label());
            continue;
        };
        let taken = existing.iter().any(|b| {
            b.modmask == modmask_of(k)
                && b.key.eq_ignore_ascii_case(combo.rsplit(" + ").next().unwrap_or(""))
                && !b.description.starts_with("clickwork:")
        });
        if taken {
            tracing::warn!(
                "{combo} is already bound in Hyprland; leaving it, the evdev hotkey stays for slot {}",
                HK_IDS[i]
            );
            continue;
        }
        let cmd = format!("{} --cmd {}", shell_quote(&exe), WORDS[i]);
        let code = format!(
            "hl.bind({}, hl.dsp.exec_cmd({}), {{ description = {} }})",
            hypr::lua_str(&combo),
            hypr::lua_str(&cmd),
            hypr::lua_str(&format!("clickwork:{}", WORDS[i]))
        );
        if eval(&code) {
            mine.push(combo);
            handled |= 1 << i;
        }
    }
    HANDLED.store(handled, Ordering::Relaxed);
    if handled != 0 {
        tracing::info!(
            "compositor hotkeys bound: {}",
            hk.iter()
                .enumerate()
                .filter(|(i, _)| handled & (1 << i) != 0)
                .map(|(_, k)| k.label())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_read_the_way_the_config_writes_them() {
        let f9 = Hotkey { vk: 0x78, ctrl: false, alt: false, shift: false };
        assert_eq!(combo_of(&f9).as_deref(), Some("F9"));
        let ca = Hotkey { vk: 0x78, ctrl: true, alt: true, shift: false };
        assert_eq!(combo_of(&ca).as_deref(), Some("CTRL + ALT + F9"));
        assert_eq!(modmask_of(&ca), 12);
        let q = Hotkey { vk: 0x51, ctrl: false, alt: false, shift: true };
        assert_eq!(combo_of(&q).as_deref(), Some("SHIFT + q"));
        assert_eq!(shell_quote("/a b/it's"), "'/a b/it'\\''s'");
        for (_, vk) in crate::HOTKEY_CHOICES.iter().skip(1) {
            assert!(keysym_of_vk(*vk).is_some(), "vk {vk:#x}");
        }
    }
}
