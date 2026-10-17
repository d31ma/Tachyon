# Self-Hosted Runners for the Rust Rewrite Workflow

This guide explains how to run `.github/workflows/rust-ci.yml` on machines
that the maintainer owns: one macOS machine, one Windows desktop, and WSL
(Windows Subsystem for Linux) with Ubuntu on that Windows desktop. Runs on
GitHub-hosted runners can take about 90 minutes, mostly because jobs wait in
a queue. Local runners remove that wait.

The repository `d31ma/TACHYON` is public. Code from a fork must never run on
these machines. Read [Security Rules](#security-rules) before you register a
runner.

## How the Switch Works

Every job in `rust-ci.yml` chooses its runner with one expression. A job uses
a self-hosted runner only when **all** of these are true:

1. the event is `push`;
2. the pushed ref is a branch (`refs/heads/...`): `develop`,
   `codex/rust-rewrite`, or `work/**`;
3. the repository variable `TACHYON_SELF_HOSTED` is exactly `true`.

In every other case the job uses the same GitHub-hosted runner as before:

- every `pull_request` run, from a fork or from this repository;
- every `workflow_dispatch` run;
- the qualification run that `publish.yml` starts for a release tag (the ref
  is `refs/tags/v*`, so release evidence always comes from GitHub-hosted
  runners).

The Android emulator job also needs a second variable,
`TACHYON_SELF_HOSTED_ANDROID`, set to `true`. Without it, that job stays on
`ubuntu-latest` even when the main switch is on, because the emulator needs
hardware virtualization (`/dev/kvm`) inside WSL.

The expression in a job looks like this:

```yaml
runs-on: ${{ (github.event_name == 'push' && startsWith(github.ref, 'refs/heads/') && vars.TACHYON_SELF_HOSTED == 'true') && fromJSON('["self-hosted","tachyon","linux"]') || 'ubuntu-latest' }}
```

The two matrix jobs keep `matrix.os` as the hosted label. Each matrix entry
also has a `self_hosted_labels` value. Job names, artifact names, and every
`runner.os` condition stay the same.

The repository policy test
`self_hosted_runners_serve_only_opted_in_branch_pushes` in
`crates/tachyon-contracts/tests/repository_policy.rs` fails when a job in
`rust-ci.yml` can reach a self-hosted runner without this guard, or when any
other workflow names a self-hosted runner.

### Job Mapping

| Job | Hosted runner | Self-hosted labels | Machine |
| --- | --- | --- | --- |
| `rewrite-policy` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |
| `native` (Linux entry) | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |
| `native` (macOS entry) | `macos-latest` | `self-hosted, tachyon, macos` | Mac |
| `native` (Windows entry) | `windows-latest` | `self-hosted, tachyon, windows` | Windows |
| `managed-storage` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL (Docker) |
| `supply-chain` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |
| `browser` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |
| `macos-native` | `macos-latest` | `self-hosted, tachyon, macos` | Mac |
| `linux-native` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL (Docker) |
| `windows-native` | `windows-latest` | `self-hosted, tachyon, windows` | Windows (interactive desktop) |
| `android-native` | `ubuntu-latest` | `self-hosted, tachyon, linux` only with `TACHYON_SELF_HOSTED_ANDROID` | WSL (`/dev/kvm`) |
| `compatibility` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |
| `fuzz` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |
| `sanitizers` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |
| `soak` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |
| `provenance` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |
| `release-lifecycle` (Linux entry) | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |
| `release-lifecycle` (macOS entry) | `macos-latest` | `self-hosted, tachyon, macos` | Mac |
| `coverage` | `ubuntu-latest` | `self-hosted, tachyon, linux` | WSL |

GitHub compares labels without regard to case. Every self-hosted runner gets
the labels `self-hosted`, its operating system (`Linux`, `macOS`, or
`Windows`), and its architecture (`X64` or `ARM64`) by itself. So you add only
one custom label, `tachyon`, when you register a runner.

### Machine Requirements

- The Mac must use Apple silicon. The macOS jobs download arm64 builds (Dart)
  and read `JAVA_HOME_21_arm64`.
- The Windows desktop must use an x86-64 processor. The sanitizer jobs build
  for `x86_64-unknown-linux-gnu`, the compatibility job downloads
  `ty-linux-x64`, and the Android job boots an `x86_64` system image.
- The Linux jobs together need much memory. Give WSL at least 16 GB of memory
  and 200 GB of free disk space.

## Security Rules

A self-hosted runner runs whatever a job tells it to run. On a public
repository, anyone can open a pull request from a fork, and a pull request
run uses the workflow file **from the pull request**. A fork can therefore
change `runs-on` to `[self-hosted, tachyon, linux]`. The switch in
`rust-ci.yml` protects only against the workflow as it is on our branches. These
rules close the rest of the gap.

1. **Require approval for fork runs.** In the repository, open Settings →
   Actions → General → "Approval for running fork pull request workflows from
   contributors" and choose **Require approval for all external
   contributors** (older pages call it "all outside collaborators"). A fork
   pull request then runs nothing until a maintainer approves it.
2. **Read before you approve.** Before you approve a fork run, read every
   change under `.github/`, `scripts/`, and any build script. Do not approve a
   run that adds `self-hosted`, changes `runs-on`, or changes how a job picks
   its runner. If you are not sure, do not approve it; close and reopen the
   pull request after the change is removed.
3. **Never use `pull_request_target`** in a workflow that can reach these
   runners. The policy test rejects it in `rust-ci.yml`.
4. **Use a dedicated, low-privilege account** on every machine. The account
   is never an administrator and never has the maintainer's password, SSH
   keys, cloud credentials, browser profiles, or tokens. The workflow needs
   only the job's own `GITHUB_TOKEN`.
5. **Never run the runner as an administrator or as root.** Install system
   software from a separate administrator account, then return to the runner
   account.
6. **Register runners only on `d31ma/TACHYON`** (repository level), with the
   `tachyon` label. Do not add them to other repositories. If `d31ma` is an
   organization, you can instead put them in a runner group that allows only
   this repository.
7. **Treat the machines as exposed.** A bad job could reach other devices on
   the home network. Do not keep private data on the runner accounts. Keep
   the operating systems and the runner application up to date (the runner
   updates itself by default).
8. **Turn the switch off when a machine is off.** A job that waits for an
   offline self-hosted runner does not fall back to a hosted runner. It waits
   in the queue and fails after 24 hours.

## Steps for Every Machine

1. Create the dedicated runner account. The steps for each machine are below.
   Name it `runner` on every machine; the tool-cache paths below assume that
   name.
2. Sign in to GitHub as a repository administrator. Open Settings → Actions →
   Runners → **New self-hosted runner**. Choose the operating system and
   architecture. GitHub shows a download command, a checksum command, and a
   `config` command with a registration token. The token expires after one
   hour.
3. As the `runner` account, run the download and checksum commands in the
   runner folder named for the machine below.
4. Run `config` with these options (use the token from the page):

   ```sh
   ./config.sh --url https://github.com/d31ma/TACHYON --token <TOKEN> \
     --name <machine-name> --labels tachyon --work <large-volume-path>/work \
     --unattended
   ```

   On Windows, use `.\config.cmd` with the same options.
5. Check on the Runners page that the runner shows the labels
   `self-hosted`, `tachyon`, its operating system, and its architecture.
6. Write the runner's environment file (`.env` in the runner folder; one
   `NAME=value` per line). The runner reads it when it starts. The values for
   each machine are below. Also check `.path` in the same folder: it is the
   `PATH` that jobs see. It is written when you run `config`, so install the
   tools first, or edit `.path` after you install more tools. Restart the
   runner after you change either file.
