use super::win32::*;
use super::{APP_TITLE, EndAction, METRICS_TTL_US, WindowAnchor, now_us, wide};
use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, AtomicIsize, AtomicU64, Ordering};

static HWND_CACHE: AtomicIsize = AtomicIsize::new(0);
static HWND_LAST_TRY: AtomicU64 = AtomicU64::new(0);
static VS: [AtomicI32; 4] =
    [AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(1), AtomicI32::new(1)];
static VS_LAST: AtomicU64 = AtomicU64::new(0);

/// Our own top-level window, cached and validated against the process id.
pub fn app_hwnd() -> HWND {
    let cached = HWND_CACHE.load(Ordering::Relaxed);
    if cached != 0 {
        let hwnd = HWND(cached as *mut c_void);
        if unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return hwnd;
        }
        HWND_CACHE.store(0, Ordering::Relaxed);
    }
    let now = now_us();
    let last = HWND_LAST_TRY.load(Ordering::Relaxed);
    if last != 0 && now.saturating_sub(last) < 1_000_000 {
        return HWND::default();
    }
    HWND_LAST_TRY.store(now, Ordering::Relaxed);

    unsafe {
        let title = wide(APP_TITLE);
        if let Ok(hwnd) = FindWindowW(None, PCWSTR(title.as_ptr()))
            && !hwnd.0.is_null() {
                let mut pid = 0u32;
                let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == GetCurrentProcessId() {
                    HWND_CACHE.store(hwnd.0 as isize, Ordering::Relaxed);
                    return hwnd;
                }
            }
    }
    HWND::default()
}

pub fn apply_system_backdrop(hwnd: HWND, backdrop: i32) {
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let dark_mode: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_mode as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
        // DWMWA_SYSTEMBACKDROP_TYPE: 1 = none, 2 = Mica, 3 = Acrylic, 4 = Tabbed.
        let backdrop_type: i32 = backdrop;
        let result = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(38),
            &backdrop_type as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
        if result.is_err() && backdrop > 1 {
            let bb = DWM_BLURBEHIND {
                dwFlags: DWM_BB_ENABLE,
                fEnable: true.into(),
                hRgnBlur: HRGN::default(),
                fTransitionOnMaximized: false.into(),
            };
            let _ = DwmEnableBlurBehindWindow(hwnd, &bb);
        }
    }
}

fn virtual_screen() -> (i32, i32, i32, i32) {
    let now = now_us();
    let last = VS_LAST.load(Ordering::Relaxed);
    if last == 0 || now.saturating_sub(last) >= METRICS_TTL_US {
        unsafe {
            VS[0].store(GetSystemMetrics(SM_XVIRTUALSCREEN), Ordering::Relaxed);
            VS[1].store(GetSystemMetrics(SM_YVIRTUALSCREEN), Ordering::Relaxed);
            VS[2].store(GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1), Ordering::Relaxed);
            VS[3].store(GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1), Ordering::Relaxed);
        }
        VS_LAST.store(now, Ordering::Relaxed);
    }
    (
        VS[0].load(Ordering::Relaxed),
        VS[1].load(Ordering::Relaxed),
        VS[2].load(Ordering::Relaxed),
        VS[3].load(Ordering::Relaxed),
    )
}

/// `w - 1` as the denominator so the right/bottom-most pixel stays reachable.
pub fn normalize_abs(x: i32, y: i32, vx: i32, vy: i32, vw: i32, vh: i32) -> (i32, i32) {
    let dx = (vw - 1).max(1) as f64;
    let dy = (vh - 1).max(1) as f64;
    let nx = (((x - vx) as f64 / dx) * 65535.0).round().clamp(0.0, 65535.0) as i32;
    let ny = (((y - vy) as f64 / dy) * 65535.0).round().clamp(0.0, 65535.0) as i32;
    (nx, ny)
}

