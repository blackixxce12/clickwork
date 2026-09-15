# Clickwork on Linux

Clickwork runs natively on Linux under Wayland. The window, the editor, the script engine, the picture search and the OCR are the same program as on Windows; the platform layer underneath is new. This page says what that layer is made of, what it needs from the machine, and where Linux behaves differently.

The supported compositor is **Hyprland**. Most of the platform is plain Wayland and works on any wlroots-style compositor (Sway, river, niri…), but window lookup, anchoring, workspace isolation and hide-to-tray are answered by Hyprland's IPC socket, and those features are limited elsewhere. GNOME and KDE do not offer the protocols this needs (virtual pointer, screencopy, layer-shell) and are not supported.

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

Other compositors: Sway, river and Wayfire offer the same protocols as Hyprland and get playback, capture, the overlay and OCR, but window lookup and hide-to-tray go through Hyprland's socket and are limited there; niri likewise. KDE Plasma and GNOME offer neither a virtual pointer nor screencopy, so on those the window runs but playback and the picture search cannot. `install.sh` says which case it found.

`--doctor` is the first thing to run. It lists every protocol, device and service the program depends on and says which are missing:

```
  ✔ Wayland display             wayland-1
  ✔ Hyprland IPC                Hyprland 0.56.2 ...
  ✔ output eDP-1                logical 1600x900 at 0,0  scale 1.60  physical 2560x1440 at 0,0  (focused)
  ✔ zwlr_virtual_pointer_v1     mouse playback
  ✔ zwp_virtual_keyboard_v1     keyboard playback
  ✔ zwlr_screencopy_v1          picture search, OCR, pixel condition
  ✔ zwlr_layer_shell_v1         the see-through overlay
  ✔ screen capture              320x240 in 4.1 ms, then 2.30 ms each
  ✖ input devices readable      0 of 24 - recording and evdev hotkeys will not work; ...
  ✔ tesseract                   languages: eng, rus; default 'rus+eng'
  ...
```

## Permissions

Wayland shows a program only the input aimed at its own windows. To **record** what you do in other applications, and to react to a **hotkey** while a game is in front, Clickwork reads the input devices themselves, `/dev/input/event*`, exactly as the compositor does. That takes read access, which a normal user does not have.

Two ways to grant it; either is enough:

- **The udev rule** the package installs, `/usr/lib/udev/rules.d/70-clickwork-input.rules`. It tags input devices `uaccess`, which gives the user seated at the machine read access, the same mechanism that makes game controllers readable. Reload it once (`sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=input`) or log out and in.
- **The `input` group**: `sudo usermod -aG input $USER`, then log out and in again.

Without either, everything else still works: playback, scripts, picture search, OCR, the scheduler, the tray. The program says so once in its log and in `--doctor`, and keeps checking every few seconds, so the fix takes effect without a restart. Playback does *not* need the permission: synthetic input goes through the compositor.

## What each feature is made of

| Feature | Windows | Linux |
|---|---|---|
| Mouse and keyboard playback | `SendInput` | `zwlr_virtual_pointer_v1` and `zwp_virtual_keyboard_v1`, a virtual mouse and keyboard the compositor accepts from any client |
| Recording, hotkeys | `WH_MOUSE_LL`, `WH_KEYBOARD_LL`, `RegisterHotKey` | evdev (`/dev/input`), with the pointer position asked from Hyprland |
| Screen capture (picture search, OCR, pixel condition) | Desktop Duplication, GDI | `zwlr_screencopy_v1`, a rectangle of one output copied into shared memory |
| Window titles, rectangles, focus, move/resize/close | Win32 window functions | Hyprland IPC (`hyprctl -j clients` and the Lua dispatchers) |
| The see-through overlay and HUD | colour-keyed layered window, GDI | a `zwlr_layer_shell_v1` overlay surface per output, drawn in software, click-through |
| Text recognition | `Windows.Media.Ocr` | **Tesseract 5**, loaded at run time; language packs from the distribution |
| UI element steps ("Press element", "Wait for element") | UI Automation | AT-SPI2, the accessibility bus that GTK, Qt and Electron applications describe their controls on |
| Tray icon | `Shell_NotifyIcon` | StatusNotifierItem over D-Bus (waybar, Noctalia, KDE, GNOME with the extension) |
| Notifications | tray balloon | `org.freedesktop.Notifications` (mako, dunst, swaync…) |
| Clipboard | Win32 clipboard | data-control protocol, works without focus |
| Virtual-desktop isolation | `IVirtualDesktopManager` | Hyprland workspaces: recording and playback pause while the window's workspace is not on screen, or while a layer-shell surface covers it |
| Screen recording while a macro runs | Media Foundation | `gpu-screen-recorder` or `wf-recorder`, whichever is installed |
| Shutdown / reboot / sleep / hibernate / log off | `InitiateSystemShutdownEx` | `systemctl poweroff|reboot|suspend|hibernate`, `loginctl terminate-session` |
| Single instance | named mutex | a Unix socket in `$XDG_RUNTIME_DIR`, which the command line also talks to |
| Settings and macros | `%APPDATA%\Clickwork` | `~/.config/clickwork` (or `$XDG_CONFIG_HOME/clickwork`) |

