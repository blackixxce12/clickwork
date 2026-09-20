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
        &if hypr { super::hypr::version() } else { "not a Hyprland session".into() },
    );
    let win = super::backend::backend();
    let can = win.answers();
    row(
        "window backend",
        can.any(),
        &if can.any() {
            format!("{} - can {}", win.name(), can.can())
        } else {
            "none - window steps, the window title and the cursor position have nothing to \
             answer them here, and hide-to-tray falls back to minimising"
                .to_string()
        },
    );
    // A KDE session that refused us looks exactly like a session with no window
    // protocol, and the difference is one line in a desktop file. Saying which
    // is the whole job of this screen.
    if super::kwin::refused() {
        println!(
            "    this is KWin, and it withheld org_kde_plasma_window_management: the running\n\
             \x20   binary's path matches no desktop file carrying X-KDE-Wayland-Interfaces,\n\
             \x20   which is expected when running from a build directory rather than from an\n\
             \x20   installed package"
        );
    }
    // The half that explains a feature going quiet. A backend that answers some of
    // the questions is the ordinary case now rather than the exception, and "it
    // does nothing and says nothing" is exactly what this screen exists to prevent.
    if can.any() && !can.cannot().is_empty() {
        println!("    cannot {}", can.cannot());
    }

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
    match super::platform::cursor_pos_checked() {
        Some((x, y)) => println!("    cursor now: {x},{y} (physical)"),
        None => println!("    cursor now: unknown - no backend to ask"),
    }
    // Surfaces on the top and overlay levels, which are the ones drawn over the
    // windows. Recording and playback hold while one of them covers a screen, so a
    // run that refuses to start says here what is in front of it.
    let above = super::hypr::layers();
    for l in above.iter().filter(|l| l.level >= super::hypr::LEVEL_TOP) {
        println!(
            "    over the windows on {}: {} - {}x{} at {},{}{}",
            l.monitor,
            l.namespace,
            l.w,
            l.h,
            l.x,
            l.y,
            if l.pid == std::process::id() as i64 { "  (ours)" } else { "" }
        );
    }
    println!(
        "    input right now: {}",
        match super::vdesk::blocking_layer() {
            Some(ns) => format!("going to `{ns}`, not to the windows"),
            None => "reaching the windows".to_string(),
        }
    );

    // ---- windows and workspaces -------------------------------------------
    // Naming the window in front proves the whole path - socket, JSON, geometry -
    // rather than only that the socket answered.
    // Through the backend rather than through Hyprland, because there is more
    // than one backend now and this row is about what the program can ask, not
    // about who it happens to be asking. The rectangle is printed only where the
    // backend measures one; a wlroots session names its windows perfectly well
    // and has no geometry to print, and printing zeroes there would be the exact
    // confusion the capability set was added to end.
    let all = win.windows();
    let windows = all.len();
    row(
        "window integration",
        can.windows && windows > 0,
        &if !can.windows {
            "nothing here lists windows: window steps, window anchors and hide-to-tray have \
             nothing to ask"
                .to_string()
        } else if let Some(c) = win.active_window() {
            let where_ = if can.geometry {
                let (x, y, w, h) = layout.rect_to_phys(c.rect);
                format!(" {w}x{h} at {x},{y} (physical)")
            } else {
                String::new()
            };
            format!(
                "{windows} windows; in front '{}' [{}]{where_}",
                crate::clip(&c.title, 40),
                c.class
            )
        } else {
            format!("{windows} windows, none of them in front")
        },
    );
    // A Hyprland fact reported as a Hyprland fact, the way `layers()` above is.
    // No portable protocol carries it, and `--doctor` is its only reader, so it
    // would be lost entirely if the generic row were the only one.
    if hypr && let Some(c) = super::hypr::active_window() && c.xwayland {
        println!("    the window in front is an XWayland client");
    }

    // This process has no window - `--doctor` answers and returns before the GUI
    // starts - so the running instance is looked up by app id, ordered the way
    // `own_window` orders it so an open handbook cannot answer for the main window.
    let mut mine: Vec<&super::backend::Window> =
        all.iter().filter(|c| c.class == crate::APP_ID).collect();
    mine.sort_by_key(|c| (c.title != crate::APP_TITLE, c.id.clone()));
    let visible = win.visible_workspaces();
    row(
        "workspace isolation",
        can.workspaces,
        &if !can.workspaces {
            // True of a wlroots session as much as of a bare one, and for a reason
            // worth stating rather than blaming on the compositor: no wlroots
            // protocol says which workspace a window is on. The workspaces can be
            // listed and even told apart from the ones in view; the window cannot
            // be placed among them, so the question this program asks has no
            // answer here.
            "nothing here says which workspace a window is on: recording and playback never \
             pause for a workspace switch"
                .to_string()
        } else {
            let seen =
                visible.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
            match mine.first() {
                Some(c) if c.workspace_name.starts_with("special:clickwork") => format!(
                    "in view: {seen}; ours is parked on {} - put away on purpose, which does not count as away",
                    c.workspace_name
                ),
                Some(c) if visible.contains(&c.workspace_id) => format!(
                    "in view: {seen}; ours is on {} - in sight, so nothing pauses",
                    c.workspace_name
                ),
                Some(c) => format!(
                    "in view: {seen}; ours is on {} - out of sight, so recording and playback pause",
                    c.workspace_name
                ),
                None => format!("in view: {seen}; no running instance to place"),
            }
        },
    );

    // ---- protocols --------------------------------------------------------
    let names = super::wl::globals();
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
