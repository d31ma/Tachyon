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

@'
{"application":{"name":"Phase Five","id":"dev.tachyon.phase-five","version":"0.1.0","entry_route":"/"}}
'@ | Set-Content -Encoding utf8 (Join-Path $fixture 'tachyon.json')

@'
<main aria-label="Phase Five demo">
  <h1>Phase Five</h1>
  <p>Cross-platform native adapters.</p>
  <button aria-label="Increase count" data-tachyon-action="increment:count">Add one</button>
  <output aria-label="Count" data-tachyon-bind="count" data-tachyon-state="0">0</output>
  <input aria-label="Your name" data-tachyon-bind="name" data-tachyon-state="" placeholder="Name">
  <details aria-label="More detail"><summary>More detail</summary><p>Disclosure content.</p></details>
  <x-chart aria-label="Sales chart"><p>Chart fallback</p></x-chart>
</main>
'@ | Set-Content -Encoding utf8 (Join-Path $fixture 'client\pages\tac.html')

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
    'PhaseFive\application.manifest',
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

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::RootElement

function Find-Descendant([string] $name) {
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty, $name)
    return $root.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants, $condition)
}

try {
    Write-Host '==> asserting the native window and controls'
    $window = Find-Descendant 'Phase Five'
    if ($null -eq $window) { throw 'the generated window never appeared to UI Automation' }

    # Win32 exposes a control's window text as its accessible name, so the
    # button is located by its visible caption. See PHASE_5_SPEC.md section 6.
    $button = Find-Descendant 'Add one'
    if ($null -eq $button) { throw 'the native button is not exposed to UI Automation' }
    $controlType = $button.Current.ControlType.ProgrammaticName
    if ($controlType -ne 'ControlType.Button') {
        throw "the button surfaced as $controlType"
    }

    foreach ($name in @('Phase Five', 'Count', 'Sales chart')) {
        if ($null -eq (Find-Descendant $name)) {
            throw "accessible name '$name' never reached UI Automation"
        }
    }
    Write-Host 'OK: native window, button, heading, output, and surface are exposed'

    Write-Host '==> asserting native interaction'
    $invoke = $button.GetCurrentPattern(
        [System.Windows.Automation.InvokePattern]::Pattern)
    $invoke.Invoke()
    Start-Sleep -Seconds 2
    if ($null -eq (Find-Descendant '1')) {
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
