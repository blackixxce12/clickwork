//! Global shortcuts through the desktop portal.
//!
//! The evdev hotkeys in `hooks` need read access to the input devices. This is
//! the other road, for a machine without it: `org.freedesktop.portal.GlobalShortcuts`
//! registers named shortcuts with the desktop, and the desktop decides which keys
//! fire them. Only the slots the compositor has not already taken are asked for, so
//! on Hyprland - where `hyprbinds` binds every hotkey itself - usually none are, and
//! the whole registration is skipped with a line in the log saying so. Where slots
//! do get registered, the decision is a line in the desktop's config:
//!
//! ```text
//! hl.bind("F9", hl.dsp.global("clickwork:stop"))
//! ```
//!
//! (`hyprctl globalshortcuts` lists the names that were actually registered, which
//! is the list to check before writing that line.)
//! Nothing here can fail loudly - a desktop without the portal is common - so
//! every problem is one line in the log at most.
//!
//! This is the middle rung of three, under `hyprbinds` and over `hooks`. The rung
//! above knows the exact key it bound and can therefore retire a slot from the
//! evdev hook once and for all; a slot it took is never asked for here at all.
//! This rung cannot say the same. The desktop picks the key, may pick none, and
//! never says which, so a shortcut registered here is no promise that anything
//! will ever fire it - and refusing the evdev hook the slot on that evidence would
//! leave a hotkey dead on every desktop that registers a shortcut without binding
//! a key, which is what Hyprland does until the user writes the config line above.
//! So the arbitration happens after the press rather than before it: both tiers
//! stamp the slot they are about to act on, and the one that finds a fresh stamp
//! stands down. Reading the devices directly is the shorter road, so in practice
//! the evdev hook is usually the one that wins the race; what matters is that the
//! press runs once.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

static SESSION: OnceLock<Option<OwnedObjectPath>> = OnceLock::new();

/// (id, description), in `HK_IDS` order - the slot numbering all three tiers use,
/// which is what lets them trade a bare bit index and mean the same thing by it.
const SHORTCUTS: [(&str, &str); 7] = [
    ("record", "Clickwork: record / stop recording"),
    ("play", "Clickwork: play / stop"),
    ("stop", "Clickwork: emergency stop"),
    ("pause", "Clickwork: pause / resume"),
    ("faster", "Clickwork: faster"),
    ("slower", "Clickwork: slower"),
    ("skip", "Clickwork: skip this step"),
];

/// Bit `i` set: the portal holds a shortcut for slot `i`, so a press the evdev hook
/// sees may be one this tier is about to deliver too.
static REGISTERED: AtomicU32 = AtomicU32::new(0);

/// When each slot was last acted on, in `crate::now_us()` terms. Zero means never,
/// which is why the stamp itself never is.
static DELIVERED_US: [AtomicU64; 7] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// How far apart one press may look to the two tiers. Generous for a D-Bus signal
/// chasing a key the compositor has already dealt with.
///
/// It is also a debounce, and only on the slots the portal answered for: two taps of
/// `faster` closer together than this count once. 80 ms is the shortest interval that
/// still covers a portal round-trip, and it is under the ~120 ms a key repeat takes to
/// start, so holding the key still ramps. A tier-1 slot never reaches here at all.
const ECHO_US: u64 = 80_000;

/// Takes slot `i` for whichever tier asks first: true when this press is the
/// caller's to act on, false when the other tier acted on it a moment ago.
///
/// A slot the portal never registered is the evdev hook's alone and goes through
/// without a stamp, so rapid taps of one of those still count every time.
pub fn claim(i: usize) -> bool {
    let Some(slot) = DELIVERED_US.get(i) else { return true };
    if REGISTERED.load(Ordering::Relaxed) & (1 << i) == 0 {
        return true;
    }
    let now = crate::now_us().max(1);
    slot.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last| {
        (last == 0 || now.saturating_sub(last) >= ECHO_US).then_some(now)
    })
    .is_ok()
}

/// A hotkey written the way the XDG shortcuts syntax behind `preferred_trigger`
/// wants it: modifier words and an X11 keysym joined by `+`, as in `CTRL+ALT+F9`.
///
/// `None` for an empty slot, and the shortcut then goes out with no preference at
/// all - deliberately, because that is what keeps `faster`, `slower` and `skip`
/// listed in the desktop's own binding UI, the only way to reach them while their
/// slots here are unset, which is how they ship.
fn trigger_of(hk: &crate::Hotkey) -> Option<String> {
    if hk.vk == 0 {
        return None;
    }
    let key = super::hyprbinds::keysym_of_vk(hk.vk)?;
    let mut parts: Vec<&str> = Vec::new();
    if hk.ctrl {
        parts.push("CTRL");
    }
    if hk.alt {
        parts.push("ALT");
    }
    if hk.shift {
        parts.push("SHIFT");
    }
    parts.push(&key);
    Some(parts.join("+"))
}

