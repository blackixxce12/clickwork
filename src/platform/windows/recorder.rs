//! Writes what is on screen to an H.264 file while a macro runs.
//!
//! Through Media Foundation's sink writer, and that choice is the whole design. The
//! obvious alternative is to talk to each vendor's encoder directly - NVENC, AMF,
//! Quick Sync - which means three SDKs, three sets of vendor headers and three code
//! paths to keep working on hardware this project cannot test on. Media Foundation
//! already has all three registered as transforms and picks whichever is present; ask
//! it for hardware and it uses the GPU, ask it on a machine with none and it falls
//! back to software without the caller knowing. One API, no third-party SDK, and the
//! same `windows` crate every other Win32 call here goes through.
//!
//! **Plain MP4, and the index is written when the recording is stopped.** The first
//! version of this asked for `MF_MPEG4SINK_MOOV_BEFORE_MDAT`, believing it produced a
//! fragmented file that survives a crash. It does not - that attribute is about putting
//! the index first for progressive download - and what it actually produced here was a
//! file holding `ftyp`, `uuid` and `mdat` and **no `moov` at all**, which no player will
//! open. The self-test looks for the `moov` box now, because a check that read only the
//! first four bytes passed that file happily.
//!
//! So stopping a recording matters: `stop` waits for `Finalize`, and a process killed
//! mid-recording leaves a file with no index. That is the same bargain OBS describes
//! when it recommends MKV, and the reason MKV is not the answer here is that Media
//! Foundation does not write it. A real fragmented-MP4 sink is what would close this,
//! and it is not written yet.
//!
//! **Frames come from the capture path that already exists.** Desktop Duplication when
//! it can, GDI when it cannot, exactly as the image search gets them - which means a
//! recording costs the same readback the vision code already proved is cheap, and no
//! second capture backend has to be written or tested.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use windows::Win32::Media::MediaFoundation::*;

/// Whether a recording is running, for anything that wants to show it.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// Frames written, so the window can say something is actually happening.
static FRAMES: AtomicU64 = AtomicU64::new(0);
/// Frames the capture could not produce at all.
static DROPPED: AtomicU64 = AtomicU64::new(0);
/// Frames written again because the capture could not keep up with the rate that
/// was asked for. This is the honest measure of whether that rate was achievable:
/// the video is still real time, but it has fewer distinct pictures in it than it
/// claims frames.
static DUPLICATED: AtomicU64 = AtomicU64::new(0);
/// Set to ask the thread to finish and finalise the file.
static STOP: AtomicBool = AtomicBool::new(false);
/// How long the encoder was actually taking frames, and how long it spent getting
/// ready before that.
///
/// Worth publishing rather than inferring from the caller's clock, and the reason
/// is a measurement that read as a bug: starting Media Foundation, creating the
/// sink and negotiating a hardware encoder took nearly two seconds on the machine
/// this was written on, so six seconds of wall clock was four seconds of recording
/// and the video looked a third too short. It was not - the caller was timing the
/// wrong thing. Anything that lines a recording up against a macro's timeline needs
/// to know this too, because the same two seconds sit between pressing record and
/// the first frame.
static SETUP_MS: AtomicU64 = AtomicU64::new(0);
static RECORDED_MS: AtomicU64 = AtomicU64::new(0);

pub fn running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

/// Milliseconds spent starting up, and milliseconds actually recording.
pub fn timings() -> (u64, u64) {
    (SETUP_MS.load(Ordering::Relaxed), RECORDED_MS.load(Ordering::Relaxed))
}

/// Distinct frames captured, frames missed, and frames written more than once.
pub fn counters() -> (u64, u64, u64) {
    (
        FRAMES.load(Ordering::Relaxed),
        DROPPED.load(Ordering::Relaxed),
        DUPLICATED.load(Ordering::Relaxed),
    )
}

/// Quality, as the three words a person actually chooses between.
///
/// Bitrate rather than a quality parameter because the sink writer's hardware
/// transforms do not all honour the same rate-control knobs, and a number of bits
/// per second is the one thing every H.264 encoder understands.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Quality {
    /// Enough for a bug report of a user interface.
    Low,
    #[default]
    High,
    /// For a game, where the whole frame changes every frame.
    VeryHigh,
}

impl Quality {
    pub fn at(i: usize) -> Self {
        [Self::Low, Self::High, Self::VeryHigh][i.min(2)]
    }
    /// Bits per second for a given frame size, scaled with the area so that the
    /// same choice means the same thing on a laptop and on a 4K desktop.
    fn bitrate(self, w: u32, h: u32) -> u32 {
        let mpx = (w as f64 * h as f64) / 1_000_000.0;
        let per_mpx = match self {
            Self::Low => 2_500_000.0,
            Self::High => 6_000_000.0,
            Self::VeryHigh => 12_000_000.0,
        };
        (mpx * per_mpx).clamp(1_000_000.0, 120_000_000.0) as u32
    }
}

