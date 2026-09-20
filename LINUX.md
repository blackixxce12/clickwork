# Clickwork on Linux

Clickwork runs natively on Linux under Wayland. The window, the editor, the script engine, the picture search and the OCR are the same program as on Windows; the platform layer underneath is new. This page says what that layer is made of, what it needs from the machine, and where Linux behaves differently.

The supported compositor is **Hyprland**: most of the platform is plain Wayland and runs on any wlroots-style compositor, and Hyprland's IPC socket is the only thing that answers every question the program asks about a window. Elsewhere the window half falls back to whatever the compositor's own protocols carry, and what you get depends on which those are. On **sway, river, Wayfire** and other wlroots compositors, `zwlr_foreign_toplevel_management_v1` lists windows, names the one in front, finds our own, focuses, closes and fullscreens them — and cannot measure one, name the process behind it, find the pointer or say which workspace it is on. On **KDE Plasma**, KWin's own protocols do rather more than that, and only for an *installed* copy of the program; see *KDE Plasma* below. **GNOME** is not supported: Mutter offers none of the protocols this needs. `--doctor` prints, for the session you are actually in, both what the backend can answer and what it cannot.

*[Русская версия → LINUX_RU.md](LINUX_RU.md)*

## Installing

**The installer** works out the distribution and the desktop, installs what the program needs from the distribution's own repositories (Rust, Tesseract with a language pack for your locale, the accessibility bus, the portal backend for your compositor), builds Clickwork, installs it, and ends with `--doctor`:

```bash
git clone https://github.com/blackixxce12/clickwork.git
cd clickwork
./install.sh            # --deps for dependencies only, --build for the rest only
```

On Arch-family systems it builds a real package and installs it with `pacman`; on Debian, Ubuntu, Fedora and openSUSE it installs under `/usr/local`.

**Arch Linux / CachyOS by hand** — the package is built from the tree, and depends on `tesseract`, `tesseract-data-eng`, `at-spi2-core`, `xdg-desktop-portal` and `libnotify`, so `pacman` pulls those in:

```bash
cd packaging/arch
makepkg -f
sudo pacman -U clickwork-*.pkg.tar.zst
sudo pacman -S tesseract-data-rus       # or any other language pack
```

**Any distribution by hand** — build from source with Rust 1.98 or newer:

```bash
cargo build --release
./target/release/clickwork --doctor
```

**On KDE Plasma the program has to be installed, not run out of the build directory.** KWin keeps five Wayland interfaces off the registry and grants them only to a client whose executable path resolves to an installed desktop file naming them, and it refuses `org.kde.KWin.ScreenShot2` over D-Bus on the same terms. The file the packages install, `io.github.blackixxce12.clickwork.desktop`, carries both keys — `install.sh` writes the same file under `/usr/local` with `Exec` rewritten to match:

```ini
X-KDE-Wayland-Interfaces=org_kde_plasma_window_management,org_kde_kwin_fake_input
X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2
```

The first list is separated by a **comma**, not by the semicolon a desktop file uses everywhere else: KWin reads the key through KService, which splits on KConfig's comma, so a semicolon-separated list arrives as one unrecognised string and the gate stays shut with no error anywhere. `Exec` must stay an absolute path, because that is what KWin matches against. Until the program is installed, a KDE session gives it no window backend, no capture and no playback — no window steps, no anchoring, no workspace isolation, no picture search, no text reader, no pixel condition and nothing to replay a macro with — and `--doctor` explains the first two refusals in so many words.

Other compositors: Sway, river and Wayfire offer the same playback, capture, overlay and OCR protocols as Hyprland, and answer questions about windows through `zwlr_foreign_toplevel_management_v1` — enough to list the windows, name the one in front and focus, close or fullscreen it; not enough to measure one, name the process behind it, find the pointer, say which workspace it is on, or move, resize or centre it. Verified on sway 1.12, on sway 1.9 in CI and in a Debian container; niri likewise, as far as the protocols it carries. Coming back from the tray works there through `zwlr_foreign_toplevel_handle_v1.activate(seat)`, measured pulling the window out of sway's scratchpad; hiding has no portable answer at all — no protocol puts a window out of sight and sway ignores minimise — so the tray reports what actually happened instead of relabelling itself after a call that did nothing. KDE Plasma answers more than that: `org_kde_plasma_window_management` lists windows, measures them, names processes and says which virtual desktop a window is on; `org_kde_kwin_fake_input` replays mouse and keyboard; and `org.kde.KWin.ScreenShot2` reads the screen back, because KWin implements no screencopy. Verified on KWin 6.7.5. What is missing there is typing text, recording a mouse move, and moving, resizing or centring a window. GNOME is the case that cannot: Mutter offers no foreign-toplevel protocol, no screencopy, no virtual pointer and no virtual keyboard, so a recording can be made and nothing can be replayed or searched for. `install.sh` says which case it found.