pub unsafe fn send_absolute_mouse_move(x: i32, y: i32) {
    unsafe {
        let (vx, vy, vw, vh) = virtual_screen();
        let (nx, ny) = normalize_abs(x, y, vx, vy, vw, vh);
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: nx,
                    dy: ny,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE
                        | MOUSEEVENTF_ABSOLUTE
                        | MOUSEEVENTF_VIRTUALDESK,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        if !crate::selftest::send_blocked() {
            SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }
}

pub fn begin_high_res_timer() {
    unsafe {
        let _ = timeBeginPeriod(1);
    }
}
pub fn end_high_res_timer() {
    unsafe {
        let _ = timeEndPeriod(1);
    }
}

/// Colour of a screen pixel, or None if the read failed.
pub fn screen_pixel(x: i32, y: i32) -> Option<(u8, u8, u8)> {
    unsafe {
        let hdc = GetDC(None);
        if hdc.is_invalid() {
            return None;
        }
        let c = GetPixel(hdc, x, y);
        ReleaseDC(None, hdc);
        if c.0 == 0xFFFF_FFFF {
            return None;
        }
        Some(((c.0 & 0xFF) as u8, ((c.0 >> 8) & 0xFF) as u8, ((c.0 >> 16) & 0xFF) as u8))
    }
}


// ---------------------------------------------------------------------
// Desktop Duplication
// ---------------------------------------------------------------------
//
// `BitBlt` out of the desktop DC costs about six milliseconds before it has
// copied a single useful pixel, and the same blit between two memory DCs of
// the same size costs 0.13 ms. Both numbers are printed by `--selftest vision`
// under "Where a capture goes". The gap is the readback: the composited
// desktop does not live in system memory, and GDI has to go and fetch it every
// time. Nothing about the destination bitmap changes that - the table prices a
// DIB section against the device bitmap it replaced and they come out equal.
//
// Desktop Duplication is the interface that does not pay it. The compositor
// hands over the surface it already has, the copy that reaches the CPU is only
// the rectangle that was asked for, and - the part that matters most for a
// script polling a settled screen - a frame that has not changed is not sent
// at all, so the poll costs one sub-rectangle copy out of a texture that is
// already there.
//
// Every failure here falls back to GDI rather than failing the capture: an
// older machine, a remote session, a driver that says no, a rectangle that
// straddles two monitors, or a rotated display all keep working exactly as
// they did.
#[cfg(windows)]
mod dupe {
    use super::super::win32::RECT;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use windows::Win32::Graphics::Direct3D::{
        D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0,
    };
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION,
        D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    };
    use windows::Win32::Graphics::Dxgi::Common::{
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION_IDENTITY, DXGI_SAMPLE_DESC,
    };
    use windows::Win32::Graphics::Dxgi::{
        DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
        DXGI_OUTDUPL_MOVE_RECT, IDXGIDevice, IDXGIOutput1, IDXGIOutputDuplication,
        IDXGIResource,
    };
    use windows::core::Interface as _;

    /// Turned off by a configuration switch. Process-wide, because it is a
    /// setting rather than a discovery.
    static ENABLED: AtomicBool = AtomicBool::new(true);

    // Turned off for good, on this thread, by three consecutive failures - a
    // machine that cannot do this should stop being asked twenty times a
    // second.
    //
    // Per thread rather than process-wide, and that is not a detail. Each
    // thread duplicates the output for itself, so a thread that cannot get one
    // - because another already holds it, because it started while a
    // full-screen application had the output, because the monitor it asked
    // about has since been unplugged - is saying something about itself and
    // not about the machine. A global count would let one unlucky search
    // thread put every future playback back on the slow path for the rest of
    // the session, and nothing would ever say why.
    thread_local! {
        static STRIKES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }
    /// Counts what the fast path actually did, for the benchmark and the log.
    pub static HITS: AtomicU64 = AtomicU64::new(0);
    pub static REUSED: AtomicU64 = AtomicU64::new(0);
    pub static MISSES: AtomicU64 = AtomicU64::new(0);

    const MAX_STRIKES: u64 = 3;

    pub fn set_enabled(on: bool) {
        ENABLED.store(on, Ordering::Relaxed);
        if on {
            // Only this thread's count. Another thread that has given up did so
            // for a reason that flipping a setting does not change; it clears
            // its own when a capture next works.
            STRIKES.with(|c| c.set(0));
        }
    }

    pub fn enabled() -> bool {
        ENABLED.load(Ordering::Relaxed) && STRIKES.with(|c| c.get()) < MAX_STRIKES
    }

    pub fn counters() -> (u64, u64, u64) {
        (
            HITS.load(Ordering::Relaxed),
            REUSED.load(Ordering::Relaxed),
            MISSES.load(Ordering::Relaxed),
        )
    }

    pub fn reset_counters() {
        HITS.store(0, Ordering::Relaxed);
        REUSED.store(0, Ordering::Relaxed);
        MISSES.store(0, Ordering::Relaxed);
    }

    fn strike(why: &str) {
        let n = STRIKES.with(|c| {
            let n = c.get() + 1;
            c.set(n);
            n
        });
        if n == MAX_STRIKES {
            tracing::warn!(
                "desktop duplication gave up on this thread after {n} failures ({why}); \
                 screen captures fall back to GDI"
            );
        }
    }

    /// One duplicated output plus the textures that go with it.
    ///
    /// `latest` is a full-size copy of the last frame the compositor handed
    /// over, kept on the GPU. It exists so that a poll which finds no new frame
    /// still has the current screen to cut a rectangle out of: a settled screen
    /// sends nothing, and without this the second look would have nothing to
    /// look at.
    struct Dup {
        device: ID3D11Device,
        ctx: ID3D11DeviceContext,
        dup: IDXGIOutputDuplication,
        /// The output's rectangle in desktop coordinates.
        bounds: RECT,
        latest: Option<ID3D11Texture2D>,
        /// Staging texture sized to the last rectangle asked for.
        stage: Option<(ID3D11Texture2D, u32, u32)>,
        /// What changed in the frame `pump` last brought across, in desktop
        /// coordinates, and whether that list can be believed.
        ///
        /// The compositor sends a frame for any pixel anywhere, so "did the
        /// screen change" is almost always yes and is not a useful question at
        /// the interval a script polls. *Where* it changed is the useful one.
        /// Empty with `dirty_known` set means genuinely nothing moved; anything
        /// with `dirty_known` clear means "assume all of it", which is what the
        /// caller must do when the driver reports no metadata.
        dirty: Vec<RECT>,
        dirty_known: bool,
    }

    thread_local! {
        static DUP: std::cell::RefCell<Option<Dup>> = const {
            std::cell::RefCell::new(None)
        };
    }

    pub fn release() {
        DUP.with(|d| {
            *d.borrow_mut() = None;
        });
        // A run that has ended takes its verdict with it. The next one may find
        // the output free, the resolution settled, or the game closed.
        STRIKES.with(|c| c.set(0));
    }

    /// Builds a duplication for the output that wholly contains `want`.
    ///
    /// A rectangle spanning two monitors is refused rather than stitched: one
    /// duplication is one output, and a script that sweeps both screens is
    /// better served by the path that already handles it than by half a frame.
    unsafe fn build(want: RECT) -> Option<Dup> {
        unsafe {
            let mut device: Option<ID3D11Device> = None;
            let mut ctx: Option<ID3D11DeviceContext> = None;
            let levels = [D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0];
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                super::super::win32::HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut ctx),
            )
            .ok()?;
            let device = device?;
            let ctx = ctx?;

            let dxgi: IDXGIDevice = device.cast().ok()?;
            let adapter = dxgi.GetAdapter().ok()?;
            let mut i = 0u32;
            while let Ok(output) = adapter.EnumOutputs(i) {
                i += 1;
                let desc = output.GetDesc().ok()?;
                // A rotated output arrives rotated, and un-rotating it here
                // would cost more than the readback it saves.
                if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY {
                    continue;
                }
                let b = desc.DesktopCoordinates;
                let holds = want.left >= b.left
                    && want.top >= b.top
                    && want.right <= b.right
                    && want.bottom <= b.bottom;
                if !holds {
                    continue;
                }
                let out1: IDXGIOutput1 = output.cast().ok()?;
                let dup = out1.DuplicateOutput(&device).ok()?;
                return Some(Dup {
                    device,
                    ctx,
                    dup,
                    bounds: b,
                    latest: None,
                    stage: None,
                    dirty: Vec::new(),
                    dirty_known: false,
                });
            }
            None
        }
    }

    impl Dup {
        /// Pulls the newest frame across, if the compositor has one.
        ///
        /// `Ok(false)` means nothing changed, which is the common answer while a
        /// script waits for a button and is the reason this is fast at all.
        unsafe fn pump(&mut self, patient: bool) -> Result<bool, windows::core::Error> {
            unsafe {
                let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
                let mut res: Option<IDXGIResource> = None;
                // Nothing yet to fall back on: wait briefly for the first
                // frame. Afterwards never block - a poll that sleeps to learn
                // that nothing moved is worse than the blit it replaced.
                //
                // The wait is short and it is struck against, because the
                // compositor sends a frame when something *changes*: a screen
                // that is genuinely frozen sends nothing, and a patient wait
                // followed by a GDI fallback would then be slower than GDI on
                // its own. Three of those and this thread stops asking.
                let timeout = if patient { 60 } else { 0 };
                match self.dup.AcquireNextFrame(timeout, &mut info, &mut res) {
                    Ok(()) => {}
                    Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(false),
                    Err(e) => return Err(e),
                }
                let got = (|| -> Result<bool, windows::core::Error> {
                    let res = res.ok_or_else(|| {
                        windows::core::Error::from(DXGI_ERROR_ACCESS_LOST)
                    })?;
                    // AccumulatedFrames of 0 with a desktop pointer is a
                    // cursor-only update: the pixels behind it did not move,
                    // and the texture it hands over does not hold a desktop
                    // image at all.
                    //
                    // Until 1.7.0 this read `&& self.latest.is_some()`, which
                    // let the very first acquire through - and the first
                    // acquire is the one most likely to be cursor-only,
                    // because a cursor twitch is what wakes the compositor on
                    // an otherwise still screen. The black texture was then
                    // copied in as "the screen", and because `capture` had
                    // returned `Some`, the GDI fallback below it never ran.
                    // The visible symptom was that the *first* image search of
                    // every run silently found nothing: a macro whose first
                    // step was `Click image` with `stop` on a miss ended on
                    // that step, while the same macro inside a `While` loop
                    // corrected itself on the second look and nobody noticed.
                    //
                    // Refusing it whatever the state is the whole fix: with no
                    // first frame yet, `capture` now returns `None` and GDI
                    // answers, which is what it was always meant to do.
                    if info.LastPresentTime == 0 {
                        return Ok(false);
                    }
                    let tex: ID3D11Texture2D = res.cast()?;
                    if self.latest.is_none() {
                        let mut desc = D3D11_TEXTURE2D_DESC::default();
                        tex.GetDesc(&mut desc);
                        desc.Usage = D3D11_USAGE_DEFAULT;
                        desc.BindFlags = 0;
                        desc.CPUAccessFlags = 0;
                        desc.MiscFlags = 0;
                        let mut made: Option<ID3D11Texture2D> = None;
                        self.device.CreateTexture2D(&desc, None, Some(&mut made))?;
                        self.latest = made;
                    }
                    let Some(dst) = self.latest.as_ref() else {
                        return Ok(false);
                    };
                    // Stays on the GPU. The only thing that crosses to the CPU
                    // is the rectangle the caller asked for, below.
                    self.ctx.CopyResource(dst, &tex);
                    Ok(true)
                })();
                // Before `ReleaseFrame`, which is the only window in which the
                // metadata is valid. A failure here is not a failure of the
                // capture: it just means nobody may assume anything about where
                // the frame changed.
                if got.as_ref().copied().unwrap_or(false) {
                    self.collect_dirty(info.TotalMetadataBufferSize);
                } else {
                    self.dirty.clear();
                    self.dirty_known = false;
                }
                let _ = self.dup.ReleaseFrame();
                got
            }
        }

        /// Reads the move and dirty rectangles of the frame currently held.
        ///
        /// Two lists, and both matter. A dirty rectangle is somewhere repainted;
        /// a move rectangle is a block the compositor copied from one place to
        /// another, which changes the destination *and* vacates the source, so
        /// both ends go in. Anything short of that would report a region as
        /// untouched when a window had just slid off it.
        ///
        /// Output-local coordinates on the way in, desktop coordinates on the way
        /// out, because that is what every caller thinks in.
        unsafe fn collect_dirty(&mut self, budget: u32) {
            unsafe {
                self.dirty.clear();
                self.dirty_known = false;
                if budget == 0 {
                    // No metadata offered. Not an error - a driver may simply not
                    // provide it - but nothing may be skipped on the strength of
                    // it either.
                    return;
                }
                let cap = budget as usize;
                let (dx, dy) = (self.bounds.left, self.bounds.top);

                let mut moves: Vec<DXGI_OUTDUPL_MOVE_RECT> =
                    vec![DXGI_OUTDUPL_MOVE_RECT::default();
                         cap / size_of::<DXGI_OUTDUPL_MOVE_RECT>() + 1];
                let mut got = 0u32;
                if self
                    .dup
                    .GetFrameMoveRects(
                        (moves.len() * size_of::<DXGI_OUTDUPL_MOVE_RECT>()) as u32,
                        moves.as_mut_ptr(),
                        &mut got,
                    )
                    .is_err()
                {
                    return;
                }
                for m in &moves[..(got as usize / size_of::<DXGI_OUTDUPL_MOVE_RECT>())] {
                    let d = m.DestinationRect;
                    self.dirty.push(RECT {
                        left: d.left + dx,
                        top: d.top + dy,
                        right: d.right + dx,
                        bottom: d.bottom + dy,
                    });
                    self.dirty.push(RECT {
                        left: m.SourcePoint.x + dx,
                        top: m.SourcePoint.y + dy,
                        right: m.SourcePoint.x + dx + (d.right - d.left),
                        bottom: m.SourcePoint.y + dy + (d.bottom - d.top),
                    });
                }

                let mut rects: Vec<RECT> =
                    vec![RECT::default(); cap / size_of::<RECT>() + 1];
                let mut got2 = 0u32;
                if self
                    .dup
                    .GetFrameDirtyRects(
                        (rects.len() * size_of::<RECT>()) as u32,
                        rects.as_mut_ptr(),
                        &mut got2,
                    )
                    .is_err()
                {
                    self.dirty.clear();
                    return;
                }
                for r in &rects[..(got2 as usize / size_of::<RECT>())] {
                    self.dirty.push(RECT {
                        left: r.left + dx,
                        top: r.top + dy,
                        right: r.right + dx,
                        bottom: r.bottom + dy,
                    });
                }
                self.dirty_known = true;
            }
        }

        /// Copies one rectangle of the last frame into system memory, BGRA.
        unsafe fn read(&mut self, x: i32, y: i32, w: u32, h: u32) -> Option<Vec<u8>> {
            unsafe {
                let src = self.latest.as_ref()?.clone();
                let fits = self.stage.as_ref().is_some_and(|(_, sw, sh)| *sw == w && *sh == h);
                if !fits {
                    let desc = D3D11_TEXTURE2D_DESC {
                        Width: w,
                        Height: h,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                        Usage: D3D11_USAGE_STAGING,
                        BindFlags: 0,
                        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                        MiscFlags: 0,
                    };
                    let mut made: Option<ID3D11Texture2D> = None;
                    self.device.CreateTexture2D(&desc, None, Some(&mut made)).ok()?;
                    self.stage = made.map(|t| (t, w, h));
                }
                let (stage, _, _) = self.stage.as_ref()?;
                // Desktop coordinates into output-local ones.
                let lx = (x - self.bounds.left).max(0) as u32;
                let ly = (y - self.bounds.top).max(0) as u32;
                let region = D3D11_BOX {
                    left: lx,
                    top: ly,
                    front: 0,
                    right: lx + w,
                    bottom: ly + h,
                    back: 1,
                };
                self.ctx.CopySubresourceRegion(
                    stage,
                    0,
                    0,
                    0,
                    0,
                    &src,
                    0,
                    Some(&region),
                );
                let mut map = D3D11_MAPPED_SUBRESOURCE::default();
                self.ctx.Map(stage, 0, D3D11_MAP_READ, 0, Some(&mut map)).ok()?;
                let row = (w as usize) * 4;
                let n = row * (h as usize);
                let mut buf: Vec<u8> = Vec::with_capacity(n);
                let pitch = map.RowPitch as usize;
                if map.pData.is_null() || pitch < row {
                    self.ctx.Unmap(stage, 0);
                    return None;
                }
                // The pitch is the driver's, not ours, and is routinely larger
                // than the row: copying the block whole would shear the image.
                for r in 0..h as usize {
                    std::ptr::copy_nonoverlapping(
                        (map.pData as *const u8).add(r * pitch),
                        buf.as_mut_ptr().add(r * row),
                        row,
                    );
                }
                buf.set_len(n);
                self.ctx.Unmap(stage, 0);
                Some(buf)
            }
        }
    }

    thread_local! {
        /// Counts the frames the compositor has actually handed this thread.
        ///
        /// Thread-local because `DUP` is: two threads searching at once have
        /// separate duplication objects and separate notions of "the current
        /// frame", and one thread's serial says nothing about the other's.
        static SERIAL: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
        /// Cleared whenever this thread takes an answer from anything but
        /// duplication, so a serial can never span a frame nobody counted.
        static SERIAL_VALID: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// A number that changes when the screen does, or `None` when that cannot
    /// be known.
    ///
    /// Desktop Duplication answers "nothing has changed" while a script waits for
    /// a button, and that is the common answer - so a caller that looked at the
    /// screen and worked something out can, on the same serial, use what it
    /// worked out instead of working it out again.
    ///
    /// `None` is the important half. The GDI fallback is a blit and carries no
    /// such signal, and neither does a rectangle duplication could not serve, so
    /// anything that cannot know must look again. Never guess here: a stale
    /// answer is a script clicking where a button no longer is.
    pub fn frame_serial() -> Option<u64> {
        SERIAL_VALID.with(|v| v.get()).then(|| SERIAL.with(|c| c.get()))
    }

    /// Says the next serial cannot be trusted, for a caller that got its pixels
    /// from somewhere duplication had no part in.
    pub fn invalidate_serial() {
        SERIAL_VALID.with(|v| v.set(false));
    }

    /// Where the last frame this thread pumped differs from the one before it,
    /// in desktop coordinates.
    ///
    /// `None` means "nobody can say" - no duplication object, a driver that
    /// offers no metadata, or a frame that arrived through GDI - and a caller
    /// that gets it must treat the whole screen as changed. An empty list is the
    /// opposite and much stronger claim: the frame arrived and nothing in it
    /// moved.
    pub fn last_dirty() -> Option<Vec<(i32, i32, i32, i32)>> {
        DUP.with(|cell| {
            let slot = cell.borrow();
            let d = slot.as_ref()?;
            if !d.dirty_known {
                return None;
            }
            Some(
                d.dirty
                    .iter()
                    .map(|r| (r.left, r.top, r.right - r.left, r.bottom - r.top))
                    .collect(),
            )
        })
    }

    /// The fast path. `None` means "GDI, please" and is never an error the
    /// caller has to handle.
    pub fn capture(x: i32, y: i32, w: i32, h: i32) -> Option<Vec<u8>> {
        if !enabled() {
            return None;
        }
        // Saturating, not wrapping. A `Read text` step takes its rectangle from
        // whatever somebody typed into the box, and an x of `i32::MAX` would
        // otherwise wrap `right` to a negative number - which is a rectangle the
        // containment test below would happily accept.
        let want = RECT {
            left: x,
            top: y,
            right: x.saturating_add(w),
            bottom: y.saturating_add(h),
        };
        DUP.with(|cell| {
            let mut slot = cell.borrow_mut();
            let holds = slot.as_ref().is_some_and(|d| {
                want.left >= d.bounds.left
                    && want.top >= d.bounds.top
                    && want.right <= d.bounds.right
                    && want.bottom <= d.bounds.bottom
            });
            if !holds {
                *slot = None;
                *slot = unsafe { build(want) };
            }
            let Some(d) = slot.as_mut() else {
                MISSES.fetch_add(1, Ordering::Relaxed);
                strike("no duplicable output holds the rectangle");
                SERIAL_VALID.with(|v| v.set(false));
                return None;
            };
            let patient = d.latest.is_none();
            match unsafe { d.pump(patient) } {
                Ok(true) => {
                    HITS.fetch_add(1, Ordering::Relaxed);
                    STRIKES.with(|c| c.set(0));
                    // A genuinely new frame. Everything derived from the old one
                    // is now stale; see `frame_serial`.
                    SERIAL.with(|c| c.set(c.get().wrapping_add(1)));
                }
                Ok(false) if d.latest.is_some() => {
                    // Nothing changed, so the frame already here is the screen -
                    // and the serial deliberately does not move, which is what
                    // lets a caller reuse what it worked out last time.
                    REUSED.fetch_add(1, Ordering::Relaxed);
                    STRIKES.with(|c| c.set(0));
                }
                Ok(false) => {
                    // Never got a first frame; GDI can answer this one. A strike,
                    // so a screen that never changes cannot make every capture
                    // pay the wait before falling back anyway.
                    MISSES.fetch_add(1, Ordering::Relaxed);
                    strike("no first frame arrived");
                    SERIAL_VALID.with(|v| v.set(false));
                    return None;
                }
                Err(e) => {
                    // Access lost happens on a resolution change, a session
                    // switch, or a full-screen application taking the output.
                    // Rebuilding is the documented cure, and it is not a
                    // failure worth a strike.
                    tracing::debug!("desktop duplication reset: {e}");
                    *slot = None;
                    MISSES.fetch_add(1, Ordering::Relaxed);
                    SERIAL_VALID.with(|v| v.set(false));
                    return None;
                }
            }
            let out = unsafe { d.read(x, y, w as u32, h as u32) };
            if out.is_none() {
                MISSES.fetch_add(1, Ordering::Relaxed);
                *slot = None;
                strike("the staging copy failed");
            }
            // Only a rectangle duplication actually served carries a serial the
            // caller may lean on.
            SERIAL_VALID.with(|v| v.set(out.is_some()));
            out
        })
    }
}

