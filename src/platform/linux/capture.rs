//! Reading the screen back through `zwlr_screencopy_manager_v1`.
//!
//! The compositor already has the picture; this asks it to copy one rectangle of
//! one output into shared memory. There is no GDI on Wayland, no `BitBlt`, and no
//! second path to fall back to - a compositor that refuses screencopy leaves the
//! picture search blind, and the counters say so.
//!
//! Frames come back in the output's *physical* pixels while the request is made in
//! its logical ones. `super::geom` is the bridge; the rectangle asked for here is
//! physical, the region sent to the compositor is the logical rectangle that
//! covers it, and the result is cut back to the pixels wanted.
//!
//! One session per thread, like the Windows duplication object it replaces: the
//! playback thread and a search thread each hold their own connection and never
//! wait on one another.

use super::wl::{Base, ShmBuf};
use crate::vision::{Frame, Order};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_registry, wl_seat, wl_shm, wl_shm_pool,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy as _, QueueHandle, delegate_noop};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1,
};

pub static HITS: AtomicU64 = AtomicU64::new(0);
pub static MISSES: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct Pending {
    buffer: Option<(wl_shm::Format, u32, u32, u32)>,
    y_invert: bool,
    buffer_done: bool,
    ready: bool,
    failed: bool,
}

struct St {
    base: Base,
    pending: Pending,
}

impl AsMut<Base> for St {
    fn as_mut(&mut self) -> &mut Base {
        &mut self.base
    }
}

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
wayland_client::delegate_dispatch!(St: [wl_output::WlOutput: usize] => Base);
wayland_client::delegate_dispatch!(St: [zxdg_output_v1::ZxdgOutputV1: usize] => Base);
delegate_noop!(St: ignore wl_shm::WlShm);
delegate_noop!(St: ignore wl_seat::WlSeat);
delegate_noop!(St: ignore wl_buffer::WlBuffer);
delegate_noop!(St: wl_shm_pool::WlShmPool);
delegate_noop!(St: wl_compositor::WlCompositor);
delegate_noop!(St: zxdg_output_manager_v1::ZxdgOutputManagerV1);
delegate_noop!(St: zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1);

impl Dispatch<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, ()> for St {
    fn event(
        st: &mut Self,
        _: &zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
        ev: zwlr_screencopy_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwlr_screencopy_frame_v1::Event as E;
        match ev {
            E::Buffer { format, width, height, stride } => {
                if let Ok(f) = format.into_result() {
                    st.pending.buffer = Some((f, width, height, stride));
                }
            }
            E::Flags { flags } => {
                st.pending.y_invert = flags
                    .into_result()
                    .map(|f| f.contains(zwlr_screencopy_frame_v1::Flags::YInvert))
                    .unwrap_or(false);
            }
            E::BufferDone => st.pending.buffer_done = true,
            E::Ready { .. } => st.pending.ready = true,
            E::Failed => st.pending.failed = true,
            _ => {}
        }
    }
}

struct Session {
    conn: Connection,
    queue: EventQueue<St>,
    qh: QueueHandle<St>,
    st: St,
    mgr: zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
    /// The last buffer, kept while its size and format hold.
    buf: Option<ShmBuf>,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    static STRIKES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

impl Session {
    fn connect() -> Option<Self> {
        let conn = Connection::connect_to_env().ok()?;
        let (globals, mut queue) = registry_queue_init::<St>(&conn).ok()?;
        let qh = queue.handle();
        let base = Base::bind(&globals, &qh);
        let mgr = globals
            .bind::<zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1, St, ()>(&qh, 1..=3, ())
            .ok()?;
        let mut st = St { base, pending: Pending::default() };
        let _ = queue.roundtrip(&mut st);
        let _ = queue.roundtrip(&mut st);
        if st.base.shm.is_none() {
            return None;
        }
        Some(Self { conn, queue, qh, st, mgr, buf: None })
    }