`--doctor` is the first thing to run. It lists every protocol, device and service the program depends on and says which are missing — and, because more than one kind of session answers questions about windows now and each answers a different subset, it names the window backend and prints both halves of what that backend can do:

```
  ✔ Wayland display             wayland-1
  ✔ Hyprland IPC                Hyprland 0.56.2 ...
  ✔ window backend              Hyprland - can list the windows, measure them, name the process behind one, find the pointer, ...
  ✔ output eDP-1                logical 1600x900 at 0,0  scale 1.60  physical 2560x1440 at 0,0  (focused)
  ✔ window integration          7 windows; in front 'Brave' [brave] 2560x1400 at 0,40 (physical)
  ✔ workspace isolation         in view: 1; ours is on 1 - in sight, so nothing pauses
  ✔ mouse playback              zwlr_virtual_pointer_v1
  ✔ keyboard playback           zwp_virtual_keyboard_v1
  ✔ zwlr_screencopy_v1          picture search, OCR, pixel condition
  ✔ zwlr_layer_shell_v1         the see-through overlay
  ✔ screen capture              320x240 in 4.1 ms, then 2.30 ms each
  ✖ input devices readable      0 of 24 - recording and evdev hotkeys will not work; ...
  ✔ tesseract                   languages: eng, rus; default 'rus+eng'
  ...
```

That is a Hyprland session, where every question about a window has an answer. On anything else the row to read is **`window backend`**: it names the backend and lists what it can answer, and a line under it lists what it cannot.

```
  ✔ window backend              KWin - can list the windows, measure them, name the process behind one, say which workspace one is on, focus, close and fullscreen one
    cannot find the pointer, move, resize, centre and park one
```

Read the `cannot` line first when a window step, an anchor or the workspace pause does nothing on your machine: a backend that cannot answer something says so and the step is refused, rather than being handed a plausible-looking zero.

The second half is the one that explains a feature going quiet. On sway the same row reads:

```
  ✔ window backend              wlroots - can list the windows, focus, close and fullscreen one
    cannot measure them, name the process behind one, find the pointer, say which workspace one is on, move, resize, centre and park one
```

## KDE Plasma

KDE is supported, with one condition and one gap.

**The program has to be installed.** KWin keeps five Wayland interfaces off the registry and hands them only to a client whose executable path resolves to a `.desktop` file that asks for them by name. Two keys do it, both in `packaging/arch/io.github.blackixxce12.clickwork.desktop` and shipped by all three packages:

```ini
X-KDE-Wayland-Interfaces=org_kde_plasma_window_management,org_kde_kwin_fake_input
X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2
```

The first list is **comma-separated** — the one place a desktop file does not use a semicolon — and `Exec` has to stay an absolute path, because that is what KWin matches against. A binary run out of a build directory matches no desktop file, so KWin withholds both: **no window backend and no capture at all**, which takes the picture search, the text reader and the pixel condition with it. That is indistinguishable from a compositor implementing nothing, so `--doctor` says which it is:

```
    this is KWin, and it withheld org_kde_plasma_window_management: the running
    binary's path matches no desktop file carrying X-KDE-Wayland-Interfaces,
    which is expected when running from a build directory rather than from an
    installed package
    KWin refused the capture: this needs an installed desktop file carrying
    X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2, so a binary run
    from a build directory has no picture search, no text reader and no pixel
    condition on this compositor
```

Install it — `./install.sh`, or `makepkg` and `pacman -U` — and both come back.

