//! One running copy, and a way to talk to it.
//!
//! Windows used a named mutex and `FindWindow`. Here a Unix socket in the runtime
//! directory does both jobs: whoever binds it is the instance, and a second launch
//! that finds it bound sends `show` and leaves.

use std::io::{Read as _, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::OnceLock;

static LISTENER: OnceLock<UnixListener> = OnceLock::new();

pub fn path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    #[cfg(feature = "inspect")]
    let name = "clickwork-inspect.sock";
    #[cfg(not(feature = "inspect"))]
    let name = "clickwork.sock";
    dir.join(name)
}

/// Sends one word to the running instance and returns its reply.
pub fn send(word: &str) -> Option<String> {
    let mut s = UnixStream::connect(path()).ok()?;
    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(1500)));
    s.write_all(word.as_bytes()).ok()?;
    let _ = s.shutdown(std::net::Shutdown::Write);
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    Some(out)
}

/// Binds the socket. `false` when another instance already answers on it.
pub fn acquire() -> bool {
    let p = path();
    if send("ping").is_some_and(|r| r.trim() == "pong") {
        return false;
    }
    // Nobody answered: a stale file from a crash, or nothing at all.
    let _ = std::fs::remove_file(&p);
    let listener = match UnixListener::bind(&p) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("single-instance socket {} could not be bound: {e}", p.display());
            return true;
        }
    };
    if LISTENER.set(listener).is_err() {
        return true;
    }
    let _ = std::thread::Builder::new().name("instance".into()).spawn(serve);
    true
}

fn serve() {
    let Some(listener) = LISTENER.get() else { return };
    for stream in listener.incoming() {
        let Ok(mut s) = stream else { continue };
        let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(500)));
        let mut buf = String::new();
        let _ = s.read_to_string(&mut buf);
        let reply = match buf.trim() {
            "ping" => "pong",
            "show" => {
                crate::set_window_visible(true);
                "ok"
            }
            "hide" => {
                crate::set_window_visible(false);
                "ok"
            }
            "quit" => {
                crate::quit_application();
                "ok"
            }
            "record" | "play" | "stop" | "pause" | "faster" | "slower" | "skip" => {
                match crate::GLOBAL_STATE.get() {
                    Some(st) => {
                        match buf.trim() {
                            "record" => crate::toggle_recording(st),
                            "play" => crate::toggle_playback(st),
                            "stop" => crate::stop_everything(st),
                            "pause" => crate::toggle_pause(st),
                            "faster" => crate::nudge_speed(st, 1.25),
                            "slower" => crate::nudge_speed(st, 0.8),
                            _ => st.skip_step.store(true, std::sync::atomic::Ordering::Relaxed),
                        }
                        "ok"
                    }
                    None => "starting",
                }
            }
            "status" => {
                // `starting` until the interface has drawn, not merely until the
                // shared state exists. The state is set during setup, long before
                // eframe has a surface, so answering `idle` there told a caller
                // the program was up while its window had not appeared - and a
                // caller that then asked about windows got none.
                if !crate::ui_painted() {
                    "starting"
                } else if let Some(st) = crate::GLOBAL_STATE.get() {
                    if st.playing.load(std::sync::atomic::Ordering::Relaxed) {
                        "playing"
                    } else if st.recording.load(std::sync::atomic::Ordering::Relaxed) {
                        "recording"
                    } else {
                        "idle"
                    }
                } else {
                    "starting"
                }
            }
            _ => "?",
        };
        let _ = s.write_all(reply.as_bytes());
    }
}
