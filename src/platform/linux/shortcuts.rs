//! Global shortcuts through the desktop portal.
//!
//! The evdev hotkeys in `hooks` need read access to the input devices. This is
//! the other road, for a machine without it: `org.freedesktop.portal.GlobalShortcuts`
//! registers seven named shortcuts with the desktop, and the desktop decides
//! which keys fire them. On Hyprland that decision is a line in the config:
//!
//! ```text
//! hl.bind("F9", hl.dsp.global("clickwork:stop"))
//! ```
//!
//! (`hyprctl globalshortcuts` lists the exact names once the program is up.)
//! Nothing here can fail loudly - a desktop without the portal is common - so
//! every problem is one line in the log at most.

use std::collections::HashMap;
use std::sync::OnceLock;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

static SESSION: OnceLock<Option<OwnedObjectPath>> = OnceLock::new();

/// (id, description, suggested trigger)
const SHORTCUTS: [(&str, &str, &str); 7] = [
    ("record", "Clickwork: record / stop recording", "F6"),
    ("play", "Clickwork: play / stop", "F7"),
    ("stop", "Clickwork: emergency stop", "F9"),
    ("pause", "Clickwork: pause / resume", "F8"),
    ("faster", "Clickwork: faster", ""),
    ("slower", "Clickwork: slower", ""),
    ("skip", "Clickwork: skip this step", ""),
];

/// Starts the portal session on its own thread. Returns at once.
pub fn start() {
    let _ = std::thread::Builder::new().name("shortcuts".into()).spawn(|| {
        if let Err(e) = run() {
            tracing::info!("global-shortcut portal not used: {e}");
        }
    });
}

pub fn stop() {
    if let Some(Some(session)) = SESSION.get()
        && let Ok(conn) = Connection::session()
        && let Ok(p) = Proxy::new(&conn, PORTAL, session.as_str(), "org.freedesktop.portal.Session")
    {
        let _ = p.call::<_, _, ()>("Close", &());
    }
}

fn request_path(conn: &Connection, token: &str) -> Result<String, zbus::Error> {
    let unique = conn.unique_name().map(|n| n.to_string()).unwrap_or_default();
    let sender = unique.trim_start_matches(':').replace('.', "_");
    Ok(format!("{PORTAL_PATH}/request/{sender}/{token}"))
}

