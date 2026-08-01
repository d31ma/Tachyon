# Phase 5 Evidence

This is the validation record for [`PHASE_5_SPEC.md`](PHASE_5_SPEC.md). Every
claim below names the command that produced it and the machine class it ran
on. Claims without recorded evidence are listed in §6 as open gaps, not as
achievements.

Build machine for the locally executed evidence: macOS 26 (Darwin 25.5.0),
Apple silicon, Xcode 26.6, Swift 6.3.3, Rust 1.97.1, Docker 29.6.2,
`x86_64-w64-mingw32-gcc` 16.1.0.

## 0. Re-verification, 2026-07-31

Every runnable profile below was executed again on that date and reproduced the
recorded result: iOS on iPhone 17 Pro / iOS 26.5 (launched as pid 13540, count
`0 → 2`, 12 bound input events, isolated `WebSurface`, and a lifecycle log
ending in `controller.suspended`), Android on the `tachyon-gate` emulator, and
Linux in the pinned container. macOS is blocked on granting Accessibility
permission to the process that runs the probe. Windows still has no execution
evidence. The gate status this feeds is in [`CUTOVER.md`](CUTOVER.md).

## 1. Automated Gate

`cargo test --workspace --all-targets --all-features --locked`

| Suite | Coverage |
| --- | --- |
| `native::host` | literal escaping for Swift, Kotlin, C, and XML; bounded toolchain version banners |
| `native::ios` | UIKit-only host source, lifecycle events, no script message handler, launchable simulator plist, build-machine triple |
| `native::linux` | GTK4 adapters, ephemeral `WebKitGTK` session, `gtk_accessible_update_property` usage, no script message handler, valid `GApplication` id, desktop entry |
| `native::windows` | deterministic repeated lowering, embedded state and action tables, common-controls and per-monitor DPI manifest, host-appropriate compiler selection |
| `native::android` | valid Java packages for hyphenated, numeric-leading, and reserved segments; platform-view host with no `addJavascriptInterface`; pinned Gradle and manifest surface |
| `phase5_cli` | per-target output isolation, iOS bundle shape and signature, Apple-target semantic parity, identical planning diagnostics across all five targets |

The Apple-parity test is the mechanical enforcement of spec §1.2: it fails if
two targets lower one project to anything other than the same Native UI v1
document apart from the target tag and the platform-named fallback reason.

## 2. iOS — simulator-tested

Toolchain: `xcrun --sdk iphonesimulator swiftc`, target
`arm64-apple-ios17.0-simulator`. Device: iPhone 17 Pro, iOS 26.5
(`063A637F-1E5B-4C4C-BC21-9FF8206D7E4C`).

```bash
ty build <project> --target ios
xcrun simctl install <udid> dist/ios/PhaseFive.app
xcrun simctl launch <udid> dev.tachyon.phase-five
```

Result: `Built ios app with 1 routes (native_nodes=24 web_surfaces=1)`.

| Claim | Observation |
| --- | --- |
| The bundle installs and launches | `simctl launch` returned pid 78562 |
| Native adapters render | Screenshot shows a native large-title heading, body text, `UIButton`, bound output, `UITextField`, and `DisclosureGroup` |
| Button dispatches `increment` | Two taps on "Add one" moved the bound `count` output from `0` to `2` |
| Text field binds input | Typing `Ada Lovelace` into the native field populated the bound `name` state |
| Disclosure toggles | Tapping "More detail" revealed "Disclosure content." |
| Fallback is a subtree, not the page | `<x-chart>` rendered as an isolated local-bundle `WKWebView` showing "Chart fallback" while every sibling stayed native |
| Lifecycle is recorded | `Library/Logs/Tachyon/dev.tachyon.phase-five.jsonl` contained `controller.created`, `route.opened`, `websurface.created`, `websurface.attached`, `controller.mounted`, `controller.active`, and `controller.suspended` after backgrounding |

