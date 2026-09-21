//! Reading the screen back on KWin, through `org.kde.KWin.ScreenShot2`.
//!
//! KWin implements no screencopy of any kind - not the wlroots one, not the
//! `ext_` successor - so this is not a second road to the same place, it is the
//! only road a KDE session has. `super::capture` tries it first and falls back to
//! screencopy, which on KWin is never there and everywhere else is.
//!
//! It is also, unexpectedly, the faster of the two: a 200x200 rectangle measured
//! **1.5 ms** against screencopy's 6 ms on Hyprland, and the cost is nearly flat
//! from one pixel to a whole screen, because most of it is a fixed D-Bus round
//! trip rather than pixels. About 0.8 ms of that is KWin walking every installed
//! desktop file to decide whether we are allowed - on its main thread, so a tight
//! search loop taxes the whole desktop rather than only this process. That check
//! is gone in KWin 6.8.
//!
//! **The call is refused unless the program is installed**, the same way the
//! window protocol is, and through a second key in the same file:
//! `X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2`. A refusal arrives
//! as a D-Bus error, so swallowing it into `None` would leave a picture search
//! that silently sees nothing on a session where the interface is present and
//! answers its version query perfectly. It is named in the log instead, once.
//!
//! And it is told apart by its name, because it is not the only error the call
//! can come back with, and none of the others is about permission. An area KWin
//! cannot serve or a descriptor it cannot use is a complaint about the request;
//! `Cancelled` is KWin failing to render the shot at all, which is what a
//! session composited without OpenGL answers to a request that is perfectly
//! good; and a bus that has lost KWin answers in its place. Filed under
//! "refused", any of them would pass for the ordinary state of a build
//! directory, and `--selftest session`, which accepts a refusal that is
//! admitted, would accept a broken capture with it.

use crate::vision::{Frame, Order};
use std::collections::HashMap;
use std::io::Read as _;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedFd, OwnedValue, Value};

const SERVICE: &str = "org.kde.KWin.ScreenShot2";
const PATH: &str = "/org/kde/KWin/ScreenShot2";

/// `QImage::Format`, as the reply reports it. KWin up to 6.7 sends
/// `ARGB32_Premultiplied`, which is blue-first in memory; 6.8 changed to
/// `RGBX8888`, which is red-first. Today's compositor happens to match what this
/// program already tags its frames with, so hardcoding it would pass every test
/// on every machine that exists and start returning colour-swapped pictures the
/// moment users upgrade - and it would not error, it would quietly sag every
/// match score, because luma with the channels swapped is merely a different
/// number.
const FORMAT_ARGB32_PREMULTIPLIED: u32 = 6;
const FORMAT_RGBX8888: u32 = 16;

/// The refusal, and the one error name that means it. KWin replies with it when
/// the calling binary's path matches no installed desktop file that grants the
/// interface.
const NOT_AUTHORIZED: &str = "org.kde.KWin.ScreenShot2.Error.NoAuthorized";

fn order_of(format: u32) -> Option<Order> {
    match format {
        FORMAT_ARGB32_PREMULTIPLIED => Some(Order::Bgra),
        FORMAT_RGBX8888 => Some(Order::Rgba),
        _ => None,
    }
}

struct Shot {
    conn: Connection,
    /// The interface version. `hide-caller-windows` arrived at 5 and defaults to
    /// *true*, which would drop this program's own window and its own overlay
    /// out of every frame - and anchoring a step to our own interface is a thing
    /// people do. Below 5 the key is not sent at all rather than sent and
    /// ignored, so the intent stays legible.
    version: u32,
}

static SHOT: OnceLock<Option<Shot>> = OnceLock::new();
static TOLD_REFUSED: AtomicBool = AtomicBool::new(false);
/// The first error that was not a refusal, kept whole for `--doctor` and
/// `--selftest session`.
static FAULT: OnceLock<String> = OnceLock::new();