**What an installed copy gets.** Windows come from `org_kde_plasma_window_management`: it lists them, **measures** them, **names the process** behind one and says which **virtual desktop** it is on, and focuses, closes and fullscreens. It cannot find the pointer, and cannot move, resize, centre or park a window — so hiding to the tray minimises rather than parking the window out of the way; see *Hide to tray*. Capture goes through `org.kde.KWin.ScreenShot2` over D-Bus, because KWin implements no screencopy of any kind, and it is the quicker road: about 2 ms a frame, and nearly flat from a small rectangle to a whole screen, because most of the cost is a fixed D-Bus round trip rather than pixels. Playback goes through `org_kde_kwin_fake_input`, because KWin implements neither wlroots input protocol; the mouse and recorded keys both play. `zwlr_layer_shell_v1` is there, so the overlay has what it needs. The window backend, the capture and the playback were measured on KWin 6.7.5.

**The gap is typing text**, and only typing — see *Keyboard layouts*.

## Permissions

Wayland shows a program only the input aimed at its own windows. To **record** what you do in other applications, and to react to a **hotkey** while a game is in front, Clickwork reads the input devices themselves, `/dev/input/event*`, exactly as the compositor does. That takes read access, which a normal user does not have.

Two ways to grant it; either is enough:

- **The udev rule** the package installs, `/usr/lib/udev/rules.d/70-clickwork-input.rules`. It tags input devices `uaccess`, which gives the user seated at the machine read access, the same mechanism that makes game controllers readable. Reload it once (`sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=input`) or log out and in.
- **The `input` group**: `sudo usermod -aG input $USER`, then log out and in again.

Without either, everything else still works: playback, scripts, picture search, OCR, the scheduler, the tray. The program says so once in its log and in `--doctor`, and keeps checking every few seconds, so the fix takes effect without a restart. Playback does *not* need the permission: synthetic input goes through the compositor.

## What each feature is made of

| Feature | Windows | Linux |
|---|---|---|
| Mouse and keyboard playback | `SendInput` | `zwlr_virtual_pointer_v1` and `zwp_virtual_keyboard_v1`, a virtual mouse and keyboard the compositor accepts from any client; on KWin, which implements neither, `org_kde_kwin_fake_input` — a recorded key replays more faithfully there, under the user's own keymap and active group, while typing a *character* needs `keyboard_keysym` from interface version 6 and is refused |
| Recording, hotkeys | `WH_MOUSE_LL`, `WH_KEYBOARD_LL`, `RegisterHotKey` | evdev (`/dev/input`), with the pointer position asked from the window backend — Hyprland's socket is the only one that answers it, and neither KWin's protocol nor the wlroots one exposes a cursor at all, so where nobody can say where the pointer went the move is not recorded rather than recorded at the origin |
| Screen capture (picture search, OCR, pixel condition) | Desktop Duplication, GDI | `zwlr_screencopy_v1`, a rectangle of one output copied into shared memory; on KDE `org.kde.KWin.ScreenShot2` over D-Bus, because KWin implements no screencopy of any kind — and the faster of the two, at about 2 ms a frame on KWin 6.7.5 against 6 ms through screencopy on this Hyprland, nearly flat from one pixel to a whole screen |
| Window titles, rectangles, focus, move/resize/close | Win32 window functions | Hyprland IPC (`hyprctl -j clients` and the Lua dispatchers), which answers all of it; on KDE `org_kde_plasma_window_management` — titles, rectangles, the process, the virtual desktop, focus, close and fullscreen, but no move or resize; on wlroots compositors `zwlr_foreign_toplevel_management_v1` — titles, focus, close and fullscreen only. Each backend declares what it can answer and `--doctor` prints both halves |
| The see-through overlay and HUD | colour-keyed layered window, GDI | a `zwlr_layer_shell_v1` overlay surface per output, drawn in software, click-through |
| Text recognition | `Windows.Media.Ocr` | **Tesseract 5**, loaded at run time; language packs from the distribution |
| UI element steps ("Press element", "Wait for element") | UI Automation | AT-SPI2, the accessibility bus that GTK, Qt and Electron applications describe their controls on |
| Tray icon | `Shell_NotifyIcon` | StatusNotifierItem over D-Bus (waybar, Noctalia, KDE, GNOME with the extension) |
| Notifications | tray balloon | `org.freedesktop.Notifications` (mako, dunst, swaync…) |
| Clipboard | Win32 clipboard | data-control protocol, works without focus |
| Virtual-desktop isolation | `IVirtualDesktopManager` | Hyprland workspaces, and KWin's virtual desktops: recording and playback pause while the window's workspace is not on screen — and on Hyprland also while a layer-shell surface covers the screen, which is the only place the surfaces in front can be asked about. On wlroots compositors nothing can say which workspace a window is on, so the pause is not attempted there — a real id compared against a defaulted zero would be a pause nobody could lift |
| Screen recording while a macro runs | Media Foundation | `gpu-screen-recorder` or `wf-recorder`, whichever is installed |
| Shutdown / reboot / sleep / hibernate / log off | `InitiateSystemShutdownEx` | `systemctl poweroff|reboot|suspend|hibernate`, `loginctl terminate-session` |
| Single instance | named mutex | a Unix socket in `$XDG_RUNTIME_DIR`, which the command line also talks to |
| Settings and macros | `%APPDATA%\Clickwork` | `~/.config/clickwork` (or `$XDG_CONFIG_HOME/clickwork`) |