#[cfg(windows)]
pub use dupe::{
    counters as capture_counters, frame_serial, last_dirty,
    reset_counters as reset_capture_counters, set_enabled as set_fast_capture,
};

/// A memory DC and a DIB kept alive between captures, one per thread.
///
/// Three costs used to be paid on every single look at the screen: a memory DC
/// and a bitmap created and destroyed (GDI object churn, against a per-process
/// quota of 10 000 objects), a fresh zeroed allocation, and a `GetDIBits` that
/// copied and reformatted the whole frame a second time.
///
/// A DIB section removes the second copy outright - `BitBlt` writes straight
/// into memory this process can already read - and caching it removes the
/// churn. The playback thread looks at the same rectangle thousands of times in
/// a row, so the cache hits essentially always; a size change throws it away
/// and builds the next one.
struct DibCache {
    mem: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
}

impl DibCache {
    /// `None` if GDI would not give us either object. Nothing is leaked on the
    /// way out: each step undoes itself.
    unsafe fn new(w: i32, h: i32) -> Option<Self> {
        unsafe {
            let mem = CreateCompatibleDC(None);
            if mem.is_invalid() {
                return None;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // top-down, so row 0 is the top one
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let made = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0);
            let Ok(bmp) = made else {
                let _ = DeleteDC(mem);
                return None;
            };
            if bits.is_null() {
                let _ = DeleteObject(HGDIOBJ(bmp.0));
                let _ = DeleteDC(mem);
                return None;
            }
            let old = SelectObject(mem, HGDIOBJ(bmp.0));
            Some(Self { mem, bmp, old, bits: bits as *mut u8, w, h })
        }
    }

