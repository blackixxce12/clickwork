//! Workspace isolation, the Hyprland reading of "virtual desktop".
//!
//! Recording and playback pause while this program's window is on a workspace
//! that is not on screen - the same rule the Windows build applies to virtual
//! desktops, for the same reason: a macro fired at a desktop the user has
//! switched away from is a macro clicking at things they cannot see.

use crate::{DESKTOP_TTL_US, now_us};
use std::cell::RefCell;

thread_local! {
    static CACHE: RefCell<(u64, bool)> = const { RefCell::new((0, true)) };
}

pub fn init_thread() {}

fn query() -> bool {
    if !super::hypr::available() {
        return true;
    }
    let Some(me) = super::hypr::own_window() else {
        // Hidden in the tray, or not up yet: nothing to be away from.
        return true;
    };
    if me.workspace.name.starts_with("special:clickwork") {
        return true;
    }
    super::hypr::visible_workspace_ids().contains(&me.workspace.id)
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

/// Hyprland has no Task View; nothing to keep clicks out of.
pub fn shell_switcher_in_front() -> bool {
    false
}