## Coordinates and scale

Hyprland lays windows out and moves the cursor in *logical* pixels: a 2560-pixel-wide screen at scale 1.6 is 1600 logical pixels across. Pictures, on the other hand, are physical: a button is a button at the panel's resolution, and a template snipped from a screenshot is physical too.

Clickwork works in **physical pixels everywhere**, as it does on Windows, and converts at the edges: the cursor position it records is the compositor's logical position times the monitor's scale, and the position it replays is divided back. Where the pointer *is*, though, is a question only Hyprland's socket answers — the wlroots and KWin backends cannot — and a move is written down only when something can say where the pointer went, so elsewhere mouse moves are not recorded at all and steps that read the pointer read the top-left corner. The log says so once, and `--doctor` says which case you are in. A recording made at scale 1.6 carries `dpi: 154` in its session note, so the pre-flight check can say *"150 % → now 100 %"* if the macro moves to another machine.

Templates should therefore be cut at physical resolution. Every screenshot tool does that by default (`grim`, `hyprshot`, `slurp | grim -g`); if yours writes logical-size images, the *other scales* option in the picture panel still finds them.

## Keyboard layouts

A recorded keystroke is a scan code — the physical key — and it is replayed as that key. Through `zwp_virtual_keyboard_v1` — Hyprland, sway, river — the virtual keyboard carries the **same layout Hyprland gave your real keyboard** (`input:kb_layout`, variant and options, group included), compiled afresh and its group followed once a second, and where there is no Hyprland to ask, the layout comes from `XKB_DEFAULT_LAYOUT` and its family instead; on KDE the key goes through `org_kde_kwin_fake_input`, which hands KWin the evdev code and lets KWin read it under your own keymap and your active group — the more faithful of the two, and it needs none of the copying. So `hello` recorded on an English layout types `руддщ` after you switch to Russian, exactly as on Windows, and the session note's layout line is there to warn you.

Text that is *typed* rather than replayed — a script's `Type text`, an expander entry — goes through a generated keymap in which every needed character has a key of its own, so any Unicode types correctly regardless of layout.

**On KDE it does not, and this is the one real gap there.** Typing a *character* rather than pressing a key *position* needs `keyboard_keysym`, which KWin added at version 6 of `org_kde_kwin_fake_input` and which the protocol bindings this program is built against stop one version short of. Sending a key position instead would type a different character on any layout but a plain `us`, so the program refuses rather than typing something else, says so once in its log, and says so in `--doctor`'s `keyboard playback` row. **Replaying a recorded key is unaffected** — and is more faithful on KDE than on wlroots: KWin reads the evdev code under your own keymap and your currently active group, with none of the keymap compiling, modifier mirroring and once-a-second group polling the wlroots road needs.

## Hotkeys and compositor keybinds

On Hyprland every hotkey slot is registered as a **compositor keybind** the moment the program starts, through the IPC socket:

```lua
hl.bind("F6", hl.dsp.exec_cmd("'/usr/bin/clickwork' --cmd record"), { description = "clickwork:record" })
```

