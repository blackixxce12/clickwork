# Changelog

All notable changes to Clickwork are recorded here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/).

[🇷🇺 Русская версия](CHANGELOG_RU.md)

---

## [2.0.0]

The program is called Clickwork now, and it runs on Linux.

The rename changes no behaviour, on purpose: a rename is easier to trust when nothing
else moves at the same time. The Linux port is the other half of the release - the same
window, editor, script engine, picture search and macro format as on Windows, on top of
a platform layer written for Wayland, with Hyprland as the supported compositor.

### Added

- **A Linux platform layer** (`src/linux/`), selected at compile time where the Windows
  one was. Mouse and keyboard playback go through `zwlr_virtual_pointer_v1` and
  `zwp_virtual_keyboard_v1`; the virtual keyboard carries the same layout Hyprland gave
  the real one, group included, so a recorded scan code replays as the key it came from.
  Text that is typed rather than replayed goes through a generated keymap with one key per
  character, so any Unicode types correctly.
- **Screen capture through `zwlr_screencopy_v1`** for the picture search, the text
  reader and the pixel condition. Rectangles are asked for in physical pixels and
  converted to the compositor's logical ones at the edge, so a fractional scale (1.6
  is common) neither blurs a template nor shifts a click.
- **Recording and hotkeys from evdev.** Wayland shows a program only its own input, so
  the recorder reads `/dev/input/event*` as the compositor does. Read access is needed;
  the package installs a udev rule granting it to the seated user, and `usermod -aG input`
  is the other way. Without it the program says so once and keeps checking.
- **Text recognition through Tesseract 5.** The library is opened at run time
  (`libtesseract.so.5`) rather than linked, so a machine without it still runs everything
  else. The OCR language list is whatever `tesseract-data-*` packages are installed;
  the default reads in the desktop's language plus English. The five preparation
  profiles and the format check work unchanged.
- **Window lookup, anchoring, move/resize/close and workspace isolation through
  Hyprland's IPC socket**, using the 0.56 Lua dispatchers.
- **The overlay and HUD as a layer-shell surface** per output, drawn in software,
  click-through, crisp at fractional scale through `wp_viewporter`.
- **UI element steps through AT-SPI2**, the accessibility bus: find a control by name
  or role in GTK, Qt and Electron applications and press it through its own action.
- **A tray icon as a StatusNotifierItem**, desktop notifications through
  `org.freedesktop.Notifications`, the clipboard through data-control (works without
  focus), screen recording through `gpu-screen-recorder` or `wf-recorder`, power
  actions through `systemctl` and `loginctl`.
- **`clickwork --doctor`** lists every protocol, device and service the program depends
  on and says which are missing, with the fix.
- **The command line talks to the running instance**: `--stop`, `--record`,
  `--play-toggle`, `--pause`, `--faster`, `--slower`, `--skip`, `--show`, `--hide`,
  `--quit`, `--status` - what a compositor keybind calls. The seven hotkeys are also
  registered with the GlobalShortcuts portal.
- **An Arch package** (`packaging/arch/PKGBUILD`), a desktop entry, icons and the udev
  rule. `LINUX.md` and `LINUX_RU.md` document the port.

### Changed

- **Everything the program calls itself.** The window title, the executable, the crate,
  the Windows file metadata, the tray and overlay window classes, the single-instance
  mutex, the log file, and the product name in all six languages of the handbook.

