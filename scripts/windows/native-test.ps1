$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$script:process = $null
$script:window = $null
$script:desktop = $null
$script:deadline = $null
$script:devToolsPort = 9343

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
        if ($name.Length -gt 160) { $name = $name.Substring(0, 160) + '...' }
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

function Send-DevToolsCommand($socket, [int] $id, [string] $method, [string] $parameters) {
    $text = '{"id":' + $id + ',"method":"' + $method + '","params":' + $parameters + '}'
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($text)
    $segment = [System.ArraySegment[byte]]::new($bytes)
    $sent = $socket.SendAsync($segment, [System.Net.WebSockets.WebSocketMessageType]::Text, $true,
        [System.Threading.CancellationToken]::None)
    if (-not $sent.Wait(2000)) { throw "DevTools command $method timed out" }
}

function Receive-DevToolsMessage($socket, [DateTime] $until) {
    $buffer = [byte[]]::new(65536)
    $message = [System.IO.MemoryStream]::new()
    do {
        $wait = [int][Math]::Max(0, ($until - [DateTime]::UtcNow).TotalMilliseconds)
        $segment = [System.ArraySegment[byte]]::new($buffer)
        $received = $socket.ReceiveAsync($segment, [System.Threading.CancellationToken]::None)
        if (-not $received.Wait($wait)) { return $null }
        $message.Write($buffer, 0, $received.Result.Count)
    } while (-not $received.Result.EndOfMessage)
    return [System.Text.Encoding]::UTF8.GetString($message.ToArray())
}

function Write-DevToolsMessage([string] $message) {
    if ($message.StartsWith('{"method":"Runtime.executionContext')) { return }
    if ($message.Length -gt 4000) { $message = $message.Substring(0, 4000) + '...' }
    Write-Host "WebView2: $message"
}

function Get-WebViewPage {
    $targets = Invoke-RestMethod -TimeoutSec 3 -Uri "http://127.0.0.1:$($script:devToolsPort)/json/list"
    foreach ($target in @($targets)) {
        Write-Host ("WebView2 target: type={0}; url={1}; title={2}" -f $target.type, $target.url, $target.title)
    }
    return @($targets | Where-Object { $_.type -eq 'page' }) | Select-Object -First 1
}

function Write-WebViewDiagnostics {
    $page = Get-WebViewPage
    if ($null -eq $page) { Write-Host 'WebView2: no page target'; return }
    $socket = [System.Net.WebSockets.ClientWebSocket]::new()
    try {
        $connected = $socket.ConnectAsync([Uri] $page.webSocketDebuggerUrl, [System.Threading.CancellationToken]::None)
        if (-not $connected.Wait(3000)) { Write-Host 'WebView2: DevTools connection timed out'; return }
        Send-DevToolsCommand $socket 1 'Log.enable' '{}'
        Send-DevToolsCommand $socket 2 'Runtime.enable' '{}'
        $expression = "JSON.stringify({ state: document.readyState, url: location.href, " +
            "host: typeof globalThis.__tachyonNativeHostCall, " +
            "navigation: performance.getEntriesByType('navigation').map(entry => entry.responseStatus), " +
            "resources: performance.getEntriesByType('resource').slice(0, 40).map(entry => entry.name + ' ' + entry.responseStatus), " +
            "html: document.documentElement.outerHTML.slice(0, 3000) })"
        $parameters = '{"returnByValue":true,"expression":' + (ConvertTo-Json -InputObject $expression -Compress) + '}'
        Send-DevToolsCommand $socket 3 'Runtime.evaluate' $parameters
        $until = [DateTime]::UtcNow.AddSeconds(5)
        for ($count = 0; $count -lt 200; $count++) {
            $message = Receive-DevToolsMessage $socket $until
            if ($null -eq $message) { Write-Host 'WebView2: DevTools answer timed out'; return }
            Write-DevToolsMessage $message
            if ($message.StartsWith('{"id":3,')) { return }
        }
    }
    finally {
        $socket.Abort()
        $socket.Dispose()
    }
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

function Write-NativeFailureDiagnostics($process, $window, [string] $log) {
    Write-Host 'Windows native failure diagnostics (before owned-process cleanup):'
    Write-NativeLogTail $log
    Write-OwnedProcessState $process
    try { Write-WebViewDiagnostics }
    catch { Write-Host "WebView2 diagnostics were incomplete: $($_.Exception.Message)" }
    Write-UiaFailureDiagnostics $window
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
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$($script:devToolsPort)"
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
        try { Write-NativeFailureDiagnostics $script:process $script:window $log }
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
