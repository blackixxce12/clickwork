//! Windows virtual keys and scan codes against Linux `evdev` key codes.
//!
//! The macro file format is the Windows one - a key event is a virtual key, a scan
//! code and an "extended" flag - and it stays that way so a recording moves between
//! the two builds untouched. Linux key codes 1..=88 *are* the PC/AT set-1 scan codes
//! the Windows column holds, which is not a coincidence: both descend from the same
//! keyboard controller. Everything past that is the E0-prefixed set, which Windows
//! reports as the base code plus the extended flag and Linux numbers on its own.

use crate::MouseButton;

/// (evdev code, virtual key, scan code, extended)
const TABLE: &[(u16, u16, u16, bool)] = &[
    (1, 0x1B, 0x01, false),    // Esc
    (2, 0x31, 0x02, false),    // 1
    (3, 0x32, 0x03, false),
    (4, 0x33, 0x04, false),
    (5, 0x34, 0x05, false),
    (6, 0x35, 0x06, false),
    (7, 0x36, 0x07, false),
    (8, 0x37, 0x08, false),
    (9, 0x38, 0x09, false),
    (10, 0x39, 0x0A, false),
    (11, 0x30, 0x0B, false),   // 0
    (12, 0xBD, 0x0C, false),   // -
    (13, 0xBB, 0x0D, false),   // =
    (14, 0x08, 0x0E, false),   // Backspace
    (15, 0x09, 0x0F, false),   // Tab
    (16, 0x51, 0x10, false),   // Q
    (17, 0x57, 0x11, false),
    (18, 0x45, 0x12, false),
    (19, 0x52, 0x13, false),
    (20, 0x54, 0x14, false),
    (21, 0x59, 0x15, false),
    (22, 0x55, 0x16, false),
    (23, 0x49, 0x17, false),
    (24, 0x4F, 0x18, false),
    (25, 0x50, 0x19, false),   // P
    (26, 0xDB, 0x1A, false),   // [
    (27, 0xDD, 0x1B, false),   // ]
    (28, 0x0D, 0x1C, false),   // Enter
    (29, 0xA2, 0x1D, false),   // Left Ctrl
    (30, 0x41, 0x1E, false),   // A
    (31, 0x53, 0x1F, false),
    (32, 0x44, 0x20, false),
    (33, 0x46, 0x21, false),
    (34, 0x47, 0x22, false),
    (35, 0x48, 0x23, false),
    (36, 0x4A, 0x24, false),
    (37, 0x4B, 0x25, false),
    (38, 0x4C, 0x26, false),   // L
    (39, 0xBA, 0x27, false),   // ;
    (40, 0xDE, 0x28, false),   // '
    (41, 0xC0, 0x29, false),   // `
    (42, 0xA0, 0x2A, false),   // Left Shift
    (43, 0xDC, 0x2B, false),   // backslash
    (44, 0x5A, 0x2C, false),   // Z
    (45, 0x58, 0x2D, false),
    (46, 0x43, 0x2E, false),
    (47, 0x56, 0x2F, false),
    (48, 0x42, 0x30, false),
    (49, 0x4E, 0x31, false),
    (50, 0x4D, 0x32, false),   // M
    (51, 0xBC, 0x33, false),   // ,
    (52, 0xBE, 0x34, false),   // .
    (53, 0xBF, 0x35, false),   // /
    (54, 0xA1, 0x36, false),   // Right Shift
    (55, 0x6A, 0x37, false),   // Num *
    (56, 0xA4, 0x38, false),   // Left Alt
    (57, 0x20, 0x39, false),   // Space
    (58, 0x14, 0x3A, false),   // CapsLock
    (59, 0x70, 0x3B, false),   // F1
    (60, 0x71, 0x3C, false),
    (61, 0x72, 0x3D, false),
    (62, 0x73, 0x3E, false),
    (63, 0x74, 0x3F, false),
    (64, 0x75, 0x40, false),
    (65, 0x76, 0x41, false),
    (66, 0x77, 0x42, false),
    (67, 0x78, 0x43, false),
    (68, 0x79, 0x44, false),   // F10
    (69, 0x90, 0x45, true),    // NumLock
    (70, 0x91, 0x46, false),   // ScrollLock
    (71, 0x67, 0x47, false),   // Num 7
    (72, 0x68, 0x48, false),
    (73, 0x69, 0x49, false),
    (74, 0x6D, 0x4A, false),   // Num -
    (75, 0x64, 0x4B, false),
    (76, 0x65, 0x4C, false),
    (77, 0x66, 0x4D, false),
    (78, 0x6B, 0x4E, false),   // Num +
    (79, 0x61, 0x4F, false),
    (80, 0x62, 0x50, false),
    (81, 0x63, 0x51, false),
    (82, 0x60, 0x52, false),   // Num 0
    (83, 0x6E, 0x53, false),   // Num .
    (86, 0xE2, 0x56, false),   // the 102nd key
    (87, 0x7A, 0x57, false),   // F11
    (88, 0x7B, 0x58, false),   // F12
    (96, 0x0D, 0x1C, true),    // Num Enter
    (97, 0xA3, 0x1D, true),    // Right Ctrl
    (98, 0x6F, 0x35, true),    // Num /
    (99, 0x2C, 0x37, true),    // PrintScreen
    (100, 0xA5, 0x38, true),   // Right Alt
    (102, 0x24, 0x47, true),   // Home
    (103, 0x26, 0x48, true),   // Up
    (104, 0x21, 0x49, true),   // PageUp
    (105, 0x25, 0x4B, true),   // Left
    (106, 0x27, 0x4D, true),   // Right
    (107, 0x23, 0x4F, true),   // End
    (108, 0x28, 0x50, true),   // Down
    (109, 0x22, 0x51, true),   // PageDown
    (110, 0x2D, 0x52, true),   // Insert
    (111, 0x2E, 0x53, true),   // Delete
    (113, 0xAD, 0x20, true),   // Mute
    (114, 0xAE, 0x2E, true),   // Volume down
    (115, 0xAF, 0x30, true),   // Volume up
    (117, 0xBB, 0x59, false),  // Num =
    (119, 0x13, 0x45, false),  // Pause
    (121, 0x6C, 0x7E, false),  // Num , (separator)
    (125, 0x5B, 0x5B, true),   // Left Super
    (126, 0x5C, 0x5C, true),   // Right Super
    (127, 0x5D, 0x5D, true),   // Menu
    (142, 0x5F, 0x5F, true),   // Sleep
    (155, 0xB4, 0x6C, true),   // Mail
    (158, 0xA6, 0x6A, true),   // Browser back
    (159, 0xA7, 0x69, true),   // Browser forward
    (163, 0xB0, 0x19, true),   // Next track
    (164, 0xB3, 0x22, true),   // Play/pause
    (165, 0xB1, 0x10, true),   // Previous track
    (166, 0xB2, 0x24, true),   // Stop
    (172, 0xAC, 0x32, true),   // Browser home
    (140, 0xB7, 0x21, true),   // Calculator
    (217, 0xAA, 0x65, true),   // Browser search
    (183, 0x7C, 0x64, false),  // F13
    (184, 0x7D, 0x65, false),
    (185, 0x7E, 0x66, false),
    (186, 0x7F, 0x67, false),
    (187, 0x80, 0x68, false),
    (188, 0x81, 0x69, false),
    (189, 0x82, 0x6A, false),
    (190, 0x83, 0x6B, false),
    (191, 0x84, 0x6C, false),
    (192, 0x85, 0x6D, false),
    (193, 0x86, 0x6E, false),
    (194, 0x87, 0x76, false),  // F24
];

