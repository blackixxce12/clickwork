//! Text recognition through Tesseract.
//!
//! The library is opened at run time with `dlopen` rather than linked: a machine
//! without it still starts, the picture search still runs, and only a `Read text`
//! step says "install tesseract". The handful of C entry points used are declared
//! by hand from `capi.h`; they have not changed in years and a binding generator
//! would cost a build-time dependency for nothing.
//!
//! One engine per language per thread. `TessBaseAPI` is not thread-safe, and
//! initialising one loads a twenty-megabyte model, so the playback thread keeps
//! its own and reuses it for every reading of the run.

use crate::ocr::TextBox;
use crate::vision::{Frame, Order};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

type Handle = *mut c_void;

#[allow(non_snake_case)]
struct Api {
    _lib: libloading::Library,
    Create: unsafe extern "C" fn() -> Handle,
    Delete: unsafe extern "C" fn(Handle),
    Init3: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    End: unsafe extern "C" fn(Handle),
    SetVariable: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    SetPageSegMode: unsafe extern "C" fn(Handle, c_int),
    SetImage: unsafe extern "C" fn(Handle, *const u8, c_int, c_int, c_int, c_int),
    SetSourceResolution: unsafe extern "C" fn(Handle, c_int),
    Recognize: unsafe extern "C" fn(Handle, *mut c_void) -> c_int,
    Clear: unsafe extern "C" fn(Handle),
    GetIterator: unsafe extern "C" fn(Handle) -> *mut c_void,
    IterDelete: unsafe extern "C" fn(*mut c_void),
    IterText: unsafe extern "C" fn(*mut c_void, c_int) -> *mut c_char,
    IterNext: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
    IterBox: unsafe extern "C" fn(
        *mut c_void,
        c_int,
        *mut c_int,
        *mut c_int,
        *mut c_int,
        *mut c_int,
    ) -> c_int,
    Confidence: unsafe extern "C" fn(*mut c_void, c_int) -> f32,
    DeleteText: unsafe extern "C" fn(*mut c_char),
    Languages: unsafe extern "C" fn(Handle) -> *mut *mut c_char,
    DeleteTextArray: unsafe extern "C" fn(*mut *mut c_char),
    Version: unsafe extern "C" fn() -> *const c_char,
}

// The function pointers are plain code addresses; the library stays loaded for the
// life of the process.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

static API: OnceLock<Option<Api>> = OnceLock::new();
static TESSDATA: OnceLock<Option<PathBuf>> = OnceLock::new();

const RIL_TEXTLINE: c_int = 2;

fn library_names() -> Vec<String> {
    let mut v = Vec::new();
    if let Ok(p) = std::env::var("CLICKWORK_TESSERACT_LIB")
        && !p.trim().is_empty()
    {
        v.push(p);
    }
    v.extend(["libtesseract.so.5", "libtesseract.so", "libtesseract.so.4"].map(String::from));
    v
}

fn load() -> Option<Api> {
    let mut last = String::new();
    for name in library_names() {
        let lib = match unsafe { libloading::Library::new(&name) } {
            Ok(l) => l,
            Err(e) => {
                last = format!("{name}: {e}");
                continue;
            }
        };
        macro_rules! sym {
            ($s:literal) => {
                match unsafe { lib.get::<*const ()>($s) } {
                    Ok(s) => unsafe { std::mem::transmute::<*const (), _>(*s) },
                    Err(e) => {
                        tracing::warn!("{name} lacks {}: {e}", String::from_utf8_lossy($s));
                        continue;
                    }
                }
            };
        }
        let api = Api {
            Create: sym!(b"TessBaseAPICreate\0"),
            Delete: sym!(b"TessBaseAPIDelete\0"),
            Init3: sym!(b"TessBaseAPIInit3\0"),
            End: sym!(b"TessBaseAPIEnd\0"),
            SetVariable: sym!(b"TessBaseAPISetVariable\0"),
            SetPageSegMode: sym!(b"TessBaseAPISetPageSegMode\0"),
            SetImage: sym!(b"TessBaseAPISetImage\0"),
            SetSourceResolution: sym!(b"TessBaseAPISetSourceResolution\0"),
            Recognize: sym!(b"TessBaseAPIRecognize\0"),
            Clear: sym!(b"TessBaseAPIClear\0"),
            GetIterator: sym!(b"TessBaseAPIGetIterator\0"),
            IterDelete: sym!(b"TessResultIteratorDelete\0"),
            IterText: sym!(b"TessResultIteratorGetUTF8Text\0"),
            IterNext: sym!(b"TessResultIteratorNext\0"),
            IterBox: sym!(b"TessPageIteratorBoundingBox\0"),
            Confidence: sym!(b"TessResultIteratorConfidence\0"),
            DeleteText: sym!(b"TessDeleteText\0"),
            Languages: sym!(b"TessBaseAPIGetAvailableLanguagesAsVector\0"),
            DeleteTextArray: sym!(b"TessDeleteTextArray\0"),
            Version: sym!(b"TessVersion\0"),
            _lib: lib,
        };
        let ver = unsafe { CStr::from_ptr((api.Version)()) }.to_string_lossy().into_owned();
        tracing::info!("tesseract {ver} loaded from {name}");
        return Some(api);
    }
    tracing::warn!("tesseract is not installed ({last}); text recognition is unavailable");
    None
}