A compositor bind consumes the key before the application in front sees it, which is the property the evdev path cannot offer: F6 starts a recording and Brave never opens its memory-saver popup. The binds follow the hotkey settings (change one in the window and the bind changes), are removed while you press a new key to bind, and are removed again on exit. A combo you already bind yourself in `hyprland.conf` is left to you - the log says so - and that slot keeps working through the input devices instead. `hyprctl binds` lists them with their `clickwork:` descriptions.

Elsewhere, and as a fallback, hotkeys come from evdev the moment the input devices are readable, in every application, including full-screen games; there the key also reaches the application in front. The defaults are function keys nothing minds.

Two more roads, for a machine that cannot read the devices or a user who prefers to write the binds by hand:

- **The command line talks to the running instance.** `clickwork --stop`, `--record`, `--play-toggle`, `--pause`, `--faster`, `--slower`, `--skip`, `--show`, `--hide`, `--quit`, `--status`. In `hyprland.conf` (0.56 Lua syntax):

  ```lua
  hl.bind("F9", hl.dsp.exec_cmd("clickwork --stop"))
  hl.bind("SUPER + F7", hl.dsp.exec_cmd("clickwork --play-toggle"))
  ```

- **The GlobalShortcuts portal.** Of the seven actions (`record`, `play`, `stop`, `pause`, `faster`, `slower`, `skip`), Clickwork registers with `xdg-desktop-portal` only the ones the compositor has not already bound itself — so on Hyprland, where it binds all seven, usually none, and the log says as much. `hyprctl globalshortcuts` lists what was actually registered, with the application id the portal assigned; `hl.bind("F9", hl.dsp.global("io.github.blackixxce12.clickwork:stop"))` binds one of those, so check that list first.

## Text recognition

Tesseract is not linked in; it is opened at run time (`libtesseract.so.5`), so a machine without it still runs everything else and a `Read text` step says *install tesseract*. Language data is read from `/usr/share/tessdata` or `$TESSDATA_PREFIX`. The language list in the OCR panel is whatever `tesseract-data-*` packages are installed; leaving it on *system* reads in the desktop's language plus English (`rus+eng` on a Russian desktop), which is the right default for a game in English on a Russian system.

The five preparation profiles (`None`, `Ui`, `Small`, `Game`, `Digits`, `Auto`) work as on Windows, and matter more here: Tesseract was trained on documents too, and a pale HUD number over artwork reads far better after the *Game* profile has cut it to black and white.

`CLICKWORK_TESS_PSM=<n>` overrides the page-segmentation mode (7 single line, 6 block, 11 sparse) if the automatic choice by region shape is wrong for your screen.

## Hide to tray

winit cannot hide a Wayland window, so on Hyprland the window is **moved to a special workspace** (`special:clickwork`) and fetched back to the workspace in view when you click the tray icon or run `clickwork --show`. That road needs a backend that can put a window where it is not, so it is Hyprland's alone.

Elsewhere the two directions are not the same road, and only one of them is reliable. **Coming back** goes through the backend's own activate — `zwlr_foreign_toplevel_handle_v1.activate(seat)` on wlroots, measured pulling the window out of sway's scratchpad from a process holding no focus at all, and `set_state` on KWin. Where no backend can steer a window, nothing brings it back, because winit's `focus_window` on Wayland is an empty function and un-minimising reaches a winit that logs *Unminimizing is ignored on Wayland*; the log then says the window could not be shown and **leaves the tray menu as it was**, rather than offering *Hide window* for a window nobody can see. **Hiding has no portable answer at all**: no Wayland protocol puts a window out of sight, so all that is left is asking the compositor to minimise, which is advisory — sway ignores it outright, measured — and nothing from in here can tell whether yours honoured it. So off Hyprland hiding may simply do nothing and leave the window in front of you, with the tray menu offering *Show window* beside it; that is the compositor declining, not the tray.