## Coordinates and scale

Hyprland lays windows out and moves the cursor in *logical* pixels: a 2560-pixel-wide screen at scale 1.6 is 1600 logical pixels across. Pictures, on the other hand, are physical: a button is a button at the panel's resolution, and a template snipped from a screenshot is physical too.

Clickwork works in **physical pixels everywhere**, as it does on Windows, and converts at the edges: the cursor position it records is the compositor's logical position times the monitor's scale, and the position it replays is divided back. A recording made at scale 1.6 carries `dpi: 154` in its session note, so the pre-flight check can say *"150 % → now 100 %"* if the macro moves to another machine.

Templates should therefore be cut at physical resolution. Every screenshot tool does that by default (`grim`, `hyprshot`, `slurp | grim -g`); if yours writes logical-size images, the *other scales* option in the picture panel still finds them.

## Keyboard layouts

A recorded keystroke is a scan code — the physical key — and it is replayed as that key through a virtual keyboard carrying the **same layout Hyprland gave your real keyboard** (`input:kb_layout`, variant and options, group included). So `hello` recorded on an English layout types `руддщ` after you switch to Russian, exactly as on Windows, and the session note's layout line is there to warn you.

Text that is *typed* rather than replayed — a script's `Type text`, an expander entry — goes through a generated keymap in which every needed character has a key of its own, so any Unicode types correctly regardless of layout.

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

winit cannot hide a Wayland window, so on Hyprland the window is **moved to a special workspace** (`special:clickwork`) and fetched back to the workspace in view when you click the tray icon or run `clickwork --show`. On other compositors it is minimised instead, which is the most a Wayland client may ask for.

Closing the window (the X, or your compositor's close bind) hides it the same way while **Close to tray** is on - the program keeps running, which is what a scheduled macro needs. The first time it happens in a session a notification says where the window went. To quit for real: the tray menu's *Exit*, or `clickwork --quit`.

## What is different, honestly

- **Recording needs a permission** (above). Windows needs none.
- **Hotkeys are not swallowed.** The key also reaches the application in front.
- **No "unchanged frame" shortcut.** Desktop Duplication tells the Windows build when the screen has not changed, and a polling script then costs nothing. Screencopy has no such signal, so every look is a real copy — a few milliseconds for a region, more for a full screen. Keep search areas small.
- **Window responsiveness (the FPS-like figure) is not measured.** It came from timing a message through the target window's message loop; there is no such loop to time on Wayland. The frame guard still works with a configured frame time.
- **"Maximise" is fullscreen.** Hyprland tiles; the closest thing to a maximised window is a fullscreen one. "Minimise" parks the window on `special:clickwork_min`; "Restore" brings it back.
- **Mica and Acrylic do not exist.** The *Fluent* and *Glass* themes draw their own translucency and look fine; the system backdrop is simply not there.
- **Exported players are Linux executables** without an extension, and the AutoHotkey export is still AutoHotkey — a Windows format.
- **Elevated windows are not a thing**, and neither is anti-cheat's view of `SendInput`; a game reading evdev directly (rare) sees no virtual devices at all.

## Files

Everything lives in `~/.config/clickwork`: `config.json`, `macro.json`, `profiles/`, `templates/`, `lang/`, `logs/clickwork.log.*`. Set `CLICKWORK_PORTABLE=1` to keep everything next to the executable instead, as the Windows build does when the folder is writable.
