$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$script:process = $null
$script:window = $null
$script:desktop = $null
$script:deadline = $null

function Enable-Msvc {
    if (Get-Command cl.exe -ErrorAction SilentlyContinue) { return }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { throw 'MSVC requires Visual Studio Build Tools' }
    $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $installation) { throw 'MSVC x64 tools are not installed' }
    $developer = Join-Path $installation 'Common7\Tools\VsDevCmd.bat'
    $environment = & $env:ComSpec /d /s /c "`"$developer`" -no_logo -arch=x64 -host_arch=x64 >nul && set"
    if ($LASTEXITCODE -ne 0) { throw 'could not initialize the MSVC developer environment' }
    foreach ($line in $environment) {
        if ($line -match '^([^=]+)=(.*)$') {
            [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process')
        }
    }
}

function Install-WebViewSdk([string] $version, [string] $directory) {
    $archive = "$directory.zip"
    Invoke-WebRequest -TimeoutSec 90 -Uri "https://api.nuget.org/v3-flatcontainer/microsoft.web.webview2/$version/microsoft.web.webview2.$version.nupkg" -OutFile $archive
    $expected = 'bc09150b179246ac90189649b13be8e6b11b3ac200e817e18df106e1f3cf489e'
    if ((Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant() -ne $expected) {
        throw 'the WebView2 SDK package checksum did not match'
    }
    Expand-Archive -Path $archive -DestinationPath $directory -Force
    Remove-Item $archive
}

function Enable-WebViewSdk {
    if ($env:TAC_WEBVIEW2_SDK) { return }
    $version = '1.0.3485.44'
    $directory = Join-Path $env:TEMP "tachyon-webview2-$version"
    if (-not (Test-Path (Join-Path $directory 'build\native\include\WebView2.h'))) {
        Install-WebViewSdk $version $directory
    }
    $env:TAC_WEBVIEW2_SDK = $directory
}

function Write-UiaFailureDiagnostics($window) {
    if ($null -eq $window) {
        Write-Host 'UIA: no top-level window was found for the owned native process'
        return
    }
    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $queue = [System.Collections.Queue]::new()
    $queue.Enqueue($window)
    $remaining = 64
    $until = [DateTime]::UtcNow.AddSeconds(3)
    while ($queue.Count -gt 0 -and $remaining -gt 0 -and [DateTime]::UtcNow -lt $until) {
        $element = $queue.Dequeue()
        $current = $element.Current
        $name = ($current.Name -replace '[\r\n\t]', ' ')
        if ($name.Length -gt 600) { $name = $name.Substring(0, 600) + '...' }
        Write-Host ("UIA: type={0}; name={1}; class={2}; pid={3}; offscreen={4}" -f
            $current.ControlType.ProgrammaticName, $name, $current.ClassName,
            $current.ProcessId, $current.IsOffscreen)
        $remaining--
        $child = $walker.GetFirstChild($element)
        while ($null -ne $child -and $queue.Count -lt $remaining -and [DateTime]::UtcNow -lt $until) {
            $queue.Enqueue($child)
            $child = $walker.GetNextSibling($child)
        }
    }
    Write-Host ("UIA diagnostic traversal ended: nodes={0}; queued={1}" -f (64 - $remaining), $queue.Count)
}

function Write-CompanionAnswers([string] $app) {
    $probe = {
        param([string] $app)
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class TachyonCompanionProbe
{
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate IntPtr Invoke(IntPtr request);

    [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr LoadLibraryW(string path);

    [DllImport("kernel32", CharSet = CharSet.Ansi, ExactSpelling = true)]
    private static extern IntPtr GetProcAddress(IntPtr module, string name);

    public static string Call(string library, string request)
    {
        IntPtr module = LoadLibraryW(library);
        if (module == IntPtr.Zero) return "load failed: " + Marshal.GetLastWin32Error();
        IntPtr entry = GetProcAddress(module, "tac_native_invoke");
        if (entry == IntPtr.Zero) return "no tac_native_invoke export";
        Invoke invoke = (Invoke)Marshal.GetDelegateForFunctionPointer(entry, typeof(Invoke));
        byte[] bytes = Encoding.UTF8.GetBytes(request + "\0");
        IntPtr text = Marshal.AllocHGlobal(bytes.Length);
        try
        {
            Marshal.Copy(bytes, 0, text, bytes.Length);
            return ReadText(invoke(text));
        }
        finally { Marshal.FreeHGlobal(text); }
    }

    private static string ReadText(IntPtr answer)
    {
        if (answer == IntPtr.Zero) return "null answer";
        int length = 0;
        while (length < 65536 && Marshal.ReadByte(answer, length) != 0) length++;
        byte[] bytes = new byte[length];
        Marshal.Copy(answer, bytes, 0, length);
        return Encoding.UTF8.GetString(bytes);
    }
}
'@
        $owner = [guid]::NewGuid().ToString()
        $request = '{"op":"init","props":{},"route":"/","service":"gateRoot","owner":"' + $owner + '"}'
        foreach ($name in @('TachyonCompanion.dll', 'TachyonRustCompanion.dll')) {
            $library = Join-Path $app "bin\$name"
            "Companion {0} init gateRoot: {1}" -f $name, [TachyonCompanionProbe]::Call($library, $request)
        }
    }
    & powershell.exe -NoProfile -NonInteractive -Command $probe -args $app | ForEach-Object { Write-Host $_ }
}

function Write-NativeLogTail([string] $log) {
    if (-not (Test-Path $log)) {
        Write-Host 'Native lifecycle log: absent'
        return
    }
    Write-Host 'Native lifecycle log (last 32 records):'
    Get-Content -LiteralPath $log -Tail 32 | ForEach-Object { Write-Host $_ }
}

function Write-OwnedProcessState($process) {
    if ($null -eq $process) { return }
    $process.Refresh()
    Write-Host ("Owned process: pid={0}; exited={1}" -f $process.Id, $process.HasExited)
    if ($process.HasExited) {
        Write-Host ("Owned process exit code: {0}" -f $process.ExitCode)
        return
    }
    Write-Host ("Owned window: handle={0}; responding={1}" -f $process.MainWindowHandle, $process.Responding)
    Get-CimInstance Win32_Process -Filter "ParentProcessId = $($process.Id)" -OperationTimeoutSec 2 |
        Select-Object -First 16 Name, ProcessId, ParentProcessId |
        Format-Table -AutoSize | Out-String | Write-Host
}

function Write-NativeFailureDiagnostics($process, $window, [string] $log, [string] $app) {
    Write-Host 'Windows native failure diagnostics (before owned-process cleanup):'
    Write-NativeLogTail $log
    Write-OwnedProcessState $process
    Write-UiaFailureDiagnostics $window
    try { Write-CompanionAnswers $app }
    catch { Write-Host "Companion diagnostics were incomplete: $($_.Exception.Message)" }
}

function Stop-NativeProcesses($process) {
    if ($null -eq $process) { return }
    $browsers = @(Get-CimInstance Win32_Process -Filter (
        "ParentProcessId = $($process.Id) AND Name = 'msedgewebview2.exe'") -OperationTimeoutSec 2 |
        Select-Object -First 16 |
        ForEach-Object { Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue })
    $owned = @($process) + $browsers
    if (-not $process.HasExited) { $process.CloseMainWindow() | Out-Null }
    $grace = [DateTime]::UtcNow.AddSeconds(5)
    foreach ($ownedProcess in $owned) {
        $wait = [Math]::Max(0, [int]($grace - [DateTime]::UtcNow).TotalMilliseconds)
        if (-not $ownedProcess.WaitForExit($wait)) {
            & taskkill /PID $ownedProcess.Id /T /F | Out-Null
        }
    }
    $until = [DateTime]::UtcNow.AddSeconds(5)
    foreach ($ownedProcess in $owned) {
        $wait = [Math]::Max(0, [int]($until - [DateTime]::UtcNow).TotalMilliseconds)
        if (-not $ownedProcess.WaitForExit($wait)) { throw 'owned native process cleanup timed out' }
    }
}

function Remove-NativeFixture([string] $fixture) {
    $until = [DateTime]::UtcNow.AddSeconds(5)
    while (Test-Path -LiteralPath $fixture) {
        try { Remove-Item -LiteralPath $fixture -Recurse -Force; return }
        catch [System.IO.IOException] {
            if ([DateTime]::UtcNow -ge $until) { throw }
            Start-Sleep -Milliseconds 200
        }
    }
}

function Build-NativeApp([string] $fixture) {
    python scripts/native/desktop-fixture.py $fixture windows
    if ($LASTEXITCODE -ne 0) { throw 'fixture generation failed' }
    $ty = $env:TAC_BIN
    if (-not $ty) {
        cargo build --locked --bin ty
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
        $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
        $ty = Join-Path $target 'debug\ty.exe'
    }
    & $ty build $fixture --target windows
    if ($LASTEXITCODE -ne 0) { throw 'Windows WebView2/native companion packaging failed' }
}

function Assert-NativeArtifacts([string] $out, [string] $app) {
    foreach ($relative in @(
        'NativeGate\bin\NativeGate.exe',
        'NativeGate\bin\TachyonRustCompanion.dll',
        'NativeGate\bin\TachyonCompanion.dll',
        'NativeGate\application.manifest',
        'NativeGate\resources\NativeIndex.json',
        'NativeGate\resources\WebBundle\index.html',
        'NativeGate\resources\WebBundle\items\_id\index.html',
        'NativeGate\resources\WebBundle\shared\native-gate.css',
        'NativeGate\resources\WebBundle\.tachyon\services\GateRootService.js',
        'artifact-manifest.json', 'tachyon.host.json'
    )) {
        if (-not (Test-Path (Join-Path $out $relative))) { throw "missing artifact $relative" }
    }
    $hostManifest = Get-Content -Raw (Join-Path $out 'tachyon.host.json') | ConvertFrom-Json
    if ($hostManifest.schemaVersion -ne 3 -or $hostManifest.renderMode -ne 'bundle' -or
        $hostManifest.companions.Count -ne 2) { throw 'native host descriptor mismatch' }
    $webHash = (Get-FileHash (Join-Path $out 'web\index.html')).Hash
    $nativeHash = (Get-FileHash (Join-Path $app 'resources\WebBundle\index.html')).Hash
    if ($webHash -ne $nativeHash) { throw 'native host does not contain the emitted web bundle' }
}

function Find-Control([string] $name) {
    if ($null -eq $script:window) {
        $condition = [System.Windows.Automation.PropertyCondition]::new(
            [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $script:process.Id)
        $script:window = $script:desktop.FindFirst(
            [System.Windows.Automation.TreeScope]::Children, $condition)
    }
    if ($null -eq $script:window) { return $null }
    $condition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::NameProperty, $name)
    return $script:window.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants, $condition)
}

function Expect-Controls([string[]] $names) {
    $stageDeadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($script:process.HasExited) { throw "native application exited with $($script:process.ExitCode)" }
        $missing = @($names | Where-Object { $null -eq (Find-Control $_) })
        if ($missing.Count -eq 0) { Write-Host ("OK: " + ($names -join ', ')); return }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $stageDeadline -and [DateTime]::UtcNow -lt $script:deadline)
    throw "rendered controls missing: $($missing -join ', ')"
}

function Invoke-Control([string] $name) {
    Expect-Controls @($name)
    $control = Find-Control $name
    $pattern = $null
    if (-not $control.TryGetCurrentPattern(
        [System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
        throw "rendered control '$name' exposes no UI Automation InvokePattern"
    }
    ([System.Windows.Automation.InvokePattern]$pattern).Invoke()
}

function Test-RenderedControls([string] $app, [string] $log) {
    if (Test-Path $log) { Remove-Item $log }
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    $script:desktop = [System.Windows.Automation.AutomationElement]::RootElement
    $script:process = Start-Process -FilePath (Join-Path $app 'bin\NativeGate.exe') -PassThru
    $script:deadline = [DateTime]::UtcNow.AddSeconds(120)

    Expect-Controls @('Root route', 'Root count 0', 'Shared module ready', 'Verify native Root')
    Invoke-Control 'Verify native Root'
    Expect-Controls @('Root count 7', 'Native Rust 14', 'OS ready', 'Styles ready',
        'Route boundary ready', 'Publish received')
    Invoke-Control 'Open second'
    Expect-Controls @('Second route', 'Second count 0', 'Shared module ready')
    Invoke-Control 'Verify native Second'
    Expect-Controls @('Second count 9', 'Native CSharp 18', 'OS ready', 'Styles ready',
        'Route boundary ready', 'Publish received')
    Invoke-Control 'Return root'
    Expect-Controls @('Root route', 'Root count 0')
    Invoke-Control 'Leave app'
    Start-Sleep -Milliseconds 500
    Expect-Controls @('Root route', 'Root count 0', 'Verify native Root')
}

function Assert-LifecycleEvents([string] $log) {
    if (-not (Test-Path $log)) { throw 'native lifecycle log was not produced' }
    $events = Get-Content $log
    foreach ($eventName in @('controller.created', 'controller.active', 'controller.destroyed', 'companion.loaded')) {
        if (-not ($events -match [regex]::Escape("`"event`":`"$eventName`""))) {
            throw "missing lifecycle event $eventName"
        }
    }
}

