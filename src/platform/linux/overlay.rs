//! The see-through overlay: a layer-shell surface per output, drawn in software.
//!
//! `zwlr_layer_shell_v1` is what bars and notifications use to sit above every
//! window without being one. An overlay-layer surface anchored to all four edges
//! with an empty input region is exactly the colour-keyed layered window of the
//! Windows build: it covers the screen, it takes no clicks, it never has focus,
//! and it costs one shared-memory buffer per output.
//!
//! Rectangles are drawn by hand and text with `ab_glyph` over whatever monospace
//! face fontconfig names. The buffer is the output's physical size and is shown
//! at its logical size through `wp_viewporter`, so a fractional scale draws
//! crisp one-pixel lines rather than a resampled blur.

use super::wl::{Base, ShmBuf};
use crate::{HUD, HUD_CORNER, SIGHTING, hud};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_region, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

static RUNNING: AtomicBool = AtomicBool::new(false);
static WANTED: AtomicBool = AtomicBool::new(false);
/// Why this session will never show an overlay, once that much is settled.
static UNAVAILABLE: OnceLock<&'static str> = OnceLock::new();
/// When an attempt that might still have worked gave up, so the next one waits on
/// a clock instead of on the next repaint. Zero means there has not been one.
static LAST_FAIL_US: AtomicU64 = AtomicU64::new(0);

/// Turns the overlay on or off. Cheap and idempotent; safe to call every frame.
///
/// Every frame is the whole difficulty: the UI asks for this once per repaint, so
/// "it is not running" must not be read as "start it again now". A compositor that
/// does not implement `zwlr_layer_shell_v1` would otherwise be handed a thread per
/// frame for the length of the session, each one dying the same way. An answer that
/// cannot change is kept, and from then on the overlay says it is unavailable; a
/// session that merely has no compositor this second is tried again in a few.
pub fn set_enabled(on: bool) {
    let was = WANTED.swap(on, Ordering::Relaxed);
    if on {
        if UNAVAILABLE.get().is_some() {
            return;
        }
        let last = LAST_FAIL_US.load(Ordering::Relaxed);
        if last != 0 && crate::now_us().saturating_sub(last) < 5_000_000 {
            return;
        }
        if !RUNNING.swap(true, Ordering::Relaxed)
            && std::thread::Builder::new().name("overlay".into()).spawn(run).is_err()
        {
            // The flag is claimed before the thread exists, so a spawn that does not
            // happen has to hand it back; otherwise the overlay is off for the rest
            // of the session and nothing ever says why.
            RUNNING.store(false, Ordering::Relaxed);
        }
        return;
    }
    let _ = was;
}

pub fn shutdown() {
    set_enabled(false);
}

/// False once this session has been shown to have no way of drawing over the screen.
///
/// Not the same question as "will the overlay work", and deliberately so: nothing is
/// known until an attempt has come back, so this answers true before the first one.
/// It is here to stop asking, not to promise anything.
pub fn available() -> bool {
    UNAVAILABLE.get().is_none()
}

/// A short reason it cannot be, for the doctor and for anyone who would rather say
/// so than leave the user watching an empty screen. Only a refusal that asking
/// again cannot mend appears here, so this stays `None` while the overlay has
/// simply not started yet.
pub fn unavailable_reason() -> Option<&'static str> {
    UNAVAILABLE.get().copied()
}

struct Pane {
    idx: usize,
    surface: wl_surface::WlSurface,
    layer: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    viewport: Option<wp_viewport::WpViewport>,
    bufs: [Option<ShmBuf>; 2],
    which: usize,
    /// Logical size the compositor configured.
    lw: u32,
    lh: u32,
    configured: bool,
    closed: bool,
}

