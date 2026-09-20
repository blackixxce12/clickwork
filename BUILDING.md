# Building the binaries

How `Clickwork.exe`, `Clickwork.msi`, `Clickwork.msix` and the Arch package are made,
what each platform can and cannot do, and the traps that cost time the first time.

Everything below was either run or observed. Where something is reasoned rather than
tested it says so.

---

## The one fact that decides everything else

**The released Windows binaries are built with the GNU ABI, not MSVC.** The shipped
`Clickwork.exe` carries paths like

```
C:\Users\Administrator\.rustup\toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\...
```

so the target is `x86_64-pc-windows-gnu`. That is lucky rather than incidental: the GNU
target cross-compiles cleanly from Linux with mingw-w64, while the MSVC one needs the
Windows SDK. Keep it. A check against `x86_64-pc-windows-msvc` proves nothing about what
is actually shipped.

The resulting executable is self-contained: `objdump -p` shows imports only from Windows
system DLLs (`kernel32`, `user32`, `combase`, `d3d11`, `mfplat`, `api-ms-win-crt-*`) and
none of `libgcc_s_seh-1.dll`, `libwinpthread-1.dll` or `libstdc++-6.dll`. No mingw runtime
has to be shipped beside it, and it will not fail the Store's app-launch test for a
missing DLL.

---

## What can be built where

| Artefact | Linux | Windows 11 | CI (`windows-latest`) |
|---|---|---|---|
| `Clickwork.exe` | **yes** | yes | yes |
| `Clickwork.msi` | **no** — see below | yes | yes |
| `Clickwork.msix` | yes, the hard way | yes | **yes, preferred** |
| `clickwork-*.pkg.tar.zst` | yes | no | no |

CI does all three Windows artefacts on a real Windows runner and is the answer to
almost every question here: `.github/workflows/windows.yml`. Run it from the Actions tab
with **Run workflow** to get an MSIX, or just push and take the exe and MSI.