- **The settings folder moved** from `%APPDATA%\MacroRecorder\` to
  `%APPDATA%\Clickwork\`, and 2.0.0 brings the old one along. On the first run, if the
  new folder is not there and the old one is, the old folder is moved across. If the
  move fails - a file held open, a permission - the old folder is used where it stands,
  because the one outcome worth ruling out is somebody starting from nothing. Macros,
  templates, profiles, run history and settings survive the rename either way.

  A portable installation, with its files next to the executable, never touches
  `%APPDATA%` and is unaffected.

- **The single-instance mutex was renamed with everything else**, which means a 1.9.6
  build and a 2.0.0 build do not see each other. Close the old one before running the
  new one.

### Changed on Linux

- Hide-to-tray on Hyprland parks the window on a special workspace and fetches it back,
  because a Wayland window cannot be hidden by its owner.
- On Linux the settings live in `~/.config/clickwork`; the folder next to the executable
  is used only when `CLICKWORK_PORTABLE=1` asks for it.
- The exported standalone player is a Linux executable without an extension.

### Known differences

- Hotkeys cannot be swallowed: the key also reaches the application in front.
- There is no "unchanged frame" signal from screencopy, so every look at the screen is
  a real copy; keep search areas small.
- The window-responsiveness figure (the FPS-like number) is not measured on Wayland.
- "Maximise" is fullscreen and "minimise" parks the window on a special workspace.

---

## [1.9.6]

The main window, gone over with the interface actually in front of us.

Every item here came out of looking at the running program rather than reading its
source: a development build carrying egui's inspection protocol was driven from
outside, screenshotted, resized and scrolled. Two of the six turned out to be things
no amount of reading would have found.

### Changed

- **Twenty-seven sections are now in seven groups**, the same grouping the handbook
  already uses for the same material. The list was flat and in code order, which is
  not an order anyone reads by: image search, one of the two things this program is
  for, sat twenty-second, below the text expander. It is now first under **What it
  sees**. The groups are captions and rules, not another level of collapsing — a
  second click to reach a setting would have cost more than the flat wall did.

- **The status line is a pinned panel instead of the last line of the scroll.** It
  says whether a recording or a run is going on, and the one person guaranteed not to
  have scrolled past twenty-seven sections to read it was the person who wanted that.

- **The window uses its width.** The scroll area used to shrink to its widest line, so
  a window widened to 1240 points left roughly three quarters of itself empty and
  gained nothing. Rules, headers and hit targets now span the window, and past 900
  points the groups split into two columns — at 1240 the whole program fits on one
  screen with nothing to scroll. The controls inside a section still sit at their
  natural width, which is deliberate: a checkbox stretched across 1800 points is not an
  improvement.

- **The handbook button moved into the status panel.** It used to float in an overlay
  anchored to the window's right edge, on top of whatever was underneath it, which at
  the bottom of the list meant on top of the status band.

- **Hotkeys are named in one place.** Record and Play were labelled twice — greyed
  beside their buttons, and again in the status line in a different format — while the
  emergency stop, which has no button at all, appeared only in the status line. The
  status line keeps all three; the labels beside the buttons are gone.

- **The icons are off the section headers.** Twenty-seven thin monochrome glyphs at
  text weight and text colour did not help anyone scan a list; the group captions do
  that job now. Action buttons keep theirs.

### Fixed

- Two of the four checkboxes under **Appearance** carried an emoji and two did not, in
  all six languages.
- The banner comment above the screen-recording section named the hotkeys section.

---

## [1.9.5]

A display over the game, a timeline of every run, and a video of what happened.

Two things this release does not do are worth saying first. It does not measure frame
rate — that needs an ETW session against the DXGI providers, which needs administrator
rights, and the reasons are in the `perf` module and have not changed. And the
VirusTotal badges are gone from both READMEs, along with the argument built on them.

### Added

- **A heads-up display, drawn over everything, that keeps working with this window in
  the tray.** That last part is the point: the thing being watched is a game in front
  of it, and a display that vanished when the window was minimised would be a display
  nobody could use.

  Four blocks, each shown only when it has something to say:

  ```
  RESPONSE          MACRO             RELIABILITY       STATE
  142 FPS           farm              Fallbacks 2       REC       --
  96 1% low         Run       17      Recovery  0       RUN       PLAYING
  71 0.1% low       Step      42/86   OCR retry 1
  7.0 ms                              Missed    0
  max  48.2 ms
  hitches 3
  ```

  Five presets — off, minimal, responsiveness, automation, everything — and four
  corners. Off by default, and it costs a relaxed atomic load per step when it is.

  **`RESPONSE`, not `GAME`, and the heading is doing real work.** What is under it is
  how quickly the target window answers an empty message, not how long anything took to
  render. The `perf` module has said so since it was written; a number labelled FPS with
  no heading over it would be a claim this program cannot make.

  The overlay it draws in already existed, already ran on its own thread with its own
  message loop, and already redrew only when there was something new. All three of those
  are why this is a small change rather than a subsystem — and the third one is now
  enforced for the display too: the state block is rewritten on every pass of the window,
  and the sequence moves only when the words actually differ. Comparing four short
  vectors is cheaper than one repaint of a layered surface.

- **A timeline of every run.** Each step now leaves a mark — when it began, where it
  sat, and which step it was — and the run history shows them under the run they belong
  to:

  ```
  Timeline: 12 steps
    00:00.000    303 ms  #0   Wait 300 ms
    00:00.303    504 ms  #1   Wait 500 ms
    00:41.220   1314 ms  #5   Click image: claim
  ```

  A step's length is the gap to the next mark, so nothing is written twice and the two
  numbers cannot disagree. Marks are three integers and no text — a night macro makes a
  great many of them — and how each step reads is worked out from the macro afterwards,
  by the step's id, which is what survives somebody inserting a line above it. Slow steps
  are coloured, and the one that ended a failed run is coloured differently.

  The last twenty thousand marks are kept rather than the first, because a run that fails
  at hour nine is one whose beginning nobody needs; the record says how many fell off the
  front so a timeline can never quietly present itself as the whole run.

  Positions read as `mm:ss.mmm`, which is how a video player writes one.

- **Screen recording, to H.264, alongside a macro recording.** Off by default. Tick it
  and pressing Record writes a video into `recordings/` for as long as the macro
  recording lasts.

  Through Media Foundation, which is what makes this a few hundred lines instead of
  three vendor SDKs: NVENC, AMF and Quick Sync are all registered with it as transforms,
  so asking for hardware gets whichever the machine has, and a machine with none falls
  back to software without the caller knowing. No third-party dependency, and the same
  `windows` crate every other call in this program goes through.

  Measured on a 2560×1440 desktop: 30 frames a second, none missed, about 1.8 s of
  set-up before the first frame while the encoder is negotiated. `--selftest record=N`
  records for N seconds and checks what came out.

### Fixed

- **Three bugs in the recorder, each found by checking the file rather than the code.**
  They are worth listing because each one produced a file that looked fine from the
  outside:

  1. **No index at all.** The first version asked for `MF_MPEG4SINK_MOOV_BEFORE_MDAT` in
     the belief that it produced a crash-resistant fragmented MP4. It does not, and what
     it produced here was `ftyp uuid mdat` and **no `moov`** — eleven megabytes of
     picture that no player will open. The self-test now looks for the `moov` box; the
     check it had before, which read the first four bytes, passed that file happily.

  2. **Time from a frame counter.** Stamping frame *n* at *n*/30 s describes six seconds
     of screen as four seconds of video, played half again too fast.

  3. **And stamping with the clock did not fix it**, which is the interesting one: the
     duration a player reports comes from the declared frame rate and the number of
     frames, not from the times on the samples. The stream has to genuinely be a
     constant rate, so a capture that takes longer than one frame now writes the frame
     it did get once per slot that passed. The picture is copied once and the sample
     re-timed, so a repeat costs no second copy of fifteen megabytes.

  The proof is arithmetic on the file: 6.27 s of video for 6.25 s of recording.

- **A measurement that read as a bug and was not.** The recorder appeared to manage only
  21 frames a second until the setup time was counted separately — 1.8 s of Media
  Foundation and encoder negotiation was being charged to the recording. It does 30.
  Both numbers are published now, because anything lining a video up against a timeline
  needs to know about that 1.8 s too.

### Changed

- **The whole-screen wait warning is now judged on cost rather than on the clock.** It
  used to fire on any full-screen image wait longer than three seconds, which got both
  ends wrong: two and a half seconds on a 4K desktop is expensive and five seconds inside
  a 320×240 rectangle is nothing.

  It now multiplies the area by the number of looks the engine will really make, and
  shows the working:

  ```
  ⚠ #0 waiting for this picture costs: 250 × 11 ms ≈ 2.8 s / 30 s
  ```

  Looks × cost of one ≈ processor time / the wait it sits in — which is the combination
  that matters, because the wall clock is what the person experiences and the processor
  time is what the machine pays. The per-search figure comes from a straight line through
  the points `--selftest vision` prints; on the desktop it was checked on, the model said
  11 ms and the measurement said 11.3.

- **The VirusTotal section is gone from both READMEs**, badges, scan table, hash list and
  the "69 of 71 vendors" notice with it. In its place is a security section that says the
  same true things without a number that has to be re-earned every release: the source is
  open, here is how to check a download's hash, and here is every capability the program
  asks the operating system for and why. The honest note that input injectors get flagged
  stays — it cites nothing and is still true.

### Testing

- **288 unit tests.** The wait-cost rule is covered on all four of its edges: it fires on
  thirty seconds across a 1440p desktop with the engine's own count, fires on two and a
  half seconds across 4K, stays quiet on five seconds inside a small rectangle, and stays
  quiet when the area is one it cannot size.

- The display was checked on screen rather than reasoned about, and the checking found
  two defects. The first version drew `●`, `▶` and `■` as empty boxes, because the
  fixed-pitch face that keeps the columns from dancing has no such glyphs; it uses words
  now. The second bumped its sequence on every write, so a layered window repainted ten
  times a second to draw the same words — the flicker the overlay's own redraw rule
  exists to prevent. Two screenshots five seconds apart, showing `Step 2/8` and then
  `Step 6/8`, are what proved the fix for the second did not freeze the first.

- The full self-test set was run against the release build: `dryrun` (10 checks),
  `target` (8), `recovery` (6), `script=500` (12), `timing`, `vision`, `simd` and
  `churn=120`. No failures.

---

## [1.9.4]

Ten optimisations, measured. One survived, and it was not any of them.

A list of ten ways to make the image search faster was put to this release. Nine were
measured and refused, two of them on arithmetic alone. What the measuring found
instead is that the largest cost in a script is not in the search at all — it is in
searching *the whole screen* over and over while waiting for something to appear, and
the cure for that has been in the program since 1.5.0 and nobody is told to use it.

So this release says so. It also keeps the instruments, because a refusal is only
worth having if the next person can re-check it in one command.

Nothing about how macros record or run changed. The format is still 5.

### Added

- **The pre-flight now warns about a long wait for a picture across the whole
  screen**, and says how many looks that will be.

  A wait polls every 120 ms until its picture turns up. A full-screen look costs about
  **11 ms** on a 2560×1440 desktop against **1.2 ms** inside a 320×240 rectangle —
  close to linear in the area — so a thirty-second full-screen wait is 250 searches
  and seconds of processor spent deciding, over and over, that nothing has appeared
  yet. Measured end to end: a ten-second wait costs **1.8–2.3 s of CPU**.

  Telling it where to look is a change of one dropdown and is already supported —
  `Rect`, `Active window`, `Near where it was last`. That is why this is a warning and
  not an optimisation: the fix is the user's to make, costs nothing, and is worth an
  order of magnitude.

  It is deliberately quiet below three seconds, where the repetition is a dozen looks
  and nobody needs the advice, and quiet whenever the area is already bounded. The
  count it prints is the timeout divided by the engine's own poll interval, which is
  now a named constant used by all four wait loops — a forecast that had drifted from
  the thing it forecasts would be worse than none.

### Changed

- **`--selftest vision` reports what the screen actually does**, which is what turned
  three plausible optimisations into three refusals:

  - whether a frame survives the interval a wait sleeps for, at 120 ms and at 16 ms;
  - how the compositor localises its damage — rectangles per frame and the share of
    the desktop they cover;
  - whether that damage report can be believed, by comparing two frames in full and
    counting pixels that changed *outside* every rectangle DXGI named;
  - what the coarse pass's threads are actually worth, which had never been timed.

  All of it is printed rather than asserted: these are properties of a machine, not of
  this code. On another GPU the numbers may say the opposite, and then the answers
  below change with them.

### Performance

*Nothing here got faster. This section is what was measured and refused, so the next
attempt starts from the numbers rather than from the same reasoning.*

- **Integral images for `shrink` / `shrink_mask` — refused on arithmetic.** The
  proposal counts `step²` work per coarse pixel and concludes the pass is expensive.
  There are `(W/step) × (H/step)` coarse pixels, so that is `W × H` in total: box
  shrink already reads every source pixel exactly once, in one linear pass. A summed-
  area table needs a full `W × H` pass to build, plus 15 MB, before it can answer
  anything — strictly more work. No measurement needed to decline this one.

- **Integral images for `sum_i` and `sum_ii` — refused, twice over.** The target is
  right: the correlation is where the time is. But `dot(I,T)` has to load every pixel
  of the window regardless, and the kernel already accumulates all three sums in one
  pass — per 16-float vector that is two loads and three arithmetic operations.
  Removing two of the three removes no loads at all, so the ceiling is around 1.5× on
  the fine pass rather than a multiple.

  The second reason is fatal on its own: the planes are scaled to 0..1 rather than
  0..255 precisely because `di = sum_ii - n·mi·mi` is a catastrophic-cancellation
  form. An f32 integral over 3.7 M pixels reaches ~2²¹·⁸, where one ulp is about 0.4,
  and recovering a 32×32 window's sum as a difference of two such numbers reintroduces
  exactly the precision loss that scaling exists to prevent. f64 integrals would be
  30 MB each, so the working set is ~75 MB rather than the 45 MB the proposal
  estimates.

- **Skipping a search when the screen has not changed — built, measured, removed.**
  Desktop Duplication says when nothing has moved, and on an unchanged frame the
  program was doing a full readback and a full search over byte-identical pixels. A
  frame serial and a cached answer were added, keyed on the template set, the exact
  rectangle and the edge mode.

  It never fired. **The frame was unchanged 0 times out of 16** across the 120 ms a
  wait sleeps — and 0 of 16 at 16 ms, so it is not frames piling up between polls. A
  desktop sends the compositor a frame for any pixel anywhere: a clock, a caret, a
  notification. Three interleaved rounds of a ten-second wait showed no saving, and
  the cache was removed.

- **Damage regions — the right idea, and it has nothing to work with here.** The
  weaker whole-frame version having failed, the strong one was instrumented properly:
  `GetFrameDirtyRects` *and* `GetFrameMoveRects`, because a move changes its
  destination and vacates its source and counting only one would call a region
  untouched when a window had just slid off it.

  On this machine DXGI reports **one rectangle, covering the entire desktop, on every
  frame, at both intervals**. There is no locality to exploit, so no skip was written:
  it would have been the whole-frame cache again, with more code and a stale-answer
  risk that ends in a click on the wrong thing. The collection and the numbers stayed,
  so a machine whose compositor does localise its damage can be recognised in one
  command — and half the feature is then already written.

- **Threading was priced for the first time.** The `vs 1 thread` column compared the
  *answer*, never the time. It now does both. On sixteen logical cores the coarse pass
  buys **1.73× at 32×32, 1.41× at 64×64, 1.08× at 128×128 and 0.99× at 256×256**.

  The feared oversubscription cannot happen: `threads = (rows / 48).clamp(1, cores)`
  yields three to five in practice, not sixteen, and only one macro plays at a time.

- **Sparse masked correlation, and a better coarse descriptor**, are the two proposals
  still standing on their merits and neither is acted on. The first needs evidence that
  transparent templates are common, which nothing collects: a screen grab has no alpha
  at all, so masks exist only for hand-cropped PNGs. The second is partly already here
  as the edge mode, and the failure it aims at — thin structure vanishing under an
  aggressive shrink — is what `COARSE_KEEP = 4` was introduced to fix, with a test and
  a measurement behind it.

### Fixed

- **`clippy::missing_const_for_thread_local` is a false positive in Rust 1.98**, firing
  on initializers that already are `const` — reproduced on a twelve-line crate with
  nothing in it but two `const {}` statics. It named ten blocks here, all already
  written the way it asks. A crate-level `allow` with that explanation restores a clean
  `cargo clippy --all-targets` without touching correct code, and is worth re-checking
  when the toolchain moves.

### Testing

- **288 unit tests**, up from 287. The new one covers all three sides of the warning:
  it fires on a thirty-second full-screen wait with the count the engine will really
  make, stays quiet when the area is bounded, and stays quiet below three seconds.

- The full self-test set was run against the release build: `dryrun` (10 checks),
  `target` (8), `recovery` (6), `script=500` (12), `timing`, `vision`, `simd` and
  `churn=120`. No failures.

---

## [1.9.3]

The window learns to sleep.

A settled window that nobody is looking at was still waking the processor and the
graphics card ten times a second, for a picture that could not have changed. This
release is that, three concurrency faults found while looking for it, and a pass over
the source against everything Rust 1.98 and clippy have to say about it.

Nothing about how macros record or run changed. The format is still 5.

### Changed

- **The main window no longer repaints on a timer while nothing is happening.** Every
  pass of the UI ended with an unconditional *"ask me again in a tenth of a second"*.
  It did that because the things the window shows — the transport flags, the step
  tally, the speed — are written by six other threads, and polling was the only way it
  knew to notice them.

  Those threads now say so themselves. A single shared `Context` and a `wake_ui()` call
  at each transition — record started or stopped, playback started, finished or
  cancelled, pause toggled, speed nudged, window brought back from the tray — replaces
  the poll. The tick that remains is for the counters that genuinely do change on their
  own, and only a run or a recording has any; when neither is going the window sleeps
  and is woken.

  Measured on release builds, run alternately, window idle and settled:

  | | frames per second | CPU, five 20–30 s samples |
  |---|---|---|
  | 1.9.2 | 8.8 | 1.56 %, 1.64 % of one core |
  | 1.9.3 | 0.25 | 0.00 % – 0.26 %, median 0.00 % |

  The 1.9.3 samples are not all zero because the scheduler and the responsiveness
  sampler still wake on their own timers; the point is that the window no longer does.

  It is also *quicker*, which is the part worth stressing. A hotkey pressed while
  another application is in front used to reach the window on the next tick — up to a
  tenth of a second later. It now arrives in the same millisecond, because the thread
  that handled it says so rather than waiting to be asked.

  Interaction and animation are untouched: egui already requests the frames its own
  animations need, so a section still expands over fifteen frames and then goes quiet.

### Fixed

- **A lock was held across every window-close handler, which is the one thing its
  documentation promised it was not.** The parked closes were taken with
  `std::mem::take(&mut *PENDING_CLOSES.lock())` written directly in the iterator
  expression of a `for` — and a temporary guard built there lives until the end of the
  loop, not until the end of the expression. So the lock really was held while each
  handler ran.

  Rust's temporary lifetimes make this invisible at the call site: the code reads
  exactly like the correct version, and the comment beside it asserted the behaviour it
  did not have. Split into two statements, which is what ends the guard's life before
  the handlers start. A test asserts it on the source and was checked against the
  broken form to be sure it fails on it.

  Like the 1.9.2 fault it is hardening rather than something anybody hit — but it is
  the same shape, and the same argument applies.

- **The low-level mouse hook took two mutexes on every movement of the pointer.** The
  last recorded position was a pair of `Mutex<i32>`, read and written from the
  `WH_MOUSE_LL` callback — which runs on every movement of the pointer anywhere on the
  desktop, and which Windows silently unhooks if it dawdles. `start_recording`, on the
  UI thread, took both of the same mutexes. Nothing had gone wrong, but the arrangement
  meant the UI thread could stall system-wide mouse input, and vice versa.

  It is one `AtomicU64` now, both coordinates packed into the word and read back with a
  single `swap`. That also closes a gap the pair had: a reset landing between the two
  mutexes left one coordinate at the sentinel and turned the next delta into a
  subtraction from `i32::MIN`. The subtraction is saturating as well, so the callback
  matches the rule this file states for hook callbacks — that they cannot panic.

- **The current-path lock outlived the `if let` that read it**, in the playback
  engine's resolution of a called macro. The same temporary-lifetime rule as above,
  found the same way. Read into a local first.

### Performance

- **A `String` was reallocated on every frame**, under a lock the playback loop takes on
  every step: `apply_config_to_state` runs once per pass and assigned the target window
  title with `clone`. It is `clone_from` now, so the common case — the title has not
  changed — reuses the buffer that is already there.

- **A `StepRun` was built on every step of playback and immediately dropped**, because
  the run tally used the eager `or_insert` rather than `or_insert_with`.

### Internal

- **The pixel-to-grey conversions use `as_chunks`**, so the four reads per pixel are
  proved in range once at the type level instead of four times per pixel, and both
  outputs are filled once rather than zeroed and then overwritten — a 15 MB memset per
  call on a 1440p haystack, thrown away immediately.

  **Measured, and it is a wash.** Four rounds of `--selftest vision` interleaved
  between the two builds put every template and haystack size inside the run-to-run
  spread. It is recorded here rather than quietly dropped because the negative result
  is the useful one: it says the time in an image search is in correlating the pixels,
  not in reading them, which is where the four SIMD kernels already are. The change is
  kept for the zeroing pass it does not do and for being the plainer code.

- **`cargo clippy --all-targets` is clean.** It reported around a dozen warnings in the
  test module for several releases — known noise that had to be read past. Fixed:
  `as_chunks` for constant chunk sizes, a let-chain for a nested `if let`, struct-update
  syntax for the config builders, and the two `MAX_CALL_DEPTH` bounds moved into a
  `const` block, so a cap that drifts out of range now fails the build rather than a
  test run. A new warning there means something now.

- **The non-test source was checked against about thirty-five modernisation lints** —
  `as_chunks`, let-chains, `div_ceil`, `is_none_or`, the `manual_*` family and the rest
  — and came back with **nothing**. It was already idiomatic for the edition it is
  written in. Recorded so the next pass does not repeat the search.

- **`suboptimal_flops` is still deliberately not applied** in the correlation kernels.
  Clippy suggests `f32::mul_add`, which without `target_feature = "fma"` compiles to a
  call into libm rather than to an FMA instruction — a slowdown, not a speed-up. The
  vector kernels that *can* use FMA already do, through intrinsics.

### Build

- **The release binary is 10 879 488 B**, 3 072 B *smaller* than 1.9.2 despite everything
  above. eframe's `inspection` feature — the debug port this release's main fix was found
  and measured with — costs 830 976 B of msgpack and AccessKit machinery, and a shipped
  build never opens that port. It is carried during investigation and removed before the
  build that goes out; adding it back is one word in `Cargo.toml`, and the README says
  where.

### Testing

- **287 unit tests**, up from 286. The new one asserts that the parked window closes
  are taken into a binding of their own, and it was verified against the broken form
  rather than assumed to work.

- The full self-test set was run against the release build: `dryrun` (10 checks),
  `target` (8), `recovery` (6), `script=500` (12), `timing`, `vision`, `simd` and
  `churn=120`. No failures. Playback accuracy is unchanged — p99 between 1 and 22 µs
  across the nine timing scenarios over two runs, no drift, no burst on any of them.

---

## [1.9.2]

The other three windows, checked rather than assumed.

1.9.1 fixed a handbook that could not be closed, and the fix was to settle which side
owns *is it open*. The editor, the variables window and the run history are built the
same way and were never exercised the same way, so this release does that — by hand,
one window at a time — and hardens the one place where the same class of fault could
still occur.

### Fixed

- **A close could be dropped by a window that was busy.** An extra window's callback
  takes the application lock with `try_lock` and gives up the frame if it cannot have
  it, which is right: a dropped frame of drawing is invisible, and blocking would risk
  a deadlock. But the click on the X was read *inside* that lock, and
  `close_requested` is true for exactly **one** frame — so a frame that could not draw
  was also a frame that silently threw the close away, and the window stayed open.

  The close is now read before the lock and parked where the main window drains it on
  its next pass. Nothing holds either lock while waiting for the other, so the two
  cannot deadlock.

  This is hardening rather than a fault anybody hit: nothing outside the UI thread
  takes that lock today, so the `try_lock` does not currently fail. It becomes a real,
  intermittent *"sometimes the editor will not close"* the first time anything else
  does — which is exactly the kind of thing that is cheap to prevent and expensive to
  diagnose. A test asserts the order in the source, because building an `AppInner` and
  a viewport in a test would assert on a copy of the thing rather than the thing.

### Notes

- **The editor, the variables window and the run history were checked on screen**, each
  one opened, used, and closed with the main window both open and minimised: the editor
  across all three of its views, the run history against real entries, the variables
  window while a run was not going. All three open, stay open with the main window in
  the taskbar, close on the X, and stay closed. None of them shared the handbook's
  fault — their open flags are written by the button that opens them and by the close
  callback, and by nothing else, so there was never a second owner to fight with.

- Nothing about how macros run changed. The format is still 5.

### Testing

- **286 unit tests**, up from 285. The new one asserts that an extra window reads its
  close before it takes the drawing lock, and parks it rather than acting on it there.

- The full self-test set was run against the release build: `dryrun`, `target`,
  `recovery`, `script=500`, `simd` and `churn=120`. No failures.

---

## [1.9.1]

Six things reported against 1.9.0, the two that were not what they looked like, and one
this release introduced and had to fix again before it was done.

### Fixed

- **The handbook could not be closed.** Not by its X, not by F1, not with the main
  window open — the window shut and came straight back. This one was introduced by the
  deferred-viewport fix below and reported against 1.9.1 itself.

  Two owners of one fact. The handbook window is deferred, so whether it is up lives in
  a small shared state; the main window kept its own copy and wrote it back every frame
  as *if mine, then theirs* — the close set theirs to false, and the next frame of the
  main window set it to true again before anything reached the screen.

  The window owns it now and the main window only gets to ask: it pushes a change it
  made itself — exactly the way it already pushed which article to open — and otherwise
  takes the window's answer. The handshake is a five-line function with a test on each
  of its transitions, including the one that was inverted.

  It shipped because closing the window is the one thing the 1.9.1 pass never did. The
  handbook was checked minimised, maximised, in six languages and against its own
  markup, and every one of those checks left it open.

  **Escape and F1 from inside the handbook were tried and are not there.** A deferred
  viewport is handed the key *release* and never the press — measured, not assumed:
  `Key(Escape, false)` arrives and `Key(Escape, true)` does not — so there is nothing
  for a shortcut to match. Closing on the release would shut the handbook when somebody
  pressed Escape in another window and let go over this one. **F1** still toggles it
  from the main window, and the handbook's own X closes it from anywhere.

- **The extra windows froze when the main window was minimised.** The editor, the
  handbook, the variables window and the run history could not be scrolled, moved or
  even closed once the program was in the taskbar.

  They were *immediate* viewports, which are drawn from inside the parent's frame —
  and on Windows a minimised window is never given a frame, so everything it owned
  stopped with it. They are now **deferred** viewports declared from `App::logic`
  rather than `App::update`: each callback reaches the application through a weak
  handle instead of borrowing it, and the program asks for a repaint while any of them
  is open. Minimise the main window now and the handbook keeps scrolling.

- **An empty square beside every drop-down.** The chevron in a combo box's own label
  was U+25BE, which the shipped monospace font has and the proportional font it was
  drawn with does not. It has been a `.notdef` box next to a perfectly good arrow for
  several releases; egui paints that arrow itself, so the character is simply gone.

  Two faults in the check let it ship, and both are fixed:

  - The source scanner read literal characters only, so a character written as a
    `\u{...}` escape was invisible to it. It decodes the escapes now.
  - The check itself compared a character's atlas rectangle against **U+10FFFD**, a
    noncharacter the shaper discards outright — so there was nothing to compare with,
    and every real box passed. It now compares against assigned codepoints the fonts
    genuinely lack, and it is checked against what the window actually draws.

  What replaced the old curated allow-list is a measurement rather than a list, which
  is the part that matters: a list is only ever as good as the last person to update it.

- **The window sometimes came back as a tiny square.** eframe restores whatever winit
  reported at the moment the program closed, and closing it while it minimises writes
  nonsense — this machine had a 64-pixel inner size saved in `app.ron`, and every
  launch after that was a 64-pixel square in the corner. `with_min_inner_size` does not
  help, because the restored size is applied after the builder. The first frame now
  looks at the size that actually arrived and asks for the default back when it is
  below the minimum.

- **The handbook described a playback profile the window did not offer.** *Custom* is a
  description rather than a choice — it is what the settings are once you change one by
  hand — and it was documented but never shown. The drop-down now lists it when the
  settings match none of the three presets, and choosing it does nothing, because there
  is nothing for it to do.

- **The handbook showed its own markup.** The grammar had `**bold**` and `` `code` ``
  but not `*emphasis*`, and the prose used all three — so every `*like this*` rendered
  with the stars visible, in English, Ukrainian, Portuguese, Spanish and Chinese. There
  were 396 of them. Russian was the one language that never used the form and so was
  the one place the fault could not be seen.

  A single star is markup now, drawn as a slant because the shipped proportional font
  has one weight. It has to touch the word it marks, so `2 * 3` is still arithmetic;
  and `` `code` `` inside emphasis comes back out still emphasised, which is what the
  closing star of *play from `a` to `b`* needs in order to have something to close.

  The test that keeps it out is on the **output** of the splitter rather than on the
  source: no run that reaches the page may contain a star or a backtick.

- **The handbook called steps by names the interface does not use.** Every step name in
  the articles now matches the window exactly, and a test asserts it, so the two cannot
  drift apart again.

### Added

- **The handbook in all six interface languages.** Ukrainian, Portuguese, Spanish and
  Chinese join English and Russian: forty-six articles each, written rather than
  machine-translated, and the book follows whichever language the interface is set to.
  A test asserts that all six times forty-six exist and that none of them is a stub.

### Changed

- **The handbook scrolls smoothly.** The search was recomputed every frame, lowercasing
  all forty-six articles each time; the result is now cached against the query and the
  language. Scrolling is animated, the window keeps painting while the pointer is over
  it rather than only when an event arrives, and opening an article starts it at its
  own top instead of at the scroll offset of the one before it.

### Testing

- **285 unit tests**, up from 278. The seven new ones assert that the glyph check agrees
  with what the window actually draws; that every character the source draws has a
  glyph in the fonts shipped with the program; that only Chinese text leans on a font
  that comes from Windows; that every topic exists in every one of the six languages;
  that a window restored at a nonsense size comes back at the default while a
  small-but-usable one is left exactly as it was; that no article shows its own
  markup in any of the six languages; and that the handbook window, not the main
  window, is what decides the handbook is shut.

- The full self-test set was run against the release build: `dryrun`, `target`,
  `recovery`, `script=500`, `simd` and `churn=120`. No failures.

---

## [1.9.0]

The release that stopped explaining itself in the margins.

Until now the program described itself in fifty-one paragraphs of hover text scattered
through the panels, ten of which were drawn permanently under their section headings.
That had two faults, and the second is the one that mattered. The first is clutter:
several hundred words of prose sat in the window, and nobody was reading them.

The second is that a tooltip can only ever answer *what is this control*. It cannot
answer *what is this program for*, and somebody looking at **Macro package**, **Macro
library** and **Templates folder** for the first time is asking the second question
about all three at once — and getting three unrelated sentences, none of which says
that the first is for sending a macro to a person, the second for calling one from
another macro, and the third for the pictures they both depend on.

### Added

- **A handbook, built in.** Forty-six articles covering every panel, every button and
  every idea the program is built on — what a thing is, when to reach for it, what
  goes wrong, and what to do about it. Press **?** in the bottom-right corner, or
  **F1** anywhere, or find it in the command palette.

  It is not a list of controls. Alongside the reference articles there are the ones
  that were never anywhere: *Three ways to hit a button*, which is the mental model
  the whole program rests on; *How a macro usually gets built*, which is the path that
  costs the least rework; and *When it does not find the button*, which is the six
  checks in the order that finds the cause fastest.

- **It opens where you already were.** Every section header remembers which article it
  belongs to, so pressing **?** with **Text on screen** open opens the handbook at
  *Text on screen*. That is what makes removing the tooltips an improvement rather than
  a loss: help at the point of need, without the point of need being covered in prose.

- **Search that narrows.** Every word typed must match, not any of them — a list that
  grows as you type is a list you cannot narrow.

### Changed

- **The fifty-one explanatory strings are gone**, along with the ten paragraphs drawn
  permanently under section headings. The panels now carry labels and controls and
  nothing else.

  One of the fifty-one was not an explanation but a warning — on script steps that do
  something irreversible. It was already duplicated by the **⚠** in the step's own
  line, which is visible without hovering, so nothing was lost there either.

- **The handbook is written in English and Russian.** Those are the two languages this
  project's own documentation exists in. The other four fall back to the English text
  rather than to a machine translation of it, while the interface around it stays in
  its own language. As with every other string in the program, `lang/<code>.json` can
  override any of it without a rebuild.

### Notes

- Nothing about how macros run changed in this release. The format is still 5, every
  file loads exactly as it did, and the pre-flight check, the step traces and the run
  summaries all behave as they did in 1.8.0.

### Testing

- **278 unit tests**, up from 271. The seven new ones assert that every article exists
  in both languages and is long enough to be an explanation rather than a restated
  label; that ids are unique and every section header points at one that exists; that
  the markup contains only the five things it claims and balances its delimiters; that
  the inline splitter produces the runs the renderer expects; that search requires
  every word; that the four fallback languages really do read English while Russian
  does not; and — the one that keeps this release honest — that **no explanatory string
  has crept back into the interface**.

---

## [1.8.0]

The release about telling you what the program already knew.

Nothing here is a new thing a macro can do. Every feature in it is a fact the
program had worked out, used to make a decision, and then thrown away — the rungs
of a cascade that failed before the one that worked, the machine a recording was
made on, the pictures in the folder nothing asks for, the number of times a run had
to fall back on its second choice. 1.7.0 came out of a test matrix that kept finding
*quiet wrong states*: things that did not crash, did the wrong thing once, and
behaved. This release is the answer to that class of fault — say it out loud, before
the run where possible and after it where not.

### Added

- **The pre-flight check now stands in front of the Run button.** `health::check`
  has existed since 1.6.0 as a button somebody could press. It is now what happens
  when a macro starts, wherever it starts from — the Play button, the hotkey, the
  scheduler, the queue, `--no-gui`. Errors stop the run and warnings do not, which
  is the same rule everywhere; in the window the refusal is a dialog with **Run it
  anyway**, and where there is no window it is a refusal and a log line.

  A rehearsal is exempt on purpose. Trying a macro you already suspect is broken is
  exactly what a rehearsal is for, and it sends no input.

- **`--check`.** Checks the macro named by `--play` and exits without running a step
  of it: **0** if it is fit to run, **1** if it is not, **2** if the file could not
  be read. Warnings print and do not change the code.

  This is the half the window cannot provide. A macro in Task Scheduler runs at four
  in the morning with nobody to read a panel, and the failure people actually hit —
  a template renamed, a called macro moved — is knowable in a tenth of a second
  before any input is sent. Put this in front of the real run and the night's work
  either happens or does not happen, instead of happening halfway.

  `--no-check` lets a headless run start anyway. The check still runs and is still
  logged; the point is to have said so, not to have won the argument.

- **Three new rules, and the checks that passed.**
  - **A recursive `Call`** — a chain that comes back to a macro already on it. The
    interpreter has always survived this by giving up eight deep, which means doing
    the first seven levels' worth of clicking first.
  - **Reading text with no recogniser language set** — the fault 1.7.0 documented and
    could not warn about. Said once for the macro, not once per step.
  - **A display that is not the one this was recorded on** — scale or size, and only
    where something positional was actually recorded. A macro that aims by element or
    by picture is indifferent to the scale, and telling its author about a resolution
    change would be telling them about the weather.

  The report also now says what it *looked at*: `✔ 42 steps checked`,
  `✔ 3 pictures found`, `✔ 2 called macros found`, `✔ no recursive calls`,
  `✔ every block closes, every step is reachable`. A list of complaints cannot tell
  "nothing is wrong" from "nothing was checked", and those are different news at one
  in the morning. Deliberately not a third severity: these are not findings, and
  giving them a colour and a weight in the score would fill the report with forty
  lines saying nothing happened.

- **Format 5: where a recording was made.** Each recording now notes the screen size,
  the scale, which monitor of how many, the application in front, its window, and the
  keyboard layout. Nothing replays from any of it.

  The comparison is the feature. Every row that differs from this machine, right now,
  is coloured and shown as `150 % → now 100 %` — which turns *"why does this macro
  click twenty pixels low today?"* from a bisect into a glance. The same comparison
  feeds the pre-flight rule above.

  Off switch under **Recording**, because it goes into the macro file and a macro is
  a thing people send each other: the front window's title and its executable name
  are in there.

- **"Why a step did that".** The whole cascade, in order, with the number that decided
  each rung:

  ```
  ✖ UI Automation  —  nothing found        (2 ms)
  ✖ Image          —  0.610 / 0.85        (41 ms)
  ✔ Window-relative —  Roblox              (0 ms)

  Where:  recorded 812, 641 → actual 794, 655  (-18, +14)
  Took: 43 ms   Cycle: 7
  ```

  Every number in that block was already computed — the score is in `match_score`
  whether the picture matched or not, and the resolver walked the rungs to pick one.
  All that was missing was not discarding the ones that lost. The last 64 steps that
  aimed at something are kept, always, whether or not anything is watching: you
  cannot switch this on after the thing you wanted to explain has happened.

- **What a run had to do, beside what it did.** The history now records, per run:
  fallback resolutions, extra OCR preparation passes, recovery blocks entered, the
  milliseconds spent inside them, and steps that missed.

  `Completed` was never the whole answer. A run that finished having fallen back
  three times, read the screen twice more than it meant to and gone through a
  recovery block is a macro that is about to stop finishing — and it looked exactly
  like a healthy one. A run whose every miss policy says *carry on* finishes, and
  finishing is not the same as having worked.

  Counts, and deliberately **no second score**. Each line is something the program
  observed; a number out of a hundred would be an opinion, and an opinion in the same
  shape as the pre-flight score — which measures something else entirely — would
  invite a comparison that means nothing.

- **The templates folder, against what the macro asks of it.** Pictures used,
  pictures named and missing, pictures nothing in this macro names, and pictures that
  are byte-for-byte the same image saved twice — compared by decoded pixels, not by
  file bytes, because the duplicate people accumulate is the same crop re-saved by a
  different editor.

  Nothing is deleted, and the wording is *"this macro does not name"* rather than
  *"unused"*, with the reason on screen: the folder is shared by every macro on the
  machine and this one cannot see the others. A list that reads as permission is how
  somebody deletes a picture four other macros depend on.

### Fixed

- **A macro from a later version of this program loaded, lost half of itself, and
  saved the loss back over the original.** Serde discards fields it does not
  recognise without a word, so such a file opened here looking perfectly healthy;
  saving it wrote back only the part this build understood, and the loss was visible
  nowhere until the machine that wrote it opened it again.

  Reading forwards is a direction nobody had looked in — every compatibility test in
  the project ran the other way. The file still loads, because most of it will work
  and refusing outright would strand somebody whose other machine is one release
  ahead. What has stopped is saving over it without being asked: the load says so in
  the status bar at the moment it is found out, and the save puts up a dialog naming
  both formats.

- **Two glyphs that have never drawn.** `✓` (U+2713) and `✗` (U+2717) are not in the
  bundled font: they render as empty boxes. Both had been on the hand-curated list the
  glyph test checks against since 1.6.0, so the test blessed them — and the panels that
  used them were never looked at with human eyes, because their matrix rows had not
  been driven.

  The package panel has therefore shown `□ claim.png` instead of a tick since
  dependency checking shipped. Everything now uses the heavy forms, `✔` (U+2714) and
  `✖` (U+2716), which do draw; the two that do not have been struck off the allow-list,
  so the test now catches them rather than permitting them.

  Found by driving the 1.8.0 matrix rows: the step-trace panel drew three boxes where
  it should have drawn two crosses and a tick.

- **The newer-file save dialog offered "Don't run it".** It reused the pre-flight
  gate's cancel string, so a dialog asking whether to *save* answered a question about
  running. Its own wording now, in all six languages. (Matrix row T-36.)

- **A newer-format file, saved over knowingly, kept its old version number.** Accepting
  the loss correctly dropped the fields this build cannot read — and then wrote the file
  back still claiming format 99. A version number that disagrees with what is in the
  file is the exact fault the whole guard exists to prevent, only written by us this
  time. It is now stamped with this format on the way out, and reopening it does not
  warn again. (Matrix row T-38, plus a regression test.)

- **A folder template read as `0 B` in the resource audit.** The size came from
  `<name>.png`, which a folder template does not have. "0 B" beside a folder holding
  three pictures reads as *an empty file, delete it* — the one impression that list
  must never give. It now adds the variants up.

- **A template kept as a folder of variants was reported as a missing picture.** A
  folder of PNGs is one template with several variants — the same button in two
  themes — and `load_template_set` has loaded them since they were introduced. The
  list of what is on disk only ever looked at `*.png` files, so the checker called a
  folder template missing and the packager left it out of packages.

  Harmless while the check was a button somebody pressed. Not harmless in this
  release, where a false error stands between the user and the Run button — which is
  what turned it up. `template_exists` is now one function that the checker, the
  packager and the audit all share.

### Changed

- A macro saved by this version says format 5. Files at versions 1 through 4 load
  exactly as before and are written back at 5; nothing about how they behave changes.
- `call_exists` in the checker now goes through `resolve_call_path` — the same
  function the interpreter uses — instead of its own copy of the three-places rule.

### Testing

- **271 unit tests**, up from 255. The sixteen new ones cover: a file from a later
  version being noticed and not relabelled; an older file being written back at the
  current format; the recording note surviving a round trip while `from_future` never
  reaches the disk; DPI read as the percentage Windows displays; a macro that calls
  itself; a three-macro ring; a diamond that is *not* a cycle; a disabled `Call` that
  cannot make one; the recogniser-language warning said once for nine steps; a
  changed display mentioned for a flat recording and not for an image-aimed macro;
  the passed-checks counts being per picture rather than per step; a run record
  carrying its effort while a pre-1.8.0 line still loads; text clipping counted in
  characters, which is what stops a Cyrillic screen reading from panicking a format
  call; and a newer-format file, saved over knowingly, coming back as this format
  rather than still claiming the old one.

---

## [1.7.0]

The release that came out of running the manual test matrix end to end.

Nothing here is a new feature anyone asked for. Six things were found to be wrong by
driving the program the way a person would - a real desktop, real synthetic input, a
real screen to look at - and this is the release that fixes them. Two of the six had
been wrong since 1.5.0 and were invisible precisely because they were quiet: the
program did not crash, it just did the wrong thing once and then behaved.

### Fixed

- **The first image search of every run found nothing.** Desktop Duplication answers a
  request with a cursor-only update when nothing else has moved, and that update
  carries no desktop image at all. The guard against it read
  `if cursor_only && we_already_have_a_frame`, which let the very first one through -
  and the first one is the likeliest to be cursor-only, because a cursor twitch is
  what wakes the compositor on a still screen. The black texture was copied in as
  "the screen", and because the capture had *succeeded* the GDI fallback beneath it
  never ran.

  A macro whose first step was `Click image` with *stop the script* on a miss ended
  on that step. The same macro inside a `Wait for` loop corrected itself on the
  second look, which is why this survived three releases.

- **A wide, short template could be found confidently in the wrong place.** The search
  sweeps a shrunken copy of the screen first and then refines around the winner. A
  template much wider than it is tall loses the rows that tell it apart when it
  shrinks, so the coarse pass can prefer somewhere else entirely and the fine pass
  never looks at the right place. Measured: a 170x32 button came back **123 px below
  itself** with a score of **0.867** - above the default threshold of 0.85, so the
  step reported success and `Click image` would have clicked the wrong thing.

  The fine pass now gets the best four coarse candidates instead of the best one. The
  same button is now found at its own centre, score 0.9999. Full-screen searches cost
  about 3 % more; a silent wrong click cost rather more than that.

- **A correlation score could be `NaN`.** On a window of one flat colour the sum of
  squares and the square of the sum are the same number, and in floating point the
  subtraction can land a whisker below zero; the square root of that is `NaN`, and
  `NaN <= EPSILON` is false, so the guard meant to catch it passed it through. It was
  invisible while every comparison was a `>` test - `NaN` loses those silently - and
  surfaced the moment the code above started keeping a shortlist. Variance is clamped
  at zero now, and the `NaN` check is spelled out.

- **`Call macro` by a relative name did not work under `--no-gui`.** A `Call` step
  resolves a bare name next to the macro that named it, and the headless path never
  recorded which file the macro came from. It looked beside the executable and in
  `macros/`, said so in the log, and stopped. The same macro worked in the window.

- **`--no-gui` ignored almost all of the settings.** The headless path took four
  values - repeat count, speed, absolute mouse, delay between repeats - and built
  everything else from defaults. Twenty-four settings were quietly dropped, including
  **the frame-rate guard, window anchoring, pause-while-not-in-front, human-like
  movement, timing jitter, fast screen capture and the screenshot on a failed step**.

  A macro set up in the window and then put into Task Scheduler behaved differently,
  with nothing said about it. `run_headless` now applies the configuration in full and
  the caller's four values on top, which is the order the window has always used.

- **`--selftest vision` reported a capture fault that was not there.** Its
  cross-check cuts the most contrasty square it can find from a GDI frame and looks
  for it in a duplicated one. Contrast is not uniqueness: a square of horizontal
  interface banding has plenty of variance and matches equally well a few pixels to
  the left, and the test duly announced *WRONG PLACE - off by 4 px* on a machine
  where the two paths agreed exactly. It now also compares the pixels where the
  square was cut from, and says so when the square simply appears in more than one
  place. A test that cries wolf about the capture path costs somebody a day.

### Added

- **A language for text recognition.** Under **Text on screen**: *Reads in*, listing
  the recognisers Windows actually has installed, or the display languages as before.

  It exists because the two are often not the same. On a Russian Windows reading an
  English game, `Gems: 1,250` came back as `Gems :` with the digits lost and the zero
  of `02:34` read as a Cyrillic **а**. No amount of pixel preparation argues a
  recogniser out of the wrong alphabet - measured, all five preparation profiles and
  *try each* returned the identical wrong answer. Setting the language to `en-US` on
  the same machine reads `1,250` and hands `Read number` the 1250 it had been losing.

  Stored as `ocr_lang` in `config.json`; empty means the Windows display languages,
  which is what every version before this did and still the default.

### Testing

- **255 unit tests**, up from 254. The new one plants a wide, short template and a
  decoy that shrinks to the same flat grey, and asserts the search comes back with
  the real one.
- **The matrix pass that produced this release** covered about 170 rows with evidence,
  including all of section K, all of the self-running `.exe` footer, and the step-mode
  rows nobody had touched. It also produced two findings that turned out to be the
  test being wrong rather than the program - both are written up in `TESTING.md`,
  because the way a measurement misleads is worth keeping.

### Notes

- Recording cannot be tested by a program. The recorder discards injected input on
  purpose - otherwise it would record its own playback - and everything a script can
  synthesise is injected. Sections B, F and the click-snipping rows still need human
  hands.

---

## [1.6.0]

The release about not trusting a coordinate.

A recording has always been a list of coordinates, and coordinates are the brittle
part: the window moves, the resolution changes, the list opens one row lower, and
every click lands somewhere it should not. The cure has existed since 1.2 — ask
Windows for the button by name, or find its picture — but as separate steps somebody
had to choose between and build by hand.

1.6.0 makes that the program's job. It also makes the program able to say what it
just did, why it stopped, and what it is about to do — which turns out to be the
same problem seen from the other end.

### Added

#### Targets — one thing to point at, several ways to find it

- **`Click target`, `Wait for target`, `Read target`.** One step that says *what* to
  press and lets the program work out *how*:

  ```
  Click "Start"
      1. UI element        Button, name "Start"
      2. image             rec_20260826_0342_01
      3. window-relative   Game +412,318
      4. coordinate        1245, 720
  ```

  Tried in order; the first that resolves wins. The step records which one did, so
  the run history and the overlay can say *how* a click was aimed rather than only
  where.

- **Reliability: Maximum · Balanced · Fast.** The whole interface to that cascade.
  You pick a word, not an order of methods.

- **Recording now remembers what was clicked, not just where.** At the moment of each
  click it keeps the window title, the process, the window rectangle, the click's
  position inside that window, and — where the application exposes one — the UI
  Automation element under the cursor. All of it is free at that moment and
  impossible to recover an hour later.

- **Analyze recording.** A table of what each click is now and what it could be, with
  a tick per row. *Apply* rewrites the script.

- **Markers.** A name for a place in the recording, and the fix for the trap the
  README has always warned about: `Play events 73…184` names events by number, and
  deleting an event in the editor leaves every number saying what it said. A marker
  moves with the events around it.

#### Rehearse, check, diagnose

- **Test run.** Play the whole macro with nothing sent. Pictures are still searched
  for, text still read, variables still counted; every click and keystroke is counted
  instead of sent. `Run a program`, `Quit the app` and `Close a window` are stepped
  over, and marked ⚠ in the list.

- **A screenshot and an explanation when a step gives up:**

  ```
  the best match for "claim" scored 0.41, and the step asks for 0.85
  Try:
    • the picture was cut at 150 % and this screen is at 100 %
    • it searched the whole screen — telling it where to look is faster
  ```

  No model and no network. Every number in that sentence was already known.

- **Run history** — what happened the last twenty times, newest first, with the
  screenshot attached.

- **Macro health.** Check it without running it: missing pictures, `Play events`
  ranges that no longer fit the recording, loops with no way out, fixed coordinates,
  a recovery policy naming a block nobody wrote — plus a reliability score and a plain
  list of what this macro is able to do.

- **Step statistics** — how many times each step ran, how often it worked, its average
  and its worst time. Kept against the step itself rather than its position.

#### Surviving the night

- **Recovery blocks.** A fifth answer to *what if it is not there*: run a named block
  of steps, then try once more. Retrying is right when the thing was not there yet;
  it is useless when something is *in the way*, and this deals with the obstruction.

- **Adaptive waits.** Wait as long as this step has usually needed, learned from the
  run history, with your number still the ceiling.

- **Window actions** — activate, minimize, maximize, restore, move, resize, centre,
  close, and wait until a window is in front, appears or closes. One step, one
  dropdown.

- **Target window by program, not just title.** `roblox` finds
  `RobloxPlayerBeta.exe`, and it is found again if the game restarts.

- **The clipboard as a step**, including **wait until it changes** — how a macro knows
  a copy happened rather than guessing at a delay.

- **Nine built-in values** — `{clipboard}`, `{window.title}`, `{process.name}`,
  `{time}`, `{date}`, `{mouse.x}`, `{mouse.y}`, `{screen.width}`, `{screen.height}`.

- **Notify** from the tray, and **Screenshot** to a file.

#### Building it without building it

- **Multi-select.** Ctrl+click and Shift+click, then duplicate, delete, enable,
  disable, or **wrap in If / Repeat / Group** in one press.

- **Group** — a name for a run of steps, with no effect on how the macro runs.

- **Twelve templates** — wait for a button then click it, handle a popup, log in,
  retry-and-recover, wait until an app is ready, farm until a counter, run until a
  time, and more.

- **Macro library** — every macro in your `macros/` folder, insertable as a call.

- **Optimize recording** — strips hand tremor, auto-repeat, the walk to the starting
  position and idle time past two seconds. **Shows you what it would remove before it
  removes anything.**

- **Command palette.** `Ctrl+K`, then type part of a name.

- **Playback profiles** — *Desktop*, *Game*, *Human-like*.

- **Case-aware snippets** — *Exact case*, *Any case*, *Follow case*, where `ADDR`
  gives a shouted replacement and `Addr` a capitalised one.

#### Packaging

- **`.mrpkg` — a macro and everything it needs, in one file.** The pictures it looks
  for and the macros it calls travel with it. Still gzipped JSON: open it in a text
  editor and read it.

  This closes two of the limitations this README has always listed: an exported
  `.exe` needing its `templates/` folder beside it, and a `Call` step naming a file
  that has to travel separately.

- **Package inspector** — what a macro needs and whether it is all here, before you
  send it anywhere.

- **A capability list before you run somebody else's macro** — mouse, keyboard, screen
  capture, OCR, UI Automation, clipboard, files, windows, and with a ⚠ the three worth
  a second look: starting programs, shutting the PC down, calling another macro file.
  Worked out from the steps, so it cannot be wrong about a macro somebody handed you.

- **Several macros open at once**, and a **queue** that plays them one after another.

#### One executable that compiles itself for every processor

- **The `.v3.exe` is gone, and nothing was lost with it.** The image search — the one
  loop in this program where the instruction set is worth anything, everything else
  waits on the GPU or on Windows — is now compiled **four times into the same
  binary**, and the right one is chosen from CPUID when the program starts.

  | Kernel | What `-C target-cpu` would have to say | 128×128 search, 1280×720, one thread |
  |---|---|---|
  | scalar | *(fallback)* | 19.9 ms |
  | sse2 | `x86-64`, `x86-64-v2` | 6.2 ms — **3.2×** |
  | avx | `sandybridge`, `bdver1-4` | 4.9 ms — **4.1×** |
  | avx2 | `x86-64-v3`, `znver1-3` | 4.7 ms — **4.2×** |
  | avx512 | `x86-64-v4`, `znver4`, `znver5` | not on the machine this was measured on |

  Most of the win is the first step, and every x86-64 machine now gets it without a
  flag or a second download. Four instruction sets in one binary cost **20 480 bytes**
  — measured, both builds made here: 10 208 768 B before, 10 229 248 B after.

  Two things this deliberately does **not** claim. It is not nine whole-program
  builds in one file: `-C target-cpu` is a property of a compilation, rustc emits
  one, and the only way to have nine is to ship nine and unpack one at start-up —
  which is self-extraction, which is the single strongest antivirus heuristic there
  is, and this project does not even use UPX for that reason. And `#[target_feature]`
  enables *instructions*, not a *scheduling model*: a `znver3` build also gets Zen 3's
  instruction ordering, and no run-time choice can. That part still belongs to
  whatever the build was made for. It is also the small part.

