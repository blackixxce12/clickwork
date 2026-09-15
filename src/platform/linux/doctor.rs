//! `--doctor`: what this machine can and cannot do, in one screen.
//!
//! Every Linux feature depends on something outside the program - a protocol the
//! compositor offers, a device the user may read, a library that may be installed
//! - and each of them fails quietly at the moment it is needed. This asks all of
//! them up front and says which is which, so "recording records nothing" is a
//! line of output rather than an evening.

use std::time::Instant;

fn row(name: &str, ok: bool, detail: &str) {
    println!("  {} {:<26} {}", if ok { "✔" } else { "✖" }, name, detail);
}

pub fn run() {
    println!("Clickwork {} - Linux doctor\n", crate::APP_VERSION);

    // ---- session ----------------------------------------------------------
    let wayland = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    row("Wayland display", !wayland.is_empty(), &wayland);
    let hypr = super::hypr::available();
    row(
        "Hyprland IPC",
        hypr,
        &if hypr { super::hypr::version() } else { "not a Hyprland session: window lookup and hide-to-tray are limited".into() },
    );

    // ---- outputs ----------------------------------------------------------
    let layout = super::geom::layout();
    for m in &layout.mons {
        row(
            &format!("output {}", m.name),
            true,
            &format!(
                "logical {}x{} at {},{}  scale {:.2}  physical {}x{} at {},{}{}",
                m.lw, m.lh, m.lx, m.ly, m.scale, m.pw, m.ph, m.px, m.py,
                if m.focused { "  (focused)" } else { "" }
            ),
        );
    }
    let (vx, vy, vw, vh) = layout.virtual_phys();
    println!("    virtual screen (physical): {vw}x{vh} at {vx},{vy}");
    let cur = super::platform::cursor_pos();
    println!("    cursor now: {},{} (physical)", cur.0, cur.1);

    // ---- windows and workspaces -------------------------------------------
    // Naming the window in front proves the whole path - socket, JSON, geometry -
    // rather than only that the socket answered.
    let all = super::hypr::clients();
    let windows = all.iter().filter(|c| c.is_real()).count();
    row(
        "window integration",
        hypr && windows > 0,
        &if !hypr {
            "not a Hyprland session: window steps, window anchors and hide-to-tray have nothing to ask"
                .to_string()
        } else if let Some(c) = super::hypr::active_window() {
            let (x, y, w, h) = layout.rect_to_phys(c.rect());
            format!(
                "{windows} windows; in front '{}' [{}] {w}x{h} at {x},{y} (physical){}",
                crate::clip(&c.title, 40),
                c.class,
                if c.xwayland { ", xwayland" } else { "" }
            )
        } else {
            format!("{windows} windows, none of them in front")
        },
    );

    // This process has no window - `--doctor` answers and returns before the GUI
    // starts - so the running instance is looked up by app id, ordered the way
    // `own_window` orders it so an open handbook cannot answer for the main window.
    let mut mine: Vec<&super::hypr::Client> =
        all.iter().filter(|c| c.is_real() && c.class == crate::APP_ID).collect();
    mine.sort_by_key(|c| (c.title != crate::APP_TITLE, c.focus_history_id));
    let visible = super::hypr::visible_workspace_ids();
    row(
        "workspace isolation",
        hypr,
        &if !hypr {
            "not a Hyprland session: recording and playback never pause for a workspace switch"
                .to_string()
        } else {
            let seen =
                visible.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
            match mine.first() {
                Some(c) if c.workspace.name.starts_with("special:clickwork") => format!(
                    "in view: {seen}; ours is parked on {} - put away on purpose, which does not count as away",
                    c.workspace.name
                ),
                Some(c) if visible.contains(&c.workspace.id) => format!(
                    "in view: {seen}; ours is on {} - in sight, so nothing pauses",
                    c.workspace.name
                ),
                Some(c) => format!(
                    "in view: {seen}; ours is on {} - out of sight, so recording and playback pause",
                    c.workspace.name
                ),
                None => format!("in view: {seen}; no running instance to place"),
            }
        },
    );

    // ---- protocols --------------------------------------------------------
    let mut names: Vec<String> = Vec::new();
    if let Ok(conn) = wayland_client::Connection::connect_to_env()
        && let Ok((globals, _q)) = wayland_client::globals::registry_queue_init::<Probe>(&conn)
    {
        names = globals.contents().clone_list().into_iter().map(|g| g.interface).collect();
    }
    let has = |n: &str| names.iter().any(|x| x == n);
    row("zwlr_virtual_pointer_v1", has("zwlr_virtual_pointer_manager_v1"), "mouse playback");
    row("zwp_virtual_keyboard_v1", has("zwp_virtual_keyboard_manager_v1"), "keyboard playback");
    row("zwlr_screencopy_v1", has("zwlr_screencopy_manager_v1"), "picture search, OCR, pixel condition");
    let layer_shell = has("zwlr_layer_shell_v1");
    row(
        "zwlr_layer_shell_v1",
        layer_shell,
        if layer_shell {
            "the see-through overlay"
        } else {
            "absent - there is no overlay on this compositor, so the display and the \
             debug rectangles stay off however they are switched"
        },
    );
    row("wp_viewporter", has("wp_viewporter"), "crisp overlay at fractional scale");
    row(
        "data-control",
        has("zwlr_data_control_manager_v1") || has("ext_data_control_manager_v1"),
        "clipboard without focus",
    );

    // ---- capture ----------------------------------------------------------
    let t0 = Instant::now();
    let frame = super::capture::capture(vx, vy, 320.min(vw), 240.min(vh));
    let dt = t0.elapsed();
    match frame {
        Some(f) => {
            let t1 = Instant::now();
            let n = 20;
            for _ in 0..n {
                let _ = super::capture::capture(vx, vy, 320.min(vw), 240.min(vh));
            }
            let per = t1.elapsed().as_secs_f64() * 1000.0 / n as f64;
            row("screen capture", true, &format!("{}x{} in {:.1} ms, then {per:.2} ms each", f.w, f.h, dt.as_secs_f64() * 1000.0));
        }
        None => row("screen capture", false, "no frame came back"),
    }

    // ---- input devices ----------------------------------------------------
    let mut readable = 0;
    let mut total = 0;
    if let Ok(dir) = std::fs::read_dir("/dev/input") {
        for e in dir.flatten() {
            let p = e.path();
            if !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event")) {
                continue;
            }
            total += 1;
            if std::fs::File::open(&p).is_ok() {
                readable += 1;
            }
        }
    }
    row(
        "input devices readable",
        readable > 0,
        &if readable > 0 {
            format!("{readable} of {total} - recording and hotkeys work")
        } else {
            format!(
                "0 of {total} - recording and evdev hotkeys will not work; add yourself to the \
                 `input` group (usermod -aG input $USER, then log in again) or install the udev rule"
            )
        },
    );

    // ---- virtual input ----------------------------------------------------
    row("virtual input ready", super::inject::available(), "pointer and keyboard objects created");

    // ---- OCR --------------------------------------------------------------
    match super::tess::unavailable_reason() {
        None => {
            let langs = super::tess::installed_languages();
            row(
                "tesseract",
                true,
                &format!(
                    "languages: {}; default '{}'",
                    langs.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join(", "),
                    super::tess::default_languages()
                ),
            );
            // Read whatever is in the top-left corner, as a smoke test.
            let (rw, rh) = (600.min(vw), 60.min(vh));
            if let Some(f) = super::capture::capture(vx, vy, rw, rh) {
                let t = Instant::now();
                match super::tess::recognize(&f, "", 1) {
                    Ok(boxes) => {
                        let text = crate::ocr::joined(&boxes).replace('\n', " | ");
                        println!(
                            "    read the top-left {rw}x{rh} in {:.0} ms: {}",
                            t.elapsed().as_secs_f64() * 1000.0,
                            if text.is_empty() { "(nothing)".to_string() } else { crate::clip(&text, 120) }
                        );
                    }
                    Err(e) => println!("    reading the top-left corner failed: {e}"),
                }
            }
        }
        Some(why) => row("tesseract", false, why),
    }

    // ---- desktop services -------------------------------------------------
    let a11y = zbus::blocking::Connection::session()
        .ok()
        .and_then(|c| {
            zbus::blocking::Proxy::new(&c, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus")
                .ok()
                .and_then(|p| p.call::<_, _, String>("GetAddress", &()).ok())
        });
    row("accessibility bus", a11y.is_some(), &a11y.unwrap_or_else(|| "no org.a11y.Bus - element steps are blind".into()));
    let portal = zbus::blocking::Connection::session().ok().and_then(|c| {
        zbus::blocking::Proxy::new(
            &c,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.GlobalShortcuts",
        )
        .ok()
        .and_then(|p| p.get_property::<u32>("version").ok())
    });
    row("GlobalShortcuts portal", portal.is_some(), &portal.map(|v| format!("version {v}")).unwrap_or_else(|| "absent - hotkeys need the input devices".into()));
    let sni = zbus::blocking::Connection::session().ok().and_then(|c| {
        zbus::blocking::Proxy::new(&c, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus")
            .ok()
            .and_then(|p| p.call::<_, _, bool>("NameHasOwner", &("org.kde.StatusNotifierWatcher",)).ok())
    });
    row("tray (StatusNotifierWatcher)", sni.unwrap_or(false), "a bar that shows tray icons");
    let recorder = ["gpu-screen-recorder", "wf-recorder"]
        .iter()
        .find(|t| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(t).is_file())));
    row("screen recorder", recorder.is_some(), recorder.unwrap_or(&"install gpu-screen-recorder or wf-recorder"));

    // ---- layout -----------------------------------------------------------
    let (rules, model, layout_names, variant, options) = super::inject::layout_names();
    println!(
        "\n  keyboard layout: '{layout_names}' variant '{variant}' options '{options}' rules '{rules}' model '{model}'; active: {}",
        super::platform::keyboard_layout()
    );
    println!("  data directory: {}", crate::paths::data_dir().display());
}

struct Probe;
impl wayland_client::Dispatch<wayland_client::protocol::wl_registry::WlRegistry, wayland_client::globals::GlobalListContents>
    for Probe
{
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_registry::WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &wayland_client::globals::GlobalListContents,
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
    }
}
