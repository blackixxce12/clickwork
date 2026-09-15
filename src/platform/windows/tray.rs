use super::win32::*;
use super::*;
use std::ffi::c_void;
use std::sync::atomic::AtomicIsize;

static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static TRAY_ADDED: AtomicBool = AtomicBool::new(false);

/// The hidden window the tray icon belongs to, or a null handle when there is no
/// icon. `Notify` needs it: a balloon has to come from an icon that exists.
pub fn hwnd() -> HWND {
    if !TRAY_ADDED.load(Ordering::Relaxed) {
        return HWND::default();
    }
    HWND(TRAY_HWND.load(Ordering::Relaxed) as *mut c_void)
}

fn icon_handle(hinst: HINSTANCE) -> HICON {
    unsafe {
        // Resource id 1 is what winresource assigns to the embedded icon.
        //
        // The cast is `MAKEINTRESOURCE`: Win32 encodes a resource *number* in the
        // pointer argument rather than passing a pointer at all. Clippy reads it
        // as a hand-made dangling pointer and offers `std::ptr::dangling`, which
        // would be a different value and would silently stop finding the icon.
        #[allow(clippy::manual_dangling_ptr)]
        if let Ok(icon) = LoadIconW(Some(hinst), PCWSTR(1 as *const u16))
            && !icon.is_invalid() {
                return icon;
            }
        LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
    }
}

fn notify_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_APP_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    let tip = wide(APP_TITLE);
    let n = tip.len().min(nid.szTip.len());
    nid.szTip[..n].copy_from_slice(&tip[..n]);
    nid
}

/// Creates the message-only window that owns the tray icon.
pub fn init() {
    unsafe {
        let hinst = GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).unwrap_or_default();
        let class = w!("ClickworkTrayWnd");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Clickwork Tray"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinst),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                warn!("tray window could not be created: {e}");
                return;
            }
        };
        TRAY_HWND.store(hwnd.0 as isize, Ordering::Relaxed);

        let nid = notify_data(hwnd, icon_handle(hinst));
        if Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            TRAY_ADDED.store(true, Ordering::Relaxed);
            info!("tray icon added");
        } else {
            warn!("Shell_NotifyIconW(NIM_ADD) failed");
        }
    }
}

/// True once the icon is actually in the notification area.
pub fn is_active() -> bool {
    TRAY_ADDED.load(Ordering::Relaxed)
}

pub fn shutdown() {
    if !TRAY_ADDED.swap(false, Ordering::Relaxed) {
        return;
    }
    unsafe {
        let hwnd = HWND(TRAY_HWND.load(Ordering::Relaxed) as *mut c_void);
        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        let _ = DestroyWindow(hwnd);
    }
}

unsafe fn show_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let show_label = if WINDOW_VISIBLE.load(Ordering::Relaxed) {
            w!("Hide window")
        } else {
            w!("Show window")
        };
        let _ = AppendMenuW(menu, MF_STRING, TRAY_ID_SHOW as usize, show_label);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, TRAY_ID_RECORD as usize, w!("Record / stop"));
        let _ = AppendMenuW(menu, MF_STRING, TRAY_ID_PLAY as usize, w!("Play / stop"));
        let _ = AppendMenuW(menu, MF_STRING, TRAY_ID_STOP as usize, w!("Emergency stop"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, TRAY_ID_EXIT as usize, w!("Exit"));

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Required so the menu closes when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            pt.x,
            pt.y,
            Some(0),
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);

        let state = GLOBAL_STATE.get();
        match cmd.0 as u32 {
            TRAY_ID_SHOW => toggle_main_window(),
            TRAY_ID_RECORD => {
                if let Some(s) = state {
                    toggle_recording(s);
                }
            }
            TRAY_ID_PLAY => {
                if let Some(s) = state {
                    toggle_playback(s);
                }
            }
            TRAY_ID_STOP => {
                if let Some(s) = state {
                    stop_everything(s);
                }
            }
            TRAY_ID_EXIT => quit_application(),
            _ => {}
        }
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    unsafe {
        if msg == WM_APP_TRAY {
            match lp.0 as u32 {
                0x0202 => toggle_main_window(), // WM_LBUTTONUP
                0x0205 | 0x007B => show_menu(hwnd), // WM_RBUTTONUP / WM_CONTEXTMENU
                _ => {}
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}