- **`--simd <set>`** pins the kernel by hand — `auto` (the default), `scalar`, `sse2`,
  `avx`, `avx2`, `avx512`, or the `-C target-cpu` spelling that maps onto one of them
  (`x86-64-v3`, `znver3`, …). A set this processor does not have is not an error: it
  says so and runs the widest one it can.

- **`--selftest simd`** lists every kernel in the binary, says which ones this
  processor can run, races them on the same search and checks they all find the
  planted template in the same place — the agreement column matters more than the
  milliseconds, because a kernel that is quick and wrong finds buttons in the wrong
  place and would only do it on the machines that have it.

- **AVX-512 has to earn its place.** On Zen 4 the 512-bit operations go through
  256-bit units and on several Intel parts they pull the core's clock down, so the
  wide and the narrow kernel are raced once at start-up — about a millisecond — and
  the wide one is taken only if it wins by five per cent. A table of model numbers
  would have been wrong within a year.

### Changed

- **File format 3 → 4.** `markers` is new, and `Play events` may carry the two marker
  names it sits between. Both are optional, so a version 3 macro loads and behaves
  exactly as it did — there is a test that says so.

- **The flow-control steps read as English.** `While` is now *Repeat while*, `If` is
  *Do this if*, `Else` is *Otherwise*, `Break` is *Stop the loop*. Only the words
  changed.