fn shot() -> Option<&'static Shot> {
    SHOT.get_or_init(|| {
        let conn = Connection::session().ok()?;
        let proxy = zbus::blocking::proxy::Builder::<Proxy>::new(&conn)
            .destination(SERVICE)
            .ok()?
            .path(PATH)
            .ok()?
            .interface(SERVICE)
            .ok()?
            // Not the default lazy caching: reading a number that never changes
            // would otherwise register a bus-wide signal subscription for it.
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .ok()?;
        let version: u32 = proxy.get_property("Version").ok()?;
        tracing::info!("org.kde.KWin.ScreenShot2 version {version}");
        Some(Shot { conn, version })
    })
    .as_ref()
}

/// Is KWin's screenshot interface on this session bus?
pub fn available() -> bool {
    shot().is_some()
}

/// Has a capture been refused for want of permission?
///
/// `--doctor` asks, because a refusal and a compositor that simply cannot
/// capture look identical from the outside - an empty frame either way - and the
/// difference is one line in a desktop file. The log line this also writes is
/// not enough on its own: it goes to the log file, and whoever runs `--doctor`
/// is reading the terminal.
pub fn refused() -> bool {
    TOLD_REFUSED.load(Ordering::Relaxed)
}

/// The first error a capture came back with that was *not* a refusal.
///
/// Nothing an installed desktop file would change: a request KWin rejected, a
/// shot it could not render, or a call that never got KWin's answer at all.
/// `--doctor` prints it whole, for the same reason it names the refusal.
pub fn fault() -> Option<&'static str> {
    FAULT.get().map(String::as_str)
}

/// Is this KWin's refusal, as opposed to any other error the call came back
/// with - KWin's own, the bus's, or zbus's?
fn is_refusal(e: &zbus::Error) -> bool {
    matches!(e, zbus::Error::MethodError(name, _, _) if name.as_str() == NOT_AUTHORIZED)
}

