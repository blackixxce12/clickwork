//! Two coordinate systems, and the rule for going between them.
//!
//! The compositor lays windows out, moves the cursor and takes clicks in
//! *logical* pixels: a 2560-wide screen at scale 1.6 is 1600 logical pixels across.
//! The picture search, the text reader and the pixel condition look at *physical*
//! pixels, because a button is a button at the resolution the panel has and a
//! template snipped from a screenshot is physical too.
//!
//! The Windows build never had this problem: it declares itself per-monitor DPI
//! aware and Windows hands it physical pixels for everything. To keep the rest of
//! the program - forty thousand lines that think in one unit - unchanged, the
//! platform layer speaks physical pixels throughout and converts at the edges:
//! here, once, in both directions.
//!
//! Physical origins are placed so that monitors never overlap whatever their
//! scales: a monitor's physical origin is its logical origin multiplied by the
//! *largest* scale in the layout, and inside a monitor a logical step is `scale`
//! physical pixels. On the single monitor nearly everybody has, that is exactly
//! "multiply by the scale" and nothing else.

use super::hypr;
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct Mon {
    pub id: i64,
    pub name: String,
    /// Logical rectangle.
    pub lx: i32,
    pub ly: i32,
    pub lw: i32,
    pub lh: i32,
    pub scale: f64,
    /// Physical rectangle, in the shared physical space described above.
    pub px: i32,
    pub py: i32,
    pub pw: i32,
    pub ph: i32,
    pub focused: bool,
    pub active_workspace: i64,
    pub special_workspace: i64,
}

impl Mon {
    pub fn contains_logical(&self, x: f64, y: f64) -> bool {
        x >= self.lx as f64
            && y >= self.ly as f64
            && x < (self.lx + self.lw) as f64
            && y < (self.ly + self.lh) as f64
    }
    pub fn contains_phys(&self, x: i32, y: i32) -> bool {
        x >= self.px && y >= self.py && x < self.px + self.pw && y < self.py + self.ph
    }
    pub fn to_phys(&self, lx: f64, ly: f64) -> (i32, i32) {
        (
            self.px + ((lx - self.lx as f64) * self.scale).round() as i32,
            self.py + ((ly - self.ly as f64) * self.scale).round() as i32,
        )
    }
    pub fn to_logical(&self, px: i32, py: i32) -> (f64, f64) {
        (
            self.lx as f64 + (px - self.px) as f64 / self.scale,
            self.ly as f64 + (py - self.py) as f64 / self.scale,
        )
    }
    fn dist2_logical(&self, x: f64, y: f64) -> f64 {
        let cx = x.clamp(self.lx as f64, (self.lx + self.lw - 1) as f64);
        let cy = y.clamp(self.ly as f64, (self.ly + self.lh - 1) as f64);
        (x - cx).powi(2) + (y - cy).powi(2)
    }
    fn dist2_phys(&self, x: i32, y: i32) -> i64 {
        let cx = x.clamp(self.px, self.px + self.pw - 1);
        let cy = y.clamp(self.py, self.py + self.ph - 1);
        ((x - cx) as i64).pow(2) + ((y - cy) as i64).pow(2)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub mons: Vec<Mon>,
    pub max_scale: f64,
}

impl Layout {
    pub fn from_hypr(list: &[hypr::Monitor]) -> Self {
        let mons: Vec<&hypr::Monitor> = list.iter().filter(|m| !m.disabled).collect();
        let max_scale = mons
            .iter()
            .map(|m| if m.scale > 0.0 { m.scale } else { 1.0 })
            .fold(1.0_f64, f64::max);
        let mons = mons
            .into_iter()
            .map(|m| {
                let scale = if m.scale > 0.0 { m.scale } else { 1.0 };
                let (lw, lh) = m.logical_size();
                Mon {
                    id: m.id,
                    name: m.name.clone(),
                    lx: m.x,
                    ly: m.y,
                    lw,
                    lh,
                    scale,
                    px: (m.x as f64 * max_scale).round() as i32,
                    py: (m.y as f64 * max_scale).round() as i32,
                    pw: (lw as f64 * scale).round().max(1.0) as i32,
                    ph: (lh as f64 * scale).round().max(1.0) as i32,
                    focused: m.focused,
                    active_workspace: m.active_workspace.id,
                    special_workspace: m.special_workspace.id,
                }
            })
            .collect();
        Self { mons, max_scale }
    }