- **Every step now has an identity**, filled in the first time an older macro is
  loaded, so statistics and learned timings follow the step rather than its position.

- **`Click image`, `Press element` and `Click` are unchanged and still load.** They
  simply stop being the first thing the Add menu offers.

- **Built with Rust 1.98.0**, and the release profile moved from `opt-level = "z"` to
  `opt-level = 3`. The image search runs at half the time it used to — 22 ms to 11 ms
  on a 2560×1440 desktop — at the cost of about 1.7 MB of binary.

- **The correlation kernels carry two accumulator chains instead of one**, and take
  two vectors per turn of the loop. This is not a micro-optimisation, it is the
  difference between the tiers coming out in the right order: written the obvious way
  with a single accumulator, the AVX2 kernel measured *slower* than the plain AVX one
  it exists to beat — 5.3 ms against 4.5 on a 128×128 template — because one chain
  makes the loop latency-bound rather than throughput-bound, and an FMA's four-cycle
  latency is longer than a plain add's three. `--selftest simd` is what caught it,
  which is the argument for printing a row per kernel rather than one speed-up
  number.

### Fixed

- **`EXPANDER.md` was wrong about capitals.** It said `ADDR` and `addr` were the same
  abbreviation. They never were — the comparison has always been exact and the shouted
  form simply did nothing. The behaviour the page described is now available as *Any
  case*.