    #[inline]
    fn bytes(&self) -> usize {
        (self.w as usize) * (self.h as usize) * 4
    }
}

impl Drop for DibCache {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.mem, self.old);
            let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            let _ = DeleteDC(self.mem);
        }
    }
}

thread_local! {
    static DIB: std::cell::RefCell<Option<DibCache>> = const {
        std::cell::RefCell::new(None)
    };
}

/// Drops this thread's cached bitmap. Called when a playback run ends, so a
/// full-screen grab does not sit on 14 MB of committed memory afterwards.
pub fn release_capture_cache() {
    DIB.with(|c| {
        *c.borrow_mut() = None;
    });
    #[cfg(windows)]
    dupe::release();
}

/// Just the screen DC, taken and given back. The fixed cost of asking GDI for
/// the desktop, with no pixels moved. Benchmark only.
pub fn probe_screen_dc() -> bool {
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return false;
        }
        ReleaseDC(None, screen);
        true
    }
}

/// A capture that stops before the copy out: DC, `BitBlt` into the cached DIB,
/// DC back. What the frame costs to *reach*, as opposed to to own. Benchmark
/// only - the pixels are left in the cache and nobody reads them.
pub fn probe_blt(x: i32, y: i32, w: i32, h: i32) -> bool {
    if w <= 0 || h <= 0 {
        return false;
    }
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return false;
        }
        let ok = DIB.with(|cell| {
            let mut slot = cell.borrow_mut();
            if !slot.as_ref().is_some_and(|c| c.w == w && c.h == h) {
                *slot = None;
                *slot = DibCache::new(w, h);
            }
            let Some(cache) = slot.as_ref() else { return false };
            BitBlt(cache.mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY).is_ok()
        });
        ReleaseDC(None, screen);
        ok
    }
}

