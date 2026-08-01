# Phase 5 Windows native gate.
#
# Builds the generated Win32 host, launches it, drives the native button
# through UI Automation, and asserts that the bound state and the lifecycle
# log respond. This is the authoritative Windows execution evidence; a
# cross-compile from another host proves buildability only.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$fixture = Join-Path $env:TEMP 'ty-windows-fixture'
$out = Join-Path $fixture 'dist\windows'
$app = Join-Path $out 'PhaseFive'
$bundleId = 'dev.tachyon.phase-five'

if (Test-Path $fixture) { Remove-Item -Recurse -Force $fixture }
New-Item -ItemType Directory -Force -Path (Join-Path $fixture 'client\pages') | Out-Null

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$config = @'
{"application":{"name":"Phase Five","id":"dev.tachyon.phase-five","version":"0.1.0","entry_route":"/"}}
'@
[System.IO.File]::WriteAllText((Join-Path $fixture 'tachyon.json'), $config, $utf8NoBom)

$view = @'
<main aria-label="Phase Five demo">
  <h1>Phase Five</h1>
  <p>Cross-platform native adapters.</p>
  <button aria-label="Increase count" data-tachyon-action="increment:count">Add one</button>
  <output aria-label="Count" data-tachyon-bind="count" data-tachyon-state="0">0</output>
  <input aria-label="Your name" data-tachyon-bind="name" data-tachyon-state="" placeholder="Name">
  <details aria-label="More detail"><summary>More detail</summary><p>Disclosure content.</p></details>
  <x-chart aria-label="Sales chart"><p>Chart fallback</p></x-chart>
</main>
'@
[System.IO.File]::WriteAllText((Join-Path $fixture 'client\pages\tac.html'), $view, $utf8NoBom)

Write-Host '==> building ty'
cargo build --locked --bin ty
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
$ty = Join-Path $targetDir 'debug\ty.exe'

Write-Host '==> generating the Windows application'
& $ty build $fixture --target windows
if ($LASTEXITCODE -ne 0) { throw 'ty build --target windows failed' }

Write-Host '==> asserting published layout'
foreach ($relative in @(
    'PhaseFive\bin\PhaseFive.exe',
    'PhaseFive\bin\PhaseFive.exe.manifest',
    'PhaseFive\resources\NativeIndex.json',
    'PhaseFive\resources\NativeUI\root.json',
    'artifact-manifest.json',
    'capability-manifest.json',
    'project\tachyon_host.c'
)) {
    $path = Join-Path $out $relative
    if (-not (Test-Path $path)) { throw "missing published artifact $relative" }
}
$native = Get-Content -Raw (Join-Path $out 'native-ui\root.json')
if ($native -notmatch '"target": "windows"') { throw 'Native UI is not targeted at windows' }

Write-Host '==> launching the generated application'
$log = Join-Path $env:LOCALAPPDATA "Tachyon\$bundleId.jsonl"
if (Test-Path $log) { Remove-Item -Force $log }
$process = Start-Process -FilePath (Join-Path $app 'bin\PhaseFive.exe') -PassThru
Start-Sleep -Seconds 5

# Standard Win32 HWNDs expose their semantic UIA types through Microsoft's
# client-side proxy providers. PowerShell 7 does not register that provider
# table merely because UIAutomationClient is referenced, leaving genuine
# Button/Edit/Static windows on the fallback provider as generic panes.
Add-Type -AssemblyName UIAutomationClient, UIAutomationClientsideProviders, UIAutomationTypes
$providerAssembly = [UIAutomationClientsideProviders.UIAutomationClientSideProviders].Assembly.GetName()
[System.Windows.Automation.ClientSettings]::RegisterClientSideProviderAssembly($providerAssembly)
$root = [System.Windows.Automation.AutomationElement]::RootElement

