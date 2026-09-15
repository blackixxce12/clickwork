//! A see-through window over everything, drawing what the script just looked at.
//!
//! A layered Win32 window rather than a second eframe viewport, for three reasons.
//! The first is decisive: a transparent viewport needs the GL config chosen at
//! start-up to carry an alpha channel, and on a machine whose driver does not offer
//! one eframe says so in the log and creates the window *opaque* - which for a
//! full-screen overlay means covering the desktop in grey. A colour-keyed layered
//! window has no such dependency. The second is that this costs no GL surface and
//! no second render loop. The third is that GDI draws in physical pixels, which is
//! what every rectangle here is already measured in, so a mixed-DPI desktop needs no
//! conversion and nothing drifts on the second monitor.
use super::win32::*;
use super::{HUD_CORNER, SIGHTING, hud, wide};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

/// Painted where nothing should be seen. Windows makes every pixel of exactly
/// this colour transparent, and a transparent pixel is also not clickable.
const KEY: u32 = 0x0010_0F0E;

static RUNNING: AtomicBool = AtomicBool::new(false);
static WANTED: AtomicBool = AtomicBool::new(false);
static HWND_SLOT: AtomicIsize = AtomicIsize::new(0);

/// Turns the overlay on or off. Cheap and idempotent; safe to call every frame.
pub fn set_enabled(on: bool) {
    let was = WANTED.swap(on, Ordering::Relaxed);
    if on {
        // Not skipped when it was already wanted: switching it off and straight
        // back on can catch the thread mid-teardown, and then nothing would ever
        // put the window back while the box stayed ticked. Two atomics a frame.
        if !RUNNING.swap(true, Ordering::Relaxed) {
            let _ = std::thread::Builder::new()
                .name("overlay".into())
                .spawn(|| unsafe { run() });
        }
        return;
    }
    if !was {
        return;
    }
    // The thread notices `WANTED` and takes its own window down: a window may
    // only be destroyed by the thread that created it. The message is only to
    // wake it sooner than its next tick.
    let h = HWND_SLOT.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(h as *mut std::ffi::c_void)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

pub fn shutdown() {
    set_enabled(false);
}

unsafe fn run() {
    unsafe {
        let hinst = GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).unwrap_or_default();
        let class = w!("ClickworkOverlayWnd");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: class,
            // Erased in the key colour, so the window is invisible from the
            // moment it appears rather than from its first paint. Without this
            // the surface starts as whatever was in memory, and a full-screen
            // window showing that for one frame is startling.
            hbrBackground: CreateSolidBrush(COLORREF(KEY)),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let (vx, vy, vw, vh) = super::platform::virtual_screen_rect();
        let hwnd = CreateWindowExW(
            // Layered for the colour key, transparent so clicks fall through to
            // whatever is underneath, no-activate and tool-window so it never
            // takes focus or appears in Alt-Tab.
            WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOOLWINDOW
                | WS_EX_TOPMOST
                | WS_EX_NOACTIVATE,
            class,
            PCWSTR(wide("Clickwork overlay").as_ptr()),
            WS_POPUP,
            vx,
            vy,
            vw,
            vh,
            None,
            None,
            Some(hinst),
            None,
        );
        let hwnd = match hwnd {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("overlay window could not be created: {e}");
                RUNNING.store(false, Ordering::Relaxed);
                return;
            }
        };
        tracing::info!("overlay window up at {vx},{vy} {vw}x{vh}");
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(KEY), 0, LWA_COLORKEY);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        HWND_SLOT.store(hwnd.0 as isize, Ordering::Relaxed);

        let mut seen = u64::MAX;
        let mut msg = MSG::default();
        while WANTED.load(Ordering::Relaxed) {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    WANTED.store(false, Ordering::Relaxed);
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // Repaint only when there is something new to draw. A layered window
            // redrawn ten times a second for no reason is a visible flicker and a
            // pointless slice of a core.
            // Either snapshot moving is a reason to redraw, and neither moving
            // is a reason not to. Summed rather than compared pairwise because
            // one number is all the loop needs to remember.
            let now = SIGHTING.lock().seq.wrapping_add(super::HUD.lock().seq);
            if now != seen {
                seen = now;
                let _ = InvalidateRect(Some(hwnd), None, true);
                let _ = UpdateWindow(hwnd);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        HWND_SLOT.store(0, Ordering::Relaxed);
        let _ = DestroyWindow(hwnd);
        RUNNING.store(false, Ordering::Relaxed);
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                paint(hwnd, hdc);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_CLOSE => {
                WANTED.store(false, Ordering::Relaxed);
                LRESULT(0)
            }
            // Nothing should ever hit this window, but say so anyway.
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// The heads-up display: four blocks of short lines in one corner.
///
/// Laid out here and nowhere else, from strings somebody else made. The overlay
/// knows what a corner is and what a line is; it does not know what a step is,
/// which is what lets this keep working with the main window in the tray.
///
/// Headings are part of the honesty. `RESPONSE` is not `GAME`, because what is
/// under it is how quickly the target window answers its messages and not how
/// fast anything rendered - see the `perf` module. A number labelled FPS with no
/// heading over it would be a claim this program cannot make.
unsafe fn draw_hud(mem: HDC, w: i32, h: i32) {
    unsafe {
        let d = hud();
        let blocks: [(&str, &Vec<String>, u32); 4] = [
            ("RESPONSE", &d.game, 0x00C8_C8C8),
            ("MACRO", &d.macro_lines, 0x0078_DC50),
            ("RELIABILITY", &d.reliability, 0x003C_BEFF),
            ("STATE", &d.state, 0x00E6_E6E6),
        ];
        if blocks.iter().all(|(_, lines, _)| lines.is_empty()) {
            return;
        }

        // A fixed-pitch face, so numbers that change do not shuffle the column.
        let font = CreateFontW(
            -15, 0, 0, 0, FW_SEMIBOLD.0 as i32, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
            (FIXED_PITCH.0 | FF_MODERN.0) as u32,
            PCWSTR(wide("Consolas").as_ptr()),
        );
        let old_font = SelectObject(mem, HGDIOBJ(font.0));
        SetBkMode(mem, TRANSPARENT);

        const LINE: i32 = 17;
        const PAD: i32 = 10;
        const WIDTH: i32 = 190;
        let rows: i32 = blocks
            .iter()
            .filter(|(_, l, _)| !l.is_empty())
            .map(|(_, l, _)| l.len() as i32 + 1)
            .sum();
        let box_h = rows * LINE + PAD * 2;

        // Four corners, inset far enough not to sit under a window button.
        let corner = HUD_CORNER.load(Ordering::Relaxed);
        let x0 = if corner == 0 || corner == 3 { w - WIDTH - 24 } else { 24 };
        let y0 = if corner <= 1 { 24 } else { h - box_h - 24 };

        // A dark plate behind the text. Without it the numbers are unreadable on
        // anything pale, and an overlay you have to squint at is not one.
        let plate = CreateSolidBrush(COLORREF(0x0018_1818));
        let rc = RECT { left: x0, top: y0, right: x0 + WIDTH, bottom: y0 + box_h };
        FillRect(mem, &rc, plate);
        let _ = DeleteObject(HGDIOBJ(plate.0));

        let mut y = y0 + PAD;
        for (title, lines, colour) in blocks {
            if lines.is_empty() {
                continue;
            }
            let head = wide(title);
            SetTextColor(mem, COLORREF(0x0090_9090));
            let _ = TextOutW(mem, x0 + PAD, y, &head[..head.len() - 1]);
            y += LINE;
            SetTextColor(mem, COLORREF(colour));
            for line in lines {
                let t = wide(line);
                let _ = TextOutW(mem, x0 + PAD, y, &t[..t.len() - 1]);
                y += LINE;
            }
        }

        SelectObject(mem, old_font);
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
}

/// Draws into an off-screen bitmap and blits it once.
///
/// Straight onto the window would flash: the key-coloured fill and the
/// rectangles over it are two separate presentations of a layered surface.
unsafe fn paint(hwnd: HWND, hdc: HDC) {
    unsafe {
        let mut rc = RECT::default();
        if GetClientRect(hwnd, &mut rc).is_err() {
            return;
        }
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        if w <= 0 || h <= 0 {
            return;
        }
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));

        let key_brush = CreateSolidBrush(COLORREF(KEY));
        FillRect(mem, &rc, key_brush);
        let _ = DeleteObject(HGDIOBJ(key_brush.0));

        let seen = SIGHTING.lock().clone();
        let (ox, oy, _, _) = super::platform::virtual_screen_rect();
        let frame = |x: i32, y: i32, rw: i32, rh: i32, colour: u32, width: i32| {
            let pen = CreatePen(PS_SOLID, width, COLORREF(colour));
            let old_pen = SelectObject(mem, HGDIOBJ(pen.0));
            let old_brush = SelectObject(mem, GetStockObject(NULL_BRUSH));
            let _ = Rectangle(mem, x - ox, y - oy, x - ox + rw, y - oy + rh);
            SelectObject(mem, old_brush);
            SelectObject(mem, old_pen);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        };

        // Blue: where it was allowed to look. Amber: where text was read.
        // Violet: the interface element. Green or red: the match itself.
        if let Some((x, y, rw, rh)) = seen.area {
            frame(x, y, rw, rh, 0x00FF_A05A, 1);
        }
        if let Some((x, y, rw, rh)) = seen.text {
            frame(x, y, rw, rh, 0x003C_BEFF, 1);
        }
        if let Some((x, y, rw, rh)) = seen.element {
            frame(x, y, rw, rh, 0x00FF_78B4, 2);
        }
        if let Some((x, y, rw, rh, score)) = seen.hit {
            // The colour is the answer and the number under it is why.
            let col = if score >= 0.85 { 0x0078_DC50 } else { 0x005A_5AF0 };
            frame(x, y, rw, rh, col, 2);
            let label = wide(&format!("{score:.3}"));
            SetBkMode(mem, TRANSPARENT);
            SetTextColor(mem, COLORREF(col));
            let _ = TextOutW(mem, x - ox, y - oy + rh + 2, &label[..label.len() - 1]);
        }
        if !seen.note.is_empty() {
            let label = wide(&seen.note);
            SetBkMode(mem, TRANSPARENT);
            SetTextColor(mem, COLORREF(0x00E6_E6E6));
            let _ = TextOutW(mem, 12, 12, &label[..label.len() - 1]);
        }

        draw_hud(mem, w, h);

        let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
    }
}
