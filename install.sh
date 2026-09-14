#!/usr/bin/env bash
# Clickwork installer for Linux.
#
# Works out the distribution and the desktop it is running on, installs what the
# program needs from the distribution's own repositories, builds Clickwork from
# this source tree, installs it, and finishes with `clickwork --doctor` so you can
# see what works on this machine.
#
#   ./install.sh            everything, asking for sudo when it needs it
#   ./install.sh --deps     dependencies only
#   ./install.sh --build    build and install only (dependencies already there)
#   ./install.sh --no-input skip adding you to the `input` group
#
# Arch-family systems get a real package through makepkg (removable with pacman);
# everything else is installed under /usr/local.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
do_deps=1; do_build=1; add_input=1
for a in "$@"; do
  case "$a" in
    --deps) do_build=0 ;;
    --build) do_deps=0 ;;
    --no-input) add_input=0 ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done

say()  { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
note() { printf '\033[1;33m  ->\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# ---- what is this machine ---------------------------------------------------

. /etc/os-release 2>/dev/null || true
distro="${ID:-unknown}"; like="${ID_LIKE:-}"
if command -v pacman >/dev/null; then pm=pacman
elif command -v apt-get >/dev/null; then pm=apt
elif command -v dnf >/dev/null; then pm=dnf
elif command -v zypper >/dev/null; then pm=zypper
else pm=none; fi

desktop="${XDG_CURRENT_DESKTOP:-}"
session="${XDG_SESSION_TYPE:-}"
comp=other
if [ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ] || [[ "$desktop" == *Hyprland* ]]; then comp=hyprland
elif [ -n "${SWAYSOCK:-}" ] || [[ "$desktop" == *sway* ]]; then comp=sway
elif [ -n "${NIRI_SOCKET:-}" ] || [[ "$desktop" == *niri* ]]; then comp=niri
elif [[ "$desktop" == *KDE* ]]; then comp=kde
elif [[ "$desktop" == *GNOME* ]]; then comp=gnome
elif [[ "$desktop" == *river* ]]; then comp=river
elif [[ "$desktop" == *Wayfire* || "$desktop" == *wayfire* ]]; then comp=wayfire
fi

say "Distribution: ${PRETTY_NAME:-$distro} (package manager: $pm)"
say "Session: ${session:-?} on ${desktop:-?} (detected: $comp)"
if [ "$session" != "wayland" ]; then
  note "Clickwork is a Wayland program; on an X11 session it will not be able to play, record or capture."
fi
case "$comp" in
  hyprland) note "Hyprland: everything is supported." ;;
  sway|river|wayfire) note "$comp: playback, capture, overlay and OCR work; window lookup and hide-to-tray need Hyprland's IPC and are limited." ;;
  niri) note "niri: playback, capture and OCR work; window lookup is limited." ;;
  kde|gnome) note "$comp offers neither wlr-virtual-pointer nor wlr-screencopy: the window runs, but playback and the picture search cannot. See LINUX.md." ;;
  *) note "Unknown compositor: run clickwork --doctor afterwards to see what it offers." ;;
esac

# ---- dependencies -------------------------------------------------------------