# A name alone is ambiguous: the host window and its child controls can share
# one caption, so an ancestor Pane answers to the button's name. Matching by
# name and control type together is what makes the lookup unambiguous. The
# search still starts at the ancestor it is given and widens to the desktop,
# because the element that answers to the window's name is not guaranteed to
# be the top-level window that owns the controls.
function Find-Descendant(
    [System.Windows.Automation.AutomationElement] $ancestor,
    [string] $name,
    $controlType = $null
) {
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty, $name)
    foreach ($scope in @($ancestor, $root)) {
        if ($null -eq $scope) { continue }
        $found = $scope.FindAll(
            [System.Windows.Automation.TreeScope]::Descendants, $condition)
        foreach ($match in $found) {
            if ($null -eq $controlType -or $match.Current.ControlType -eq $controlType) {
                return $match
            }
        }
    }
    return $null
}

# Printed only when an assertion fails, so a CI failure reports the tree that
# was actually exposed instead of only the name that was missing.
function Write-AutomationTree(
    [System.Windows.Automation.AutomationElement] $element,
    [int] $depth = 0
) {
    if ($null -eq $element -or $depth -gt 4) { return }
    $pad = ' ' * ($depth * 2)
    # The window class is the datum that separates the two explanations for a
    # control reported as a pane: a host that never created a standard control,
    # or standard controls that the automation proxies did not annotate.
    $line = "$pad$($element.Current.ControlType.ProgrammaticName)" `
        + " '$($element.Current.Name)'" `
        + " class='$($element.Current.ClassName)'" `
        + " hwnd=$($element.Current.NativeWindowHandle)"
    Write-Host $line
    $children = $element.FindAll(
        [System.Windows.Automation.TreeScope]::Children,
        [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($child in $children) { Write-AutomationTree $child ($depth + 1) }
}

try {
    Write-Host '==> asserting the native window and controls'
    $window = Find-Descendant $root 'Phase Five'
    if ($null -eq $window) { throw 'the generated window never appeared to UI Automation' }

    # Win32 exposes a control's window text as its accessible name, so the
    # button is located by its visible caption. See PHASE_5_SPEC.md section 6.
    $button = Find-Descendant $window 'Add one' ([System.Windows.Automation.ControlType]::Button)
    if ($null -eq $button) {
        Write-Host '--- UI Automation tree below the matched window ---'
        Write-AutomationTree $window
        throw 'the native button is not exposed to UI Automation'
    }
    $controlType = $button.Current.ControlType.ProgrammaticName
    if ($controlType -ne 'ControlType.Button') {
        throw "the button surfaced as $controlType"
    }

    foreach ($name in @('Phase Five', 'Count', 'Sales chart')) {
        if ($null -eq (Find-Descendant $window $name)) {
            Write-Host '--- UI Automation tree below the matched window ---'
            Write-AutomationTree $window
            throw "accessible name '$name' never reached UI Automation"
        }
    }
    Write-Host 'OK: native window, button, heading, output, and surface are exposed'

    Write-Host '==> asserting native interaction'
    $invoke = $button.GetCurrentPattern(
        [System.Windows.Automation.InvokePattern]::Pattern)
    $invoke.Invoke()
    Start-Sleep -Seconds 2
    if ($null -eq (Find-Descendant $window '1')) {
        Write-Host '--- UI Automation tree below the matched window ---'
        Write-AutomationTree $window
        throw 'invoking the native button never updated the bound output'
    }
    Write-Host 'OK: invoking the native button incremented the bound state to 1'
}
finally {
    if (-not $process.HasExited) { $process.CloseMainWindow() | Out-Null }
    Start-Sleep -Seconds 2
    if (-not $process.HasExited) { $process.Kill() }
}

Write-Host '==> lifecycle log'
if (-not (Test-Path $log)) { throw "the host never wrote $log" }
$events = Get-Content $log
$events | Write-Host
foreach ($event in @('controller.created', 'controller.active', 'state.increment',
                     'controller.destroyed')) {
    if (-not ($events -match [regex]::Escape("`"event`":`"$event`""))) {
        throw "missing lifecycle event $event"
    }
}

Write-Host 'PASS: Windows native gate'