/// Starts recording the whole desktop into `path`.
///
/// Returns immediately; the work happens on its own thread. `stop` ends it and
/// waits for the file to be closed properly, because an unfinalised recording is
/// a file with no duration in it.
pub fn start(
    path: std::path::PathBuf,
    fps: u32,
    quality: Quality,
) -> std::io::Result<()> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err(std::io::Error::other("a recording is already running"));
    }
    STOP.store(false, Ordering::SeqCst);
    FRAMES.store(0, Ordering::Relaxed);
    DROPPED.store(0, Ordering::Relaxed);
    DUPLICATED.store(0, Ordering::Relaxed);
    SETUP_MS.store(0, Ordering::Relaxed);
    RECORDED_MS.store(0, Ordering::Relaxed);
    let fps = fps.clamp(5, 60);
    let started = std::thread::Builder::new()
        .name("screen-recorder".into())
        .spawn(move || {
            if let Err(e) = unsafe { run(&path, fps, quality) } {
                tracing::warn!("screen recording stopped: {e}");
            }
            RUNNING.store(false, Ordering::SeqCst);
        });
    if let Err(e) = started {
        RUNNING.store(false, Ordering::SeqCst);
        return Err(e);
    }
    Ok(())
}

/// Asks the recording to finish and waits for the file to be closed.
///
/// The wait is the point. `Finalize` writes the last fragment and the index; a
/// caller that returned before it happened would hand back a path to a file still
/// being written.
pub fn stop() {
    if !RUNNING.load(Ordering::SeqCst) {
        return;
    }
    STOP.store(true, Ordering::SeqCst);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while RUNNING.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if RUNNING.load(Ordering::SeqCst) {
        tracing::warn!("the recorder did not finish within ten seconds");
    }
}

/// One hundred-nanosecond tick, which is Media Foundation's unit for everything.
const HNS: i64 = 10_000_000;

unsafe fn run(
    path: &std::path::Path,
    fps: u32,
    quality: Quality,
) -> windows::core::Result<()> {
    unsafe {
        // Both are reference-counted and both have to be undone, so the whole body
        // is wrapped and the shutdown happens on the way out whatever went wrong.
        MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET)?;
        let out = record(path, fps, quality);
        let _ = MFShutdown();
        out
    }
}