    /// Copies one logical rectangle of one output. Returns the physical pixels, the
    /// buffer's width and height, its format and whether rows are upside down.
    fn grab(
        &mut self,
        out: &wl_output::WlOutput,
        lx: i32,
        ly: i32,
        lw: i32,
        lh: i32,
    ) -> Option<(Vec<u8>, u32, u32, u32, wl_shm::Format, bool)> {
        self.st.pending = Pending::default();
        let frame = self.mgr.capture_output_region(0, out, lx, ly, lw, lh, &self.qh, ());
        let _ = self.conn.flush();
        // First the description of the buffer wanted.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !(self.st.pending.buffer_done || self.st.pending.failed)
            && !(self.mgr.version() < 3 && self.st.pending.buffer.is_some())
        {
            if std::time::Instant::now() > deadline
                || self.queue.blocking_dispatch(&mut self.st).is_err()
            {
                frame.destroy();
                return None;
            }
        }
        let (format, w, h, stride) = match (self.st.pending.failed, self.st.pending.buffer) {
            (false, Some(b)) => b,
            _ => {
                frame.destroy();
                return None;
            }
        };
        let fits = self
            .buf
            .as_ref()
            .is_some_and(|b| b.w == w && b.h == h && b.stride == stride && b.format == format);
        if !fits {
            self.buf = None;
            let shm = self.st.base.shm.clone()?;
            self.buf = ShmBuf::new(&shm, &self.qh, w, h, stride, format);
        }
        let Some(buf) = self.buf.as_ref() else {
            frame.destroy();
            return None;
        };
        frame.copy(&buf.buffer);
        let _ = self.conn.flush();
        while !(self.st.pending.ready || self.st.pending.failed) {
            if std::time::Instant::now() > deadline
                || self.queue.blocking_dispatch(&mut self.st).is_err()
            {
                frame.destroy();
                return None;
            }
        }
        let ok = self.st.pending.ready;
        let y_invert = self.st.pending.y_invert;
        frame.destroy();
        let _ = self.conn.flush();
        if !ok {
            return None;
        }
        let buf = self.buf.as_ref()?;
        Some((buf.bytes().to_vec(), w, h, stride, format, y_invert))
    }
}

fn with_session<R>(f: impl FnOnce(&mut Session) -> R) -> Option<R> {
    SESSION.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            if STRIKES.with(|s| s.get()) >= 3 {
                return None;
            }
            match Session::connect() {
                Some(s) => *slot = Some(s),
                None => {
                    let n = STRIKES.with(|s| {
                        s.set(s.get() + 1);
                        s.get()
                    });
                    if n == 3 {
                        tracing::warn!(
                            "screen capture is unavailable on this thread: no Wayland display or \
                             no zwlr_screencopy_manager_v1"
                        );
                    }
                    return None;
                }
            }
        }
        slot.as_mut().map(f)
    })
}

/// Drops this thread's session and buffer.
pub fn release() {
    SESSION.with(|cell| *cell.borrow_mut() = None);
    STRIKES.with(|s| s.set(0));
}