/// The old arrangement: a device-format bitmap made and thrown away each time,
/// which is what a DIB section replaced. Benchmark only, kept so the claim that
/// the destination was never the expensive part can be checked rather than
/// asserted.
pub fn probe_blt_ddb(x: i32, y: i32, w: i32, h: i32) -> bool {
    if w <= 0 || h <= 0 {
        return false;
    }
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return false;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));
        let ok = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY).is_ok();
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        ok
    }
}

/// The same blit with no screen at either end: cached DIB to a scratch DIB.
/// Prices the copy itself, so the screen readback can be told apart from it.
pub fn probe_blt_mem(w: i32, h: i32) -> bool {
    if w <= 0 || h <= 0 {
        return false;
    }
    unsafe {
        let src = DIB.with(|cell| {
            let mut slot = cell.borrow_mut();
            if !slot.as_ref().is_some_and(|c| c.w == w && c.h == h) {
                *slot = None;
                *slot = DibCache::new(w, h);
            }
            slot.as_ref().map(|c| c.mem)
        });
        let Some(src) = src else { return false };
        let Some(dst) = DibCache::new(w, h) else { return false };
        BitBlt(dst.mem, 0, 0, w, h, Some(src), 0, 0, SRCCOPY).is_ok()
    }
}

/// Grabs a rectangle of the screen.
///
/// The pixels come back in GDI's own BGRA order and are not touched on the way
/// out; the `Frame` says which order they are in and the two consumers that
/// care read that. See `vision::Order`.
pub fn capture(x: i32, y: i32, w: i32, h: i32) -> Option<crate::vision::Frame> {
    if w <= 0 || h <= 0 {
        return None;
    }
    // A rectangle big enough to overflow the multiply below is not a rectangle
    // any screen has; refusing it here keeps every later cast honest.
    if (w as i64) * (h as i64) > (1i64 << 28) {
        return None;
    }
    // The compositor's own copy, when this machine will give us one.
    if let Some(px) = dupe::capture(x, y, w, h) {
        return Some(crate::vision::Frame {
            x,
            y,
            w: w as u32,
            h: h as u32,
            px,
            order: crate::vision::Order::Bgra,
        });
    }
    capture_gdi(x, y, w, h)
}

/// The fallback, and the only path before 1.5.0. Kept whole rather than folded
/// into the caller: `--selftest vision` prices one against the other, and a
/// machine where duplication misbehaves is one configuration switch away from
/// this being all that runs.
pub fn capture_gdi(x: i32, y: i32, w: i32, h: i32) -> Option<crate::vision::Frame> {
    if w <= 0 || h <= 0 || (w as i64) * (h as i64) > (1i64 << 28) {
        return None;
    }
    // A blit says nothing about whether the screen moved, so nothing derived
    // from the frame it returns may be reused.
    dupe::invalidate_serial();
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let out = DIB.with(|cell| {
            let mut slot = cell.borrow_mut();
            let fits = slot.as_ref().is_some_and(|c| c.w == w && c.h == h);
            if !fits {
                // Dropped first, so the old objects are gone before the new ones
                // are asked for rather than both being held at once.
                *slot = None;
                *slot = DibCache::new(w, h);
            }
            let cache = slot.as_ref()?;
            if BitBlt(cache.mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY).is_err() {
                return None;
            }
            // `Vec::with_capacity` rather than `vec![0; n]`: the zeroing pass is
            // a full write of the frame that the copy immediately overwrites.
            let n = cache.bytes();
            let mut buf: Vec<u8> = Vec::with_capacity(n);
            std::ptr::copy_nonoverlapping(cache.bits, buf.as_mut_ptr(), n);
            buf.set_len(n);
            Some(buf)
        });
        ReleaseDC(None, screen);
        Some(crate::vision::Frame {
            x,
            y,
            w: w as u32,
            h: h as u32,
            px: out?,
            order: crate::vision::Order::Bgra,
        })
    }
}

pub fn virtual_screen_rect() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
        )
    }
}

/// Reads a bitmap off the clipboard, which is what Win+Shift+S leaves behind.
///
/// Handles the 24/32-bit DIBs that the Snipping Tool and browsers produce,
/// including the V4/V5 headers where the colour masks live inside the header.
pub fn clipboard_image() -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let result = (|| {
            const CF_DIB: u32 = 8;
            let handle = GetClipboardData(CF_DIB).ok()?;
            let hglobal = HGLOBAL(handle.0);
            let ptr = GlobalLock(hglobal) as *const u8;
            if ptr.is_null() {
                return None;
            }
            let size = GlobalSize(hglobal);
            let bytes = std::slice::from_raw_parts(ptr, size);
            let out = parse_dib(bytes);
            let _ = GlobalUnlock(hglobal);
            out
        })();
        let _ = CloseClipboard();
        result
    }
}

fn rd_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn rd_i32(b: &[u8], at: usize) -> i32 {
    rd_u32(b, at) as i32
}

/// Turns a packed DIB into top-down RGBA.
fn parse_dib(b: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if b.len() < 40 {
        return None;
    }
    let hdr = rd_u32(b, 0) as usize;
    let w = rd_i32(b, 4);
    let raw_h = rd_i32(b, 8);
    let bpp = u16::from_le_bytes([b[14], b[15]]) as u32;
    let compression = rd_u32(b, 16);
    if w <= 0 || raw_h == 0 || !(bpp == 24 || bpp == 32) {
        return None;
    }
    let bottom_up = raw_h > 0;
    let h = raw_h.unsigned_abs();
    let w_u = w as u32;

    // BI_BITFIELDS adds three masks after a plain 40-byte header; V4/V5 headers
    // are bigger and already contain them.
    let mut offset = hdr;
    if compression == 3 && hdr == 40 {
        offset += 12;
    }
    let stride = ((w_u * bpp).div_ceil(32) * 4) as usize;
    if b.len() < offset + stride * h as usize {
        return None;
    }

    let mut out = vec![0u8; (w_u * h * 4) as usize];
    let bytes_pp = (bpp / 8) as usize;
    for row in 0..h {
        let src_row = if bottom_up { h - 1 - row } else { row } as usize;
        let src = offset + src_row * stride;
        for x in 0..w_u as usize {
            let s = src + x * bytes_pp;
            let d = ((row as usize) * w_u as usize + x) * 4;
            out[d] = b[s + 2];
            out[d + 1] = b[s + 1];
            out[d + 2] = b[s];
            // Opaque, whatever the source says. A 32-bit DIB has an alpha
            // channel and GDI leaves it at zero, so copying it through would
            // make every captured pixel fully transparent - which is exactly
            // why the image search never consults alpha either. The test that
            // used to be here chose 255 in both arms and had presumably been
            // meant to read the source; clippy noticed, and the answer is that
            // the constant is correct and the test never was.
            out[d + 3] = 255;
        }
    }
    Some((w_u, h, out))
}

/// Local wall-clock time: year, month, day, weekday (0 = Monday), hour, minute.
///
/// Straight from Windows rather than a date crate: no dependency, and it is
/// already the user's timezone and DST, which is what a schedule means.
pub fn local_time() -> (u16, u16, u16, u8, u16, u16) {
    unsafe {
        let t = windows::Win32::System::SystemInformation::GetLocalTime();
        let monday_based = ((t.wDayOfWeek + 6) % 7) as u8;
        (t.wYear, t.wMonth, t.wDay, monday_based, t.wHour, t.wMinute)
    }
}

/// Title of whatever window currently has focus.
pub fn foreground_title() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..n as usize]))
    }
}

pub fn cursor_pos() -> (i32, i32) {
    unsafe {
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        (p.x, p.y)
    }
}

