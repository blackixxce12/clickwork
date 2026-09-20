//! The pieces of a Wayland connection every subsystem here needs: the registry,
//! the outputs with their logical and physical geometry, shared memory buffers.
//!
//! Each subsystem - injection, capture, the overlay - opens a connection of its
//! own and owns its event queue outright. A single shared connection would need a
//! lock around every dispatch, and the playback thread would then wait on the
//! overlay thread's repaint to move the cursor. Three sockets to the compositor
//! cost nothing measurable.

use std::os::fd::{AsFd as _, FromRawFd as _, OwnedFd};
use wayland_client::globals::{GlobalList, GlobalListContents};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_registry, wl_seat, wl_shm, wl_shm_pool,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};

/// One output, as Wayland describes it.
#[derive(Clone, Debug)]
pub struct OutputInfo {
    pub wl: wl_output::WlOutput,
    pub name: String,
    /// Logical position and size, from `xdg_output`.
    pub lx: i32,
    pub ly: i32,
    pub lw: i32,
    pub lh: i32,
    /// The current mode, in physical pixels, as the panel reports it.
    pub pw: i32,
    pub ph: i32,
    pub rotated: bool,
    pub int_scale: i32,
    pub done: bool,
}

impl OutputInfo {
    fn new(wl: wl_output::WlOutput) -> Self {
        Self {
            wl,
            name: String::new(),
            lx: 0,
            ly: 0,
            lw: 0,
            lh: 0,
            pw: 0,
            ph: 0,
            rotated: false,
            int_scale: 1,
            done: false,
        }
    }

    /// Physical pixels per logical pixel, fractional when the compositor is.
    pub fn scale(&self) -> f64 {
        let (pw, ph) = if self.rotated { (self.ph, self.pw) } else { (self.pw, self.ph) };
        if self.lw > 0 && pw > 0 {
            let sx = pw as f64 / self.lw as f64;
            let sy = if self.lh > 0 { ph as f64 / self.lh as f64 } else { sx };
            // Both axes carry the same scale; the mean forgives one of them being
            // rounded the other way.
            ((sx + sy) / 2.0 * 1000.0).round() / 1000.0
        } else {
            self.int_scale.max(1) as f64
        }
    }
}

/// What the registry offered and what was bound, minus anything specific to a
/// subsystem.
#[derive(Default)]
pub struct Base {
    pub outputs: Vec<OutputInfo>,
    pub shm: Option<wl_shm::WlShm>,
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub seat: Option<wl_seat::WlSeat>,
    pub xdg_output_mgr: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
}

impl AsMut<Base> for Base {
    fn as_mut(&mut self) -> &mut Base {
        self
    }
}

impl Base {
    /// Binds the common globals. Outputs are bound too, but their geometry only
    /// arrives with the next round trip; callers do one before reading it.
    pub fn bind<S>(globals: &GlobalList, qh: &QueueHandle<S>) -> Self
    where
        S: Dispatch<wl_shm::WlShm, ()>
            + Dispatch<wl_compositor::WlCompositor, ()>
            + Dispatch<wl_seat::WlSeat, ()>
            + Dispatch<zxdg_output_manager_v1::ZxdgOutputManagerV1, ()>
            + Dispatch<wl_output::WlOutput, usize>
            + Dispatch<zxdg_output_v1::ZxdgOutputV1, usize>
            + 'static,
    {
        let shm = globals.bind::<wl_shm::WlShm, S, ()>(qh, 1..=1, ()).ok();
        let compositor =
            globals.bind::<wl_compositor::WlCompositor, S, ()>(qh, 4..=6, ()).ok();
        let seat = globals.bind::<wl_seat::WlSeat, S, ()>(qh, 1..=9, ()).ok();
        let xdg_output_mgr = globals
            .bind::<zxdg_output_manager_v1::ZxdgOutputManagerV1, S, ()>(qh, 1..=3, ())
            .ok();
        let mut outputs = Vec::new();
        for g in globals.contents().clone_list() {
            if g.interface == wl_output::WlOutput::interface().name {
                let idx = outputs.len();
                let out = globals.registry().bind::<wl_output::WlOutput, usize, S>(
                    g.name,
                    g.version.min(4),
                    qh,
                    idx,
                );
                if let Some(m) = xdg_output_mgr.as_ref() {
                    let _ = m.get_xdg_output(&out, qh, idx);
                }
                outputs.push(OutputInfo::new(out));
            }
        }
        Self { outputs, shm, compositor, seat, xdg_output_mgr }
    }

