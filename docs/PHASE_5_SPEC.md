# Phase 5 Specification: Platform Expansion

This document is normative for Phase 5. It defines how one resolved Native UI
v1 document becomes an application on each supported platform, what each
platform host must guarantee, and what evidence promotes a platform from
*buildable* to a higher tier in [`SUPPORT_TIERS.md`](SUPPORT_TIERS.md).

Phase 5 adds no new view syntax, no new adapter, and no new capability. It
adds platform hosts and the evidence that proves them.

## 1. Target Model

A native build selects exactly one target:

| Target    | Directory          | Host technology                     | Toolchain                |
| --------- | ------------------ | ----------------------------------- | ------------------------ |
| `macos`   | `dist/macos/`      | `SwiftUI` over `AppKit` controls    | `xcrun swiftc`           |
| `ios`     | `dist/ios/`        | `SwiftUI` over `UIKit` controls     | `xcrun -sdk iphonesimulator swiftc` |
| `linux`   | `dist/linux/`      | GTK4 widgets, `WebKitGTK` surfaces  | `cc` + `pkg-config`      |
| `windows` | `dist/windows/`    | Win32 common controls               | `mingw-w64` `gcc`        |
| `android` | `dist/android/`    | Platform Android views              | Gradle + Android SDK     |

### 1.1 Target Isolation

- Each target publishes only under its own directory.
- A failed build for one target never modifies another target's output.
- A failed build publishes nothing under its own directory and preserves the
  previous complete application.

### 1.2 Shared Lowering

Every target consumes the same fully resolved Phase 3 web output and the same
`NativePlanner`. Two builds of one project for two targets must produce
byte-identical Native UI v1 documents apart from:

1. the `target` discriminator, and
2. the human-readable `reason` on a `web_surface`, which names its platform.

No other divergence is permitted. Adapter selection, node identities,
accessible names, roles, properties, events, and initial state are
platform-neutral.

## 2. Platform-Neutral Staging

Before any toolchain runs, every target stages an identical resource tree:

```text
<stage>/native-index.json          inspectable route index
<stage>/native-ui/<key>.json       inspectable Native UI v1 documents
<stage>/web-surfaces/<id>/index.html
<stage>/web/                       resolved Phase 3 web output
<stage>/capability-manifest.json   Capability Manifest v1
<stage>/project/                   generated host source
```

and mirrors it into the platform bundle under fixed names:

```text
NativeIndex.json
NativeUI/<key>.json
WebSurfaces/<id>/index.html
WebBundle/
CapabilityManifest.json
```

Artifact Manifest v1 is written last, covers every other published file, and
records the real platform toolchain name and version.

## 3. Host Requirements

Every generated host must:

1. reject a Native UI v1 document whose `contract_version` is not `1` or whose
   `target` is not the host's own platform;
2. render every adapter listed in §4 with the platform's own control for that
   adapter, never an emulated or web-drawn substitute;
3. carry each node's accessible name to the platform accessibility API;
4. carry each node's identity to the platform's testing identifier where the
   platform has one;
5. apply `increment`, `toggle`, text input, and disclosure state exactly as the
   Phase 4 declarative ABI defines them;
6. record `controller.created`, `controller.mounted`, `controller.active`,
   `controller.suspended`, `controller.destroyed`, `route.opened`,
   `route.failed`, and the `state.*` and `websurface.*` events to a bounded
   per-application log;
7. expose **no** bridge of any kind to `WebSurface` content;
8. fail closed to a visible error state rather than render a partial view.

### 3.1 Lifecycle Delivery

A host records a lifecycle event only when its platform actually delivers the
corresponding notification. `controller.destroyed` is recorded on graceful
termination. Platforms that terminate applications without a graceful
notification — notably iOS and Android under system-initiated kill — are not
required to record it, and the absence of that record is not a defect.

## 4. Adapter Coverage