7. Install the exact Rust toolchains before the first run, so that the
   workflow's `rustup toolchain install` steps have nothing to download and
   two jobs never install the same toolchain at the same time:

   ```sh
   rustup toolchain install 1.97.1 --profile minimal \
     --component clippy,rustfmt,llvm-tools-preview
   rustup target add wasm32-unknown-unknown --toolchain 1.97.1
   rustup default 1.97.1
   ```

   On the WSL runner, also run:

   ```sh
   rustup toolchain install nightly-2026-07-20 --profile minimal --component rust-src
   ```

### Pinned Toolchains

The workflow is the source of truth. When a pin changes there, it changes
here. Most tools are installed by the job itself; the column "Before the first
run" lists what the machine must already have.

| Tool | Pin in `rust-ci.yml` | How the job gets it | Before the first run |
| --- | --- | --- | --- |
| Rust | `1.97.1` with `clippy`, `rustfmt`, `llvm-tools-preview`, target `wasm32-unknown-unknown` | `rustup` | `rustup` for the runner account; install the toolchain (step 7) |
| Rust nightly | `nightly-2026-07-20` with `rust-src` | `rustup` (fuzz, sanitizers) | install it in WSL (step 7) |
| Cargo tools | cargo-deny 0.19.7, cargo-fuzz 0.13.1, cargo-cyclonedx 0.5.7, cargo-auditable 0.7.0, cargo-llvm-cov 0.8.6 | `cargo install --locked` | a C compiler and linker |
| Node.js | `24.18.0` | `actions/setup-node` in some jobs | Node.js 24.18.0 on `PATH`, because the `browser`, `macos-native`, and `android-native` jobs call `node` without a setup step |
| Bun | `1.4.3` | `oven-sh/setup-bun` | nothing |
| Python | `3.14.6` | `actions/setup-python` in `native` | `python3` on macOS and WSL; `python` on Windows (used by `windows-native`) |
| Ruby | `4.0.6` | `ruby/setup-ruby` | a writable hosted tool-cache path (see each machine) |
| Go | `1.27` | `actions/setup-go` | nothing |
| .NET | `10.0.302` with workloads `wasm-tools` and `wasm-tools-net9` | `actions/setup-dotnet`, then `dotnet workload install` | `DOTNET_INSTALL_DIR` set to a folder the runner account owns |
| TypeScript | `7.0.2` | `npm install --no-save` | nothing |
| Emscripten | the `emsdk` and `emscripten` lines in `runtimes/ruby-web/sources.txt` | `scripts/ci/install-emscripten.mjs` | Python on `PATH` |
| CHEX, FYLO | checksum pins in `scripts/ci/` | `scripts/ci/install-chex.mjs`, `scripts/ci/install-fylo.mjs` | nothing |
| JDK | 21 | preinstalled, read from `JAVA_HOME_21_arm64` (macOS) or `JAVA_HOME_21_X64` (Linux, Windows) | a JDK 21 and the matching variable in `.env` |
| Kotlin, Kotlin/Wasm stdlib | `2.1.10` | checksum-pinned download (macOS) | `curl`, `unzip`, `shasum` |
| Dart | `3.13.1` | checksum-pinned download | `curl`, `unzip` |
| Swift | `6.3.3` and the Swift SDK for WebAssembly | signed package installed into the runner account's home (macOS) | nothing |
| TeaVM | `0.15.0` | checksum-pinned jars (macOS) | nothing |
| PHP | `8.4` | `shivammathur/setup-php` (macOS) | Homebrew that the runner account can use; `runner=self-hosted` in `.env` |
| Lua | the `lua` line in `runtimes/lua-web/sources.txt`, when the workflow pins it | built from source with `make` | `make` and a C compiler |
| Playwright browsers | Chromium, Firefox, WebKit from the locked `bun.lock` | `bunx playwright install` (`--with-deps` on Linux uses `sudo apt-get`) | on Linux, passwordless `sudo` (see WSL) |
| Android SDK | `platform-tools`, `platforms;android-35`, `build-tools;35.0.0`, `emulator`, `system-images;android-35;google_apis;x86_64` | `sdkmanager --install` | `ANDROID_HOME` with `cmdline-tools/latest`, and `gradle` on `PATH` (a Gradle version that supports Android Gradle Plugin 8.7.3) |
| Docker | — | `docker build`, `docker run` | Docker Engine in WSL; the runner account in the `docker` group |
| Xvfb | — | inside the `scripts/linux/Dockerfile` image | nothing on the host |
| Xcode | the version that `macos-latest` uses | preinstalled | Xcode, an iOS simulator runtime, accepted license |
| Redis | — | `brew install redis` if missing (macOS) | install it once from the administrator account |
| MSVC, Windows SDK | — | preinstalled | Visual Studio Build Tools with the C++ x64 tools |
| WebView2 | SDK `1.0.3485.44` | checksum-pinned download | the WebView2 Runtime (already in Windows 11) |