    pub fn output_named(&self, name: &str) -> Option<&OutputInfo> {
        self.outputs.iter().find(|o| o.name == name)
    }
}

impl<S: AsMut<Base> + Dispatch<wl_output::WlOutput, usize>> Dispatch<wl_output::WlOutput, usize, S>
    for Base
{
    fn event(
        st: &mut S,
        _: &wl_output::WlOutput,
        ev: wl_output::Event,
        idx: &usize,
        _: &Connection,
        _: &QueueHandle<S>,
    ) {
        let Some(o) = st.as_mut().outputs.get_mut(*idx) else { return };
        match ev {
            wl_output::Event::Geometry { transform, .. } => {
                o.rotated = matches!(
                    transform.into_result().ok(),
                    Some(
                        wl_output::Transform::_90
                            | wl_output::Transform::_270
                            | wl_output::Transform::Flipped90
                            | wl_output::Transform::Flipped270
                    )
                );
            }
            wl_output::Event::Mode { flags, width, height, .. } => {
                let current = flags
                    .into_result()
                    .map(|f| f.contains(wl_output::Mode::Current))
                    .unwrap_or(false);
                if current || o.pw == 0 {
                    o.pw = width;
                    o.ph = height;
                }
            }
            wl_output::Event::Scale { factor } => o.int_scale = factor,
            wl_output::Event::Name { name } => {
                if o.name.is_empty() {
                    o.name = name;
                }
            }
            wl_output::Event::Done => o.done = true,
            _ => {}
        }
    }
}

impl<S: AsMut<Base> + Dispatch<zxdg_output_v1::ZxdgOutputV1, usize>>
    Dispatch<zxdg_output_v1::ZxdgOutputV1, usize, S> for Base
{
    fn event(
        st: &mut S,
        _: &zxdg_output_v1::ZxdgOutputV1,
        ev: zxdg_output_v1::Event,
        idx: &usize,
        _: &Connection,
        _: &QueueHandle<S>,
    ) {
        let Some(o) = st.as_mut().outputs.get_mut(*idx) else { return };
        match ev {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                o.lx = x;
                o.ly = y;
            }
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                o.lw = width;
                o.lh = height;
            }
            zxdg_output_v1::Event::Name { name } => o.name = name,
            zxdg_output_v1::Event::Done => o.done = true,
            _ => {}
        }
    }
}

/// A `wl_buffer` over a piece of memory this process can read and write.
pub struct ShmBuf {
    pub buffer: wl_buffer::WlBuffer,
    pool: wl_shm_pool::WlShmPool,
    ptr: *mut u8,
    len: usize,
    _fd: OwnedFd,
    pub w: u32,
    pub h: u32,
    pub stride: u32,
    pub format: wl_shm::Format,
}

// The mapping is process memory like any other; the Wayland objects are `Send`.
unsafe impl Send for ShmBuf {}

impl ShmBuf {
    pub fn new<S>(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<S>,
        w: u32,
        h: u32,
        stride: u32,
        format: wl_shm::Format,
    ) -> Option<Self>
    where
        S: Dispatch<wl_shm_pool::WlShmPool, ()> + Dispatch<wl_buffer::WlBuffer, ()> + 'static,
    {
        let len = (stride as usize).checked_mul(h as usize)?;
        if len == 0 || len > (1 << 30) {
            return None;
        }
        let fd = memfd(len)?;
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_fd().as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return None;
        }
        let pool = shm.create_pool(fd.as_fd(), len as i32, qh, ());
        let buffer = pool.create_buffer(0, w as i32, h as i32, stride as i32, format, qh, ());
        Some(Self { buffer, pool, ptr: ptr as *mut u8, len, _fd: fd, w, h, stride, format })
    }

    pub fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for ShmBuf {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.pool.destroy();
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.len);
        }
    }
}