/// Grabs a physical rectangle of the screen, BGRA.
pub fn capture(x: i32, y: i32, w: i32, h: i32) -> Option<Frame> {
    if w <= 0 || h <= 0 || (w as i64) * (h as i64) > (1i64 << 28) {
        return None;
    }
    // KWin implements no screencopy of any kind, and nothing else implements its
    // screenshot interface, so these two never both answer on one session: this
    // is a fork in the road rather than a preference between two paths.
    if super::kdeshot::available() {
        let hit = super::kdeshot::capture(x, y, w, h);
        if hit.is_some() {
            HITS.fetch_add(1, Ordering::Relaxed);
        } else {
            MISSES.fetch_add(1, Ordering::Relaxed);
        }
        return hit;
    }
    let layout = super::geom::layout();
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    let mut any = false;
    let result = with_session(|s| {
        for mon in &layout.mons {
            // The part of the request that lies on this monitor.
            let ix0 = x.max(mon.px);
            let iy0 = y.max(mon.py);
            let ix1 = (x + w).min(mon.px + mon.pw);
            let iy1 = (y + h).min(mon.py + mon.ph);
            if ix1 <= ix0 || iy1 <= iy0 {
                continue;
            }
            let Some(output) = s.st.base.output_named(&mon.name).map(|o| o.wl.clone()) else {
                // The name the compositor gave the output does not match the IPC's;
                // the single-output case can still be served.
                if layout.mons.len() == 1 && s.st.base.outputs.len() == 1 {
                    let o = s.st.base.outputs[0].wl.clone();
                    if !blit(s, &o, mon, &mut out, x, y, w, ix0, iy0, ix1, iy1) {
                        return false;
                    }
                    any = true;
                }
                continue;
            };
            if !blit(s, &output, mon, &mut out, x, y, w, ix0, iy0, ix1, iy1) {
                return false;
            }
            any = true;
        }
        true
    });
    match result {
        Some(true) if any => {
            HITS.fetch_add(1, Ordering::Relaxed);
            Some(Frame { x, y, w: w as u32, h: h as u32, px: out, order: Order::Bgra })
        }
        _ => {
            MISSES.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

/// Copies the physical rectangle `ix0..ix1 × iy0..iy1` of `mon` into `out`.
#[allow(clippy::too_many_arguments)]
fn blit(
    s: &mut Session,
    output: &wl_output::WlOutput,
    mon: &super::geom::Mon,
    out: &mut [u8],
    x: i32,
    y: i32,
    w: i32,
    ix0: i32,
    iy0: i32,
    ix1: i32,
    iy1: i32,
) -> bool {
    let sc = mon.scale;
    // The logical rectangle, local to the output, that covers the physical one.
    let lx0 = ((ix0 - mon.px) as f64 / sc).floor() as i32;
    let ly0 = ((iy0 - mon.py) as f64 / sc).floor() as i32;
    let lx1 = ((ix1 - mon.px) as f64 / sc).ceil() as i32;
    let ly1 = ((iy1 - mon.py) as f64 / sc).ceil() as i32;
    let (lw, lh) = ((lx1 - lx0).max(1).min(mon.lw), (ly1 - ly0).max(1).min(mon.lh));
    let lx0 = lx0.clamp(0, (mon.lw - 1).max(0));
    let ly0 = ly0.clamp(0, (mon.lh - 1).max(0));
    let Some((px, bw, bh, stride, format, y_invert)) = s.grab(output, lx0, ly0, lw, lh) else {
        return false;
    };
    // Where the buffer's first pixel sits in physical space.
    let bx = mon.px + (lx0 as f64 * sc).round() as i32;
    let by = mon.py + (ly0 as f64 * sc).round() as i32;
    let swap = matches!(format, wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888);
    let row_out = (w as usize) * 4;
    for py in iy0..iy1 {
        let sy = py - by;
        if sy < 0 || sy >= bh as i32 {
            continue;
        }
        let sy = if y_invert { bh as i32 - 1 - sy } else { sy } as usize;
        let src_row = &px[sy * stride as usize..];
        let dy = (py - y) as usize;
        for pxl in ix0..ix1 {
            let sx = pxl - bx;
            if sx < 0 || sx >= bw as i32 {
                continue;
            }
            let si = (sx as usize) * 4;
            if si + 4 > src_row.len() {
                continue;
            }
            let di = dy * row_out + ((pxl - x) as usize) * 4;
            let p = &src_row[si..si + 4];
            if swap {
                out[di] = p[2];
                out[di + 1] = p[1];
                out[di + 2] = p[0];
            } else {
                out[di] = p[0];
                out[di + 1] = p[1];
                out[di + 2] = p[2];
            }
            out[di + 3] = 255;
        }
    }
    true
}

pub fn counters() -> (u64, u64, u64) {
    (HITS.load(Ordering::Relaxed), 0, MISSES.load(Ordering::Relaxed))
}

pub fn reset_counters() {
    HITS.store(0, Ordering::Relaxed);
    MISSES.store(0, Ordering::Relaxed);
}