- **A flaky test.** `bezier_bows_away_when_curved` seeded itself from the clock and
  failed at random about one run in four hundred. It now judges two hundred seeded
  draws.

- **Three section headings and the Test run button drew empty boxes.** *Optimize recording*, *Templates* and
  *Open macros* used emoji the bundled font has no glyph for. The test meant to catch
  this blocked a range of codepoints rather than checking against what the font
  actually has, and the font's coverage has holes in it. It is now an allowlist.

### Testing

- **254 unit tests**, up from 144. Three of the new ones are about the multi-kernel
  image search: that every instruction set this processor can run finds the same
  template in the same place (at widths that are a whole number of vectors, widths
  that leave a tail in every kernel, and widths narrower than a vector, because each
  is a separate chance to read past the end of a row); that `--simd` takes the
  `-C target-cpu` names people will actually type; and that whatever gets chosen is
  something this machine can run.
- **Four new self-tests**: `--selftest dryrun` proves a test run touches nothing (it
  checks the effect, not the flag — the `Run a program` step really does name a
  command that would leave a file behind); `--selftest target` drives the cascade with
  every method but the coordinate deliberately unfindable; `--selftest recovery` walks
  a recovery block in and out; `--selftest simd` races every kernel in the binary and
  checks they agree.
- `timing`, `vision`, `script`, `churn` and `soak` all unchanged in behaviour and
  green.