if [ "$do_deps" = 1 ]; then
  say "Installing dependencies"
  case "$pm" in
    pacman)
      pkgs=(base-devel rust cargo wayland libxkbcommon libglvnd fontconfig dbus tesseract tesseract-data-eng at-spi2-core xdg-desktop-portal libnotify wl-clipboard)
      case "$comp" in
        hyprland) pkgs+=(hyprland xdg-desktop-portal-hyprland) ;;
        sway|river|wayfire) pkgs+=(xdg-desktop-portal-wlr) ;;
        niri) pkgs+=(xdg-desktop-portal-gnome) ;;
        kde) pkgs+=(xdg-desktop-portal-kde) ;;
        gnome) pkgs+=(xdg-desktop-portal-gnome) ;;
      esac
      # A language pack for the desktop's language, when the repositories have it.
      lang="${LANG%%_*}"; lang="${lang%%.*}"
      case "$lang" in
        ru) pkgs+=(tesseract-data-rus) ;; uk) pkgs+=(tesseract-data-ukr) ;; pt) pkgs+=(tesseract-data-por) ;;
        es) pkgs+=(tesseract-data-spa) ;; de) pkgs+=(tesseract-data-deu) ;; fr) pkgs+=(tesseract-data-fra) ;;
        zh) pkgs+=(tesseract-data-chi_sim) ;; ja) pkgs+=(tesseract-data-jpn) ;;
      esac
      sudo pacman -S --needed --noconfirm "${pkgs[@]}"
      ;;
    apt)
      pkgs=(build-essential cargo rustc pkg-config libwayland-dev libxkbcommon-dev libgl1 fontconfig dbus tesseract-ocr tesseract-ocr-eng at-spi2-core xdg-desktop-portal libnotify-bin wl-clipboard)
      case "$comp" in
        hyprland) pkgs+=(xdg-desktop-portal-hyprland) ;;
        sway|river|wayfire) pkgs+=(xdg-desktop-portal-wlr) ;;
        niri|gnome) pkgs+=(xdg-desktop-portal-gnome) ;;
        kde) pkgs+=(xdg-desktop-portal-kde) ;;
      esac
      lang="${LANG%%_*}"; lang="${lang%%.*}"
      case "$lang" in
        ru) pkgs+=(tesseract-ocr-rus) ;; uk) pkgs+=(tesseract-ocr-ukr) ;; pt) pkgs+=(tesseract-ocr-por) ;;
        es) pkgs+=(tesseract-ocr-spa) ;; de) pkgs+=(tesseract-ocr-deu) ;; fr) pkgs+=(tesseract-ocr-fra) ;;
        zh) pkgs+=(tesseract-ocr-chi-sim) ;; ja) pkgs+=(tesseract-ocr-jpn) ;;
      esac
      sudo apt-get update
      sudo apt-get install -y "${pkgs[@]}" || note "some packages were not found; Debian's Rust may be too old - install rustup from https://rustup.rs"
      ;;
    dnf)
      pkgs=(gcc cargo rust pkgconf-pkg-config wayland-devel libxkbcommon-devel mesa-libGL fontconfig dbus tesseract tesseract-langpack-eng at-spi2-core xdg-desktop-portal libnotify wl-clipboard)
      case "$comp" in
        hyprland) pkgs+=(xdg-desktop-portal-hyprland) ;;
        sway|river|wayfire) pkgs+=(xdg-desktop-portal-wlr) ;;
        niri|gnome) pkgs+=(xdg-desktop-portal-gnome) ;;
        kde) pkgs+=(xdg-desktop-portal-kde) ;;
      esac
      lang="${LANG%%_*}"; lang="${lang%%.*}"
      case "$lang" in
        ru) pkgs+=(tesseract-langpack-rus) ;; uk) pkgs+=(tesseract-langpack-ukr) ;; pt) pkgs+=(tesseract-langpack-por) ;;
        es) pkgs+=(tesseract-langpack-spa) ;; de) pkgs+=(tesseract-langpack-deu) ;; fr) pkgs+=(tesseract-langpack-fra) ;;
        zh) pkgs+=(tesseract-langpack-chi_sim) ;; ja) pkgs+=(tesseract-langpack-jpn) ;;
      esac
      sudo dnf install -y "${pkgs[@]}"
      ;;
    zypper)
      pkgs=(gcc cargo rust pkg-config wayland-devel libxkbcommon-devel Mesa-libGL1 fontconfig dbus-1 tesseract-ocr tesseract-ocr-traineddata-english at-spi2-core xdg-desktop-portal libnotify-tools wl-clipboard)
      case "$comp" in
        hyprland) pkgs+=(xdg-desktop-portal-hyprland) ;;
        sway|river|wayfire) pkgs+=(xdg-desktop-portal-wlr) ;;
        niri|gnome) pkgs+=(xdg-desktop-portal-gnome) ;;
        kde) pkgs+=(xdg-desktop-portal-kde) ;;
      esac
      sudo zypper install -y "${pkgs[@]}"
      ;;
    *)
      note "No known package manager. Install by hand: rust/cargo, wayland, libxkbcommon, tesseract + a language pack, at-spi2-core, xdg-desktop-portal with a backend for your compositor, libnotify."
      ;;
  esac
fi

# ---- build and install ------------------------------------------------------

if [ "$do_build" = 1 ]; then
  command -v cargo >/dev/null || die "cargo is not installed (https://rustup.rs)"
  if [ "$pm" = pacman ]; then
    say "Building the Arch package"
    (cd "$here/packaging/arch" && makepkg -f --noconfirm)
    pkg="$(ls -t "$here"/packaging/arch/clickwork-*.pkg.tar.zst | head -1)"
    say "Installing $pkg"
    sudo pacman -U --noconfirm "$pkg"
  else
    say "Building (cargo build --release)"
    (cd "$here" && cargo build --release)
    say "Installing under /usr/local"
    sudo install -Dm755 "$here/target/release/clickwork" /usr/local/bin/clickwork
    sudo sed 's|^Exec=/usr/bin/clickwork|Exec=/usr/local/bin/clickwork|' \
      "$here/packaging/arch/io.github.blackixxce12.clickwork.desktop" \
      | sudo tee /usr/local/share/applications/io.github.blackixxce12.clickwork.desktop >/dev/null
    for sz in 128 256 512; do
      sudo install -Dm644 "$here/assets/icon_$sz.png" "/usr/local/share/icons/hicolor/${sz}x${sz}/apps/clickwork.png"
    done
    sudo install -Dm644 "$here/packaging/arch/70-clickwork-input.rules" /etc/udev/rules.d/70-clickwork-input.rules
    sudo install -Dm644 "$here/LINUX.md" /usr/local/share/doc/clickwork/LINUX.md
    sudo install -Dm644 "$here/LINUX_RU.md" /usr/local/share/doc/clickwork/LINUX_RU.md
    command -v update-desktop-database >/dev/null && sudo update-desktop-database /usr/local/share/applications || true
  fi

  say "Input devices"
  # The udev rule gives the seated user read access without a group; reloading it
  # applies it to the devices already plugged in.
  sudo udevadm control --reload 2>/dev/null || true
  sudo udevadm trigger --subsystem-match=input 2>/dev/null || true
  if [ "$add_input" = 1 ] && ! id -nG "$USER" | tr ' ' '\n' | grep -qx input; then
    note "Adding $USER to the input group as a second road to the devices (takes effect after you log in again)."
    sudo usermod -aG input "$USER" || true
  fi

  say "Done. What this machine can do:"
  clickwork --doctor || true
  echo
  note "Start it from your launcher, or run: clickwork"
fi