struct St {
    base: Base,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    viewporter: Option<wp_viewporter::WpViewporter>,
    /// (serial, width, height, closed) per pane, by pane index.
    configures: Vec<(Option<(u32, u32, u32)>, bool)>,
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
delegate_noop!(St: ignore wl_surface::WlSurface);
delegate_noop!(St: wl_shm_pool::WlShmPool);
delegate_noop!(St: wl_compositor::WlCompositor);
delegate_noop!(St: wl_region::WlRegion);
delegate_noop!(St: zxdg_output_manager_v1::ZxdgOutputManagerV1);
delegate_noop!(St: zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(St: wp_viewporter::WpViewporter);
delegate_noop!(St: wp_viewport::WpViewport);

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, usize> for St {
    fn event(
        st: &mut Self,
        layer: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        ev: zwlr_layer_surface_v1::Event,
        idx: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if st.configures.len() <= *idx {
            st.configures.resize(*idx + 1, (None, false));
        }
        match ev {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                layer.ack_configure(serial);
                st.configures[*idx].0 = Some((serial, width, height));
            }
            zwlr_layer_surface_v1::Event::Closed => st.configures[*idx].1 = true,
            _ => {}
        }
    }
}

/// A monospace face for the labels, found the way any desktop program finds one.
fn font_bytes() -> Option<Vec<u8>> {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(out) = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "monospace:bold"])
        .output()
        && out.status.success()
    {
        let f = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !f.is_empty() {
            candidates.push(f);
        }
    }
    candidates.extend(
        [
            "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
            "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        ]
        .map(String::from),
    );
    candidates.into_iter().find_map(|p| std::fs::read(p).ok())
}

/// Something to draw on: a physical-pixel ARGB canvas.
struct Canvas<'a> {
    px: &'a mut [u8],
    w: i32,
    h: i32,
    stride: usize,
    scale: f32,
    font: Option<&'a ab_glyph::FontVec>,
}

impl Canvas<'_> {
    fn clear(&mut self) {
        self.px.fill(0);
    }

    #[inline]
    fn put(&mut self, x: i32, y: i32, rgb: (u8, u8, u8), a: u8) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let i = y as usize * self.stride + x as usize * 4;
        if i + 4 > self.px.len() {
            return;
        }
        // Premultiplied ARGB8888 is B, G, R, A in memory.
        let k = a as u32;
        self.px[i] = ((rgb.2 as u32 * k) / 255) as u8;
        self.px[i + 1] = ((rgb.1 as u32 * k) / 255) as u8;
        self.px[i + 2] = ((rgb.0 as u32 * k) / 255) as u8;
        self.px[i + 3] = a;
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: (u8, u8, u8), a: u8) {
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                self.put(xx, yy, rgb, a);
            }
        }
    }

    fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: (u8, u8, u8), width: i32) {
        let t = (width as f32 * self.scale).round().max(1.0) as i32;
        self.fill_rect(x, y, w, t, rgb, 255);
        self.fill_rect(x, y + h - t, w, t, rgb, 255);
        self.fill_rect(x, y, t, h, rgb, 255);
        self.fill_rect(x + w - t, y, t, h, rgb, 255);
    }

    /// Draws `text` with its top-left at (x, y); returns the advance.
    fn text(&mut self, x: i32, y: i32, size_px: f32, text: &str, rgb: (u8, u8, u8)) -> i32 {
        use ab_glyph::{Font as _, ScaleFont as _};
        let Some(font) = self.font else { return 0 };
        let scale = ab_glyph::PxScale::from(size_px * self.scale);
        let sf = font.as_scaled(scale);
        let mut pen = x as f32;
        let base = y as f32 + sf.ascent();
        for ch in text.chars() {
            let id = font.glyph_id(ch);
            let glyph = id.with_scale_and_position(scale, ab_glyph::point(pen, base));
            if let Some(og) = font.outline_glyph(glyph) {
                let b = og.px_bounds();
                og.draw(|gx, gy, c| {
                    if c > 0.05 {
                        let px = b.min.x as i32 + gx as i32;
                        let py = b.min.y as i32 + gy as i32;
                        // Over whatever is there already: text sits on the plate.
                        let a = (c * 255.0) as u8;
                        if a > 0 {
                            self.blend(px, py, rgb, a);
                        }
                    }
                });
            }
            pen += sf.h_advance(id);
        }
        (pen - x as f32) as i32
    }

    #[inline]
    fn blend(&mut self, x: i32, y: i32, rgb: (u8, u8, u8), a: u8) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let i = y as usize * self.stride + x as usize * 4;
        if i + 4 > self.px.len() {
            return;
        }
        let a = a as u32;
        let inv = 255 - a;
        let db = self.px[i] as u32;
        let dg = self.px[i + 1] as u32;
        let dr = self.px[i + 2] as u32;
        let da = self.px[i + 3] as u32;
        self.px[i] = ((rgb.2 as u32 * a + db * inv) / 255) as u8;
        self.px[i + 1] = ((rgb.1 as u32 * a + dg * inv) / 255) as u8;
        self.px[i + 2] = ((rgb.0 as u32 * a + dr * inv) / 255) as u8;
        self.px[i + 3] = (a + da * inv / 255).min(255) as u8;
    }
}