---

## [1.5.0]

The release about the two things that were still done by hand. Recording produced
coordinates and somebody turned them into pictures afterwards; a step that found
nothing said nothing and the script walked on. Both are now the program's job.

Plus the capture path, which turned out to be the floor under everything that looks
at the screen — and not for the reason anybody assumed.

### Added

- **Recording straight into picture steps.** Switch on *Snip a picture at every
  click* and a recording keeps a small square of the screen from around each click.
  When you stop, it offers to turn those clicks into `Click image` steps, writes the
  squares into `templates/` with their DPI sidecars, and rewrites the macro.

  Nothing is thrown away. Everything between one converted click and the next stays
  a `Play events` step over exactly that range, so the keystrokes, the scrolling and
  the recorded timing all survive — only the clicks become pictures. A drag is left
  alone: press, move, release is not a click that a picture can stand in for, and
  turning one into a `Click image` would drop the drag without saying so.

  This is the feature the image search was always for. It has been possible to build
  a macro out of pictures since 1.2, and it required snipping each button separately,
  naming the files, and remembering which click each one was for.

- **What a step does when it finds nothing.** Every step that looks for
  something — `Click image`, `Find image`, `Wait for`, `Find element`, `Press
  element`, and the new `Call macro` — now carries one field with four answers: carry
  on, stop the script, leave the loop, or try again *N* times and then stop.

  Until now there was only the first, and it was not a choice anybody had made. A
  `Click image` whose picture was not on screen did nothing at all and the script
  walked on, which is right for a poll inside a `While` and wrong for everything
  else. It is the difference between a night macro that stops when the game logs you
  out and one that clicks at an empty desktop until morning.

  `Carry on` stays the default, so every macro written before this release behaves
  exactly as it did — there is a test that says so.

- **Call macro.** A step that runs another macro file's script and then carries on.
  The reuse people ask for when they ask for functions, without becoming a language:
  a subroutine is an ordinary macro, edited in the same editor, played on its own to
  test it, and shared between projects as a file.

  The variables are the same ones, so a caller sets `target` before the call and
  reads `result` after it — which is what a list of steps can express without growing
  a grammar. Nesting is capped at 8; a macro that names itself stops at the cap
  rather than taking the process down with it, and `--selftest script` proves it.

- **A window that shows the variables while the script runs.** The other half of the
  debug overlay. The overlay says where the script is looking; this says what it has
  found out — every variable, its value, which step is about to run, and how many
  `Call` steps deep that step is.

  With it, **pause before each step**: the run stops before every step and waits for
  *Next step*, which turns debugging from reading a log afterwards into watching it
  happen. Stop still works while parked — a run that only one button could free, in a
  program whose whole point is a global stop key, would be a trap.

- **Desktop Duplication for screen capture.** About five times faster on a whole
  screen and twenty times on the small region a script should be using. Falls back to
  the old path on its own where the machine will not run it — an older display stack,
  a remote session, a rotated monitor, or a rectangle spanning two screens.

### Changed

- **Screen capture no longer copies the frame three times.** `CreateDIBSection` means
  `BitBlt` writes straight into memory this process can read, which removes the
  `GetDIBits` that used to copy and reformat the whole frame a second time. The
  device context, the bitmap and the buffer are kept between captures instead of
  being created and destroyed each time. And the red/blue swap is gone: a `Frame` now
  says which order its bytes are in, and the two consumers that care read that.

  That last one was undoing itself. `capture` swapped BGRA into RGBA and then
  `upscale_to_bgra` swapped it back on the way to the OCR engine — two full passes
  over a fourteen-megabyte buffer that cancelled out.

### Fixed

- **The self-running `.exe` footer trusted a length it read out of the file.** The
  last outstanding item from stage 1 of [TESTING.md](TESTING.md), and the only place
  in the program that takes a number out of bytes it did not write and then acts on
  it.

  `16 + len` was an unchecked addition. In a release build, where overflow checks are
  off, a claimed length near `u64::MAX` wraps it to zero — and the `checked_sub`
  underneath then succeeds, and a `vec![0u8; len]` asks for sixteen exabytes. With
  `panic = "abort"` a failed allocation is not an error anybody handles; it is the
  process gone. The addition is now checked, the length is capped at 64 MB before
  anything is allocated, the decompressor stops at 512 MB so a compression bomb
  cannot expand into memory, and the payload goes through `normalize` — which caps
  the event count and rejects unbalanced blocks — exactly like a file opened through
  the Open dialog. Until now the one input nobody chose was the one input nobody
  checked.

### Measured

`--selftest vision`, 2560×1440, with a game running:

| | 1.4.0 | 1.5.0 |
|---|---|---|
| One 400×300 capture, repeated 200 times | 6.06 ms | **0.12 ms** |
| Whole 2560×1440 screen | 30.2 ms | **4.0 ms** |
| One script step, 400×300 area | 7.0 ms | **1.7 ms** |
| One script step, whole screen | 52.2 ms | **32.4 ms** |

The interesting part is why, because the plan going in was wrong. Removing the GDI
object churn and the extra copy changed the number by nothing at all: `BitBlt` out of
the composited desktop costs about six milliseconds *before it has copied a useful
pixel*, and the same blit between two memory contexts costs 0.10 ms. The destination
was never the expensive part — the table prices a DIB section against the device
bitmap it replaced and they come out equal. The readback was. That is what Desktop
Duplication does not pay, and it is also why a script polling a settled screen now
costs almost nothing: a frame the compositor never sent is a frame nobody has to
read.

The benchmark checks the answer as well as the clock, and that took three goes.
Comparing two captures for equality fails on any screen with something live on it —
the first version reported that 97 % of the frame had changed, and it had, because
there was a game running. Sampling only the pixels that two consecutive captures
agreed about was better and still not enough. What works is not a pixel count at all:
cut a template out of a frame taken the old way and look for it in one taken the new
way. Channels swapped and the correlation collapses; a row pitch ignored and the hit
lands somewhere else; the wrong monitor and it is out by a screen's width. It reports
**0 px off, score 1.000**.

`--selftest script` is new: twelve checks over the four miss policies, the retry
timing, calls passing variables down and back, the recursion cap firing eight frames
deep, and the step gate parking a run and Stop releasing it. Then three thousand
rounds of every policy with calls nested under them — no press left held, every round
ending the way the interpreter says it did.

`--selftest churn=120`: 10 516 lifecycle transitions, nothing left held, no
generation escaped cancellation.

### Notes

