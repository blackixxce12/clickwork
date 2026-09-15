//! The clipboard, without a window.
//!
//! `wl-clipboard-rs` speaks the data-control protocol, which is what lets a client
//! that is not focused read and write the selection - the situation this program
//! is always in while a macro runs.

use std::io::Read as _;
use wl_clipboard_rs::paste::{ClipboardType, MimeType, Seat, get_contents};

/// The clipboard as text, or empty.
pub fn text() -> String {
    match get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text) {
        Ok((pipe, _)) => {
            let mut out = Vec::new();
            let _ = pipe.take(4 << 20).read_to_end(&mut out);
            String::from_utf8_lossy(&out).into_owned()
        }
        Err(_) => String::new(),
    }
}

/// Replaces the clipboard with `text`.
pub fn set_text(text: &str) -> bool {
    use wl_clipboard_rs::copy::{MimeType, Options, Source};
    let opts = Options::new();
    opts.copy(Source::Bytes(text.as_bytes().into()), MimeType::Text).is_ok()
}

/// An image off the clipboard, as top-down RGBA. What a screenshot tool leaves
/// behind after a region snip.
pub fn image() -> Option<(u32, u32, Vec<u8>)> {
    let (pipe, _) = get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific("image/png"),
    )
    .ok()?;
    let mut bytes = Vec::new();
    pipe.take(64 << 20).read_to_end(&mut bytes).ok()?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let mut px = rgba.into_raw();
    for p in px.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    Some((w, h, px))
}
