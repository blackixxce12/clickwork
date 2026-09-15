//! Workspace isolation, the Hyprland reading of "virtual desktop".
//!
//! Recording and playback pause while this program's window is on a workspace
//! that is not on screen - the same rule the Windows build applies to virtual
//! desktops, for the same reason: a macro fired at a desktop the user has
//! switched away from is a macro clicking at things they cannot see.
//!
//! The other question answered here is the same one from the other direction: not
//! whether the screen has moved away from the macro but whether something has been
//! drawn over it, which on Hyprland means a layer-shell surface rather than a
//! desktop. See `shell_switcher_in_front`.

use super::{geom, hypr};
use crate::{DESKTOP_TTL_US, now_us};
use std::cell::RefCell;

thread_local! {
    static CACHE: RefCell<(u64, bool)> = const { RefCell::new((0, true)) };
    static SWITCHER: RefCell<(u64, bool)> = const { RefCell::new((0, false)) };
}

pub fn init_thread() {}

fn query() -> bool {
    if !hypr::available() {
        return true;
    }
    let Some(me) = hypr::own_window() else {
        // Hidden in the tray, or not up yet: nothing to be away from.
        return true;
    };
    if me.workspace.name.starts_with("special:clickwork") {
        return true;
    }
    hypr::visible_workspace_ids().contains(&me.workspace.id)
}

/// Throttled: the hook thread asks on every event.
pub fn is_app_on_active_desktop_cached(_: ()) -> bool {
    let now = now_us();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.0 == 0 || now.saturating_sub(c.0) >= DESKTOP_TTL_US {
            c.0 = now;
            c.1 = query();
        }
        c.1
    })
}

/// How much of a monitor a surface has to take on each side before a click meant for
/// the window underneath is going to land in the surface instead. A bar is a few per
/// cent of the height, a notification a corner, a side panel a quarter of the width;
/// a launcher or a locker takes the screen.
const COVERS_PCT: i64 = 80;

fn covers(l: &hypr::Layer, mons: &[geom::Mon]) -> bool {
    if l.level < hypr::LEVEL_TOP {
        return false;
    }
    // Going away is not the same as being unmapped. A bar or a launcher that fades
    // out stays in this list at zero opacity - the bar on this very session does -
    // and counting one of those would hold recording and playback for good, with
    // nothing on screen to explain why.
    if l.alpha <= 0.01 {
        return false;
    }
    let Some(m) = mons.iter().find(|m| m.name == l.monitor) else {
        // A monitor the layout does not know about is one nobody is looking at.
        return false;
    };
    l.w as i64 * 100 >= m.lw as i64 * COVERS_PCT && l.h as i64 * 100 >= m.lh as i64 * COVERS_PCT
}

/// The namespace of whatever is standing between the pointer and the windows, if
/// anything is. Public for `--doctor`, which is the only way to see this from outside
/// while it is happening.
pub fn blocking_layer() -> Option<String> {
    if !hypr::available() {
        return None;
    }
    let layout = geom::layout();
    let me = std::process::id() as i64;
    hypr::layers()
        .into_iter()
        // Our own overlay has exactly this shape - an overlay-layer surface over the
        // whole screen - and is the one surface of this shape that is not in the way:
        // its input region is empty and every click falls through it.
        .find(|l| l.pid != me && l.namespace != super::overlay::NAMESPACE && covers(l, &layout.mons))
        .map(|l| l.namespace)
}

/// True when something is drawn over the windows that would take the clicks.
///
/// Hyprland has no Task View, but it has what Task View is an instance of: a surface
/// above every window that swallows the input aimed at what is underneath. Here that
/// is `zwlr_layer_shell_v1` - a launcher, a locker of the swaylock kind, a region
/// picker like `slurp`, wlogout. None of them is a window, so the workspace check
/// above sees nothing wrong while the clicks land in them.
///
/// The compositor will not say which surfaces take input: `j/layers` gives a
/// namespace, a level and a rectangle, and keyboard interactivity is not in the
/// answer. So the test is the geometric one - a surface on `top` or `overlay`
/// covering nearly a whole monitor is, whatever it is, in front of everything on that
/// monitor. A launcher that draws only a small box in the middle of the screen slips
/// through this on purpose: the alternative is to treat every notification popup as a
/// reason to stop, and a macro that silently refuses to run is a worse bug than the
/// one being fixed. `hyprlock` slips through too - `ext-session-lock-v1` surfaces are
/// not layers and the IPC does not mention them at all.
///
/// Cached, where the Windows version is not: there the answer is two `user32` calls,
/// here it is a socket round-trip, and the hook thread asks on every event.
pub fn shell_switcher_in_front() -> bool {
    let now = now_us();
    SWITCHER.with(|c| {
        let mut c = c.borrow_mut();
        if c.0 == 0 || now.saturating_sub(c.0) >= DESKTOP_TTL_US {
            c.0 = now;
            let front = blocking_layer();
            if front.is_some() != c.1 {
                match &front {
                    Some(ns) => tracing::info!("`{ns}` is covering the screen; holding off"),
                    None => tracing::info!("the screen is clear again"),
                }
            }
            c.1 = front.is_some();
        }
        c.1
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon() -> geom::Mon {
        geom::Mon { name: "eDP-1".into(), lw: 1600, lh: 900, ..Default::default() }
    }

    /// Visible unless a test says otherwise: `Default` would give opacity zero, and
    /// every case here but one is about a surface somebody can actually see.
    fn layer(level: i64, w: i32, h: i32) -> hypr::Layer {
        hypr::Layer { monitor: "eDP-1".into(), level, w, h, alpha: 1.0, ..Default::default() }
    }

    #[test]
    fn a_bar_is_not_in_the_way() {
        assert!(!covers(&layer(2, 1600, 61), &[mon()]));
    }

    #[test]
    fn a_notification_is_not_in_the_way() {
        assert!(!covers(&layer(3, 420, 120), &[mon()]));
    }

    #[test]
    fn a_side_panel_is_not_in_the_way() {
        assert!(!covers(&layer(3, 400, 900), &[mon()]));
    }

    #[test]
    fn the_wallpaper_is_under_the_windows() {
        assert!(!covers(&layer(0, 1600, 900), &[mon()]));
    }

    #[test]
    fn a_launcher_over_the_screen_is_in_the_way() {
        assert!(covers(&layer(3, 1600, 900), &[mon()]));
    }

    /// The bar on the machine this was written on sits on `top`, mapped, the width of
    /// the screen, at opacity zero while it is hidden. Counting it would hold playback
    /// for the rest of the session.
    #[test]
    fn a_surface_that_has_faded_out_is_not_in_the_way() {
        let mut l = layer(3, 1600, 900);
        l.alpha = 0.0;
        assert!(!covers(&l, &[mon()]));
    }

    #[test]
    fn a_surface_on_a_monitor_nobody_has_is_not() {
        let mut l = layer(3, 1600, 900);
        l.monitor = "HDMI-A-2".into();
        assert!(!covers(&l, &[mon()]));
    }
}