- The squares are cut on the collector thread, not in the input hook. A low-level
  hook holds up every keystroke and click on the machine until it returns, and a
  screen grab is milliseconds; taking one in there would make the mouse stutter for
  everybody.
- Generated `Click image` steps default to **stop the script** rather than carry on.
  A step this program wrote, that cannot find the button it was cut from, has nothing
  useful to do next. The offer has a combo box on it if you disagree.
- A `Call` step names a file, and an exported self-running `.exe` carries only its own
  macro. Export a macro that calls another and the callee has to be beside it.
- Desktop Duplication is per thread and gives up per thread. A search thread that
  cannot get a duplication is saying something about itself, not about the machine; a
  process-wide count would let one unlucky thread put every future playback back on
  the slow path for the rest of the session.

---

## [1.4.0] — unreleased

The release about looking at the screen. Four ways to find something, in the order
they should be tried, and a way to see which one is failing.

### Added

- **The expander is a command line.** An entry can now do something instead of typing
  something: play a macro (naming a file makes the abbreviation a launcher), stop
  everything, or run a program. `;farm` starts the night's macro and `;stop` ends it,
  in any window, without a hotkey.
- **Where to look.** Every image step and image condition takes a search area: the
  whole screen, the window in front, a fixed rectangle, near where the same picture
  was last seen, or **relative to another picture**. The last of those is the one a
  threshold cannot do — a row of identical buttons is identical, and which one to
  press is decided by the heading above it.

  This is the one that pays. Measured on a 2560×1440 desktop with `--selftest
  vision`, one script step looking for a 64×64 template:

  | Area | Capture | Search | Total | Looks per second |
  |---|---|---|---|---|
  | 2560×1440 | 43.8 ms | 33.8 ms | 77.7 ms | 12.9 |
  | 1280×720 | 16.8 ms | 9.8 ms | 26.7 ms | 37.5 |
  | 400×300 | 6.2 ms | 1.6 ms | 7.8 ms | 128.4 |

  Ten times the poll rate, from one field.
- **Two thresholds instead of one.** A score wobbling around a single threshold —
  0.79, 0.81, 0.79, 0.82 — reads as four state changes and a script that acts on
  each. A second, lower threshold to *lose* a picture turns that into one. Optionally
  with "found in N of the last M looks" on top.
- **One object, several pictures.** A folder under `templates/` is a set: a button's
  resting state, its hovered state and its dark-theme self are one step, not three.
- **Templates remember what they were cut at.** A picture snipped on a 150 % display
  is half again the size of the same button on a 100 % one, and no threshold bridges
  that. Saving a template now writes a small `Name.png.json` beside it, and loading
  one rescales it for the display it is about to be looked for on. Templates made
  before this release have no sidecar and are left exactly alone.
- **Find image** — a step that looks and reports without clicking, writing
  `target.found`, `.x`, `.y`, `.w`, `.h`, `.score` under a name you choose.
- **Outline matching.** Correlating gradients instead of grey levels, which survives
  a theme change and a highlighted row. One checkbox, and the thing to reach for when
  a template that used to work stops. About 1.6× the cost of an ordinary search over
  the whole screen, and much less than that over an area.
- **Preparing the pixels before reading them.** Windows OCR was built for documents:
  dark text, light paper, generous size, and it has no knobs. Screen text is none of
  those. Five profiles — none, interface, small text, game HUD, digits — do the grey,
  the contrast stretch, Otsu's threshold and the inversion that a light HUD over
  moving artwork needs. A sixth, *try each*, walks them and keeps the best reading.
- **Saying what a reading should look like.** A whole number, a decimal, a clock, or
  a small pattern (`#` a digit, `@` a letter, `?` one character, `*` any run). A
  reading that does not fit is refused and the variable is left alone, rather than
  quietly becoming a zero. This is also what *try each* judges profiles by.
- **A fit score.** Not the engine's confidence — that number is on a scale nobody can
  interpret and is not comparable between engines. This one is computed from the text:
  half whether the format parses, half how much of the reading belongs to the alphabet
  that format implies. Shown in the panel so a profile can be chosen by comparing
  numbers.
- **Variables can hold text.** What the recognition read, what the window is called,
  what is on the clipboard — none of these could be kept before. `{name}` in any step's
  text is replaced by what that variable holds. Comparisons stay numeric when both
  sides read as numbers, so a count read off the screen still compares against 10;
  otherwise they compare as text, and a new `has` asks about containment.
- **Read text**, **Get text** and **Put text** — recognition into a variable, and the
  clipboard, the title of the window in front, the program in front, or a file, in
  either direction.
- **Process running** as a condition, matched on part of the name.
- **UI Automation.** Asking Windows what is on screen instead of looking at it: an
  element found by its name is found at any resolution, under any theme, with no
  threshold to tune. Measured at 9 to 35 ms against the window in front, depending
  on how much that window exposes — faster than any picture search here, which is
  what puts it at the front of the cascade. Matching a name as a *substring* rather
  than exactly costs several times that, which is why naming a control type matters. New steps *Find element* and *Press element* and a new condition
  *Element on screen*. *Press element* asks the application to press it, so nothing
  moves on screen and the window need not even be in front; it falls back to a real
  click when the control offers nothing to press.
- **A window that shows what the script is looking at.** See-through, over everything,
  and impossible to click. It draws the search area, the match with its score, the
  rectangle text was read from, and the element that was found. A failed search tells
  you a number; a number cannot say whether it looked in the wrong place, at the wrong
  size, or at the right thing under a tooltip. A rectangle can.

### Changed

- **The correlation was doing twice the work.** For every position the window could
  land in, it worked out the template's own sum and variance again — a second full
  pass over the template for every pixel of the screen, and the answer was the same
  every time. The template is now prepared once, and the correlation takes one pass
  instead of two.
- **The inner loop uses the vector unit** where the processor has one: AVX2 and FMA,
  detected at runtime, so the plain x86-64 build gets it too. Worth 1.1× to 1.6×
  depending on template size — less than the one-pass change above, and the reason
  the two are listed separately. `--selftest vision` prints both numbers and checks
  that they agree about where the picture is.

  The accumulators are kept across the whole window rather than folded down at the
  end of each row. Per row it was three horizontal sums for every eight floats, and
  on the coarse pass — where a 32-pixel template shrinks to eight wide — that cost
  more than the multiply-adds it replaced: measured at 0.93×, a real regression,
  before it was moved.
- **The search is spread across cores.** Horizontal stripes, merged in row order with
  a strict comparison, so a tie resolves exactly where the single-threaded sweep would
  have put it.
- Brightness is now carried as 0…1 rather than 0…255. The correlation is invariant to
  both, but the one-pass form subtracts a sum of squares from a square of sums, and at
  255 that subtraction throws away most of an f32's precision on a large template.
- The clipboard helpers moved out of the expander into the platform layer, where the
  script steps can reach them too.

### Fixed

- **An element query with nothing filled in matched everything.** The three fields
  become search conditions, and three empty ones collapse to "true" — a subtree
  search for "true" returns the root, so an unfilled query reported the whole window
  as found and *Press element* fell through to clicking the middle of it. A step
  freshly added from the menu is exactly that query. It now matches nothing and says
  so in the log. Found while writing the manual test matrix, not by the test suite.

### Notes

- **UI Automation only sees what an application chooses to expose.** Unity, DirectX,
  OpenGL and canvas-drawn interfaces expose nothing at all, and across a privilege
  boundary it is limited or silent. In Roblox — the game this program was written for
  — it will find nothing. It is a feature for automating ordinary programs, and the
  right arrangement is a cascade: element, then picture, then text, then coordinates.
- Every macro written up to 1.3.5 loads unchanged. The search area defaults to the
  whole screen, the preparation profile to none, the expected format to anything, a
  bare number in `vars` is still a number, and a template with no sidecar is not
  rescaled.
- *Try each* costs one recognition per rung it climbs, and stops at the first perfect
  fit. It belongs in a step that runs occasionally, not in a tight polling loop.
- The overlay is a diagnostic and is off by default.

---

## [1.3.5] — pre-release

The text expander, and the desktop fix that 1.3.0 went out without.

### Added

- **Text expander.** Type a short abbreviation and it becomes the longer text saved for
  it. Three trigger modes — after a delimiter, behind a prefix marker, or the moment the
  abbreviation appears — set globally and overridable per entry. Replacements carry
  `{date}` and `{time}` with optional patterns, `{datetime}`, `{clipboard}`, `{cursor}`,
  `{key:Tab}` and `{random:a|b|c}`, with a backslash to escape a literal placeholder.
  Multi-line text is supported, and each entry chooses whether it is typed a character
  at a time or pasted through the clipboard. Entries live in `expansions.json`.
- Per-entry enable, a global switch that is **off until asked for**, and a list of window
  titles where the expander stays quiet — a password manager and a terminal belong there.

### Fixed

- **A macro kept running while Task View was open.** Virtual desktop isolation asks
  `IsWindowOnCurrentVirtualDesktop`, which answers honestly and unhelpfully: Task View
  is an overlay drawn *on* the current desktop, so nothing had changed as far as the
  check was concerned, while synthetic clicks landed in the desktop switcher — where
  they create desktops, close them and move windows between them. Playback and recording
  now both hold while a shell switcher owns the foreground. Only the two unambiguous
  switcher classes are matched; the Start menu shares a class with ordinary packaged
  apps, and a macro that silently refused to run against one of those would be a worse
  bug than the one being fixed.

### Notes

- The expander never fires while a macro is recording or replaying: expanding into a
  recording would write the expansion into the macro, and expanding during playback
  would fight with it.
- It refuses rather than guesses on input it cannot count. An IME commits characters
  that never matched the keystrokes the hook saw, and a dead key turns two keystrokes
  into one character; in both cases the number of backspaces needed is unknowable, so
  the buffer is emptied and nothing fires.
- The buffer of recently typed characters is capped at 64, never written to the log at
  any level, never written to disk, and emptied whenever the foreground window changes,
  a mouse button is pressed, or a modifier is held. Worth stating plainly: this is a
  privacy surface, and a tool that already draws antivirus heuristics should be explicit
  about what it keeps.

---

## [1.3.0]

A hardening release. No new user-facing features — this is what a seven-stage test
campaign found, fixed and measured. The plan is in [TESTING.md](TESTING.md).

### Fixed

- **The editor could take the whole application down.** `editor_set_time` read the
  previous event's timestamp with a raw index while reading both of its neighbours
  through `get`, and that read sits ahead of the guard meant to cover the function. A
  selection left pointing past a recording another edit had since trimmed reached it
  first; `panic = "abort"` in the release profile means the process goes, not the
  operation. Found by the new editor fuzzing, which is what it was written for.
- **A small template was searched for pathologically slowly.** The coarse grid was
  chosen from the template alone, ignoring the haystack it would sweep, so a 32 px
  template on a 2560x1440 screen was handed a step of 2 and examined a quarter of every
  pixel position with a 16x16 kernel — 236 million operations against 25 million for a
  64 px one. Measured: 465 ms against 64 ms, seven times slower for the smaller picture.
  The grid is now coarsened until the pass fits a fixed budget, and only then, so a
  small search area keeps the finer grid where it is cheap and its accuracy is worth
  having. The same template now takes 48 ms, matches land in exactly the same place, and
  the multi-scale option dropped from 390 ms to 274 ms with it.
- **"1 % low" did not report the worst 1 %.** It computed a 99th percentile, which for
  a hundred samples stops one sample short of the worst — exactly the sample the label
  promises. It is now the mean of the worst 1 %, matching the frame-time vocabulary the
  name borrows from, and averaging the tail keeps one freak sample from deciding the
  figure the frame guard is sized from.
