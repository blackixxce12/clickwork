//! Screen recording, through whichever recorder the machine has.
//!
//! Media Foundation has no counterpart here, and writing an H.264 encoder pipeline
//! is not this program's job. `gpu-screen-recorder` and `wf-recorder` both record
//! a Wayland output to an MP4, both finish the file cleanly on SIGINT, and one of
//! them is installed on most machines that would want this. Frame counts are what
//! the tools report, which is nothing; the ones shown are estimated from the clock.

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static RUNNING: AtomicBool = AtomicBool::new(false);
static FRAMES: AtomicU64 = AtomicU64::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static DUPLICATED: AtomicU64 = AtomicU64::new(0);
static SETUP_MS: AtomicU64 = AtomicU64::new(0);
static RECORDED_MS: AtomicU64 = AtomicU64::new(0);
static CHILD: Mutex<Option<(std::process::Child, std::time::Instant, u32)>> = Mutex::new(None);

pub fn running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

/// Milliseconds spent starting up, and milliseconds actually recording.
pub fn timings() -> (u64, u64) {
    (SETUP_MS.load(Ordering::Relaxed), RECORDED_MS.load(Ordering::Relaxed))
}

/// Frames captured, missed, and written twice - estimated from the clock, since
/// neither tool reports them.
pub fn counters() -> (u64, u64, u64) {
    let frames = {
        let c = CHILD.lock();
        match c.as_ref() {
            Some((_, began, fps)) if RUNNING.load(Ordering::Relaxed) => {
                began.elapsed().as_millis() as u64 * *fps as u64 / 1000
            }
            _ => FRAMES.load(Ordering::Relaxed),
        }
    };
    (frames, DROPPED.load(Ordering::Relaxed), DUPLICATED.load(Ordering::Relaxed))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Quality {
    Low,
    #[default]
    High,
    VeryHigh,
}

impl Quality {
    pub fn at(i: usize) -> Self {
        [Self::Low, Self::High, Self::VeryHigh][i.min(2)]
    }
}

fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|p| p.join(name)).find(|p| p.is_file())
}

/// Starts recording the whole desktop into `path`.
pub fn start(path: std::path::PathBuf, fps: u32, quality: Quality) -> std::io::Result<()> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err(std::io::Error::other("a recording is already running"));
    }
    FRAMES.store(0, Ordering::Relaxed);
    DROPPED.store(0, Ordering::Relaxed);
    DUPLICATED.store(0, Ordering::Relaxed);
    SETUP_MS.store(0, Ordering::Relaxed);
    RECORDED_MS.store(0, Ordering::Relaxed);
    let fps = fps.clamp(5, 60);
    let setting_up = std::time::Instant::now();
    let mut cmd = if let Some(gsr) = which("gpu-screen-recorder") {
        let q = match quality {
            Quality::Low => "medium",
            Quality::High => "high",
            Quality::VeryHigh => "very_high",
        };
        let mut c = std::process::Command::new(gsr);
        c.args(["-w", "screen", "-f", &fps.to_string(), "-q", q, "-c", "mp4", "-o"]).arg(&path);
        c
    } else if let Some(wf) = which("wf-recorder") {
        let crf = match quality {
            Quality::Low => "28",
            Quality::High => "23",
            Quality::VeryHigh => "18",
        };
        let mut c = std::process::Command::new(wf);
        c.args(["-y", "-r", &fps.to_string(), "-c", "libx264", "-p", &format!("crf={crf}"), "-f"])
            .arg(&path);
        c
    } else {
        RUNNING.store(false, Ordering::SeqCst);
        return Err(std::io::Error::other(
            "no screen recorder found: install gpu-screen-recorder or wf-recorder",
        ));
    };
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    match cmd.spawn() {
        Ok(child) => {
            SETUP_MS.store(setting_up.elapsed().as_millis() as u64, Ordering::Relaxed);
            tracing::info!("screen recording to {} at {fps} fps", path.display());
            *CHILD.lock() = Some((child, std::time::Instant::now(), fps));
            Ok(())
        }
        Err(e) => {
            RUNNING.store(false, Ordering::SeqCst);
            Err(e)
        }
    }
}

/// Asks the recorder to finish and waits for the file to be closed.
pub fn stop() {
    if !RUNNING.load(Ordering::SeqCst) {
        return;
    }
    let taken = CHILD.lock().take();
    if let Some((mut child, began, fps)) = taken {
        let ms = began.elapsed().as_millis() as u64;
        RECORDED_MS.store(ms, Ordering::Relaxed);
        FRAMES.store(ms * fps as u64 / 1000, Ordering::Relaxed);
        unsafe {
            libc::kill(child.id() as i32, libc::SIGINT);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                _ => {
                    tracing::warn!("the recorder did not finish within ten seconds");
                    let _ = child.kill();
                    break;
                }
            }
        }
    }
    RUNNING.store(false, Ordering::SeqCst);
}