use std::os::fd::AsRawFd as _;

/// An anonymous file of `len` bytes, sealed against shrinking.
pub fn memfd(len: usize) -> Option<OwnedFd> {
    let name = c"clickwork-shm";
    let raw = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING) };
    if raw < 0 {
        return None;
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    if unsafe { libc::ftruncate(fd.as_raw_fd(), len as libc::off_t) } != 0 {
        return None;
    }
    unsafe {
        libc::fcntl(fd.as_raw_fd(), libc::F_ADD_SEALS, libc::F_SEAL_SHRINK);
    }
    Some(fd)
}

/// A memfd holding `bytes`, for handing a keymap to the compositor.
pub fn memfd_with(bytes: &[u8]) -> Option<OwnedFd> {
    use std::io::Write as _;
    let fd = memfd(bytes.len().max(1))?;
    let mut f = std::fs::File::from(fd);
    f.write_all(bytes).ok()?;
    f.flush().ok()?;
    Some(OwnedFd::from(f))
}

// ---- a bare probe, for the layout fallback ------------------------------------

struct Probe {
    base: Base,
}

impl AsMut<Base> for Probe {
    fn as_mut(&mut self) -> &mut Base {
        &mut self.base
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Probe {
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

wayland_client::delegate_dispatch!(Probe: [wl_output::WlOutput: usize] => Base);
wayland_client::delegate_dispatch!(Probe: [zxdg_output_v1::ZxdgOutputV1: usize] => Base);
delegate_noop!(Probe: ignore wl_shm::WlShm);
delegate_noop!(Probe: ignore wl_seat::WlSeat);
delegate_noop!(Probe: wl_compositor::WlCompositor);
delegate_noop!(Probe: zxdg_output_manager_v1::ZxdgOutputManagerV1);

/// Every output the compositor lists, with its geometry. Empty when there is no
/// Wayland display at all.
pub fn outputs() -> Vec<OutputInfo> {
    let Ok(conn) = Connection::connect_to_env() else {
        return Vec::new();
    };
    let Ok((globals, mut queue)) = wayland_client::globals::registry_queue_init::<Probe>(&conn)
    else {
        return Vec::new();
    };
    let qh = queue.handle();
    let mut st = Probe { base: Base::bind(&globals, &qh) };
    // Two trips: one for the outputs' own events, one for xdg_output's, which are
    // requested during the first.
    let _ = queue.roundtrip(&mut st);
    let _ = queue.roundtrip(&mut st);
    st.base.outputs
}

/// Every interface the compositor advertises, by name.
///
/// This is the whole of what a session can be asked before anything is bound, and
/// it is the difference between a feature that is missing and a feature that is
/// broken - so both `--doctor` and `--selftest session` read it from here rather
/// than each opening a registry of its own. Empty when there is no display.
///
/// The list arrives with `registry_queue_init`, so no roundtrip is needed. Note
/// that it is alphabetical and `ext_*` sorts first: truncate it and the newest
/// protocols are exactly what disappears.
pub fn globals() -> Vec<String> {
    let Ok(conn) = Connection::connect_to_env() else {
        return Vec::new();
    };
    let Ok((globals, _queue)) = wayland_client::globals::registry_queue_init::<Probe>(&conn) else {
        return Vec::new();
    };
    globals.contents().clone_list().into_iter().map(|g| g.interface).collect()
}

/// The seat, when the compositor has one. Handy for a module that needs nothing
/// else from the base.
pub fn has_display() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}