/// Puts this process into a systemd scope named the way launchers name them,
/// `app-clickwork-<pid>.scope`, which is where the portal reads an application
/// id from. A program started from a terminal or a compositor `exec` has no such
/// scope and the portal refuses it with "an app id is required". Harmless when
/// systemd is not running the session: the call fails and the portal says no.
fn ensure_app_scope(conn: &Connection) {
    let cg = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    if cg.lines().any(|l| l.contains("/app-") && l.trim_end().ends_with(".scope")) {
        return;
    }
    let pid = std::process::id();
    let Ok(systemd) = Proxy::new(
        conn,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    ) else {
        return;
    };
    // A reverse-DNS id: the portal validates it as a GApplication id, which
    // needs at least two dot-separated parts, and "clickwork" alone is refused.
    let name = format!("app-clickwork-{}-{pid}.scope", crate::APP_ID);
    let props: Vec<(&str, Value)> = vec![
        ("PIDs", Value::from(vec![pid])),
        ("Description", Value::from("Clickwork")),
        ("CollectMode", Value::from("inactive-or-failed")),
    ];
    let aux: Vec<(&str, Vec<(&str, Value)>)> = Vec::new();
    match systemd.call::<_, _, OwnedObjectPath>("StartTransientUnit", &(name.as_str(), "fail", props, aux)) {
        Ok(_) => {
            // The move into the new cgroup is a job systemd runs a moment later,
            // and the portal reads `/proc/<pid>/cgroup` the first time this
            // connection talks to it - so wait for the move to have happened.
            for _ in 0..50 {
                let cg = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
                if cg.contains(&name) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            tracing::info!("running in systemd scope {name}");
        }
        Err(e) => tracing::debug!("no systemd scope for the portal: {e}"),
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    // One connection to arrange the scope, and a fresh one to talk to the portal:
    // the portal remembers what it learned about a sender the first time it saw it.
    {
        let setup = Connection::session()?;
        ensure_app_scope(&setup);
    }
    let conn = Connection::session()?;
    // xdg-desktop-portal 1.20+ no longer guesses a host application's id from
    // its cgroup; the application registers it, and the portal checks that the
    // scope above agrees. Older portals lack the interface, which is harmless.
    if let Ok(reg) = Proxy::new(&conn, PORTAL, PORTAL_PATH, "org.freedesktop.host.portal.Registry") {
        let opts: HashMap<&str, Value> = HashMap::new();
        match reg.call::<_, _, ()>("Register", &(crate::APP_ID, opts)) {
            Ok(()) => tracing::info!("registered with the portal as {}", crate::APP_ID),
            Err(e) => tracing::info!("portal registry refused {}: {e}", crate::APP_ID),
        }
    }
    let gs = Proxy::new(&conn, PORTAL, PORTAL_PATH, "org.freedesktop.portal.GlobalShortcuts")?;
    // Portal present at all?
    let _version: u32 = gs.get_property("version")?;

    // ---- CreateSession -----------------------------------------------------
    let token = "clickwork_gs1";
    let req_path = request_path(&conn, token)?;
    let req = Proxy::new(&conn, PORTAL, req_path.as_str(), "org.freedesktop.portal.Request")?;
    let mut responses = req.receive_signal("Response")?;
    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("handle_token", Value::from(token));
    opts.insert("session_handle_token", Value::from("clickwork"));
    let _: OwnedObjectPath = gs.call("CreateSession", &(opts,))?;
    let msg = responses.next().ok_or("no answer to CreateSession")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = msg.body().deserialize()?;
    if code != 0 {
        return Err(format!("CreateSession refused ({code})").into());
    }
    let session: String = results
        .get("session_handle")
        .and_then(|v| String::try_from(v.clone()).ok())
        .ok_or("no session handle")?;
    let session = OwnedObjectPath::try_from(session)?;
    let _ = SESSION.set(Some(session.clone()));

    // ---- BindShortcuts -----------------------------------------------------
    let token2 = "clickwork_gs2";
    let req_path2 = request_path(&conn, token2)?;
    let req2 = Proxy::new(&conn, PORTAL, req_path2.as_str(), "org.freedesktop.portal.Request")?;
    let mut responses2 = req2.receive_signal("Response")?;
    let shortcuts: Vec<(String, HashMap<&str, Value>)> = SHORTCUTS
        .iter()
        .map(|(id, desc, trigger)| {
            let mut m: HashMap<&str, Value> = HashMap::new();
            m.insert("description", Value::from(*desc));
            if !trigger.is_empty() {
                m.insert("preferred_trigger", Value::from(*trigger));
            }
            (id.to_string(), m)
        })
        .collect();
    let mut opts2: HashMap<&str, Value> = HashMap::new();
    opts2.insert("handle_token", Value::from(token2));
    let _: OwnedObjectPath =
        gs.call("BindShortcuts", &(session.clone(), shortcuts, "", opts2))?;
    let msg = responses2.next().ok_or("no answer to BindShortcuts")?;
    let (code, _results): (u32, HashMap<String, OwnedValue>) = msg.body().deserialize()?;
    if code != 0 {
        return Err(format!("BindShortcuts refused ({code})").into());
    }
    tracing::info!(
        "global shortcuts registered with the portal: {}",
        SHORTCUTS.iter().map(|(id, ..)| *id).collect::<Vec<_>>().join(", ")
    );

    // ---- Activated ---------------------------------------------------------
    let activated = gs.receive_signal("Activated")?;
    for msg in activated {
        let Ok((sess, id, _ts, _opts)) =
            msg.body().deserialize::<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>)>()
        else {
            continue;
        };
        if sess != session {
            continue;
        }
        let Some(state) = crate::GLOBAL_STATE.get() else { continue };
        tracing::info!("portal shortcut '{id}' activated");
        match id.as_str() {
            "record" => crate::toggle_recording(state),
            "play" => crate::toggle_playback(state),
            "stop" => crate::stop_everything(state),
            "pause" => crate::toggle_pause(state),
            "faster" => crate::nudge_speed(state, 1.25),
            "slower" => crate::nudge_speed(state, 0.8),
            "skip" => state.skip_step.store(true, std::sync::atomic::Ordering::Relaxed),
            _ => {}
        }
    }
    Ok(())
}