function Invoke-NativeGate {
    Enable-Msvc
    Enable-WebViewSdk
    $fixture = Join-Path $env:TEMP ("ty-windows-native-" + [guid]::NewGuid().ToString('N'))
    $out = Join-Path $fixture 'dist\windows'
    $app = Join-Path $out 'NativeGate'
    $log = Join-Path $env:LOCALAPPDATA 'Tachyon\dev.tachyon.desktop-gate.jsonl'
    $gateFailure = $null

    try {
        Build-NativeApp $fixture
        Assert-NativeArtifacts $out $app
        Test-RenderedControls $app $log
    }
    catch {
        $gateFailure = $_
        try { Write-NativeFailureDiagnostics $script:process $script:window $log $app }
        catch { Write-Warning "Native failure diagnostics were incomplete: $($_.Exception.Message)" }
        throw $gateFailure
    }
    finally {
        try {
            Stop-NativeProcesses $script:process
            Remove-NativeFixture $fixture
        }
        catch {
            if ($null -eq $gateFailure) { throw }
            Write-Warning "Native cleanup also failed: $($_.Exception.Message)"
        }
    }

    Assert-LifecycleEvents $log
    Write-Host 'PASS: Windows WebView2, Rust/C# native ABI, publish, shared assets, dynamic routes, and isolation'
}

Invoke-NativeGate