Closing the window (the X, or your compositor's close bind) hides it the same way while **Close to tray** is on - the program keeps running, which is what a scheduled macro needs. The close is cancelled whichever way the hiding goes, so on a compositor that ignores minimise the X leaves the window where it is instead of closing it: that is the hiding above failing, not the close button. The first time it happens in a session a notification says where the window went. To quit for real: the tray menu's *Exit*, or `clickwork --quit`.

## What is different, honestly

- **Not every compositor answers every question.** There are three window backends, and each declares what it can answer — windows, geometry, process, pointer, workspaces, steering, placing — so that half an answer is never taken for a measured one. **Hyprland**, through its IPC socket, answers all seven. **wlroots** (sway, river, Wayfire, niri) lists windows, names the one in front, finds our own, focuses, closes and fullscreens; it cannot measure a window, name the process behind it, find the pointer or say which workspace it is on. **KWin** lists windows, measures them, names the process and says which virtual desktop one is on; it cannot find the pointer or move a window. Capture on KDE goes through `org.kde.KWin.ScreenShot2`, because KWin implements no screencopy of any kind, and playback through `org_kde_kwin_fake_input`, because it implements neither wlroots input protocol; KWin carries `zwlr_layer_shell_v1` too, so the overlay has a surface to draw on there. GNOME has none of it — recording works, because evdev sits below the compositor; playback and the picture search do not. `--doctor` prints both halves, "can …" and "cannot …".
- **On KDE the program must be installed.** KWin keeps those interfaces off the registry and hands them only to a client whose executable path resolves to a `.desktop` file naming them, and refuses `ScreenShot2` the same way. The shipped desktop file carries `X-KDE-Wayland-Interfaces=org_kde_plasma_window_management,org_kde_kwin_fake_input` — **comma-separated**, because KWin reads the key through KConfig, and not the semicolon a desktop file uses everywhere else — and `X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2`. Run the binary straight out of `target/release` on a KDE session instead and there is no window backend and no capture at all; `--doctor` names that refusal and says what it needs.
- **Typing text does not work on KDE** (above). Replaying a recorded key does, and more faithfully than on wlroots.
- **Recording needs a permission** (above). Windows needs none.
- **Recording by coordinate needs a compositor that can say where the pointer is**, and only Hyprland can. A mouse move is not written down without one, and neither is a click — a click recorded at the top-left corner is not a lost click but a step that presses the corner on every playback, and the picture anchor cut around it is a crop of the corner too. So on KDE, on sway and on anything else, record with the keyboard and build the mouse steps from picture or element anchors, which is what they are for. `--doctor`'s `cannot find the pointer` is the line that says this is the case.
- **Hotkeys are not swallowed.** The key also reaches the application in front.
- **No "unchanged frame" shortcut.** Desktop Duplication tells the Windows build when the screen has not changed, and a polling script then costs nothing. Neither screencopy nor KDE's `org.kde.KWin.ScreenShot2` has such a signal, so every look is a real copy — a few milliseconds for a region, more for a full screen. Keep search areas small. The cost is not the same on both roads: a small rectangle measured about 2 ms through KWin's ScreenShot2 against about 6 ms through screencopy on this Hyprland, and the ScreenShot2 figure barely grows with the rectangle, because most of it is a fixed D-Bus round trip rather than pixels.
- **Window responsiveness (the FPS-like figure) is not measured.** It came from timing a message through the target window's message loop; there is no such loop to time on Wayland. The frame guard still works with a configured frame time.
- **"Maximise" is fullscreen.** Hyprland tiles; the closest thing to a maximised window is a fullscreen one. "Minimise" parks the window on `special:clickwork_min`; "Restore" brings it back. The parking is Hyprland's alone: the wlroots and KWin backends focus, close and fullscreen a window and cannot move, resize or centre one, so those steps fail outright rather than doing half of the job, and the log and `--doctor` name what this session cannot do.
- **Mica and Acrylic do not exist.** The *Fluent* and *Glass* themes draw their own translucency and look fine; the system backdrop is simply not there.
- **Exported players are Linux executables** without an extension, and the AutoHotkey export is still AutoHotkey — a Windows format.
- **Elevated windows are not a thing**, and neither is anti-cheat's view of `SendInput`; a game reading evdev directly (rare) sees no virtual devices at all.

## Files

Everything lives in `~/.config/clickwork`: `config.json`, `macro.json`, `profiles/`, `templates/`, `lang/`, `logs/clickwork.log.*`. Set `CLICKWORK_PORTABLE=1` to keep everything next to the executable instead, as the Windows build does when the folder is writable.