- **`cargo test` did not pass.** `roundtrip_v2` asserted that a saved macro comes back
  as format version 2, but `MacroData::new` has emitted version 3 since the script
  engine landed. `cargo build` never noticed, because a wrong assertion is still a
  well-typed one. The test now checks against `format_version()` rather than a literal,
  so the next format bump cannot repeat it.
- The `MacroData` doc comment still described the container as "format version 2".

### Changed

- `editor_insert_delay` multiplies its millisecond argument saturatingly. The UI bounds
  that argument, so nothing could reach the overflow — but a function that is total only
  because of its callers is a trap for the next caller.
- Slip events are counted rather than only logged.

### Added

- **21 tests**, taking the suite from 60 to 83, including 8 000 rounds of fuzzing over
  the editor's range and single-index operations with deliberately wrong indices, and
  fuzzing of `sanitize` against absurd values, NaN and infinity. Seeded from fixed
  values, so a failure reproduces exactly. Tests build without `--release`, which means
  overflow checks are on and any arithmetic that would wrap silently in production
  panics there instead.
- **`--selftest timing`.** The scheduler cannot be judged from outside the process — an
  event that fires 40 ms late looks exactly like one that fires on time — so this runs
  the real `playback_loop`, the real frame guard and the real slip logic with every call
  into Windows suppressed, and timestamps each dispatch against the moment it was due.
  Nine scenarios covering speed extremes, a forced stall, the guard, and human-like
  movement.
- **`--selftest vision`.** Times capture and search across region and template sizes,
  prices the multi-scale option, times OCR, and reports the cost of one script image
  step. Costs are given per megapixel so a screen larger than the one under test can be
  worked out rather than guessed at.
- **`--selftest churn[=seconds]`.** Drives the playback lifecycle at about a hundred
  transitions a second while asserting that no generation escapes cancellation, that two
  playback loops never run at once, and that every stop leaves nothing held down. A
  watchdog aborts the run if no transition completes for thirty seconds, because a
  wedged process cannot report on itself.
- **`--selftest soak[=hours]`.** Replays continuously while capturing and reading the
  screen, and samples its own private bytes, handle count and GDI objects. Growth is
  measured from the five-minute mark so allocator warm-up is not reported as a leak, and
  every row is appended to `logs/soak.csv` because the Windows console stops accepting
  writes while text is selected in it.

### Measured

Figures the documentation previously gave in words:

| | |
|---|---|
| Scheduler accuracy, p99 | 4 µs; 103 µs in the worst scenario, measured under load from a running game |
| Drift | none measurable — wall clock matched the recording to the millisecond in all nine scenarios |
| Recovery from a 400 ms stall | one slip, and zero dispatches within 500 µs of each other afterwards: the backlog does not go out as a burst |
| Frame guard cost, human-paced recording | +10 % wall clock |
| Full-screen capture, 2560x1440 | 43 ms |
| Full-screen search, 64x64 template | 68 ms; a miss costs the same as a hit, since `find` has no early exit |
| One script image step | 111 ms, so a `While` polling for a picture runs at about 9 checks a second |
| Lifecycle churn | 33 000 transitions over two runs: no press left held, no generation escaped cancellation, nothing wedged |
| Soak, 2.5 h | 3 832 captures and 1 743 OCR reads; handle count flat at 212, memory settled at 12.8 MB |

### Not done

Stated plainly rather than left for someone to discover.

- **The manual feature matrix ([TESTING_MATRIX.md](TESTING_MATRIX.md)) has not been
  worked through.** Every automated stage ran dry: `SendInput` was suppressed
  throughout. The frame guard, the slip logic and human-like movement have measured
  timings but have never actually pressed a key on a live system.
- **Script image steps still sweep the whole virtual desktop.** Giving them their own
  search region would take the poll rate from about 9 a second to about 70. It changes
  the macro format, so it is held over.
- **The soak's GDI figure measures nothing.** Sampling and capture run on the same
  thread in sequence, so the sample can never land inside a `BitBlt` and the count reads
  zero whether objects leak or not. The absence of a leak is inferred instead from
  3 832 captures completing without exhausting the per-process quota.
- One unexplained event in the first churn run: a single release sent with no press
  behind it, on the harmless side and not reproduced in 33 000 further transitions. The
  mechanism proposed for it predicted roughly a hundred occurrences per run, so that
  explanation is wrong and the cause is still open.

---

## [1.2.0]

Window-related settings gathered into one place, and three things that quietly did not work.

### Added

- **The `🖥 Target window` section now holds everything that depends on which window
  the macro is aimed at**, in three groups: which window it is, how coordinates follow
  it, and how well it keeps up. The frame guard, its automatic mode, the responsiveness
  readout, *Follow the anchored window*, *Scale with the window size* and *Remember the
  target window* have all moved here from `▶ Playback` and `🎬 Recording`, and the
  separate responsiveness section is gone.
- **`⤵ From the recording`** fills the target title from the window the recording was
  made against, so it never has to be typed by hand. It needs *Remember the target
  window* to have been on while recording; the anchored title is shown beside the
  button either way, and the button is disabled when there is none.
- **A dropdown of saved templates** beside the name field in `Click image` steps and in
  `image` conditions. A script using several pictures no longer needs any of their file
  names typed from memory. The list is read when the dropdown opens, so a template saved
  a moment ago is already in it.
- `templates/`, `profiles/`, `lang/` and `logs/` are now created at startup.

### Changed

- Human movement is seeded from where the pointer actually is, so the first jump of a
  run curves like every other one instead of teleporting.
- Time spent drawing a curved path is charged to the playback schedule. Each curve costs
  up to ~60 ms, and until now that was stolen from the events behind it, which then
  bunched up to make the difference back.
- The *Human-like movement* hint is shown under the setting instead of only on hover,
  and says plainly when it applies.

### Fixed

- **Human movement appeared to do nothing.** It draws a curve only when the pointer has
  to jump more than about 24 px, and a recording samples movement every 5 ms — so
  consecutive points are a few pixels apart and the threshold is never reached. That is
  correct behaviour, since a recording already contains real human movement, but it was
  indistinguishable from a broken setting. It applies to click-only macros (*Capture
  mouse movement* off) and to the `Click at` and `Click image` script steps, and the UI
  now says so.
- **`templates/` was not created until a PNG had been saved into it.** Folders were made
  on first use, which is no help to somebody who wanted to drop a picture in beforehand.

### Notes

- A script can use as many pictures and text regions as it likes. Every `Click image`,
  `Wait for`, `If` and `While` step carries its own template name, threshold, region and
  search text, and templates are cached per run, so a chain like *Game Results →
  Claim Rewards → the icon that opens the menu* is just three steps with three different
  templates. The only thing that was awkward was typing the names, which the new
  dropdown solves.

---

## [1.1.0]

Playback that survives a target application which cannot keep up.

### Fixed

- The `Target window` and `Window responsiveness` section headers drew as empty
  boxes. Both used emoji added in Unicode 12 and 13 (🩺 U+1FA7A, 🪟 U+1FA9F), and
  the emoji font egui bundles stops before those — every other glyph in the app is
  Emoji 11 or older, which is why nothing else was affected. They are now 🖥 and 📊,
  and a unit test fails the build if a glyph from that range is ever added again.

### Added

- **Frame-rate guard** (`▶ Playback` → *Frame-rate guard*). A game rendering at 15 FPS
  looks at its input queue about once every 67 ms, so a recorded click that lasted 8 ms
  is never seen: the button goes down and back up between two polls. The guard enforces
  three spacings, all derived from one frame time — a press is held for two frames, a
  re-press waits one frame after the release before it, and a click waits one frame
  after the cursor moved so hit-testing has caught up. It only ever lengthens: a macro
  can get slower, never faster.
  **Off by default** — most macros drive ordinary desktop software, which reads its
  queue as fast as the queue fills.
- **Automatic sizing** (*Set it from the window automatically*, on by default once the
  guard is enabled). The guard follows the measured responsiveness of the target window
  instead of a figure you have to guess. The configured FPS is the fallback used until
  a measurement exists.
- **Window responsiveness panel** (`📊 Window responsiveness`) showing frame time,
  average FPS, 1 % low, 0.1 % low and a stutter count over a rolling ten seconds.
  Requires a title under `🖥 Target window`.
- **New settings** in `config.json`:

  | Key | Type | Default | Meaning |
  |---|---|---|---|
  | `frame_guard` | bool | `false` | Enable the guard |
  | `frame_guard_fps` | 5–240 | `30` | Slowest expected frame rate, used when nothing is measured |
  | `frame_guard_auto` | bool | `true` | Size the guard from the measurement instead |
  | `perf_enabled` | bool | `false` | Keep the responsiveness panel updating |

- Three unit tests for the guard's spacing rules and one for the percentile maths.

### Changed

- **Playback no longer bursts to catch up after a stall.** The scheduler was drift-free
  against the start of the run, so a 400 ms hitch left several events already overdue
  and they went out back to back — the whole backlog landing in a single frame, which
  is exactly what a struggling application cannot absorb. Falling more than six frames
  behind now slips the entire schedule to the present instead of racing it. The slip is
  logged.
- **`Click at` and `Click image` script steps** held the button for a hardcoded 30 ms.
  That is under half a frame at 15 FPS. The hold now comes from the guard.
- Those two steps no longer re-send the click coordinates in the button event. The
  cursor has already been moved there, and sending them again moved a second time,
  re-rolling *Aim spread* and landing the click a few pixels from where it was aimed.
- `platform::find_window_rect` is now a thin wrapper over a new internal handle lookup,
  which the probe reuses. Behaviour is unchanged.

### Notes

- **The responsiveness figures are not a frame counter and do not claim to be.** Reading
  another process's real present timings means an ETW session against the DXGI providers
  — what PresentMon does — which needs administrator rights and a schema parser larger
  than this whole program. `DwmGetCompositionTimingInfo` is no substitute either: since
  Windows 8.1 it reports the compositor, which keeps ticking at the monitor's refresh
  rate however badly the game underneath is doing.
  What is measured instead is the round-trip of an empty `WM_NULL` through the target
  window's own message loop. A normal game loop drains its queue once per frame, so the
  answer arrives within about one frame — and that is precisely the delay the guard has
  to cover, because input is handled on the thread that pumps. Where a game renders on
  a separate thread the number tracks input handling rather than the rendered frame
  rate, which for this purpose is the more useful of the two. For true frame statistics,
  run PresentMon, CapFrameX or RTSS alongside.
- The probe sends one message every 25 ms, far below the rate of ordinary mouse input,
  and needs no elevation. It is inert by design: `WM_NULL` makes the window procedure
  return immediately, so what is timed is the wait in the queue rather than any work
  the message caused.
- The guard is sized from the worst 1 % of samples rather than the average. A press has
  to survive the slow frames, not the comfortable ones.

---

## [1.0.0]

First public release.

Recording and replay of mouse, keyboard, wheel and X1/X2 with microsecond timing ·
loop forever, N times or until a time limit, with shutdown/restart/sleep/hibernate/log-off ·
per-monitor DPI awareness · virtual desktop isolation · window anchoring · pixel stop
condition · built-in editor with three views · script engine with 17 step kinds,
6 conditions and variables · image search · OCR through `Windows.Media.Ocr` · scheduler ·
target window · 7 rebindable hotkeys · 9 themes · 6 languages · `.exe` and AutoHotkey
export · settings profiles · headless CLI.