fn api() -> Option<&'static Api> {
    API.get_or_init(load).as_ref()
}

/// Where the `.traineddata` files are. `TESSDATA_PREFIX` first, then the places
/// distributions put them.
fn tessdata() -> Option<&'static Path> {
    TESSDATA
        .get_or_init(|| {
            let mut candidates: Vec<PathBuf> = Vec::new();
            for var in ["CLICKWORK_TESSDATA", "TESSDATA_PREFIX"] {
                if let Some(p) = std::env::var_os(var).map(PathBuf::from) {
                    candidates.push(p.join("tessdata"));
                    candidates.push(p);
                }
            }
            candidates.extend(
                [
                    "/usr/share/tessdata",
                    "/usr/share/tesseract-ocr/5/tessdata",
                    "/usr/share/tesseract-ocr/4.00/tessdata",
                    "/usr/share/tesseract/tessdata",
                    "/usr/local/share/tessdata",
                    "/opt/homebrew/share/tessdata",
                ]
                .map(PathBuf::from),
            );
            candidates.into_iter().find(|p| {
                std::fs::read_dir(p).is_ok_and(|mut d| {
                    d.any(|e| {
                        e.is_ok_and(|e| e.path().extension().is_some_and(|x| x == "traineddata"))
                    })
                })
            })
        })
        .as_deref()
}

/// Is recognition possible on this machine?
pub fn available() -> bool {
    api().is_some() && tessdata().is_some()
}

/// A short reason it is not, for the checker and the panel.
pub fn unavailable_reason() -> Option<&'static str> {
    if api().is_none() {
        Some("libtesseract is not installed")
    } else if tessdata().is_none() {
        Some("no tesseract language data (tessdata) found")
    } else {
        None
    }
}

struct Engine {
    h: Handle,
}

impl Engine {
    fn new(lang: &str) -> Option<Self> {
        let api = api()?;
        let data = tessdata()?;
        let h = unsafe { (api.Create)() };
        if h.is_null() {
            return None;
        }
        let cdata = CString::new(data.to_string_lossy().into_owned()).ok()?;
        let clang = CString::new(lang).ok()?;
        let rc = unsafe { (api.Init3)(h, cdata.as_ptr(), clang.as_ptr()) };
        if rc != 0 {
            unsafe { (api.Delete)(h) };
            tracing::warn!("tesseract could not initialise '{lang}' from {}", data.display());
            return None;
        }
        // Quiet: the engine otherwise prints its warnings to stderr.
        let k = CString::new("debug_file").ok()?;
        let v = CString::new("/dev/null").ok()?;
        unsafe { (api.SetVariable)(h, k.as_ptr(), v.as_ptr()) };
        Some(Self { h })
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(api) = api() {
            unsafe {
                (api.End)(self.h);
                (api.Delete)(self.h);
            }
        }
    }
}

thread_local! {
    static ENGINES: RefCell<HashMap<String, Engine>> = RefCell::new(HashMap::new());
}

/// Tesseract's three-letter codes and the names shown for them.
const NAMES: &[(&str, &str)] = &[
    ("eng", "English"),
    ("rus", "Русский"),
    ("ukr", "Українська"),
    ("por", "Português"),
    ("spa", "Español"),
    ("chi_sim", "中文（简体）"),
    ("chi_tra", "中文（繁體）"),
    ("deu", "Deutsch"),
    ("fra", "Français"),
    ("ita", "Italiano"),
    ("pol", "Polski"),
    ("jpn", "日本語"),
    ("kor", "한국어"),
    ("tur", "Türkçe"),
    ("nld", "Nederlands"),
    ("swe", "Svenska"),
    ("ces", "Čeština"),
    ("bel", "Беларуская"),
    ("kaz", "Қазақ"),
    ("ara", "العربية"),
    ("hin", "हिन्दी"),
    ("vie", "Tiếng Việt"),
    ("ind", "Bahasa Indonesia"),
    ("ell", "Ελληνικά"),
    ("heb", "עברית"),
    ("hun", "Magyar"),
    ("ron", "Română"),
    ("bul", "Български"),
    ("srp", "Српски"),
    ("fin", "Suomi"),
    ("dan", "Dansk"),
    ("nor", "Norsk"),
    ("tha", "ไทย"),
];