unsafe fn record(
    path: &std::path::Path,
    fps: u32,
    quality: Quality,
) -> windows::core::Result<()> {
    unsafe {
        let setting_up = std::time::Instant::now();
        let (vx, vy, vw, vh) = super::platform::virtual_screen_rect();
        // H.264 wants even dimensions; an odd desktop is unusual but a 4:2:0
        // chroma plane of half an odd number is not a thing.
        let (w, h) = ((vw as u32) & !1, (vh as u32) & !1);
        if w < 16 || h < 16 {
            return Err(windows::core::Error::from(windows::Win32::Foundation::E_INVALIDARG));
        }

        // Fragmented output, so a recording cut short by a crash still plays.
        let attrs = {
            let mut a: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut a, 4)?;
            let a = a.ok_or_else(|| {
                windows::core::Error::from(windows::Win32::Foundation::E_FAIL)
            })?;
            a.SetUINT32(&MF_LOW_LATENCY, 1)?;
            // Ask for the GPU. On a machine with none this is simply ignored and
            // the software encoder answers, which is the behaviour that makes one
            // code path enough.
            a.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            a
        };

        let url: Vec<u16> = super::wide(&path.to_string_lossy());
        let writer = MFCreateSinkWriterFromURL(
            windows::core::PCWSTR(url.as_ptr()),
            None,
            &attrs,
        )?;

        // What comes out: H.264 at the chosen bitrate.
        let out_type = MFCreateMediaType()?;
        out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        out_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        out_type.SetUINT32(&MF_MT_AVG_BITRATE, quality.bitrate(w, h))?;
        out_type.SetUINT32(
            &MF_MT_INTERLACE_MODE,
            MFVideoInterlace_Progressive.0 as u32,
        )?;
        set_ratio(&out_type, &MF_MT_FRAME_SIZE, w, h)?;
        set_ratio(&out_type, &MF_MT_FRAME_RATE, fps, 1)?;
        set_ratio(&out_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
        let stream = writer.AddStream(&out_type)?;

        // What goes in: the BGRA the capture path already produces.
        let in_type = MFCreateMediaType()?;
        in_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        in_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        in_type.SetUINT32(
            &MF_MT_INTERLACE_MODE,
            MFVideoInterlace_Progressive.0 as u32,
        )?;
        set_ratio(&in_type, &MF_MT_FRAME_SIZE, w, h)?;
        set_ratio(&in_type, &MF_MT_FRAME_RATE, fps, 1)?;
        set_ratio(&in_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
        writer.SetInputMediaType(stream, &in_type, None)?;

        writer.BeginWriting()?;
        tracing::info!(
            "recording {w}x{h} at {fps} fps, {} kbit/s, to {}",
            quality.bitrate(w, h) / 1000,
            path.display()
        );

        let frame_hns = HNS / i64::from(fps);
        let interval = std::time::Duration::from_nanos(1_000_000_000 / u64::from(fps));
        let row = (w as usize) * 4;
        let need = row * (h as usize);
        SETUP_MS.store(setting_up.elapsed().as_millis() as u64, Ordering::Relaxed);
        let began = std::time::Instant::now();
        let mut next = began;
        // Which frame slot of the constant-rate stream comes next.
        let mut index: i64 = 0;
        // The last frame that arrived, so a capture that fails or falls behind
        // repeats a frame rather than putting a hole in the video.
        let mut last: Option<Vec<u8>> = None;

        while !STOP.load(Ordering::SeqCst) {
            let grabbed = super::platform::capture(vx, vy, w as i32, h as i32)
                .map(|f| f.px)
                .filter(|px| px.len() >= need);
            let px = match (grabbed, last.as_ref()) {
                (Some(p), _) => {
                    last = Some(p);
                    last.as_ref().expect("just stored")
                }
                (None, Some(prev)) => {
                    DROPPED.fetch_add(1, Ordering::Relaxed);
                    prev
                }
                // Nothing captured and nothing to repeat: the first frame has not
                // arrived. Wait for it rather than writing a black one.
                (None, None) => {
                    DROPPED.fetch_add(1, Ordering::Relaxed);
                    std::thread::sleep(interval);
                    continue;
                }
            };

            let buffer = MFCreateMemoryBuffer(need as u32)?;
            {
                let mut dst: *mut u8 = std::ptr::null_mut();
                buffer.Lock(&mut dst, None, None)?;
                if !dst.is_null() {
                    // Bottom row first. `MFVideoFormat_RGB32` is defined bottom-up
                    // and a screen capture is top-down, so one of the two has to
                    // turn round. Doing it here, with a positive stride, rather
                    // than by declaring a negative one: a negative stride also
                    // changes which end of the buffer the pointer is expected to
                    // name, the two conventions disagree between Media Foundation's
                    // own helpers, and the failure is a video that is silently
                    // upside down. A loop that copies the last row first cannot be
                    // read two ways, and moves the same bytes.
                    for y in 0..h as usize {
                        std::ptr::copy_nonoverlapping(
                            px.as_ptr().add((h as usize - 1 - y) * row),
                            dst.add(y * row),
                            row,
                        );
                    }
                }
                buffer.Unlock()?;
            }
            buffer.SetCurrentLength(need as u32)?;

            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            // The stream has to be a constant frame rate, and holding it is the
            // whole of what makes the recording play at the speed it was made.
            //
            // Two wrong versions came before this one. Stamping each frame with a
            // counter made a six-second recording into a four-second video played
            // half again too fast. Stamping with the clock did not fix it either:
            // the duration a player reports comes from the declared frame rate and
            // the number of frames, not from the times on the samples. Six seconds
            // of screen captured at twenty-one frames a second and declared at
            // thirty is four seconds of video however the samples are labelled.
            //
            // So the frame count is made to match the clock. Whenever a capture
            // takes longer than one frame, the frame that did arrive is written
            // again for each slot that passed - the same picture, which is what was
            // on screen for that whole time anyway, and a handful of bytes once the
            // encoder sees it has not changed. The picture is copied once and the
            // sample re-timed, so a duplicate costs no second copy of fifteen
            // megabytes.
            let slot = (began.elapsed().as_nanos() as i64 / 100) / frame_hns;
            let mut wrote = 0u64;
            while index <= slot {
                sample.SetSampleTime(index * frame_hns)?;
                sample.SetSampleDuration(frame_hns)?;
                writer.WriteSample(stream, &sample)?;
                index += 1;
                wrote += 1;
            }
            if wrote == 0 {
                // Ahead of the schedule, which happens on a small desktop. The
                // frame is simply not needed yet.
                let now = std::time::Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                }
                continue;
            }
            FRAMES.fetch_add(1, Ordering::Relaxed);
            DUPLICATED.fetch_add(wrote - 1, Ordering::Relaxed);

            // Paced against a fixed schedule rather than by sleeping a fixed
            // amount, so a slow capture does not make the recording gradually
            // slower than real time.
            next += interval;
            let now = std::time::Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now;
            }
        }

        RECORDED_MS.store(began.elapsed().as_millis() as u64, Ordering::Relaxed);
        writer.Finalize()?;
        tracing::info!(
            "recording finished: {} distinct frame(s), {} repeated, {} missed",
            FRAMES.load(Ordering::Relaxed),
            DUPLICATED.load(Ordering::Relaxed),
            DROPPED.load(Ordering::Relaxed)
        );
        Ok(())
    }
}

/// Two 32-bit halves packed into the 64-bit attribute Media Foundation uses for
/// every size and every ratio.
unsafe fn set_ratio(
    t: &IMFMediaType,
    key: &windows::core::GUID,
    hi: u32,
    lo: u32,
) -> windows::core::Result<()> {
    unsafe { t.SetUINT64(key, (u64::from(hi) << 32) | u64::from(lo)) }
}