/// Paints one output's share of the sighting and the display.
fn paint(c: &mut Canvas<'_>, ox: i32, oy: i32) {
    c.clear();
    let seen = SIGHTING.lock().clone();
    let s = c.scale;
    // Blue: where it was allowed to look. Amber: where text was read. Violet: the
    // interface element. Green or red: the match itself. Same colours as Windows.
    if let Some((x, y, w, h)) = seen.area {
        c.frame(x - ox, y - oy, w, h, (0x5A, 0xA0, 0xFF), 1);
    }
    if let Some((x, y, w, h)) = seen.text {
        c.frame(x - ox, y - oy, w, h, (0xFF, 0xBE, 0x3C), 1);
    }
    if let Some((x, y, w, h)) = seen.element {
        c.frame(x - ox, y - oy, w, h, (0xB4, 0x78, 0xFF), 2);
    }
    if let Some((x, y, w, h, score)) = seen.hit {
        let col = if score >= 0.85 { (0x50, 0xDC, 0x78) } else { (0xF0, 0x5A, 0x5A) };
        c.frame(x - ox, y - oy, w, h, col, 2);
        let label = format!("{score:.3}");
        let ty = y - oy + h + (2.0 * s) as i32;
        let tw = (label.len() as f32 * 9.0 * s) as i32 + (8.0 * s) as i32;
        c.fill_rect(x - ox, ty, tw, (19.0 * s) as i32, (0x18, 0x18, 0x18), 200);
        c.text(x - ox + (4.0 * s) as i32, ty, 15.0, &label, col);
    }
    if !seen.note.is_empty() {
        let tw = (seen.note.chars().count() as f32 * 9.0 * s) as i32 + (8.0 * s) as i32;
        c.fill_rect((8.0 * s) as i32, (8.0 * s) as i32, tw, (21.0 * s) as i32, (0x18, 0x18, 0x18), 200);
        c.text((12.0 * s) as i32, (9.0 * s) as i32, 15.0, &seen.note, (0xE6, 0xE6, 0xE6));
    }
    draw_hud(c);
}

/// The heads-up display: four blocks of short lines in one corner. See the
/// Windows `draw_hud` for the headings and why they are worded as they are.
fn draw_hud(c: &mut Canvas<'_>) {
    let d = hud();
    let blocks: [(&str, &Vec<String>, (u8, u8, u8)); 4] = [
        ("RESPONSE", &d.game, (0xC8, 0xC8, 0xC8)),
        ("MACRO", &d.macro_lines, (0x50, 0xDC, 0x78)),
        ("RELIABILITY", &d.reliability, (0xFF, 0xBE, 0x3C)),
        ("STATE", &d.state, (0xE6, 0xE6, 0xE6)),
    ];
    if blocks.iter().all(|(_, lines, _)| lines.is_empty()) {
        return;
    }
    let s = c.scale;
    let line = (17.0 * s) as i32;
    let pad = (10.0 * s) as i32;
    let width = (190.0 * s) as i32;
    let rows: i32 = blocks
        .iter()
        .filter(|(_, l, _)| !l.is_empty())
        .map(|(_, l, _)| l.len() as i32 + 1)
        .sum();
    let box_h = rows * line + pad * 2;
    let corner = HUD_CORNER.load(Ordering::Relaxed);
    let inset = (24.0 * s) as i32;
    let x0 = if corner == 0 || corner == 3 { c.w - width - inset } else { inset };
    let y0 = if corner <= 1 { inset } else { c.h - box_h - inset };
    c.fill_rect(x0, y0, width, box_h, (0x18, 0x18, 0x18), 230);
    let mut y = y0 + pad;
    for (title, lines, colour) in blocks {
        if lines.is_empty() {
            continue;
        }
        c.text(x0 + pad, y, 15.0, title, (0x90, 0x90, 0x90));
        y += line;
        for l in lines {
            c.text(x0 + pad, y, 15.0, l, colour);
            y += line;
        }
    }
}