/// A BCP-47 tag or a two-letter code as Tesseract's code. Anything already in
/// Tesseract's form passes through.
pub fn tess_code(tag: &str) -> String {
    let t = tag.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.contains('+') {
        return t.split('+').map(tess_code).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("+");
    }
    let lower = t.to_lowercase();
    if NAMES.iter().any(|(c, _)| *c == lower) || lower.len() == 3 && !lower.contains('-') {
        return lower;
    }
    let two = lower.split(['-', '_']).next().unwrap_or("");
    let region = lower.split(['-', '_']).nth(1).unwrap_or("");
    match two {
        "en" => "eng",
        "ru" => "rus",
        "uk" => "ukr",
        "pt" => "por",
        "es" => "spa",
        "zh" => {
            if matches!(region, "tw" | "hk" | "hant") {
                "chi_tra"
            } else {
                "chi_sim"
            }
        }
        "de" => "deu",
        "fr" => "fra",
        "it" => "ita",
        "pl" => "pol",
        "ja" => "jpn",
        "ko" => "kor",
        "tr" => "tur",
        "nl" => "nld",
        "sv" => "swe",
        "cs" => "ces",
        "be" => "bel",
        "kk" => "kaz",
        "ar" => "ara",
        "hi" => "hin",
        "vi" => "vie",
        "id" => "ind",
        "el" => "ell",
        "he" => "heb",
        "hu" => "hun",
        "ro" => "ron",
        "bg" => "bul",
        "sr" => "srp",
        "fi" => "fin",
        "da" => "dan",
        "no" | "nb" => "nor",
        "th" => "tha",
        _ => return lower,
    }
    .to_string()
}