/// Title + rect of the foreground window, skipping our own.
pub fn foreground_anchor() -> Option<WindowAnchor> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() || hwnd == app_hwnd() {
            return None;
        }
        let mut buf = [0u16; 256];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&buf[..len as usize]);
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return None;
        }
        Some(WindowAnchor {
            title,
            x: r.left,
            y: r.top,
            w: r.right - r.left,
            h: r.bottom - r.top,
        })
    }
}

thread_local! {
    /// Needle and result for the EnumWindows callback below.
    static FIND_STATE: std::cell::RefCell<(String, Option<HWND>)> =
        const { std::cell::RefCell::new((String::new(), None)) };
}

unsafe extern "system" fn enum_find_proc(hwnd: HWND, _lp: LPARAM) -> BOOL {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return true.into();
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return true.into();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n <= 0 {
            return true.into();
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]).to_lowercase();
        let mut stop = false;
        FIND_STATE.with(|c| {
            let mut c = c.borrow_mut();
            if c.1.is_none() && !c.0.is_empty() && title.contains(c.0.as_str()) {
                c.1 = Some(hwnd);
                stop = true;
            }
        });
        (!stop).into()
    }
}

/// Finds a top-level window by title and returns its rectangle.
///
/// Exact match first, then a case-insensitive substring search: window titles
/// pick up suffixes all the time ("Roblox" becomes "Roblox - Level 7"), and an
/// exact-only lookup made anchoring fail exactly when it was needed most.
/// Dots per inch of the display the front window is on. 96 is 100 %.
pub fn current_dpi() -> u32 {
    unsafe {
        let hwnd = GetForegroundWindow();
        let dpi = if hwnd.0.is_null() { 0 } else { GetDpiForWindow(hwnd) };
        if dpi == 0 { 96 } else { dpi }
    }
}

/// How many displays there are, and which one the window in front is on.
///
/// Numbered from 1 in `EnumDisplayMonitors` order, which is the order every
/// other tool on Windows numbers them in. Zero for the second value means the
/// question could not be answered - there is no window in front, or its monitor
/// was not in the enumeration, which happens for a fraction of a second while a
/// display is being attached.
///
/// Reported, never used to place anything. A monitor index is not a stable
/// identity across a reboot and must not become one.
pub fn monitor_here() -> (u32, u32) {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mine = if hwnd.0.is_null() {
            HMONITOR::default()
        } else {
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
        };
        // (count so far, index of `mine` once seen). A thread local rather than a
        // captured closure because the callback is a plain `extern "system"` fn.
        MONITOR_SCAN.with(|c| *c.borrow_mut() = (0, 0, mine));
        let _ = EnumDisplayMonitors(None, None, Some(count_monitors), LPARAM(0));
        MONITOR_SCAN.with(|c| {
            let (n, at, _) = *c.borrow();
            (n, at)
        })
    }
}

thread_local! {
    static MONITOR_SCAN: std::cell::RefCell<(u32, u32, HMONITOR)> =
        const { std::cell::RefCell::new((0, 0, HMONITOR(std::ptr::null_mut()))) };
}

unsafe extern "system" fn count_monitors(
    h: HMONITOR,
    _: HDC,
    _: *mut RECT,
    _: LPARAM,
) -> BOOL {
    MONITOR_SCAN.with(|c| {
        let mut c = c.borrow_mut();
        c.0 += 1;
        if c.2 == h {
            c.1 = c.0;
        }
    });
    true.into()
}

/// The keyboard layout of the window in front, as a language name.
///
/// `en-US`, `ru-RU`. The low word of an `HKL` is a language identifier, and
/// `LCIDToLocaleName` is the supported way of turning one into a name; the raw
/// hex would be correct and unreadable.
pub fn keyboard_layout() -> String {
    unsafe {
        let hwnd = GetForegroundWindow();
        let tid =
            if hwnd.0.is_null() { 0 } else { GetWindowThreadProcessId(hwnd, None) };
        let hkl = GetKeyboardLayout(tid);
        let lcid = (hkl.0 as usize & 0xFFFF) as u32;
        if lcid == 0 {
            return String::new();
        }
        let mut buf = [0u16; 85];
        let n = LCIDToLocaleName(lcid, Some(&mut buf), 0);
        if n <= 1 {
            return String::new();
        }
        // The count includes the terminating null.
        String::from_utf16_lossy(&buf[..(n - 1) as usize])
    }
}

/// Where the window in front is, for `SearchArea::ActiveWindow`.
pub fn foreground_rect() -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return None;
        }
        Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
    }
}

/// Executable name of the window in front, without its path.
///
/// `PROCESS_QUERY_LIMITED_INFORMATION` rather than the full right: it is the
/// least this needs, and it is the one that works across an elevation boundary
/// in the direction that matters.
pub fn foreground_process() -> String {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return String::new();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return String::new();
        }
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return String::new();
        }
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string()
    }
}

/// Is a process whose name contains `name` running?
///
/// A substring, without case, so `roblox` finds `RobloxPlayerBeta.exe` - asking
/// somebody to type the exact executable name is asking them to get it wrong.
pub fn process_running(name: &str) -> bool {
    let needle = name.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let exe = String::from_utf16_lossy(&entry.szExeFile[..end]);
                if exe.to_lowercase().contains(&needle) {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        found
    }
}

/// The clipboard as text. Empty when it holds something else, or nothing.
pub fn clipboard_text() -> String {
    unsafe {
        if OpenClipboard(None).is_err() {
            return String::new();
        }
        let out = (|| {
            const CF_UNICODETEXT: u32 = 13;
            let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
            let hglobal = HGLOBAL(handle.0);
            let ptr = GlobalLock(hglobal) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let mut len = 0usize;
            while *ptr.add(len) != 0 && len < 1_000_000 {
                len += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            let _ = GlobalUnlock(hglobal);
            Some(text)
        })();
        let _ = CloseClipboard();
        out.unwrap_or_default()
    }
}

/// Replaces the clipboard with `text`. False when Windows would not hand it over,
/// which happens whenever another application is holding it open.
pub fn set_clipboard_text(text: &str) -> bool {
    unsafe {
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let bytes = wide.len() * 2;
        let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, bytes) else {
            return false;
        };
        let ptr = GlobalLock(h) as *mut u16;
        if ptr.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(h);
        if OpenClipboard(None).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        const CF_UNICODETEXT: u32 = 13;
        let ok = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(h.0))).is_ok();
        let _ = CloseClipboard();
        ok
    }
}

/// The window a title (or the start of one) refers to.
fn find_window_handle(title: &str) -> Option<HWND> {
    unsafe {
        let mut hwnd = HWND::default();
        let w = wide(title);
        if let Ok(h) = FindWindowW(None, PCWSTR(w.as_ptr())) {
            hwnd = h;
        }
        if hwnd.0.is_null() {
            let needle: String =
                title.to_lowercase().chars().take(24).collect::<String>().trim().to_string();
            FIND_STATE.with(|c| *c.borrow_mut() = (needle, None));
            let _ = EnumWindows(Some(enum_find_proc), LPARAM(0));
            hwnd = FIND_STATE.with(|c| c.borrow().1.unwrap_or_default());
        }
        if hwnd.0.is_null() { None } else { Some(hwnd) }
    }
}

pub fn find_window_rect(title: &str) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let hwnd = find_window_handle(title)?;
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return None;
        }
        Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
    }
}