`setup-node`, `setup-go`, `setup-bun`, `setup-dotnet`, and the cache action
work on self-hosted runners. They keep their downloads in the runner tool
cache (`RUNNER_TOOL_CACHE`), so later runs are fast. `ruby/setup-ruby` and
`actions/setup-python` download prebuilt builds for the hosted images. Those
builds expect the same operating system version as the hosted image and the
hosted tool-cache path. Set `ImageOS` and the tool-cache paths as shown for
each machine. If `setup-ruby` stops with a message about `ImageOS` or the
tool cache, do what the message says.

### How Many Runners per Machine

One runner runs one job at a time.

- **WSL:** about 14 Linux jobs run for each push. Register 2 or 3 runners in
  WSL, each in its own folder (`~/actions-runner-1`, `~/actions-runner-2`,
  ...). More runners need more memory. Jobs from two pushes on different
  branches can run at the same time and can use the same fixed port (for
  example, the `browser` job uses port 4700), so do not register more runners
  than you need.
- **Mac:** register 1 runner. The macOS gates drive the screen and the iOS
  simulator. Two of them at the same time would disturb each other.
- **Windows:** register 1 runner. The `windows-native` gate drives the
  desktop.

## macOS Machine

If the Mac's internal disk is small, put the runner folder, the work folder,
and the caches on an external volume. The commands below call it
`/Volumes/<volume>`; use the volume's real name.

