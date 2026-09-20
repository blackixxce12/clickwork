# Platforms

Which build to take, what it can do, and where it cannot.

Clickwork is one program on two platforms. The window, the editor, the script engine, the
picture search and the text reader are the same code; underneath them sit two platform
layers that are not the same at all, and Linux adds a second axis — the compositor — that
Windows does not have.

Everything with a number against it in this file was measured on the machine or the
session named, rather than reasoned about from a protocol description. Where something was
not measured it says so.

*[Русская версия → PLATFORMS_RU.md](PLATFORMS_RU.md)*

---

## The short answer

| You are on | Take | And read |
|---|---|---|
| Windows 10 or 11 | `Clickwork.msi`, or the Store | [README](README.md) |
| Arch, CachyOS, EndeavourOS | `clickwork-*.pkg.tar.zst` | [LINUX.md](LINUX.md) |
| Debian, Ubuntu, Mint | `clickwork_*.deb` | [LINUX.md](LINUX.md) |
| Fedora, RHEL, openSUSE | `clickwork-*.rpm` | [LINUX.md](LINUX.md) |
| Any other Linux | `clickwork-*.tar.gz` | *Installing by hand*, below |
| **Hyprland** | anything above | everything works |
| **sway, river, Wayfire, labwc, niri** | anything above | *Compositors*, below |
| **KDE Plasma** | a real package, **not** the tarball unpacked by hand | *Compositors*, below |
| **COSMIC** | — | most of it does not work yet |
| **GNOME** | — | recording only |

---

## Windows against Linux, feature by feature

**Legend.** ● works · ◐ works with a condition, named in the notes column · ○ does not

| | Windows 11 | Linux | Notes |
|---|:---:|:---:|---|
| **Recording** | | | |
| Mouse and keyboard | ● | ● | Windows uses low-level hooks; Linux reads the input devices directly, below the compositor |
| Where the pointer is | ● | ◐ | Linux: Hyprland only. Elsewhere nothing can say, so moves and clicks are not recorded by coordinate — build mouse steps from picture or element anchors |
| Input aimed at other applications | ● | ● | On Linux this is the whole point of reading evdev |
| Input the compositor swallowed | ○ | ● | evdev sits below the compositor and sees keys no application ever receives |
| **Playback** | | | |
| Mouse | ● | ◐ | Every compositor measured except COSMIC |
| Keyboard, recorded keys | ● | ● | On KDE this is *more* faithful: KWin interprets the key under the user's own layout, with none of the keymap mirroring the wlroots path needs |
| Typing arbitrary text | ● | ◐ | Not on KDE — see *The gaps*, below |
| Into an elevated window | ○ | — | Windows forbids it outright and there is no way round from inside the program. Linux has no equivalent barrier |
| **Seeing the screen** | | | |
| Picture search, OCR, pixel condition | ● | ◐ | Not on COSMIC or GNOME |
| "Nothing changed" shortcut | ● | ○ | Desktop Duplication reports damage, so an unchanged screen costs nothing. No Wayland capture protocol has that signal, so every look is a real copy |
| Second capture path to fall back to | ● | ○ | Windows has Desktop Duplication and GDI. Linux has one road per compositor |
| **Windows** | | | |
| List, focus, close | ● | ◐ | Hyprland, KWin and every wlroots compositor. Not COSMIC, not GNOME |
| Geometry, so anchors work | ● | ◐ | Hyprland and KWin only |
| The process behind a window | ● | ◐ | Hyprland and KWin only |
| Move, resize, centre | ● | ◐ | Hyprland only |
| Workspace isolation | ● | ◐ | Hyprland and KWin only. No wlroots protocol says which workspace a window is on |
| **The rest** | | | |
| UI element steps | ● | ◐ | UI Automation reaches Win32, WPF, UWP and Electron broadly; AT-SPI reaches what the toolkit chooses to expose, and needs a window origin, so it needs a backend that measures |
| Text recognition | ● | ● | Windows uses its own OCR; Linux opens Tesseract at run time |
| The see-through overlay | ● | ◐ | layer-shell. Present on every compositor measured except GNOME |
| Tray icon, notifications | ● | ● | StatusNotifierItem and the freedesktop notification bus |
| Clipboard without focus | ● | ◐ | data-control. Absent on GNOME |
| Hotkeys | ● | ◐ | See the next row |
| Hotkeys that *swallow* the key | ● | ◐ | A low-level hook consumes it. On Linux only a compositor bind does; Hyprland gets them, elsewhere the key also reaches whatever is in front |
| Screen recording while a macro runs | ● | ◐ | In-process on Windows. On Linux it shells out to `gpu-screen-recorder` or `wf-recorder`, and does nothing when neither is installed |
| Shutdown, reboot, sleep, log off | ● | ● | No `sudo` needed on a systemd desktop: polkit already allows the seated user |
| Window responsiveness figure | ● | ○ | It came from timing a message through the target's message loop. Wayland has no such loop |
| Mica and Acrylic | ● | ○ | The *Fluent* and *Glass* themes draw their own translucency instead |
| Self-running exported player | ● | ● | A `.exe` on Windows, an extensionless ELF on Linux — both work |
| AutoHotkey export | ● | ● | It is a Windows format either way; the file is produced on both |