thread_local! {
    /// Handle collected by `enum_by_process_proc`, and the process name it is
    /// looking for.
    static PROC_FIND: std::cell::RefCell<(String, bool, Option<HWND>)> =
        const { std::cell::RefCell::new((String::new(), false, None)) };
}

/// Picks up the first *visible top-level* window belonging to a named process.
///
/// Visible and top-level on purpose: a process owns a great many windows, most of
/// them message-only or hidden helpers, and activating one of those does nothing
/// visible while looking exactly like a bug.
unsafe extern "system" fn enum_by_process_proc(hwnd: HWND, _l: LPARAM) -> BOOL {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        if !GetWindow(hwnd, GW_OWNER).unwrap_or_default().0.is_null() {
            return BOOL(1);
        }
        // A window with no title is a tool window or a tray host far more often
        // than it is the thing somebody meant.
        if GetWindowTextLengthW(hwnd) == 0 {
            return BOOL(1);
        }
        let mut pid = 0u32;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return BOOL(1);
        }
        let Some(path) = process_path_of(pid) else { return BOOL(1) };
        let hit = PROC_FIND.with(|c| {
            let c = c.borrow();
            if c.1 {
                // Full path, compared whole and without case.
                path.eq_ignore_ascii_case(c.0.trim())
            } else {
                // Just the file name, with `.exe` optional both ways.
                let file = std::path::Path::new(&path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let want = c.0.trim().to_lowercase();
                let want = want.strip_suffix(".exe").unwrap_or(&want);
                let have = file.strip_suffix(".exe").unwrap_or(&file);
                have == want
            }
        });
        if hit {
            PROC_FIND.with(|c| c.borrow_mut().2 = Some(hwnd));
            return BOOL(0);
        }
        BOOL(1)
    }
}

/// Full path of a process, by id.
fn process_path_of(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut n,
        );
        let _ = CloseHandle(h);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..n as usize]))
    }
}

/// The window a `WindowRef` names, by whichever of the four ways it asks for.
fn resolve_window(by: u8, value: &str) -> Option<HWND> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    unsafe {
        match by {
            // Exact title: one call, and Windows does the comparing.
            1 => {
                let w = wide(value);
                FindWindowW(None, PCWSTR(w.as_ptr()))
                    .ok()
                    .filter(|h| !h.0.is_null())
            }
            2 | 3 => {
                PROC_FIND.with(|c| *c.borrow_mut() = (value.to_string(), by == 3, None));
                let _ = EnumWindows(Some(enum_by_process_proc), LPARAM(0));
                PROC_FIND.with(|c| c.borrow().2)
            }
            // Title fragment, which is what everything before 1.6.0 did.
            _ => find_window_handle(value),
        }
    }
}

/// Does a window matching this reference exist right now?
pub fn window_exists(by: u8, value: &str) -> bool {
    resolve_window(by, value).is_some()
}

/// A balloon from the tray icon.
///
/// Chosen over a message box on purpose: a message box takes the foreground, and
/// taking the foreground away from the application being automated is how a
/// macro breaks the very run it is reporting on. A balloon appears beside the
/// clock and changes nothing.
///
/// Silently does nothing when the tray icon is switched off, which is the honest
/// behaviour - there is nowhere for it to come from.
pub fn notify(title: &str, body: &str) -> bool {
    let hwnd = crate::tray::hwnd();
    if hwnd.0.is_null() {
        tracing::info!("notify: {title} - {body} (no tray icon to show it from)");
        return false;
    }
    unsafe {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_INFO,
            dwInfoFlags: NIIF_INFO,
            ..Default::default()
        };
        // Both fields are fixed-size arrays; anything longer is cut rather than
        // refused, because a notification that is too long to show is still
        // worth showing the beginning of.
        let t = wide(title);
        for (i, c) in t.iter().take(nid.szInfoTitle.len() - 1).enumerate() {
            nid.szInfoTitle[i] = *c;
        }
        let b = wide(body);
        for (i, c) in b.iter().take(nid.szInfo.len() - 1).enumerate() {
            nid.szInfo[i] = *c;
        }
        Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool()
    }
}

/// Is it the window in front?
pub fn window_is_active(by: u8, value: &str) -> bool {
    match resolve_window(by, value) {
        Some(h) => unsafe { GetForegroundWindow() == h },
        None => false,
    }
}

pub fn window_rect_of(by: u8, value: &str) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let hwnd = resolve_window(by, value)?;
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return None;
        }
        Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
    }
}

/// Brings a window to the front, and gets away with it.
///
/// `SetForegroundWindow` alone is refused whenever the calling process does not
/// own the foreground, which is almost always here: the macro is playing and the
/// game is in front. Attaching to the foreground thread's input queue for the
/// duration is the documented way round it, and the attachment is always undone -
/// leaving two input queues joined would make the rest of the run behave very
/// strangely indeed.
unsafe fn bring_to_front(hwnd: HWND) -> bool {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() {
            return true;
        }
        let fg = GetForegroundWindow();
        let me = GetCurrentThreadId();
        let other = GetWindowThreadProcessId(fg, None);
        if other == 0 || other == me {
            return false;
        }
        let _ = AttachThreadInput(other, me, true);
        let ok = SetForegroundWindow(hwnd).as_bool();
        let _ = AttachThreadInput(other, me, false);
        ok
    }
}

/// Does one thing to one window. `arg` is the pair of numbers for move and
/// resize, and ignored otherwise.
///
/// Returns whether it happened, so a step can have a miss policy about it.
pub fn window_action(by: u8, value: &str, action: u8, arg: (i32, i32)) -> bool {
    unsafe {
        let Some(hwnd) = resolve_window(by, value) else {
            return false;
        };
        match action {
            0 => bring_to_front(hwnd),
            1 => ShowWindow(hwnd, SW_MINIMIZE).as_bool(),
            2 => ShowWindow(hwnd, SW_MAXIMIZE).as_bool(),
            3 => ShowWindow(hwnd, SW_RESTORE).as_bool(),
            // Asked to close, rather than terminated: the application gets to run
            // its own shutdown, and to refuse.
            4 => PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok(),
            5 => SetWindowPos(
                hwnd,
                None,
                arg.0,
                arg.1,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .is_ok(),
            6 => SetWindowPos(
                hwnd,
                None,
                0,
                0,
                arg.0.max(1),
                arg.1.max(1),
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .is_ok(),
            7 => {
                let mut r = RECT::default();
                if GetWindowRect(hwnd, &mut r).is_err() {
                    return false;
                }
                let (w, h) = (r.right - r.left, r.bottom - r.top);
                // Centred on the virtual desktop, which is the whole of it on one
                // monitor and the middle of the arrangement on several. Somebody
                // who wants it centred on one screen can use Move.
                let (vx, vy, vw, vh) = virtual_screen_rect();
                SetWindowPos(
                    hwnd,
                    None,
                    vx + (vw - w) / 2,
                    vy + (vh - h) / 2,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
                .is_ok()
            }
            _ => false,
        }
    }
}

thread_local! {
    /// Title, handle and when it was resolved: re-running EnumWindows forty
    /// times a second to measure latency would be its own source of latency.
    static PROBE_TARGET: std::cell::RefCell<(String, Option<HWND>, u64)> =
        const { std::cell::RefCell::new((String::new(), None, 0)) };
}

/// Round-trip of an empty message through the target window's message loop,
/// in microseconds. `None` when the window is gone or refused to answer.
///
/// `WM_NULL` is chosen because it does nothing at all: the window's procedure
/// returns immediately, so what is timed is the wait in the queue rather than
/// any work the message caused. `SMTO_ABORTIFHUNG` keeps a wedged game from
/// parking this thread for the whole timeout.
pub fn probe_window_us(title: &str, timeout_ms: u32) -> Option<u64> {
    unsafe {
        let hwnd = PROBE_TARGET.with(|c| {
            let mut c = c.borrow_mut();
            let stale = now_us().saturating_sub(c.2) > 2_000_000;
            let dead = match c.1 {
                Some(h) => !IsWindow(Some(h)).as_bool(),
                None => true,
            };
            if c.0 != title || stale || dead {
                *c = (title.to_string(), find_window_handle(title), now_us());
            }
            c.1
        })?;
        let t0 = now_us();
        let mut out: usize = 0;
        let r = SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            timeout_ms,
            Some(&mut out),
        );
        if r.0 == 0 {
            // Timed out or the window died between the check and the send.
            PROBE_TARGET.with(|c| c.borrow_mut().1 = None);
            return None;
        }
        Some(now_us().saturating_sub(t0))
    }
}

unsafe fn enable_shutdown_privilege() {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
        .is_err()
        {
            return;
        }
        let mut luid = LUID::default();
        if LookupPrivilegeValueW(None, w!("SeShutdownPrivilege"), &mut luid).is_ok() {
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            let _ = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None);
        }
        let _ = CloseHandle(token);
    }
}