`controller.destroyed` is not recorded when the simulator terminates the
process without a graceful notification. Per spec §3.1 this is expected iOS
behavior and is not counted as evidence either way.

Tier: **simulator-tested**. It is not `native-tested`: the artifact is built
against the simulator SDK, and no run on physical hardware has been recorded.

## 3. Linux — container-tested

Environment: `scripts/linux/Dockerfile` (Debian trixie, GTK4, `WebKitGTK` 6.0,
json-glib, at-spi2-core, Rust 1.97.1).

```bash
docker build -f scripts/linux/Dockerfile -t tachyon-linux-gate:1.97.1 scripts/linux
docker run --rm --cap-add SYS_ADMIN --cap-add NET_ADMIN \
  --security-opt seccomp=unconfined --security-opt apparmor=unconfined \
  -v "$PWD":/workspace -w /workspace tachyon-linux-gate:1.97.1 \
  ./scripts/linux/native-test.sh
```

Result: `PASS: Linux native gate`.

| Claim | Observation |
| --- | --- |
| The generated host compiles | `cc -std=gnu17 -O2 -Wall -Wextra -Werror` succeeded against GTK4, `WebKitGTK` 6.0, and json-glib |
| Packaging is complete | `bin/PhaseFive`, `resources/NativeIndex.json`, `resources/NativeUI/root.json`, and `dev.tachyon.phase-five.desktop` were published |
| The application runs headlessly | Launched under `Xvfb` and `dbus-run-session`; the window presented and the lifecycle log recorded `controller.created`, `route.opened`, `controller.mounted`, and `controller.active` |
| Accessible names reach the platform | All seven declared names — `Phase Five demo`, `Phase Five`, `Increase count`, `Count`, `Your name`, `More detail`, `Sales chart` — were read back over AT-SPI by `scripts/linux/a11y-probe.py` |
| Semantics map to native roles | `Increase count` → `button`; `Your name` → `text` |
| Interaction works through assistive technology | `Atspi.Action.do_action` on the button moved the bound state to `1`, observed as a new accessible name in the tree |
| The surface is isolated | `WebKitGTK` ran with an ephemeral network session and the fallback document carried `default-src 'none'` |

Two defects were found and fixed by this evidence run rather than assumed
away:

1. accessible names set on plain GTK containers never reached AT-SPI, because
   GTK4 treats a generic container as presentational — named containers are
   now wrapped in a group-role frame;
2. a button constructed with an intrinsic label derived its accessible name
   from that label and ignored the declared one — the text is now supplied as
   an accessibility-hidden child so the declared name stays authoritative.

Tier: **container-tested**. Per `SUPPORT_TIERS.md` a container does not
substitute for host-native evidence, so this is not `native-tested`.

## 4. Windows — buildable

```bash
ty build <project> --target windows
file dist/windows/PhaseFive/bin/PhaseFive.exe
```

| Claim | Observation |
| --- | --- |
| The generated host cross-compiles | `x86_64-w64-mingw32-gcc (GCC) 16.1.0` with `-Wall -Wextra -Werror -municode -mwindows` |
| The artifact is a real Windows binary | `PE32+ executable (GUI) x86-64, for MS Windows` |
| The manifest is accurate | `target` = `{os: windows, architecture: x86_64, abi: win32}`; toolchain recorded as `mingw-w64-gcc (GCC) 16.1.0` |
| Packaging is complete | `bin/PhaseFive.exe`, `bin/PhaseFive.exe.manifest`, and the full `resources/` tree were published |

Tier: **buildable**, and nothing more. Per spec §7 and the delivery principle
that cross-compilation proves buildability only, execution, interaction, and
accessibility evidence must come from the `windows-latest` CI job. No such run
is recorded yet; see §6.

Execution is gated by the `windows-native` CI job, which runs
`scripts/windows/native-test.ps1` on `windows-latest`: it launches the
generated executable, locates the window and controls through UI Automation,
invokes the native button through `InvokePattern`, and asserts both the bound
output and the lifecycle log. That job has not yet reported a run; until it
does, the tier stays `buildable`.