There is a Linux workflow beside it, `.github/workflows/linux.yml`, which builds and
tests the native binary and then runs it against a headless sway - see
[The Linux binary](#the-linux-binary-and-the-compositor-it-is-tested-against) below for
what that covers and, more importantly, what it cannot.

---

## Linux

### The executable

```bash
sudo pacman -S rustup mingw-w64-gcc mingw-w64-binutils
rustup default stable
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

`rustup` **conflicts with** Arch's `rust` package and pacman will offer to replace it;
that is expected, since the distribution package ships only the host target and there is
no way to add another to it.

`mingw-w64-binutils` is not optional even though nothing links against it: it provides
`x86_64-w64-mingw32-windres`, which is what embeds the icon. `winresource` finds it on its
own from the target triple - no configuration.

Roughly 2.5 minutes on 16 cores with fat LTO.

### The installer: not possible, and not for want of trying

WiX does not work on Linux. Its own warning - *"The WiX Toolset only supports Windows...
All behavior after this point is undefined"* - is exact rather than cautious.

- Every `Directory/@Name` is rejected with `WIX0389: ... is not a relative path`.
  Verified against a minimal project with the names `Clickwork`, `App`, `My App` and `X`:
  all four fail. Since a named directory appears in every real installer, this is
  categorical.
- The error comes from managed code (`WixToolset.Data.dll`), so running only the native
  part under wine would not help.
- The MSI database itself is built by `wixnative.exe`, a native Windows PE shipped inside
  the tool. That is the structural reason, and it is not going to change.
- WiX 4 and WiX 5 fail identically, so this is not about WiX 7's licence.

The only native Linux MSI writer is `wixl` from `msitools`, and it understands a subset of
the **WiX v3** dialect with **no WixUI support at all**. It would therefore drop the
install-folder chooser, which `packaging/Clickwork.wxs` keeps deliberately and explains at
the top of the file. A silent installer to a fixed path is a different product, not the
same one built elsewhere, so this is not offered as a fallback.

### The MSIX, if it ever has to be done without Windows

It works, and the output has been audited against genuine Microsoft packages. It is still
the second choice, because CI does the same job with Microsoft's own `makeappx.exe`.

```bash
git clone --depth 1 https://github.com/microsoft/msix-packaging.git
cd msix-packaging
# Required on a current distro: upstream pins C++14 with a bare set(), and ICU 78
# headers need C++17. A -D on the command line is ignored, so the file has to be edited.
sed -i 's/set(CMAKE_CXX_STANDARD 14)/set(CMAKE_CXX_STANDARD 17)/' CMakeLists.txt lib/xerces/CMakeLists.txt
bash makelinux.sh --pack --skip-samples --skip-tests
export LD_LIBRARY_PATH=$PWD/.vs/lib
./.vs/bin/makemsix pack -d <payload-dir> -p Clickwork.msix
```

The payload directory needs `Clickwork.exe`, `Assets/*.png` and an `AppxManifest.xml` -
see the manifest CI writes, which is the reference copy.

Verified: builds; packs; the block map re-hashes correctly with an independent script; the
zip conventions (ZIP64 EOCD always present, entry ordering, `LfhSize`, the data-descriptor
flag) are byte-for-byte the same as Microsoft's own `Microsoft.VCLibs...appx` and the
Store-signed sample in the SDK's test data; schema validation is live and rejects a bogus
element or a bad `ProcessorArchitecture`. Not verified: Partner Center ingestion itself.

Dead ends, so nobody repeats them: `dotnet build` on a `.wapproj` fails with `MSB4019`
because `Microsoft.DesktopBridge.props` ships **only inside Visual Studio**; it is in no
SDK and on no NuGet feed. Worse, `dotnet publish -p:WindowsPackageType=MSIX
-p:EnableWindowsTargeting=true` **exits 0 and silently produces no package at all**.

---

## Windows 11

Everything builds here. This is the path the releases were made on.

```powershell
winget install Rustlang.Rustup
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

The GNU target brings its own linker, so mingw does not have to be installed separately.

### The MSI

```powershell
dotnet tool install --global wix --version 5.*
wix extension add -g WixToolset.UI.wixext/5.0.2
cd packaging
Copy-Item ..\target\x86_64-pc-windows-gnu\release\clickwork.exe Clickwork.exe
wix build Clickwork.wxs -ext WixToolset.UI.wixext -arch x64 -d Version=2.0.0 -o ..\Clickwork.msi
```

**Build it from `packaging`, not from the repository root.** WiX resolves every `Source`
path against the *working directory*, not against the `.wxs` that names it - which is why
the file says `..\assets\icon.ico` and names the executable with no path at all. Run it
from the root and you get three `WIX0103: Cannot find...` errors for the icon, the
executable and `license.rtf`. This cost a CI run.

On **WiX 7 and the licence**: v6 and later require accepting the Open Source Maintenance
Fee EULA before the tool will run at all (`WIX7015`). The fee is for commercial use;
Clickwork is free and MIT-licensed, so accepting it costs nothing - but it is the owner's
decision to make, not a build step to automate. WiX 5 has no such gate and builds this
project identically, which is why the CI pins it.

### The MSIX

`makeappx.exe` comes with the Windows SDK, which Visual Studio installs but which is also
available on its own. Without any SDK at all, `Microsoft.Windows.SDK.BuildTools` on
nuget.org is a plain zip containing `makeappx.exe`, `signtool.exe`, `appxpackaging.dll`
and `opcservices.dll` - `curl` and `unzip` are enough to get them.

```powershell
makeappx pack /d msix /p Clickwork.msix /o
```

The payload layout and the manifest are in `.github/workflows/windows.yml`; that is the
copy to keep current.

---

## The MSIX and the Store

### Identity

Assigned by the Store and must match **exactly**, or ingestion rejects the package as
belonging to somebody else. From Partner Center → Product identity:

```
Package/Identity/Name                    blackixxce12.Clickwork
Package/Identity/Publisher               CN=24C507F4-8350-4138-8F2E-0A0056C2BE30
Package/Properties/PublisherDisplayName  blackixxce12
```

The published package is X64, `Windows.Desktop` with `MinVersion 10.0.17763.0`, declares
`runFullTrust`, and carries the languages `en-us ru-ru uk pt es zh-hans`. Match those
unless there is a reason not to.

### Version numbering, which is not the program's version

Four parts, and **the fourth must be `0`** - the Store reserves it. A new submission must
be **strictly higher** than the one already published, so from `2.0.0.0` the next possible
number is `2.0.1.0`.

This is packaging metadata and has nothing to do with what the program calls itself:
`APP_VERSION` is `env!("CARGO_PKG_VERSION")` and comes from `Cargo.toml`. The application
can stay `2.0.0` across many MSIX versions - exactly the way the Arch package keeps
`pkgver=2.0.0` and moves `pkgrel`.

### Signing

**Do not sign it.** The Store re-signs after certification, and signing is not required
for submission. It is also not possible to do correctly: Windows requires the certificate
subject to equal `Package/Identity/Publisher`, which here is a Store-issued GUID that no
certificate authority will ever issue a certificate for.

Structurally this is normal rather than a shortcut: in genuine Microsoft packages
`AppxSignature.p7x` and `AppxMetadata/CodeIntegrity.cat` are excluded from the block map,
so signing appends parts and never rewrites it. An unsigned package's block map is already
the final one.

---

## The Linux binary, and the compositor it is tested against

```bash
cargo build --release      # or `cargo build` while working
cargo test                 # 315 tests, and not one of them needs a compositor
```

**Only one system library is linked**, and it is worth knowing which:

```
$ ldd target/release/clickwork
libc.so.6  libm.so.6  libgcc_s.so.1  libxkbcommon.so.0
```

Everything else the program speaks to - Wayland, EGL, X11, Tesseract - is opened by
name at run time rather than linked, which is why a build needs `libxkbcommon-dev`
and nothing more, and why the package has to list its dependencies by hand: nothing
automatic can see them.

### Why `cargo test` being green is not the good news it looks like

All 315 pass on a machine with no Wayland session at all. That is the indictment
rather than the reassurance - it means no test in the suite touches a compositor, so
none of them could notice a protocol binding wrongly, a capture coming back the wrong
size, or a feature quietly doing nothing on a session that cannot carry it. The last
of those has happened and shipped: the "Fast screen capture" checkbox drove a Linux
body that was `{}`, and survived a rename, a port and a release.

The other half is a real compositor with no screen:

```bash
ci/headless-session.sh target/release/clickwork --selftest session
```

That starts a headless sway, runs the self-test inside it and cleans up. The self-test
asks every capability twice - is the protocol advertised, and does the feature on top
of it work - and fails when the two disagree in either direction. A protocol that is
absent is not a failure; a protocol that is absent while the code claims the feature
works is.

It needs **no graphics device**. `WLR_BACKENDS=headless` skips DRM and
`WLR_RENDERER=pixman` renders on the CPU. To satisfy yourself that the DRM node is
genuinely unused, take it away:

```bash
bwrap --dev-bind / / --tmpfs /dev/dri ci/headless-session.sh \
  target/release/clickwork --selftest session
```

The same two commands run in CI on every push: `.github/workflows/linux.yml`.

### What CI cannot cover, and it is not a small gap

**Hyprland cannot run on a hosted runner.** It requires a DRM node and fails at
`CBackend::create()` without one - control-tested, not assumed. Since Hyprland is the
supported compositor, everything specific to it - window lookup, anchoring, workspace
isolation, hide-to-tray, the compositor hotkey ladder - is exercised by nobody but a
person on a real session. A green tick on the Linux workflow means the portable
wlroots protocols are sound. It does not mean the window backend is.

Sway is what CI gets instead, and Ubuntu 24.04 ships 1.9 on wlroots 0.17: every
protocol this program uses today, and none of the `ext_*` successors. Those need a
newer sway than any runner image carries, so they cannot be tested there yet either.

---

## The Arch package

```bash
cd packaging/arch && makepkg -f
```

Builds from the working tree two levels up rather than from a remote, and installs with
`pacman -U`. `pkgver` stays at the application's version; `pkgrel` is what moves for a
rebuild. The `PKGBUILD` passes `--frozen`, so `Cargo.lock` must be current - run
`cargo check` after any version edit or `makepkg` fails.

---

## Traps, each of which cost a build

**A build script's `cfg` is the host, not the target.** `build.rs` used to embed the icon
under `#[cfg(windows)]`, so an executable cross-built anywhere else came out with no
Explorer icon, no taskbar icon and no version resource - silently, 151 KB smaller than the
released one. The gate has to be `CARGO_CFG_TARGET_OS`, and `winresource` has to be a
plain build-dependency rather than a `cfg(windows)` one for the same reason. Fixed; do not
reintroduce it.

**The test suite differs by platform and both numbers matter.** 315 tests on Linux, 292 on
Windows; the difference is the Linux-only modules. Both should be green. Nothing ran the
Windows suite at all until CI existed, and the answer turned out to be that it passes -
but that was not known, and "it is all mechanical" is not the same as a compiler saying so.

**Nothing local can check the Windows build.** There is no Windows toolchain on the Linux
machine and no way to run the result. Cross-compiling proves it *compiles*; only CI proves
it passes its tests. Treat any Windows-side edit made from Linux as unverified until the
workflow is green.