pub fn run_end_action(action: EndAction, delay_s: u32, reason: &str) -> anyhow::Result<()> {
    unsafe {
        match action {
            EndAction::Stop => Ok(()),
            EndAction::Shutdown | EndAction::Reboot => {
                enable_shutdown_privilege();
                let msg = wide(reason);
                let reboot = matches!(action, EndAction::Reboot);
                InitiateSystemShutdownExW(
                    PCWSTR::null(),
                    PCWSTR(msg.as_ptr()),
                    delay_s,
                    true,
                    reboot,
                    SHTDN_REASON_MAJOR_OTHER
                        | SHTDN_REASON_MINOR_OTHER
                        | SHTDN_REASON_FLAG_PLANNED,
                )
                .map_err(|e| anyhow::anyhow!("InitiateSystemShutdownExW failed: {e}"))
            }
            EndAction::LogOff => {
                enable_shutdown_privilege();
                ExitWindowsEx(
                    EWX_LOGOFF,
                    SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_MINOR_OTHER,
                )
                .map_err(|e| anyhow::anyhow!("ExitWindowsEx failed: {e}"))
            }
            EndAction::Sleep | EndAction::Hibernate => {
                enable_shutdown_privilege();
                let hibernate = matches!(action, EndAction::Hibernate);
                if SetSuspendState(hibernate, true, false) {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "SetSuspendState failed (hibernation may be disabled)"
                    ))
                }
            }
        }
    }
}

pub fn acquire_single_instance() -> bool {
    // MacroInspect is the same program built a second time, so it must not answer
    // to the shipped build's mutex. Sharing it means launching the inspect copy
    // while an ordinary Clickwork is running exits without a word, which reads
    // as a crash and costs an hour to work out.
    #[cfg(feature = "inspect")]
    let name = w!("Local\\MacroInspect_SingleInstance_v1");
    #[cfg(not(feature = "inspect"))]
    let name = w!("Local\\Clickwork_SingleInstance_v1");
    unsafe {
        match CreateMutexW(None, true, name) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    let _ = CloseHandle(handle);
                    false
                } else {
                    // Never closed on purpose: the mutex must live as long as the
                    // process. HANDLE has no Drop, so letting it fall out of scope
                    // leaves the kernel object open.
                    true
                }
            }
            Err(_) => true,
        }
    }
}

/// Hides or restores our own top-level window. `false` only when there is no
/// window yet.
///
/// The answer is there because the Linux side genuinely cannot always do this -
/// no Wayland protocol puts a window out of sight, and only some compositors
/// honour a minimise request - and the tray menu should not relabel itself after
/// a call that did nothing. Here it is `ShowWindow`, which does not fail.
pub fn set_window_hidden(hidden: bool) -> bool {
    unsafe {
        let hwnd = app_hwnd();
        if hwnd.0.is_null() {
            return false;
        }
        if hidden {
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
        true
    }
}

/// Asks the main window to close, exactly the way the close button does.
pub fn request_app_close() {
    unsafe {
        let hwnd = app_hwnd();
        if !hwnd.0.is_null() {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn focus_existing_instance() {
    unsafe {
        let title = wide(APP_TITLE);
        if let Ok(hwnd) = FindWindowW(None, PCWSTR(title.as_ptr()))
            && !hwnd.0.is_null() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
    }
}

/// Resolved dynamically so the crate needs no Win32_System_Console feature.
pub fn attach_parent_console() {
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    unsafe {
        let Ok(kernel32) = GetModuleHandleW(w!("kernel32.dll")) else {
            return;
        };
        let Some(sym) = GetProcAddress(kernel32, s!("AttachConsole")) else {
            return;
        };
        let attach: unsafe extern "system" fn(u32) -> i32 = std::mem::transmute(sym);
        let _ = attach(ATTACH_PARENT_PROCESS);
    }
}

pub fn set_dpi_awareness() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// What this process is currently costing: private bytes, open handles, GDI
/// objects.
///
/// All three are resolved at run time rather than linked. `GetProcessMemoryInfo`
/// lives in a psapi feature nothing else here needs, and a number that only a
/// soak test reads is not worth widening the build's dependency surface for. The
/// same trick is used for `AttachConsole` a few lines up.
pub fn process_cost() -> (u64, u32, u32) {
    #[repr(C)]
    #[derive(Default)]
    struct MemCountersEx {
        cb: u32,
        page_fault_count: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged_pool: usize,
        quota_paged_pool: usize,
        quota_peak_non_paged_pool: usize,
        quota_non_paged_pool: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
        private_usage: usize,
    }

    unsafe {
        let me = windows::Win32::System::Threading::GetCurrentProcess();
        let mut private = 0u64;
        let mut handles = 0u32;
        let mut gdi = 0u32;

        if let Ok(k32) = GetModuleHandleW(w!("kernel32.dll")) {
            if let Some(sym) =
                GetProcAddress(k32, s!("K32GetProcessMemoryInfo"))
            {
                let f: unsafe extern "system" fn(
                    windows::Win32::Foundation::HANDLE,
                    *mut MemCountersEx,
                    u32,
                ) -> i32 = std::mem::transmute(sym);
                let mut pmc = MemCountersEx {
                    cb: std::mem::size_of::<MemCountersEx>() as u32,
                    ..Default::default()
                };
                if f(me, &mut pmc, pmc.cb) != 0 {
                    private = pmc.private_usage as u64;
                }
            }
            if let Some(sym) =
                GetProcAddress(k32, s!("GetProcessHandleCount"))
            {
                let f: unsafe extern "system" fn(
                    windows::Win32::Foundation::HANDLE,
                    *mut u32,
                ) -> i32 = std::mem::transmute(sym);
                let mut n = 0u32;
                if f(me, &mut n) != 0 {
                    handles = n;
                }
            }
        }
        if let Ok(u32dll) = GetModuleHandleW(w!("user32.dll"))
            && let Some(sym) = GetProcAddress(u32dll, s!("GetGuiResources"))
            {
                let f: unsafe extern "system" fn(
                    windows::Win32::Foundation::HANDLE,
                    u32,
                ) -> u32 = std::mem::transmute(sym);
                gdi = f(me, 0); // GR_GDIOBJECTS
            }
        (private, handles, gdi)
    }
}