/// Why the thread stopped, and so whether starting it again could end differently.
///
/// A compositor either implements `zwlr_layer_shell_v1` or does not, and it will
/// not grow the protocol while the program is open; there is no second way to put a
/// surface over everything, so that answer is the final one. A connection that
/// could not be opened describes only this second - a compositor restarting, a
/// session not up yet - and deserves another look later.
enum Stop {
    Permanent(&'static str),
    Transient(String),
}

fn run() {
    let result = (|| -> Result<(), Stop> {
        let conn = Connection::connect_to_env().map_err(|e| Stop::Transient(e.to_string()))?;
        let (globals, mut queue) =
            registry_queue_init::<St>(&conn).map_err(|e| Stop::Transient(e.to_string()))?;
        let qh = queue.handle();
        let base = Base::bind(&globals, &qh);
        let layer_shell = globals
            .bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, St, ()>(&qh, 1..=4, ())
            .ok();
        let viewporter = globals.bind::<wp_viewporter::WpViewporter, St, ()>(&qh, 1..=1, ()).ok();
        let mut st = St { base, layer_shell, viewporter, configures: Vec::new() };
        let _ = queue.roundtrip(&mut st);
        let _ = queue.roundtrip(&mut st);
        let Some(shell) = st.layer_shell.clone() else {
            return Err(Stop::Permanent("the compositor has no zwlr_layer_shell_v1"));
        };
        let Some(compositor) = st.base.compositor.clone() else {
            return Err(Stop::Permanent("no wl_compositor"));
        };
        let font = font_bytes().and_then(|b| ab_glyph::FontVec::try_from_vec(b).ok());
        if font.is_none() {
            tracing::warn!("overlay: no monospace font found; labels will be blank");
        }

        let mut panes: Vec<Pane> = Vec::new();
        for (idx, out) in st.base.outputs.iter().enumerate() {
            let surface = compositor.create_surface(&qh, ());
            let layer = shell.get_layer_surface(
                &surface,
                Some(&out.wl),
                zwlr_layer_shell_v1::Layer::Overlay,
                "clickwork-overlay".into(),
                &qh,
                idx,
            );
            layer.set_anchor(
                zwlr_layer_surface_v1::Anchor::Top
                    | zwlr_layer_surface_v1::Anchor::Bottom
                    | zwlr_layer_surface_v1::Anchor::Left
                    | zwlr_layer_surface_v1::Anchor::Right,
            );
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
            layer.set_size(0, 0);
            // An empty input region: every click falls through to what is below.
            let region = compositor.create_region(&qh, ());
            surface.set_input_region(Some(&region));
            region.destroy();
            let viewport = st.viewporter.as_ref().map(|v| v.get_viewport(&surface, &qh, ()));
            surface.commit();
            panes.push(Pane {
                idx,
                surface,
                layer,
                viewport,
                bufs: [None, None],
                which: 0,
                lw: 0,
                lh: 0,
                configured: false,
                closed: false,
            });
        }
        let _ = conn.flush();
        // Reaching this point is the proof that a connection can be had, so the
        // retry clock starts over: a compositor that comes back and then goes again
        // is worth a line in the log rather than silence.
        LAST_FAIL_US.store(0, Ordering::Relaxed);
        tracing::info!("overlay up on {} output(s)", panes.len());

        let mut seen = u64::MAX;
        while WANTED.load(Ordering::Relaxed) {
            // A round trip, not `dispatch_pending`: nothing else reads this
            // connection, and the configure that gives each surface its size
            // would otherwise sit in the socket forever.
            let _ = queue.roundtrip(&mut st);
            let _ = conn.flush();
            // Pick up configures.
            for p in panes.iter_mut() {
                if let Some((cfg, closed)) = st.configures.get(p.idx) {
                    if let Some((_, w, h)) = cfg
                        && (*w != p.lw || *h != p.lh || !p.configured)
                    {
                        p.lw = *w;
                        p.lh = *h;
                        p.configured = true;
                        seen = u64::MAX; // redraw at the new size
                    }
                    p.closed = *closed;
                }
            }
            let now = SIGHTING.lock().seq.wrapping_add(HUD.lock().seq);
            if now != seen {
                seen = now;
                let layout = super::geom::layout();
                for p in panes.iter_mut() {
                    if !p.configured || p.closed || p.lw == 0 || p.lh == 0 {
                        continue;
                    }
                    let name = st.base.outputs.get(p.idx).map(|o| o.name.clone()).unwrap_or_default();
                    let mon = layout
                        .mons
                        .iter()
                        .find(|m| m.name == name)
                        .cloned()
                        .or_else(|| (layout.mons.len() == 1).then(|| layout.mons[0].clone()));
                    let (scale, ox, oy) = match mon.as_ref() {
                        Some(m) => (m.scale as f32, m.px, m.py),
                        None => (1.0, 0, 0),
                    };
                    let (bw, bh) = if p.viewport.is_some() {
                        (
                            (p.lw as f32 * scale).round().max(1.0) as u32,
                            (p.lh as f32 * scale).round().max(1.0) as u32,
                        )
                    } else {
                        (p.lw, p.lh)
                    };
                    let Some(shm) = st.base.shm.clone() else { break };
                    let slot = p.which;
                    p.which ^= 1;
                    let fits = p.bufs[slot].as_ref().is_some_and(|b| b.w == bw && b.h == bh);
                    if !fits {
                        p.bufs[slot] = None;
                        p.bufs[slot] =
                            ShmBuf::new(&shm, &qh, bw, bh, bw * 4, wl_shm::Format::Argb8888);
                    }
                    let Some(buf) = p.bufs[slot].as_mut() else { continue };
                    let stride = buf.stride as usize;
                    {
                        let mut canvas = Canvas {
                            px: buf.bytes_mut(),
                            w: bw as i32,
                            h: bh as i32,
                            stride,
                            scale: if p.viewport.is_some() { scale } else { 1.0 },
                            font: font.as_ref(),
                        };
                        paint(&mut canvas, ox, oy);
                    }
                    if let Some(v) = p.viewport.as_ref() {
                        v.set_source(0.0, 0.0, bw as f64, bh as f64);
                        v.set_destination(p.lw as i32, p.lh as i32);
                    }
                    p.surface.attach(Some(&buf.buffer), 0, 0);
                    p.surface.damage_buffer(0, 0, bw as i32, bh as i32);
                    p.surface.commit();
                }
                let _ = conn.flush();
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        for p in panes.drain(..) {
            p.layer.destroy();
            p.surface.destroy();
        }
        let _ = conn.flush();
        let _ = queue.roundtrip(&mut st);
        Ok(())
    })();
    // Both arms are settled before `RUNNING` is cleared: a frame that reads the
    // flag the moment it drops must already be able to see why not to act on it.
    match result {
        Ok(()) => {}
        Err(Stop::Permanent(why)) => {
            // Once, by whoever gets here first. The caller asks again every frame,
            // and the same sentence sixty times a second is not a log.
            if UNAVAILABLE.set(why).is_ok() {
                tracing::warn!("overlay unavailable: {why}; it will not be tried again");
            }
        }
        Err(Stop::Transient(why)) => {
            // Zero is the mark for "never failed", so the stamp is pushed to at
            // least one microsecond on a machine that has only just started.
            let first = LAST_FAIL_US.swap(crate::now_us().max(1), Ordering::Relaxed) == 0;
            if first {
                tracing::warn!("overlay could not start: {why}; trying again in a few seconds");
            } else {
                tracing::debug!("overlay could not start: {why}");
            }
        }
    }
    RUNNING.store(false, Ordering::Relaxed);
}