### What each is made of

| | Windows | Linux |
|---|---|---|
| Playback | `SendInput` | `zwlr_virtual_pointer_v1` + `zwp_virtual_keyboard_v1`, or `org_kde_kwin_fake_input` on KDE |
| Recording | `WH_MOUSE_LL`, `WH_KEYBOARD_LL` | evdev, `/dev/input/event*` |
| Capture | Desktop Duplication, GDI | `zwlr_screencopy_v1`, or `org.kde.KWin.ScreenShot2` on KDE |
| Windows | Win32 window functions | Hyprland IPC, `org_kde_plasma_window_management`, or `zwlr_foreign_toplevel_management_v1` |
| Elements | UI Automation | AT-SPI2 over D-Bus |
| Text recognition | `Windows.Media.Ocr` | Tesseract 5, opened at run time |
| Overlay | layered window, GDI | `zwlr_layer_shell_v1` |
| Tray | `Shell_NotifyIcon` | StatusNotifierItem over D-Bus |
| Single instance | named mutex | a Unix socket in `$XDG_RUNTIME_DIR` |
| Settings | `%APPDATA%\Clickwork` | `~/.config/clickwork` |

### Tests

| | |
|---|---|
| Windows | **292** tests, on a real Windows runner in CI |
| Linux | **315** tests, plus fifteen capability checks against a live compositor |

The extra Linux tests are the platform layer; the fifteen checks are `--selftest session`,
which asks each capability twice — is the protocol advertised, does the code do it — and
fails when the two disagree.

---

## Which Linux package

All four carry the same program. They differ in what they install around it and how old a
distribution they will start on.

| | `.deb` | `.rpm` | `.pkg.tar.zst` | `.tar.gz` |
|---|---|---|---|---|
| For | Debian, Ubuntu, Mint | Fedora, RHEL, openSUSE | Arch, CachyOS, EndeavourOS | anything else |
| Install | `apt install ./clickwork_*.deb` | `dnf install ./clickwork-*.rpm` | `pacman -U clickwork-*.pkg.tar.zst` | unpack, then `./install.sh` |
| Dependencies resolved for you | ● | ● | ● | ○ |
| udev rule, so recording works | ● | ● | ● | ◐ via `install.sh` |
| Desktop file — **required on KDE** | ● | ● | ● | ◐ via `install.sh` |
| Icons, documentation | ● | ● | ● | ● |
| **glibc floor** | **2.35** | **2.35** | your Arch | **2.35** |
| Reaches | Debian 12+, Ubuntu 22.04+ | Fedora 36+, RHEL 9+ | rolling | any glibc 2.35+ |

**The glibc floor is the one number that decides whether a package starts at all.** glibc is
backward compatible and not forward compatible, so the floor is set by the machine a
release was built on rather than by the code. The `.deb`, `.rpm` and `.tar.gz` are all built
in a `debian:bookworm` container for exactly this reason: a build on a current rolling
distribution requires `GLIBC_2.43`, which almost nothing ships, and would install cleanly on
Debian 13, Ubuntu 24.04 and Fedora 41 and then refuse to start. CI compares every build
against `packaging/glibc-floor` and fails one that exceeds it.

### Installing by hand

The tarball is for distributions with no package of their own. It carries the binary, the
udev rule, the desktop file, the icons, the documentation and `install.sh`:

```bash
tar -xzf clickwork-*.tar.gz
cd clickwork-*
./install.sh          # into /usr/local, and it says what it is doing
```

**Unpacking it and running the binary from wherever it landed is enough on most
compositors and is not enough on KDE**, where the program must be installed for KWin to
hand it the window protocol and the screenshot interface at all. `install.sh` writes the
desktop file that does that.

### What is not shipped, and why

**Flatpak and Snap.** Not an oversight. A sandboxed Wayland client sees 40 of the 71
interfaces this session advertises, and the missing ones are the virtual pointer, the
virtual keyboard, screencopy, layer-shell and both data-control protocols — which is
playback, the picture search, the overlay and the clipboard. There is no manifest key that
asks for the plain socket. A Flatpak could be built by moving injection to `/dev/uinput`,
and the result would be a Clickwork with no overlay, slower capture, and still a host-side
permission step to perform — so it is not worth it yet.

**AppImage.** Possible and not built. Tesseract is opened at run time rather than linked,
so an AppImage would have to decide whether to carry it.

---

## Compositors