    /// A layout built from what Wayland itself says about its outputs, for a
    /// compositor that is not Hyprland.
    pub fn from_outputs(list: &[super::wl::OutputInfo]) -> Self {
        let max_scale = list.iter().map(|o| o.scale()).fold(1.0_f64, f64::max);
        let mons = list
            .iter()
            .enumerate()
            .map(|(i, o)| Mon {
                id: i as i64,
                name: o.name.clone(),
                lx: o.lx,
                ly: o.ly,
                lw: o.lw.max(1),
                lh: o.lh.max(1),
                scale: o.scale(),
                px: (o.lx as f64 * max_scale).round() as i32,
                py: (o.ly as f64 * max_scale).round() as i32,
                pw: (o.lw.max(1) as f64 * o.scale()).round().max(1.0) as i32,
                ph: (o.lh.max(1) as f64 * o.scale()).round().max(1.0) as i32,
                focused: i == 0,
                active_workspace: 0,
                special_workspace: 0,
            })
            .collect();
        Self { mons, max_scale }
    }

    pub fn is_empty(&self) -> bool {
        self.mons.is_empty()
    }

    /// Union of every monitor, physical.
    pub fn virtual_phys(&self) -> (i32, i32, i32, i32) {
        if self.mons.is_empty() {
            return (0, 0, 1, 1);
        }
        let x0 = self.mons.iter().map(|m| m.px).min().unwrap_or(0);
        let y0 = self.mons.iter().map(|m| m.py).min().unwrap_or(0);
        let x1 = self.mons.iter().map(|m| m.px + m.pw).max().unwrap_or(1);
        let y1 = self.mons.iter().map(|m| m.py + m.ph).max().unwrap_or(1);
        (x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
    }

    /// Union of every monitor, logical.
    pub fn virtual_logical(&self) -> (i32, i32, i32, i32) {
        if self.mons.is_empty() {
            return (0, 0, 1, 1);
        }
        let x0 = self.mons.iter().map(|m| m.lx).min().unwrap_or(0);
        let y0 = self.mons.iter().map(|m| m.ly).min().unwrap_or(0);
        let x1 = self.mons.iter().map(|m| m.lx + m.lw).max().unwrap_or(1);
        let y1 = self.mons.iter().map(|m| m.ly + m.lh).max().unwrap_or(1);
        (x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
    }

    pub fn mon_at_logical(&self, x: f64, y: f64) -> Option<&Mon> {
        self.mons.iter().find(|m| m.contains_logical(x, y)).or_else(|| {
            self.mons
                .iter()
                .min_by(|a, b| a.dist2_logical(x, y).total_cmp(&b.dist2_logical(x, y)))
        })
    }

    pub fn mon_at_phys(&self, x: i32, y: i32) -> Option<&Mon> {
        self.mons
            .iter()
            .find(|m| m.contains_phys(x, y))
            .or_else(|| self.mons.iter().min_by_key(|m| m.dist2_phys(x, y)))
    }

    pub fn focused(&self) -> Option<&Mon> {
        self.mons.iter().find(|m| m.focused).or_else(|| self.mons.first())
    }

    pub fn to_phys(&self, lx: f64, ly: f64) -> (i32, i32) {
        match self.mon_at_logical(lx, ly) {
            Some(m) => m.to_phys(lx, ly),
            None => (lx.round() as i32, ly.round() as i32),
        }
    }

    pub fn to_logical(&self, px: i32, py: i32) -> (f64, f64) {
        match self.mon_at_phys(px, py) {
            Some(m) => m.to_logical(px, py),
            None => (px as f64, py as f64),
        }
    }

    /// A logical rectangle as a physical one, on the monitor holding its corner.
    pub fn rect_to_phys(&self, r: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
        let (x, y, w, h) = r;
        let cx = x as f64 + w as f64 / 2.0;
        let cy = y as f64 + h as f64 / 2.0;
        let Some(m) = self.mon_at_logical(cx, cy) else {
            return r;
        };
        let (px, py) = m.to_phys(x as f64, y as f64);
        (px, py, (w as f64 * m.scale).round() as i32, (h as f64 * m.scale).round() as i32)
    }

    /// Scale at a physical point; 1 when nothing is known.
    pub fn scale_at_phys(&self, px: i32, py: i32) -> f64 {
        self.mon_at_phys(px, py).map(|m| m.scale).unwrap_or(1.0)
    }
}

struct Cache {
    at_us: u64,
    layout: Arc<Layout>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Time-to-live of the cached layout, in microseconds. Monitors are not plugged in
/// forty times a second, and every capture, every cursor read and every synthetic
/// move needs the answer.
const TTL_US: u64 = 500_000;

/// The current layout, refreshed at most twice a second.
pub fn layout() -> Arc<Layout> {
    let now = crate::now_us();
    {
        let c = CACHE.lock();
        if let Some(c) = c.as_ref()
            && now.saturating_sub(c.at_us) < TTL_US
        {
            return c.layout.clone();
        }
    }
    let fresh = Arc::new(read());
    *CACHE.lock() = Some(Cache { at_us: now, layout: fresh.clone() });
    fresh
}

/// Forgets the cached layout, so the next call asks again.
pub fn invalidate() {
    *CACHE.lock() = None;
}

fn read() -> Layout {
    if hypr::available() {
        let l = Layout::from_hypr(&hypr::monitors());
        if !l.is_empty() {
            return l;
        }
    }
    let l = Layout::from_outputs(&super::wl::outputs());
    if l.is_empty() {
        // Nothing answered. One imaginary screen so arithmetic stays finite.
        Layout {
            mons: vec![Mon {
                id: 0,
                name: "unknown".into(),
                lx: 0,
                ly: 0,
                lw: 1920,
                lh: 1080,
                scale: 1.0,
                px: 0,
                py: 0,
                pw: 1920,
                ph: 1080,
                focused: true,
                active_workspace: 0,
                special_workspace: 0,
            }],
            max_scale: 1.0,
        }
    } else {
        l
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hm(id: i64, x: i32, y: i32, w: i32, h: i32, scale: f64) -> hypr::Monitor {
        hypr::Monitor {
            id,
            name: format!("M{id}"),
            width: w,
            height: h,
            x,
            y,
            scale,
            ..Default::default()
        }
    }

    #[test]
    fn single_monitor_is_plain_multiplication() {
        let l = Layout::from_hypr(&[hm(0, 0, 0, 2560, 1440, 1.6)]);
        assert_eq!(l.virtual_phys(), (0, 0, 2560, 1440));
        assert_eq!(l.virtual_logical(), (0, 0, 1600, 900));
        assert_eq!(l.to_phys(100.0, 50.0), (160, 80));
        let (lx, ly) = l.to_logical(160, 80);
        assert!((lx - 100.0).abs() < 1e-9 && (ly - 50.0).abs() < 1e-9);
    }

    #[test]
    fn mixed_scales_do_not_overlap() {
        let l = Layout::from_hypr(&[
            hm(0, 0, 0, 2560, 1440, 1.6),
            hm(1, 1600, 0, 1920, 1080, 1.0),
        ]);
        let a = &l.mons[0];
        let b = &l.mons[1];
        assert!(a.px + a.pw <= b.px, "{a:?} overlaps {b:?}");
        assert_eq!(l.to_phys(1600.0, 0.0), (b.px, 0));
        assert_eq!(l.virtual_phys().2, b.px + 1920);
    }
}