/// Grabs a physical rectangle, the same contract `super::capture::capture` has.
///
/// Coordinates go out in logical pixels and come back in physical ones, which is
/// what `native-resolution` means here. KWin scales such a capture by the largest
/// scale across every output rather than by the one the rectangle sits on - and
/// that happens to be exactly the convention `super::geom` already uses to place
/// monitors in the physical plane, so on a single-scale desktop the two agree to
/// the pixel. Where they do not, the returned rectangle is the wrong size and
/// this refuses rather than handing back a picture that is silently off by the
/// ratio between two monitors.
pub fn capture(x: i32, y: i32, w: i32, h: i32) -> Option<Frame> {
    let s = shot()?;
    let layout = super::geom::layout();

    // A rectangle over no output at all comes back **successful**, as a
    // full-size buffer of transparent black. The screencopy path returns nothing
    // in that case, having found no monitor to ask. Without this the counters
    // would record a hit for a frame that is blind by definition.
    if !layout.mons.iter().any(|m| {
        x.max(m.px) < (x + w).min(m.px + m.pw) && y.max(m.py) < (y + h).min(m.py + m.ph)
    }) {
        return None;
    }

    let scale = layout.max_scale.max(0.01);
    let lx = (x as f64 / scale).floor() as i32;
    let ly = (y as f64 / scale).floor() as i32;
    let lw = ((w as f64 / scale).ceil() as i32).max(1) as u32;
    let lh = ((h as f64 / scale).ceil() as i32).max(1) as u32;

    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert("native-resolution", Value::Bool(true));
    if s.version >= 5 {
        options.insert("hide-caller-windows", Value::Bool(false));
    }

    // Our end of the pipe. zvariant duplicates the descriptor when it serialises
    // the call, so the writer here has to be dropped afterwards or the read
    // below never reaches the end of the data.
    let (mut reader, writer) = std::io::pipe().ok()?;
    let reply: HashMap<String, OwnedValue> = {
        let dup: std::os::fd::OwnedFd = writer.try_clone().ok()?.into();
        let fd = OwnedFd::from(dup);
        let proxy = zbus::blocking::proxy::Builder::<Proxy>::new(&s.conn)
            .destination(SERVICE)
            .ok()?
            .path(PATH)
            .ok()?
            .interface(SERVICE)
            .ok()?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .ok()?;
        match proxy.call("CaptureArea", &(lx, ly, lw, lh, options, fd)) {
            Ok(r) => r,
            Err(e) => {
                // The refusal deserves a name. It means the running binary's
                // path matches no installed desktop file carrying
                // `X-KDE-DBUS-Restricted-Interfaces`, which is the ordinary
                // state of a build directory and says nothing about the code.
                if is_refusal(&e) {
                    if !TOLD_REFUSED.swap(true, Ordering::Relaxed) {
                        tracing::warn!(
                            "org.kde.KWin.ScreenShot2 refused the capture ({e}); on KWin this \
                             needs an installed desktop file carrying \
                             X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2, so a \
                             binary run from a build directory has no picture search here"
                        );
                    }
                } else if FAULT.set(e.to_string()).is_ok() {
                    // Anything else is not the refusal, whoever raised it -
                    // KWin over the request, KWin unable to render, the bus
                    // under both - and is kept apart from it so that it cannot
                    // pass for one.
                    tracing::warn!(
                        "org.kde.KWin.ScreenShot2 capture failed ({e}); this is not the \
                         permission refusal, so an installed desktop file would not help"
                    );
                }
                return None;
            }
        }
    };
    // Both copies gone, so the writer is closed and the read can finish.
    drop(writer);

    let num = |k: &str| reply.get(k).and_then(|v| u32::try_from(v).ok());
    let got_w = num("width")?;
    let got_h = num("height")?;
    let stride = num("stride")?;
    let order = order_of(num("format")?)?;

    if got_w != w as u32 || got_h != h as u32 {
        // The mixed-scale case, and the only one that reaches here. Refusing is
        // the honest answer: the alternative is a frame of the wrong size that
        // every caller would treat as the rectangle it asked for.
        tracing::warn!(
            "KWin returned {got_w}x{got_h} for a {w}x{h} request; this desktop mixes output \
             scales, which the screenshot interface resolves against the largest of them"
        );
        return None;
    }
    if stride < got_w.saturating_mul(4) {
        return None;
    }

    // Exactly the bytes the reply promised, never to the end: KWin up to and
    // including 6.7.5 can report the final block written before it reaches the
    // pipe and discard the tail, after the metadata has already said success.
    // Read this way that is a short read and a clean refusal; read to the end it
    // would be a frame with a garbage tail and no error anywhere.
    let mut raw = vec![0u8; (stride as usize) * (got_h as usize)];
    reader.read_exact(&mut raw).ok()?;

    // The rows arrive `stride` apart, which has been exactly four bytes a pixel
    // in every reply seen - but it is in the dictionary for a reason.
    let row = (got_w as usize) * 4;
    let px = if stride as usize == row {
        raw
    } else {
        let mut out = Vec::with_capacity(row * got_h as usize);
        for r in 0..got_h as usize {
            let off = r * stride as usize;
            out.extend_from_slice(&raw[off..off + row]);
        }
        out
    };

    Some(Frame { x, y, w: got_w, h: got_h, px, order })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reply as KWin sends it: an error message answering a call.
    fn reply(name: &str) -> zbus::Error {
        let call = zbus::Message::method_call(PATH, "CaptureArea").unwrap().build(&()).unwrap();
        zbus::Error::from(
            zbus::Message::error(&call.header(), name).unwrap().build(&("detail",)).unwrap(),
        )
    }

    #[test]
    fn only_the_refusal_is_a_refusal() {
        assert!(is_refusal(&reply(NOT_AUTHORIZED)));
        // Every other name the interface answers with, as its strings in KWin
        // 6.7.5 spell them: complaints about a request, and a shot KWin could
        // not render. None of them is a permission withheld.
        for other in [
            "InvalidArea",
            "FileDescriptor",
            "InvalidScreen",
            "InvalidWindow",
            "NoActiveWindow",
            "Cancelled",
        ] {
            let name = format!("org.kde.KWin.ScreenShot2.Error.{other}");
            assert!(!is_refusal(&reply(&name)), "{name} is not a refusal");
        }
        // And one that never reached KWin at all.
        assert!(!is_refusal(&zbus::Error::Failure("no bus".into())));
    }
}