/// Every language pack installed, as (code, display name), sorted by name.
pub fn installed_languages() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (Some(api), Some(_)) = (api(), tessdata()) else {
        return out;
    };
    // An engine with no language just to list what it could load.
    let Some(e) = Engine::new("") else {
        return out;
    };
    unsafe {
        let arr = (api.Languages)(e.h);
        if !arr.is_null() {
            let mut i = 0;
            loop {
                let p = *arr.add(i);
                if p.is_null() {
                    break;
                }
                let code = CStr::from_ptr(p).to_string_lossy().into_owned();
                if code != "osd" && !code.is_empty() {
                    let name = NAMES
                        .iter()
                        .find(|(c, _)| *c == code)
                        .map(|(_, n)| n.to_string())
                        .unwrap_or_else(|| code.clone());
                    out.push((code, name));
                }
                i += 1;
            }
            (api.DeleteTextArray)(arr);
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// The languages read when nobody chose one: the desktop's own language plus
/// English, whichever of the two are installed.
pub fn default_languages() -> String {
    let installed = installed_languages();
    let have = |c: &str| installed.iter().any(|(x, _)| x == c);
    let mut parts: Vec<String> = Vec::new();
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    let code = tess_code(locale.split('.').next().unwrap_or(""));
    if !code.is_empty() && have(&code) {
        parts.push(code);
    }
    if have("eng") && !parts.iter().any(|p| p == "eng") {
        parts.push("eng".into());
    }
    if parts.is_empty()
        && let Some((c, _)) = installed.first()
    {
        parts.push(c.clone());
    }
    parts.join("+")
}

/// Page segmentation for a screen crop. A strip is one line; a panel is a block;
/// anything bigger may have text anywhere in it.
fn seg_mode(w: u32, h: u32) -> c_int {
    if let Ok(v) = std::env::var("CLICKWORK_TESS_PSM")
        && let Ok(n) = v.trim().parse::<c_int>()
    {
        return n;
    }
    if h <= 48 && w >= h {
        7 // PSM_SINGLE_LINE
    } else if h <= 240 {
        6 // PSM_SINGLE_BLOCK
    } else {
        11 // PSM_SPARSE_TEXT
    }
}

/// Enlarges a frame into RGB, nearest neighbour. See the Windows `upscale_to_bgra`
/// for why nearest: glyphs are hard-edged and smoothing them hurts.
fn upscale_rgb(f: &Frame, k: u32) -> (Vec<u8>, u32, u32) {
    let (nw, nh) = (f.w * k, f.h * k);
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 3];
    let swap = f.order == Order::Bgra;
    for y in 0..nh {
        let sy = y / k;
        for x in 0..nw {
            let sx = x / k;
            let s = ((sy * f.w + sx) * 4) as usize;
            let d = ((y * nw + x) * 3) as usize;
            if s + 3 > f.px.len() {
                continue;
            }
            if swap {
                out[d] = f.px[s + 2];
                out[d + 1] = f.px[s + 1];
                out[d + 2] = f.px[s];
            } else {
                out[d] = f.px[s];
                out[d + 1] = f.px[s + 1];
                out[d + 2] = f.px[s + 2];
            }
        }
    }
    (out, nw, nh)
}

/// Recognises a frame in `lang` (Tesseract codes, `+`-joined), enlarged at least
/// `min_scale` times.
pub fn recognize(frame: &Frame, lang: &str, min_scale: u32) -> anyhow::Result<Vec<TextBox>> {
    let api = api().ok_or_else(|| anyhow::anyhow!("libtesseract is not installed"))?;
    if tessdata().is_none() {
        anyhow::bail!("no tesseract language data found (install tesseract-data-eng)");
    }
    if frame.w == 0 || frame.h == 0 {
        return Ok(Vec::new());
    }
    let lang = if lang.trim().is_empty() { default_languages() } else { tess_code(lang) };
    if lang.is_empty() {
        anyhow::bail!("no tesseract language packs are installed");
    }
    // Small interface text reads far better enlarged: aim for the short side to be
    // around 64 pixels, never past 4000 on the long side.
    let short = frame.w.min(frame.h).max(1);
    let long = frame.w.max(frame.h).max(1);
    let want = 64_u32.div_ceil(short);
    let cap = (4000 / long).max(1);
    let scale = want.max(min_scale).clamp(1, 8).min(cap);
    let (rgb, w, h) = upscale_rgb(frame, scale);

    ENGINES.with(|cell| {
        let mut map = cell.borrow_mut();
        if !map.contains_key(&lang) {
            let e = Engine::new(&lang)
                .ok_or_else(|| anyhow::anyhow!("tesseract could not load '{lang}'"))?;
            map.insert(lang.clone(), e);
        }
        let e = map.get(&lang).expect("just inserted");
        let mut out = Vec::new();
        unsafe {
            (api.SetPageSegMode)(e.h, seg_mode(frame.w, frame.h));
            (api.SetImage)(e.h, rgb.as_ptr(), w as c_int, h as c_int, 3, (w * 3) as c_int);
            (api.SetSourceResolution)(e.h, (96 * scale).clamp(70, 600) as c_int);
            if (api.Recognize)(e.h, std::ptr::null_mut()) != 0 {
                (api.Clear)(e.h);
                anyhow::bail!("tesseract failed to recognise the region");
            }
            let it = (api.GetIterator)(e.h);
            if !it.is_null() {
                loop {
                    let p = (api.IterText)(it, RIL_TEXTLINE);
                    if !p.is_null() {
                        let text = CStr::from_ptr(p).to_string_lossy().trim().to_string();
                        (api.DeleteText)(p);
                        let (mut l, mut t, mut r, mut b) = (0, 0, 0, 0);
                        let have_box =
                            (api.IterBox)(it, RIL_TEXTLINE, &mut l, &mut t, &mut r, &mut b) != 0;
                        if !text.is_empty() {
                            let k = scale as i32;
                            let (x, y, bw, bh) = if have_box {
                                (l / k, t / k, (r - l) / k, (b - t) / k)
                            } else {
                                (0, 0, 0, 0)
                            };
                            // Tesseract answers 0 to 100 for the line the iterator
                            // is standing on, and a negative number once it has run
                            // past the last one. Only a number inside that range is
                            // an answer; the rest is the engine declining to say.
                            let raw = (api.Confidence)(it, RIL_TEXTLINE);
                            let confidence = (0.0..=100.0).contains(&raw).then_some(raw / 100.0);
                            out.push(TextBox {
                                text,
                                x: frame.x + x,
                                y: frame.y + y,
                                w: bw.max(0),
                                h: bh.max(0),
                                confidence,
                            });
                        }
                    }
                    if (api.IterNext)(it, RIL_TEXTLINE) == 0 {
                        break;
                    }
                }
                (api.IterDelete)(it);
            }
            (api.Clear)(e.h);
        }
        out.sort_by_key(|b| (b.y, b.x));
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_become_codes() {
        assert_eq!(tess_code("en-US"), "eng");
        assert_eq!(tess_code("ru-RU"), "rus");
        assert_eq!(tess_code("ru"), "rus");
        assert_eq!(tess_code("zh-TW"), "chi_tra");
        assert_eq!(tess_code("zh-CN"), "chi_sim");
        assert_eq!(tess_code("rus"), "rus");
        assert_eq!(tess_code("rus+eng"), "rus+eng");
        assert_eq!(tess_code("en-US+ru-RU"), "eng+rus");
        assert_eq!(tess_code(""), "");
    }

    #[test]
    fn segmentation_follows_the_shape() {
        assert_eq!(seg_mode(200, 30), 7);
        assert_eq!(seg_mode(300, 120), 6);
        assert_eq!(seg_mode(1000, 800), 11);
    }
}