/// The Windows description of a Linux key, or `None` for a key Windows has no
/// virtual key for (the Japanese conversion keys, most media keys).
pub fn vk_of_key(code: u16) -> Option<(u16, u16, bool)> {
    TABLE.iter().find(|(c, ..)| *c == code).map(|&(_, vk, scan, ext)| (vk, scan, ext))
}

/// The Linux key for a Windows scan code, which is the faithful direction: a
/// recording carries the scan code of the key that was physically pressed.
pub fn key_of_scan(scan: u16, extended: bool) -> Option<u16> {
    if scan == 0 {
        return None;
    }
    TABLE
        .iter()
        .find(|&&(_, _, s, e)| s == scan && e == extended)
        .or_else(|| TABLE.iter().find(|&&(_, _, s, _)| s == scan))
        .map(|&(c, ..)| c)
}

/// The Linux key for a virtual key alone, for script steps and hotkeys that name
/// a key rather than a position. The unsided modifiers map to their left halves.
pub fn key_of_vk(vk: u16) -> Option<u16> {
    let vk = match vk {
        0x10 => 0xA0, // Shift
        0x11 => 0xA2, // Ctrl
        0x12 => 0xA4, // Alt
        v => v,
    };
    TABLE.iter().find(|&&(_, v, ..)| v == vk).map(|&(c, ..)| c)
}

