use super::win32::*;
use super::{DESKTOP_TTL_US, now_us};
use std::cell::RefCell;

thread_local! {
    static VDM: RefCell<Option<IVirtualDesktopManager>> = const { RefCell::new(None) };
    static CACHE: RefCell<(u64, bool)> = const { RefCell::new((0, true)) };
}

pub fn init_thread() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        if let Ok(vdm) = CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL) {
            VDM.with(|v| *v.borrow_mut() = Some(vdm));
        }
    }
}

fn query(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return true;
    }
    VDM.with(|v| match v.borrow().as_ref() {
        Some(vdm) => unsafe {
            vdm.IsWindowOnCurrentVirtualDesktop(hwnd).unwrap_or_default().as_bool()
        },
        None => true,
    })
}

/// True when one of the shell's own window switchers owns the foreground.
///
/// `IsWindowOnCurrentVirtualDesktop` answers honestly and unhelpfully here: Task
/// View is an overlay drawn *on* the current desktop, so the desktop has not
/// changed and the check below passes while synthetic clicks land in the switcher
/// - where they create desktops, close them and move windows between them.
///
/// Asked every time rather than cached, unlike the desktop query: this is two
/// user32 calls and no COM, and a cache would let a couple of hundred milliseconds
/// of clicks through before it noticed. Only the two switcher classes are listed.
/// The Start menu and Search share a class with ordinary packaged apps, and a
/// macro that silently refuses to run against one of those would be a worse bug
/// than the one being fixed.
pub fn shell_switcher_in_front() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut buf = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut buf);
        if n <= 0 {
            return false;
        }
        matches!(
            String::from_utf16_lossy(&buf[..n as usize]).as_str(),
            // Windows 11 Task View and Alt+Tab.
            "XamlExplorerHostIslandWindow"
            // Windows 10 Task View.
            | "MultitaskingViewFrame"
        )
    }
}

/// Throttled: a COM round-trip per keystroke would get the hook killed.
pub fn is_app_on_active_desktop_cached(hwnd: HWND) -> bool {
    let now = now_us();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.0 == 0 || now.saturating_sub(c.0) >= DESKTOP_TTL_US {
            c.0 = now;
            c.1 = query(hwnd);
        }
        c.1
    })
}