/// Starts the portal session on its own thread. Returns at once.
pub fn start() {
    let _ = std::thread::Builder::new().name("shortcuts".into()).spawn(|| {
        if let Err(e) = run() {
            tracing::info!("global-shortcut portal not used: {e}");
        }
    });
}

pub fn stop() {
    REGISTERED.store(0, Ordering::Relaxed);
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
    // Precedence, settled before a single D-Bus message goes out: a slot Hyprland
    // binds is taken out of the key stream before any window sees it, and asking
    // the desktop for that same key again would buy nothing but a second delivery
    // of one press. What the compositor could not take is this tier's to ask for;
    // if it took everything, there is no session here worth opening.
    // Read once: the portal takes its shortcut list at registration and there is no
    // way to amend it afterwards. So a slot that loses its compositor bind later - the
    // user moves it onto a combo Hyprland refuses - stays out of this tier for the rest
    // of the run. The evdev hook still covers it, which is why that is a shrug and not
    // a hole.
    let mine = !super::hyprbinds::HANDLED.load(Ordering::Relaxed) & 0x7F;
    if mine == 0 {
        return Err("every hotkey is a compositor bind already".into());
    }
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
    // The preference follows the keys the user actually configured rather than the
    // defaults they were written against, so the desktop is asked for the key the
    // other two tiers are watching for and all three agree on what a hotkey is.
    let hk = *crate::PENDING_HOTKEYS.lock();
    let shortcuts: Vec<(String, HashMap<&str, Value>)> = SHORTCUTS
        .iter()
        .enumerate()
        .filter(|(i, _)| mine & (1 << i) != 0)
        .map(|(i, (id, desc))| {
            let mut m: HashMap<&str, Value> = HashMap::new();
            m.insert("description", Value::from(*desc));
            if let Some(t) = trigger_of(&hk[i]) {
                m.insert("preferred_trigger", Value::from(t));
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
        SHORTCUTS
            .iter()
            .enumerate()
            .filter(|(i, _)| mine & (1 << i) != 0)
            .map(|(_, (id, _))| *id)
            .collect::<Vec<_>>()
            .join(", ")
    );

    // ---- Activated ---------------------------------------------------------
    let activated = gs.receive_signal("Activated")?;
    // Only once the signal is really subscribed: before this nothing can arrive
    // from here, and until something can, the evdev hook has no echo to watch for.
    REGISTERED.store(mine, Ordering::Relaxed);
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
        let Some(i) = SHORTCUTS.iter().position(|(sid, _)| *sid == id.as_str()) else { continue };
        if !claim(i) {
            // The evdev hook saw the same key first - straight off the device, a
            // shorter road than the compositor plus a D-Bus round trip - and has
            // already run the action.
            tracing::debug!("portal shortcut '{id}' was already delivered by the evdev hook");
            continue;
        }
        tracing::info!("portal shortcut '{id}' activated");
        match id.as_str() {
            "record" => crate::toggle_recording(state),
            "play" => crate::toggle_playback(state),
            "stop" => crate::stop_everything(state),
            "pause" => crate::toggle_pause(state),
            "faster" => crate::nudge_speed(state, 1.25),
            "slower" => crate::nudge_speed(state, 0.8),
            "skip" => state.skip_step.store(true, Ordering::Relaxed),
            _ => {}
        }
    }
    // The stream ends only when the session does, and a slot no tier owns any more
    // belongs to the evdev hook without reservation.
    REGISTERED.store(0, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tiers_number_the_slots_alike() {
        assert_eq!(SHORTCUTS.len(), crate::HK_IDS.len());
        let ca = crate::Hotkey { vk: 0x78, ctrl: true, alt: true, shift: false };
        assert_eq!(trigger_of(&ca).as_deref(), Some("CTRL+ALT+F9"));
        let f6 = crate::Hotkey { vk: 0x75, ctrl: false, alt: false, shift: false };
        assert_eq!(trigger_of(&f6).as_deref(), Some("F6"));
        let none = crate::Hotkey { vk: 0, ctrl: false, alt: false, shift: false };
        assert_eq!(trigger_of(&none), None);
    }

    #[test]
    fn only_one_tier_acts_on_a_press() {
        crate::init_epoch();
        // Nothing registered here: every press is the evdev hook's, however fast
        // they come.
        REGISTERED.store(0, Ordering::Relaxed);
        assert!(claim(4));
        assert!(claim(4));
        // Registered: the second tier to arrive finds the stamp and stands down,
        // and the slot next to it is none the wiser.
        REGISTERED.store(0x7F, Ordering::Relaxed);
        assert!(claim(4));
        assert!(!claim(4));
        assert!(claim(5));
        REGISTERED.store(0, Ordering::Relaxed);
    }
}