### Prepare the External Volume

1. The volume must use APFS or Mac OS Extended. Do not use exFAT; it has no
   owners or permissions, and Cargo builds on it are slow.
2. Turn on ownership for the volume (Finder → Get Info → clear "Ignore
   ownership on this volume", or `sudo diskutil enableOwnership /Volumes/<volume>`).
   Without ownership, every account can read every file on the volume.
3. Check that the maintainer's own folders on the volume are not readable by
   other accounts (`chmod 700` on each of them).
4. As an administrator, create the runner folders:

   ```sh
   sudo mkdir -p /Volumes/<volume>/tachyon-runner/{actions-runner,work,toolcache,cargo,rustup,cache}
   sudo chown -R runner:staff /Volumes/<volume>/tachyon-runner
   sudo chmod 700 /Volumes/<volume>/tachyon-runner
   ```

The volume must be mounted before the runner starts. If the drive is not
connected, the runner fails its jobs; turn the switch off first.

### Create the Runner Account

1. System Settings → Users & Groups → Add User. Account type: **Standard**
   (never Administrator). Name: `runner`.
2. Sign in as `runner` once in the graphical session, to create its home
   folder and to answer the first-login questions.
3. As `runner`, move the large folders to the volume with links:

   ```sh
   ln -s /Volumes/<volume>/tachyon-runner/toolcache ~/hostedtoolcache
   ln -s /Volumes/<volume>/tachyon-runner/cache ~/.cache
   ```

4. Install `rustup` as `runner`, with its folders on the volume:

   ```sh
   export CARGO_HOME=/Volumes/<volume>/tachyon-runner/cargo
   export RUSTUP_HOME=/Volumes/<volume>/tachyon-runner/rustup
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
   ```

### Install System Software as an Administrator

1. Install Xcode in `/Applications` (the version that `macos-latest` uses;
   see the GitHub `runner-images` README). Then run:

   ```sh
   sudo xcode-select --switch /Applications/Xcode.app
   sudo xcodebuild -license accept
   sudo xcodebuild -runFirstLaunch
   xcodebuild -downloadPlatform iOS
   ```

2. Install Homebrew, then `brew install redis` and
   `brew install --cask temurin@21` (JDK 21).
3. Install Node.js 24.18.0 and Python 3 from their official macOS
   installers. The `node` on `PATH` must match the workflow pin; Homebrew's
   `node@24` can be a newer 24.x.
4. `setup-php` and the Redis step call `brew` as `runner`. Let `runner` use
   the Homebrew prefix: `sudo chgrp -R staff /opt/homebrew` and
   `sudo chmod -R g+w /opt/homebrew`. (Every standard account is in `staff`;
   use a new group with only you and `runner` if the Mac has other users.)

### Register and Start the Runner

1. As `runner`, in `/Volumes/<volume>/tachyon-runner/actions-runner`, follow
   [Steps for Every Machine](#steps-for-every-machine). Use
   `--name tachyon-mac` and `--work /Volumes/<volume>/tachyon-runner/work`.
2. Write `.env`:

   ```text
   ImageOS=macos15
   RUNNER_TOOL_CACHE=/Users/runner/hostedtoolcache
   AGENT_TOOLSDIRECTORY=/Users/runner/hostedtoolcache
   CARGO_HOME=/Volumes/<volume>/tachyon-runner/cargo
   RUSTUP_HOME=/Volumes/<volume>/tachyon-runner/rustup
   DOTNET_INSTALL_DIR=/Volumes/<volume>/tachyon-runner/dotnet
   JAVA_HOME_21_arm64=/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home
   runner=self-hosted
   ```

   Use the `ImageOS` value of the current `macos-latest` image.
3. Add `/Volumes/<volume>/tachyon-runner/cargo/bin`, `/opt/homebrew/bin`, and the
   Node.js 24.18.0 folder to the start of `.path`.
4. Install the runner as a launch agent: `./svc.sh install` and
   `./svc.sh start`. A launch agent runs only while `runner` is signed in to
   the graphical session. This is required: the macOS gates open windows,
   take screenshots, and use the iOS simulator.
5. Keep `runner` signed in. Automatic login is not possible when FileVault
   is on, so after a restart sign in as `runner` once. With fast user
   switching you can return to your own account, and the runner keeps taking
   jobs. But the `macos-native` gate takes screenshots and reads windows, so
   it needs the `runner` session on the screen. While you work in your own
   account, expect that gate to fail, or turn the switch off.
6. Turn off sleep while the Mac is on power (System Settings → Energy).

### Grant Privacy Permissions

The `macos-native` job reads windows through Accessibility, uses AppleScript
to talk to System Events, and takes screenshots. The first run fails with
`macOS Accessibility permission is not enabled for this process`, or macOS
shows a prompt in the `runner` session. As an administrator, open System
Settings → Privacy & Security and allow the program that the prompt or the
error names under **Accessibility**, **Screen & System Audio Recording**, and
**Automation → System Events**. This is usually `Runner.Worker` in the
runner's `bin` folder, or the `node` program that it starts. Every job on
this runner gets these permissions, which is one more reason that the account
must hold nothing private.

## Windows Machine

The `windows-native` gate starts the generated application and reads its
window with UI Automation. It needs a real desktop. A runner that runs as a
Windows service runs in session 0, which has no desktop, so the gate fails.
The runner must run as a signed-in user.

### Create the Runner Account

1. Settings → Accounts → Other users → Add account → "I don't have this
   person's sign-in information" → "Add a user without a Microsoft account".
   Name: `runner`. Keep the type **Standard User**.
2. Sign in as `runner` once.

### Install System Software as an Administrator

1. Visual Studio 2022 Build Tools with the workload "Desktop development with
   C++" (MSVC x64 tools and a Windows SDK). The gate finds them with
   `vswhere`.
2. Git for Windows, PowerShell 7, and Python 3.14.6 (for all users, on
   `PATH`). `windows-native` calls `python` without a setup step.
3. The WebView2 Runtime (Windows 11 already has it).
4. A JDK 21, for jobs that read `JAVA_HOME_21_X64`.
5. Turn on long paths:

   ```powershell
   New-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' `
     -Name LongPathsEnabled -Value 1 -PropertyType DWORD -Force
   git config --system core.longpaths true
   ```

6. Create a Dev Drive (Settings → System → Storage → Disks & volumes → Create
   dev drive), for example `D:`. Builds are faster there. Create
   `D:\tachyon-runner` and give `runner` full control of it. Create
   `C:\hostedtoolcache\windows` and give `runner` full control of it; the
   prebuilt Ruby builds expect that path.

### Register and Start the Runner

1. As `runner`, install `rustup` with `rustup-init.exe` (default MSVC host).
2. In `D:\tachyon-runner\actions-runner`, follow
   [Steps for Every Machine](#steps-for-every-machine) with
   `--name tachyon-windows` and `--work D:\tachyon-runner\work`. Do not
   pass `--runasservice`. If `config.cmd` asks "Would you like to run the
   runner as service?", answer **N**.
3. Write `.env`:

   ```text
   ImageOS=win25
   RUNNER_TOOL_CACHE=C:\hostedtoolcache\windows
   AGENT_TOOLSDIRECTORY=C:\hostedtoolcache\windows
   DOTNET_INSTALL_DIR=D:\tachyon-runner\dotnet
   JAVA_HOME_21_X64=C:\Program Files\Eclipse Adoptium\jdk-21-hotspot
   ```

   Use the `ImageOS` value of the current `windows-latest` image, and the real
   JDK folder.
4. Start the runner when `runner` signs in. Open Task Scheduler → Create Task:
   - General: user `runner`, **Run only when user is logged on**, not "with
     highest privileges";
   - Triggers: **At log on** of `runner`;
   - Actions: start `D:\tachyon-runner\actions-runner\run.cmd`, start in
     `D:\tachyon-runner\actions-runner`;
   - Settings: clear "Stop the task if it runs longer than".
5. Sign `runner` in automatically after a restart with Sysinternals
   Autologon (it stores the password encrypted). Then turn off the screen
   saver, the lock screen timeout, and sleep for `runner`:
   `powercfg /change standby-timeout-ac 0`.
6. Keep the `runner` desktop active and unlocked while CI runs. A locked
   session, a disconnected Remote Desktop session, or a session in the
   background behind another user can make UI Automation fail. If you must
   use the PC as yourself while CI runs, expect `windows-native` to fail, or
   turn the switch off.

## WSL (Ubuntu) on the Windows Machine

Use a separate WSL distribution that holds only CI. Register it from the
Windows `runner` account: WSL distributions belong to one Windows user, and
the `runner` account is already signed in for the Windows runner.

### Create the Distribution

1. As an administrator, make sure WSL 2 is installed (`wsl --install
   --no-distribution`) and up to date (`wsl --update`).
2. As `runner`, install Ubuntu with the LTS version that `ubuntu-latest`
   uses (see the GitHub `runner-images` README), on the large drive:

   ```powershell
   wsl --install -d Ubuntu-24.04 --name tachyon-ci --location D:\tachyon-runner\wsl
   ```

   Create the Linux user `runner` when Ubuntu asks. Older WSL versions do
   not have `--name` and `--location`; then install `Ubuntu-24.04`, run
   `wsl --export Ubuntu-24.04 D:\tachyon-runner\ubuntu.tar`, and
   `wsl --import tachyon-ci D:\tachyon-runner\wsl D:\tachyon-runner\ubuntu.tar`.
3. Write `C:\Users\runner\.wslconfig`:

   ```ini
   [wsl2]
   memory=16GB
   processors=8
   swap=16GB
   nestedVirtualization=true
   vmIdleTimeout=-1

   [experimental]
   sparseVhd=true
   ```

   Change the numbers to fit the machine.
4. Inside the distribution, write `/etc/wsl.conf`, then run
   `wsl --shutdown` from Windows:

   ```ini
   [boot]
   systemd=true

   [user]
   default=runner

   [automount]
   enabled=false
   mountFsTab=false

   [interop]
   enabled=false
   appendWindowsPath=false
   ```

   With automount and interop off, a job in WSL cannot read Windows files or
   start Windows programs. Keep every runner and work folder inside the Linux
   file system (`/home/runner`), never under `/mnt/c`; it is also much faster.

### Install System Software

Run these as `runner` with `sudo`:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev openssl curl \
  unzip xz-utils git jq libyaml-0-2 libgmp10 openjdk-21-jdk
```

Then install:

- Docker Engine from Docker's Ubuntu repository (`docker-ce`,
  `docker-ce-cli`, `containerd.io`, `docker-buildx-plugin`), and add the user:
  `sudo usermod -aG docker runner`. Docker Desktop with WSL integration also
  works, but Docker Engine inside `tachyon-ci` keeps CI separate from the
  rest of the machine. The `managed-storage` job starts PostgreSQL, MySQL,
  MariaDB, and Redis with `docker run` and publishes them on `127.0.0.1`. The
  `linux-native` job builds `scripts/linux/Dockerfile` (which contains Xvfb
  and D-Bus) and runs it with `SYS_ADMIN` and no seccomp profile, so Docker
  must run as root (not rootless).
- Node.js 24.18.0 on `PATH` (for example from the official tarball in
  `/usr/local`).
- `rustup` for `runner`, then step 7 of
  [Steps for Every Machine](#steps-for-every-machine).
- For the Android job only: the Android command-line tools in
  `/home/runner/android-sdk/cmdline-tools/latest`, and Gradle on `PATH`.

Create the hosted tool-cache path that the prebuilt Python and Ruby builds
expect:

```sh
sudo mkdir -p /opt/hostedtoolcache
sudo chown runner:runner /opt/hostedtoolcache
```

### Passwordless sudo

These steps call `sudo` and cannot ask for a password:

- `sudo apt-get` in `native` and `managed-storage`;
- `bunx playwright install --with-deps` in `browser` and `compatibility`
  (Playwright runs its install script through `sudo`);
- the KVM `udev` rule in `android-native`.

A list of allowed commands is not enough for Playwright, and `apt-get` and
membership in `docker` already give root power. Give `runner` passwordless
`sudo` **inside `tachyon-ci` only**:

```sh
echo 'runner ALL=(ALL) NOPASSWD: ALL' | sudo tee /etc/sudoers.d/runner
sudo chmod 0440 /etc/sudoers.d/runner
```

This is acceptable only because the distribution holds nothing else, cannot
see Windows files, and runs only trusted branch pushes. Never do this in your
own development distribution.

### KVM for the Android Emulator

1. Inside `tachyon-ci`, check that `/dev/kvm` exists: `ls -l /dev/kvm`. It
   needs virtualization turned on in the firmware settings and
   `nestedVirtualization=true` in `.wslconfig`.
2. Add `runner` to the `kvm` group: `sudo usermod -aG kvm runner`.
3. Only after one Android job passed on this runner, set the repository
   variable `TACHYON_SELF_HOSTED_ANDROID` to `true`. If `/dev/kvm` is missing,
   leave the variable unset; the Android job then stays on `ubuntu-latest`.

### Register and Start the Runners

1. For each runner `N` (1 to 3), create `~/actions-runner-N`, and follow
   [Steps for Every Machine](#steps-for-every-machine) with
   `--name tachyon-wsl-N` and `--work /home/runner/actions-runner-N/_work`.
2. Write `.env` in each runner folder:

   ```text
   ImageOS=ubuntu24
   RUNNER_TOOL_CACHE=/opt/hostedtoolcache
   AGENT_TOOLSDIRECTORY=/opt/hostedtoolcache
   DOTNET_INSTALL_DIR=/home/runner/.dotnet
   JAVA_HOME_21_X64=/usr/lib/jvm/java-21-openjdk-amd64
   ANDROID_HOME=/home/runner/android-sdk
   ANDROID_SDK_ROOT=/home/runner/android-sdk
   ```

   Use the `ImageOS` value that matches the Ubuntu version.
3. Install each runner as a systemd service that runs as `runner` (not as
   root): `sudo ./svc.sh install runner` and `sudo ./svc.sh start`.
4. WSL stops a distribution when nothing from Windows uses it. Keep it
   running with a second Task Scheduler task for the Windows `runner`
   account: **At log on**, action `wsl.exe`, arguments
   `-d tachyon-ci --exec sleep infinity`.

## Turn the Switch On and Off

Use Settings → Secrets and variables → Actions → **Variables** →
"Repository variables", or the GitHub CLI:

```sh
gh variable set TACHYON_SELF_HOSTED --body true --repo d31ma/TACHYON
gh variable set TACHYON_SELF_HOSTED_ANDROID --body true --repo d31ma/TACHYON
```

To return every job to GitHub-hosted runners:

```sh
gh variable set TACHYON_SELF_HOSTED --body false --repo d31ma/TACHYON
```

or delete the variable (`gh variable delete TACHYON_SELF_HOSTED --repo
d31ma/TACHYON`). Any value other than exactly `true` means hosted runners.
The variable is read when a run starts. Runs that already wait for a
self-hosted runner keep waiting; cancel them and run them again after you
turn the switch off.

Before you turn the switch on, check that all three machines show their
runners as **Idle** on Settings → Actions → Runners. The switch moves every
job at once; a job for a machine that is off waits in the queue.

## Disk Cleanup

`actions/checkout` runs `git clean -ffdx` before every job, so build output
in the work folder does not grow from run to run. These folders do grow:

| Folder | Machine | Clean with |
| --- | --- | --- |
| Docker images, build cache, volumes | WSL | `docker system prune --all --volumes --force` |
| Cargo registry and Git caches (`$CARGO_HOME/registry`, `$CARGO_HOME/git`) | all | delete them; Cargo downloads again |
| Old Rust toolchains | all | `rustup toolchain list`, then `rustup toolchain uninstall <name>` for pins the workflow no longer uses |
| Tool cache (`RUNNER_TOOL_CACHE`) | all | delete versions the workflow no longer pins |
| Playwright browsers (`~/.cache/ms-playwright`) | WSL, Mac | delete old browser folders |
| Android system images and AVDs (`/tmp/tachyon-android`) | WSL | `rm -rf /tmp/tachyon-android` between runs |
| iOS simulators | Mac | `xcrun simctl delete unavailable`, `xcrun simctl erase all` |
| Xcode build data (`~/Library/Developer/Xcode/DerivedData`) | Mac | delete it |
| Gradle caches (`~/.gradle/caches`) | WSL | delete them |
| WSL disk file (`ext4.vhdx`) | Windows | `sparseVhd=true` gives freed space back; run `sudo fstrim -av` inside WSL |

Clean once a week, or when a machine has less than 50 GB free. Clean when no
job runs: stop the runners first (`./svc.sh stop`, or close `run.cmd` on
Windows).

## Remove a Runner

1. Stop it (`./svc.sh stop`, then `./svc.sh uninstall`; on Windows close
   `run.cmd` and delete the scheduled task).
2. On Settings → Actions → Runners, choose the runner → Remove, and copy the
   removal command. Run `./config.sh remove --token <TOKEN>` in the runner
   folder.
3. If no runner is left for an operating system, turn the switch off first.

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| Jobs stay "Waiting for a runner" | A machine is off, the runner is stopped, or the `tachyon` label is missing. Start it, or turn the switch off. |
| `sudo: a password is required` | Passwordless `sudo` is missing in `tachyon-ci`. |
| `permission denied ... /var/run/docker.sock` | `runner` is not in the `docker` group; sign out of WSL (`wsl --terminate tachyon-ci`) after `usermod`. |
| `setup-ruby` or `setup-python` stops about `ImageOS` or the tool cache | Set `ImageOS`, `RUNNER_TOOL_CACHE`, and `AGENT_TOOLSDIRECTORY` as above, and restart the runner. |
| `dotnet workload install` fails with access denied | `DOTNET_INSTALL_DIR` is not set to a folder that `runner` owns. |
| `JAVA_HOME_21_arm64` or `JAVA_HOME_21_X64` is empty | Add it to `.env` and restart the runner. |
| `macOS Accessibility permission is not enabled for this process` | Grant the permissions in [Grant Privacy Permissions](#grant-privacy-permissions). |
| `windows-native` cannot find the window | The runner runs as a service, or the `runner` desktop is locked or in the background. |
| The Android emulator does not boot | `/dev/kvm` is missing or not writable. Leave `TACHYON_SELF_HOSTED_ANDROID` unset until it works. |