## 5. Android — emulator-tested

Toolchain: Gradle 8.14.5, JDK 21, Android Gradle Plugin 8.7.3, build-tools
35.0.0, compileSdk 35, minSdk 26. Device: headless `pixel_6` emulator,
`system-images;android-36;google_apis;arm64-v8a`, Android 16.

```bash
ANDROID_HOME=<sdk> ./scripts/android/native-test.sh
```

Result: `PASS: Android native gate`.

| Claim | Observation |
| --- | --- |
| The APK assembles | `gradle assembleDebug` produced `PhaseFive.apk`; no Kotlin plugin and no third-party runtime dependency is used |
| It installs and launches | `adb install` succeeded; `am start -W` reported `Status: ok`, `LaunchState: COLD`, `TotalTime: 924` |
| Adapters map to platform widgets | `uiautomator` reported `Increase count` → `android.widget.Button` and `Your name` → `android.widget.EditText` |
| Accessible names reach the platform | `Phase Five demo`, `Increase count`, `Count`, `Your name`, and `More detail` were present as `content-desc` on native nodes |
| Button dispatches `increment` | Native taps moved the bound `count` output from `0` to `2`, with one `state.increment` recorded per tap |
| Text field binds input | Typing `Ada` into the native `EditText` produced `state.input` events and the visible bound value |
| Disclosure toggles | Tapping `More detail` revealed "Disclosure content." and recorded `state.disclosure` |
| Fallback is a subtree | `<x-chart>` rendered inside an isolated `WebView` showing "Chart fallback" while every sibling stayed native |
| Lifecycle is recorded | `files/tachyon/dev.tachyon.phase-five.jsonl` contained `controller.created`, `route.opened`, `websurface.attached`, `controller.mounted`, and `controller.active` |

The migrated 11-route website was rebuilt and exercised on the same emulator
on 2026-08-01. The generated APK launched without an application exception;
the mobile menu opened and closed, the light/dark theme changed, and physical
taps navigated `/` to `/docs` and then to the dynamic
`/docs/introduction` route. Dark system-bar icons remained visible on the
light status bar. The remaining Chromium log entries were WebView environment
warnings (DNS/configuration and cache setup), not process failures.

Tier: **simulator-tested** (emulator). It is not `native-tested`: no run on
physical hardware is recorded, and the APK is a debug build signed with the
debug key.

## 6. Continuous Gates

| Job | Platform | Proves |
| --- | --- | --- |
| `native` | Linux, macOS, Windows | format, check, Clippy, tests, rustdoc on the exact toolchain |
| `linux-native` | Linux container | GTK4 compilation, headless launch, AT-SPI names, roles, and activation |
| `windows-native` | `windows-latest` | Win32 compilation, launch, UI Automation names, `InvokePattern` activation, lifecycle |
| `android-native` | Linux + emulator | APK assembly, install, launch, `uiautomator` names, native tap, lifecycle |
| `macos-native` | `macos-latest` | Phase 4 macOS launch, accessibility, lifecycle, and visual parity |

## 7. Open Gaps

| Gap | What closes it |
| --- | --- |
| A recorded `windows-native` run | The job defined in `.github/workflows/rust-ci.yml` reporting on a pull request |
| iOS in CI | A `macos-latest` job booting a simulator and running the iOS gate |
| iOS and Android on physical hardware | Device provisioning, distribution signing, and a release keystore |
| Linux on a host rather than a container | A native Linux runner executing `scripts/linux/native-test.sh` |
| Windows accessible names distinct from visible text | A UI Automation provider for the generated controls, per spec §6 |
| Embedded `WebSurface` on Windows | The `WebView2` viability gate, per spec §6 |
| Any platform at `supported` | Install, upgrade, rollback, and uninstall exercises plus a published support window, per `SUPPORT_TIERS.md` |

No platform in this document is claimed as `supported`, and no Phase 5
artifact is a Tachyon preview or release.