/// The Linux key an event names, preferring the scan code.
pub fn key_of_event(vk: u16, scan: u16, extended: bool) -> Option<u16> {
    key_of_scan(scan, extended).or_else(|| key_of_vk(vk))
}

/// Is this a modifier, and which generic virtual key does it stand for?
pub fn modifier_of_vk(vk: u16) -> Option<u16> {
    match vk {
        0x10 | 0xA0 | 0xA1 => Some(0x10),
        0x11 | 0xA2 | 0xA3 => Some(0x11),
        0x12 | 0xA4 | 0xA5 => Some(0x12),
        0x5B | 0x5C => Some(0x5B),
        _ => None,
    }
}

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;
pub const BTN_SIDE: u16 = 0x113;
pub const BTN_EXTRA: u16 = 0x114;

pub fn button_of_btn(code: u16) -> Option<MouseButton> {
    Some(match code {
        BTN_LEFT => MouseButton::Left,
        BTN_RIGHT => MouseButton::Right,
        BTN_MIDDLE => MouseButton::Middle,
        BTN_SIDE => MouseButton::X1,
        BTN_EXTRA => MouseButton::X2,
        _ => return None,
    })
}

pub fn btn_of_button(b: MouseButton) -> u32 {
    (match b {
        MouseButton::Left => BTN_LEFT,
        MouseButton::Right => BTN_RIGHT,
        MouseButton::Middle => BTN_MIDDLE,
        MouseButton::X1 => BTN_SIDE,
        MouseButton::X2 => BTN_EXTRA,
    }) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_keys_share_their_scan_code() {
        for &(code, _, scan, ext) in TABLE {
            if code <= 88 && code != 69 {
                assert_eq!(code, scan, "key {code}");
                assert!(!ext, "key {code} is not extended");
            }
        }
    }

    #[test]
    fn round_trips() {
        for &(code, vk, scan, ext) in TABLE {
            assert_eq!(vk_of_key(code), Some((vk, scan, ext)));
            assert_eq!(key_of_scan(scan, ext), Some(code), "scan {scan:#x} ext {ext}");
        }
        // Enter and Num Enter share a scan code and differ by the flag.
        assert_eq!(key_of_scan(0x1C, false), Some(28));
        assert_eq!(key_of_scan(0x1C, true), Some(96));
        assert_eq!(key_of_vk(0x10), Some(42));
        assert_eq!(key_of_vk(0x0D), Some(28));
        assert_eq!(key_of_event(0, 0x48, true), Some(103));
        assert_eq!(key_of_event(0x26, 0, false), Some(103));
    }

    #[test]
    fn every_vk_the_ui_offers_has_a_key() {
        for (_, vk) in crate::HOTKEY_CHOICES.iter().skip(1) {
            assert!(key_of_vk(*vk as u16).is_some(), "vk {vk:#x}");
        }
    }
}