| Adapter              | macOS          | iOS            | Linux           | Windows        | Android      |
| -------------------- | -------------- | -------------- | --------------- | -------------- | ------------ |
| `layout.column`      | `VStack`       | `VStack`       | `GtkBox`        | stacked frame  | `LinearLayout` |
| `layout.app_bar`     | `HStack`       | `HStack`       | `GtkBox`        | stacked frame  | `LinearLayout` |
| `layout.list`        | `VStack`       | `VStack`       | `GtkBox`        | stacked frame  | `LinearLayout` |
| `layout.list_item`   | `VStack`       | `VStack`       | `GtkBox`        | stacked frame  | `LinearLayout` |
| `text.heading1..6`   | `Text`         | `Text`         | `GtkLabel`      | `STATIC`       | `TextView`   |
| `content.text`       | `Text`         | `Text`         | `GtkLabel`      | `STATIC`       | `TextView`   |
| `control.button`     | `NSButton`     | `UIButton`     | `GtkButton`     | `BUTTON`       | `Button`     |
| `control.text_field` | `NSTextField`  | `UITextField`  | `GtkEntry`      | `EDIT`         | `EditText`   |
| `content.output`     | `Text`         | `Text`         | `GtkLabel`      | `STATIC`       | `TextView`   |
| `control.disclosure` | `DisclosureGroup` | `DisclosureGroup` | `GtkExpander` | `BUTTON` + toggle | `Button` + toggle |
| `navigation.link`    | `Button`       | `Button`       | `GtkButton`     | `BUTTON`       | `Button`     |
| `content.image`      | `Label`        | `Label`        | `GtkImage`      | `STATIC`       | `TextView`   |
| `content.divider`    | `Divider`      | `Divider`      | `GtkSeparator`  | `SS_ETCHEDHORZ`| `View`       |
| `web_surface`        | `WKWebView`    | `WKWebView`    | `WebKitWebView` | placeholder §6 | `WebView`    |

## 5. `WebSurface` Isolation

On every platform that embeds a web view:

- the surface uses an ephemeral, non-persistent data store;
- scripting is enabled only for a `local_bundle` surface, never for
  `remote_url`;
- navigation is restricted to the bundle root for `local_bundle` and to the
  declared HTTPS host for `remote_url`; every other navigation is cancelled;
- no script message handler, JavaScript interface, or native object is exposed.

## 6. Declared Platform Reductions

Each reduction below is deliberate, has a named viability gate, and is
reflected in `SUPPORT_TIERS.md`. None may be silently widened.

| Reduction | Platform | Gate |
| --- | --- | --- |
| `WebSurface` renders an accessible placeholder that opens its content in the default browser instead of embedding a web view. | Windows | `WebView2` redistributable, COM binding, and offline-build viability. |
| Native UI v1 is lowered to a generated control table at build time rather than parsed at run time. | Windows | Retained until a Windows JSON dependency is justified; the published `NativeUI/*.json` remains the contract. |
| The application is built for the simulator SDK, not for a device. | iOS | Device provisioning profiles and distribution signing. |
| Accessible roles come from the widget's implicit role; only names and hidden state are set explicitly. | Linux | GTK4 does not permit post-construction role assignment. |
| A control's accessible name is its window text. A declared `aria-label` that differs from the visible text is not separately exposed. | Windows | A UI Automation provider implementation for the generated controls. |
| The APK is a debug build signed with the debug key. | Android | Release keystore management and Play signing. |

## 7. Promotion Evidence

A platform advances one tier at a time. Each tier requires the evidence below
recorded in [`PHASE_5_EVIDENCE.md`](PHASE_5_EVIDENCE.md) with reproduction
commands.

| Tier | Required evidence |
| --- | --- |
| `buildable` | The generated host compiles for the platform with warnings denied. |
| `simulator-tested` | The application installs and launches on a first-party simulator or emulator, renders its native view, and records its lifecycle. |
| `native-tested` | Every §4 adapter renders, every interaction in §3.5 succeeds, accessible names are readable through the platform accessibility API, and the `WebSurface` subtree stays isolated. |
| `supported` | `native-tested` plus install, upgrade, rollback, and uninstall exercises on real hardware, plus a published support window. |

Cross-compilation proves `buildable` and nothing more.

## 8. Failure Behavior

| Code | Condition |
| --- | --- |
| `TY1601` | Application configuration or entry route is invalid for the target. |
| `TY1602` | The resolved view cannot be planned. |
| `TY1603` | Accessibility or declarative state is invalid. |
| `TY1604` | A `WebSurface` boundary is invalid or exceeds its budget. |
| `TY1605` | Staging, host generation, toolchain execution, or publication failed. |

Every failure preserves the previous published application for that target and
leaves no partial output.