Every row was measured with the same binary and the same two commands, `--selftest session`
and `--doctor`, against that compositor. Four of these are new ground: nobody had run this
program on them before 2026-09-20.

| Compositor | Windows | Geometry | Workspaces | Capture | Playback | Overlay | Checks |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| **Hyprland** 0.56.2 | ● | ● | ● | ● 6.0 ms | ● | ● | 15/15 |
| **KDE Plasma** (KWin 6.7.5) | ● | ● | ● | ● **2.0 ms** | ● | ● | 15/15 |
| **sway** 1.12 | ● | ○ | ○ | ● 16 ms | ● | ● | 15/15 |
| **niri** 26.04 | ● | ○ | ○ | ● | ● | ● | 15/15 |
| **river** 0.4.8 | ● | ○ | ○ | ● 16 ms | ● | ● | 15/15 |
| **Wayfire** 0.11.0 | ● | ○ | ○ | ● 16 ms | ● | ● | 15/15 |
| **labwc** 0.20.2 | ● | ○ | ○ | ● | ● | ● | 15/15 |
| **COSMIC** 1.8.0 | ○ | ○ | ○ | ○ | ◐ keyboard | ● | 14/14 |
| **GNOME** (Mutter) | ○ | ○ | ○ | ○ | ○ | ○ | not measured |

Three things that table does not show.

**Capture is fastest on KDE**, which is the opposite of what the protocol list suggests:
KWin implements no screencopy at all and answers over D-Bus instead, and most of that cost
is a fixed round trip rather than pixels, so it is nearly flat from a small rectangle to a
whole screen. The 16 ms figures are software-rendered headless sessions and are a property
of the test rig rather than of the compositor.

**Hyprland is still the only one that answers everything.** KWin comes closest and cannot
find the pointer or move a window; the wlroots family lists and steers windows and measures
nothing. The program says which it is: `--doctor` prints both `can …` and `cannot …` for the
session you are actually in, and that line is the fastest answer to "why does this feature
do nothing here".

**COSMIC is missing a client, not a protocol.** cosmic-comp has dropped the wlroots
protocols in favour of the standardised successors — `ext_foreign_toplevel_list_v1`,
`ext_image_copy_capture_manager_v1` — which this program does not speak yet. So windows and
capture are absent there for a reason that can be fixed in this repository rather than in
the compositor.

### KDE Plasma, in particular

KDE works, with one condition: **the program has to be installed.** KWin keeps five Wayland
interfaces off the registry and hands them only to a client whose executable path resolves
to a desktop file asking for them, and refuses its screenshot interface the same way. A
binary run out of a build directory or an unpacked tarball gets neither — no window backend
and no capture at all — and `--doctor` says exactly that when it happens. Any of the four
packages installs the desktop file that fixes it.

---

## The gaps, stated plainly

**Typing text does not work on KDE.** Typing a *character* rather than pressing a key
*position* needs `keyboard_keysym`, which KWin added at interface version 6 and which the
protocol bindings this program is built against stop one version short of. Sending a key
position instead would type a different character on any layout but a plain `us`, so it
refuses and says so. Replaying recorded keys is unaffected and works normally.

**Recording by coordinate needs Hyprland.** Nothing else can say where the pointer is, so
neither mouse moves nor clicks are written down elsewhere — a click recorded at the corner
is not a lost click but a step that presses the corner on every playback. Record with the
keyboard and build the mouse steps from picture or element anchors, which is what they are
for.

**COSMIC has no window backend and no capture**, as above.

**GNOME can record and nothing else.** Mutter implements no foreign-toplevel protocol of
any kind, no screencopy, no virtual pointer, no virtual keyboard, no data-control and no
layer-shell. Recording works, because evdev sits below the compositor. Playback and the
picture search do not, and AT-SPI cannot fill the gap because without a window origin the
element coordinates are not degraded but wrong.

---

## How the numbers were got

| | |
|---|---|
| Hyprland | the development machine's own session, 71 Wayland interfaces |
| KWin 6.7.5 | packages unpacked into a scratch directory, `kwin_wayland --virtual`, nothing installed |
| sway 1.12 | `ci/headless-session.sh`, headless, no graphics device |
| sway 1.9 | the same script on a GitHub runner, 44 interfaces |
| niri, river, Wayfire, labwc, COSMIC | packages unpacked the same way, headless; niri has no headless mode and was measured nested inside a headless sway |
| GNOME | not measured here — from protocol sources and a prior audit |
| The packages | built in `debian:bookworm` by CI, and the `.deb` installed into a clean `debian:trixie` container and run there |

`--selftest session` prints all of this for the session you are in, and exits non-zero when
anything disagrees with anything else.

---

*[Русская версия → PLATFORMS_RU.md](PLATFORMS_RU.md)* · [README](README.md) ·
[LINUX.md](LINUX.md) · [BUILDING.md](BUILDING.md)
